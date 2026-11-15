//! Biometric vault unlock (`macos` Touch ID and `windows` Windows Hello).
//!
//! Matches the Android Keystore design (`Biometric.kt`):
//! - On macOS: requires a Developer ID signed binary (`own_team_id().is_some()`),
//!   stores a 256-bit wrapping secret in the Login Keychain bound to the signed
//!   app identity, and binds `LAContext::evaluatedPolicyDomainState()` via HKDF
//!   so any change to enrolled fingerprints invalidates the sealed key.
//! - On Windows: gates unlock behind `UserConsentVerifier` (Windows Hello),
//!   seals the 256-bit wrapping secret with user-scoped DPAPI (`CryptProtectData`),
//!   and stores it locally in Windows Credential Manager (`CRED_PERSIST_LOCAL_MACHINE`).
//! - Enforces the master password after every reboot (`kern.boottime` on macOS,
//!   a volatile registry key on Windows) and at least every 7 days.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use kryptos_core::crypto::{self, SecretKey};
use kryptos_core::{paths, storage, UnlockedVault};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::profiles::{self, Registry};
use crate::state::AppState;

const FILE: &str = "biometric.json";
const MAX_SPAN_SECS: u64 = 7 * 24 * 3600;
const TIMEOUT: Duration = Duration::from_secs(120);
#[cfg(target_os = "macos")]
const INFO_PREFIX: &str = "kryptos-macos-touchid-v1";
#[cfg(target_os = "windows")]
const INFO_PREFIX: &str = "kryptos-windows-hello-v1";

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Record {
    /// Base64 of `nonce (24) || XChaCha20-Poly1305 ciphertext` wrapping `vault.raw_key()`.
    sealed: String,
    /// Base64 of the OS biometric domain state (macOS) or random HKDF salt (Windows).
    domain: String,
    /// Unix seconds when the master password was last verified for this profile.
    master_at: u64,
    /// Platform boot-session identifier when the master password was verified.
    boot_time: i64,
}

fn record_path(profile: &str) -> Result<PathBuf, String> {
    Ok(profiles::dir_of(&paths::data_dir(), profile)?.join(FILE))
}

fn load_record(profile: &str) -> Record {
    record_path(profile)
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_record(profile: &str, rec: &Record) -> Result<(), String> {
    let path = record_path(profile)?;
    let bytes = serde_json::to_vec_pretty(rec).map_err(|e| e.to_string())?;
    storage::write_replace(&path, &bytes).map_err(|e| e.to_string())
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn master_fresh_at(rec: &Record, now: u64, current_boot: Option<i64>) -> bool {
    rec.master_at > 0 && now >= rec.master_at && now - rec.master_at < MAX_SPAN_SECS && current_boot.is_some() && Some(rec.boot_time) == current_boot
}

fn derive_wrap_key(keychain_secret: &[u8], domain: &[u8], profile: &str) -> SecretKey {
    let info = format!("{INFO_PREFIX}:{profile}");
    crypto::hkdf_key(keychain_secret, domain, info.as_bytes())
}

fn seal_key(wrap: &SecretKey, raw_key: &[u8], profile: &str) -> String {
    let (nonce, ct) = crypto::seal(wrap, raw_key, profile.as_bytes());
    let mut blob = Zeroizing::new(Vec::with_capacity(crypto::NONCE_LEN + ct.len()));
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ct);
    B64.encode(&*blob)
}

fn open_key(wrap: &SecretKey, sealed_b64: &str, profile: &str) -> Option<Zeroizing<Vec<u8>>> {
    let blob = Zeroizing::new(B64.decode(sealed_b64).ok()?);
    if blob.len() <= crypto::NONCE_LEN {
        return None;
    }
    let (nonce, ct) = blob.split_at(crypto::NONCE_LEN);
    crypto::open(wrap, nonce, ct, profile.as_bytes())
}

enum Probe {
    /// Hardware/biometry is ready. `Some(bytes)` carries enrollment domain state
    /// that invalidates sealed records when changed; `None` omits domain diffs.
    Ready(Option<Vec<u8>>),
    Lockout,
    Unavailable,
}

enum SecretLoad {
    Found(Zeroizing<Vec<u8>>),
    NotFound,
    Error,
}

#[cfg(target_os = "macos")]
mod platform {
    use std::sync::mpsc;

    use block2::RcBlock;
    use kryptos_core::peer;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAError, LAPolicy};
    use security_framework::passwords::{delete_generic_password, generic_password, set_generic_password, PasswordOptions};
    use zeroize::Zeroizing;

    use super::{Probe, SecretLoad, TIMEOUT};

    const SERVICE: &str = "com.kryptos.vault.biometric";
    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
    const POLICY: LAPolicy = LAPolicy::DeviceOwnerAuthenticationWithBiometrics;

    pub fn boot_time() -> Option<i64> {
        let mut tv: libc::timeval = unsafe { std::mem::zeroed() };
        let mut size = std::mem::size_of::<libc::timeval>();
        let r = unsafe { libc::sysctlbyname(c"kern.boottime".as_ptr(), (&mut tv as *mut libc::timeval).cast(), &mut size, std::ptr::null_mut(), 0) };
        (r == 0 && size == std::mem::size_of::<libc::timeval>() && tv.tv_sec > 0).then_some(tv.tv_sec as i64)
    }

    #[allow(deprecated)]
    fn context_domain(ctx: &LAContext) -> Option<Vec<u8>> {
        let data = unsafe { ctx.evaluatedPolicyDomainState() }?;
        let bytes = data.to_vec();
        (!bytes.is_empty()).then_some(bytes)
    }

    pub fn probe_biometry() -> Probe {
        if peer::own_team_id().is_none() {
            return Probe::Unavailable;
        }
        let ctx = unsafe { LAContext::new() };
        match unsafe { ctx.canEvaluatePolicy_error(POLICY) } {
            Ok(()) => match context_domain(&ctx) {
                Some(domain) => Probe::Ready(Some(domain)),
                None => Probe::Unavailable,
            },
            Err(err) if err.code() == LAError::BiometryLockout.0 => Probe::Lockout,
            Err(_) => Probe::Unavailable,
        }
    }

    pub fn evaluate_biometrics(reason: &str) -> Result<Option<Vec<u8>>, String> {
        if peer::own_team_id().is_none() {
            return Err("unavailable".into());
        }
        let ctx = unsafe { LAContext::new() };
        if let Err(err) = unsafe { ctx.canEvaluatePolicy_error(POLICY) } {
            return Err(if err.code() == LAError::BiometryLockout.0 { "lockout" } else { "unavailable" }.into());
        }
        // Strictly require Touch ID without falling back to the macOS account password button.
        unsafe { ctx.setLocalizedFallbackTitle(Some(&NSString::from_str(""))) };
        let reason = if reason.trim().is_empty() { "Touch ID" } else { reason };
        let (tx, rx) = mpsc::channel::<Result<(), isize>>();
        let reply = RcBlock::new(move |ok: Bool, error: *mut NSError| {
            let result = if ok.as_bool() {
                Ok(())
            } else {
                Err(unsafe { error.as_ref() }.map_or(LAError::AuthenticationFailed.0, |e| e.code()))
            };
            let _ = tx.send(result);
        });
        unsafe { ctx.evaluatePolicy_localizedReason_reply(POLICY, &NSString::from_str(reason), &reply) };
        match rx.recv_timeout(TIMEOUT) {
            Ok(Ok(())) => context_domain(&ctx).map(Some).ok_or_else(|| "failed".to_string()),
            Ok(Err(c)) if c == LAError::UserCancel.0 || c == LAError::AppCancel.0 => Err("cancelled".into()),
            Ok(Err(c)) if c == LAError::UserFallback.0 => Err("use_master".into()),
            Ok(Err(c)) if c == LAError::SystemCancel.0 => Err("interrupted".into()),
            Ok(Err(c)) if c == LAError::BiometryLockout.0 => Err("lockout".into()),
            Ok(Err(c)) if c == LAError::BiometryNotAvailable.0 || c == LAError::BiometryNotEnrolled.0 || c == LAError::PasscodeNotSet.0 => {
                Err("unavailable".into())
            }
            Ok(Err(_)) => Err("failed".into()),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                unsafe { ctx.invalidate() };
                Err("cancelled".into())
            }
            Err(_) => Err("failed".into()),
        }
    }

    pub fn save_secret(profile: &str, secret: &[u8]) -> Result<(), ()> {
        set_generic_password(SERVICE, profile, secret).map_err(|_| ())
    }

    pub fn load_secret(profile: &str) -> SecretLoad {
        match generic_password(PasswordOptions::new_generic_password(SERVICE, profile)) {
            Ok(bytes) => SecretLoad::Found(Zeroizing::new(bytes)),
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => SecretLoad::NotFound,
            Err(_) => SecretLoad::Error,
        }
    }

    pub fn delete_secret(profile: &str) {
        let _ = delete_generic_password(SERVICE, profile);
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::slice;

    use kryptos_core::crypto::SecretKey;
    use windows::core::{HRESULT, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{ERROR_NOT_FOUND, ERROR_SUCCESS, HLOCAL, LocalFree};
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };
    use windows::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE,
        REG_OPTION_VOLATILE, REG_QWORD, REG_VALUE_TYPE,
    };
    use zeroize::{Zeroize, Zeroizing};

    use super::{Probe, SecretLoad, TIMEOUT};
    use crate::confirm::winhello::{self, Availability};

    const MAX_CRED_BLOB: usize = 2560;

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn cred_target(profile: &str) -> Vec<u16> {
        to_wide(&format!("Kryptos:biometric:{profile}"))
    }

    fn dpapi_entropy(profile: &str) -> Vec<u8> {
        format!("kryptos-dpapi-biometric-v1:{profile}").into_bytes()
    }

    struct LocalBlob(CRYPT_INTEGER_BLOB, bool);

    impl Drop for LocalBlob {
        fn drop(&mut self) {
            if !self.0.pbData.is_null() {
                if self.1 && self.0.cbData > 0 {
                    // SAFETY: CryptUnprotectData allocated cbData bytes at pbData.
                    let s = unsafe { slice::from_raw_parts_mut(self.0.pbData, self.0.cbData as usize) };
                    s.zeroize();
                }
                unsafe {
                    let _ = LocalFree(Some(HLOCAL(self.0.pbData.cast())));
                }
            }
        }
    }

    struct CredGuard(*mut CREDENTIALW);

    impl Drop for CredGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CredFree(self.0.cast()) };
            }
        }
    }

    struct HkeyGuard(HKEY);

    impl Drop for HkeyGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }

    fn dpapi_protect(secret: &[u8], entropy: &[u8]) -> Result<Vec<u8>, ()> {
        let in_blob = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(secret.len()).map_err(|_| ())?,
            pbData: secret.as_ptr() as *mut u8,
        };
        let entropy_blob = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(entropy.len()).map_err(|_| ())?,
            pbData: entropy.as_ptr() as *mut u8,
        };
        let mut out = LocalBlob(CRYPT_INTEGER_BLOB::default(), false);
        unsafe {
            CryptProtectData(&in_blob, PCWSTR::null(), Some(&entropy_blob), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out.0)
                .map_err(|_| ())?;
        }
        if out.0.pbData.is_null() || out.0.cbData == 0 {
            return Err(());
        }
        let bytes = unsafe { slice::from_raw_parts(out.0.pbData, out.0.cbData as usize) };
        Ok(bytes.to_vec())
    }

    fn dpapi_unprotect(cipher: &[u8], entropy: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
        let in_blob = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(cipher.len()).ok()?,
            pbData: cipher.as_ptr() as *mut u8,
        };
        let entropy_blob = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(entropy.len()).ok()?,
            pbData: entropy.as_ptr() as *mut u8,
        };
        let mut out = LocalBlob(CRYPT_INTEGER_BLOB::default(), true);
        unsafe { CryptUnprotectData(&in_blob, None, Some(&entropy_blob), None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out.0).ok()? };
        if out.0.pbData.is_null() || out.0.cbData == 0 {
            return None;
        }
        let bytes = unsafe { slice::from_raw_parts(out.0.pbData, out.0.cbData as usize) };
        Some(Zeroizing::new(bytes.to_vec()))
    }

    /// Identifies the current Windows boot/login session via a volatile HKCU registry key
    /// so every Windows restart invalidates the previous session token and requires the master password.
    pub fn boot_time() -> Option<i64> {
        let parent_subkey = to_wide("Software\\Kryptos");
        let mut parent = HKEY::default();
        let rc = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(parent_subkey.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_READ | KEY_WRITE,
                None,
                &mut parent,
                None,
            )
        };
        if rc != ERROR_SUCCESS {
            return None;
        }
        let _parent_guard = HkeyGuard(parent);

        let session_subkey = to_wide("Software\\Kryptos\\BootSession");
        let mut session = HKEY::default();
        let rc = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(session_subkey.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_VOLATILE,
                KEY_READ | KEY_WRITE,
                None,
                &mut session,
                None,
            )
        };
        if rc != ERROR_SUCCESS {
            return None;
        }
        let session_guard = HkeyGuard(session);

        let val_name = to_wide("BootId");
        let mut kind = REG_VALUE_TYPE::default();
        let mut id: i64 = 0;
        let mut len = std::mem::size_of::<i64>() as u32;
        let q = unsafe {
            RegQueryValueExW(
                session_guard.0,
                PCWSTR(val_name.as_ptr()),
                None,
                Some(&mut kind),
                Some((&mut id as *mut i64).cast()),
                Some(&mut len),
            )
        };
        if q == ERROR_SUCCESS && kind == REG_QWORD && len == std::mem::size_of::<i64>() as u32 && id > 0 {
            return Some(id);
        }

        let rand = SecretKey::random();
        let mut raw_bytes = [0u8; 8];
        raw_bytes.copy_from_slice(&rand.as_bytes()[..8]);
        let new_id = ((u64::from_le_bytes(raw_bytes) & (i64::MAX as u64)) as i64) | 1;
        let new_bytes = new_id.to_le_bytes();
        let s = unsafe { RegSetValueExW(session_guard.0, PCWSTR(val_name.as_ptr()), None, REG_QWORD, Some(&new_bytes)) };
        (s == ERROR_SUCCESS).then_some(new_id)
    }

    pub fn probe_biometry() -> Probe {
        match winhello::check_availability() {
            Availability::Available => Probe::Ready(None),
            Availability::Busy => Probe::Lockout,
            Availability::Unavailable => Probe::Unavailable,
        }
    }

    pub fn evaluate_biometrics(reason: &str) -> Result<Option<Vec<u8>>, String> {
        winhello::evaluate(reason, TIMEOUT)
            .map(|()| None)
            .map_err(|e| if e == "timeout" { "cancelled".into() } else { e })
    }

    pub fn save_secret(profile: &str, secret: &[u8]) -> Result<(), ()> {
        let protected = dpapi_protect(secret, &dpapi_entropy(profile))?;
        if protected.len() > MAX_CRED_BLOB {
            return Err(());
        }
        let target = cred_target(profile);
        let user = to_wide("Kryptos");
        let cred = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_ptr() as *mut u16),
            CredentialBlobSize: protected.len() as u32,
            CredentialBlob: protected.as_ptr() as *mut u8,
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: PWSTR(user.as_ptr() as *mut u16),
            ..Default::default()
        };
        unsafe { CredWriteW(&cred, 0) }.map_err(|_| ())
    }

    pub fn load_secret(profile: &str) -> SecretLoad {
        let target = cred_target(profile);
        let mut ptr: *mut CREDENTIALW = std::ptr::null_mut();
        if let Err(e) = unsafe { CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None, &mut ptr) } {
            return if e.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) {
                SecretLoad::NotFound
            } else {
                SecretLoad::Error
            };
        }
        let guard = CredGuard(ptr);
        let cred = unsafe { &*guard.0 };
        if cred.CredentialBlob.is_null() || cred.CredentialBlobSize == 0 {
            return SecretLoad::NotFound;
        }
        let cipher = unsafe { slice::from_raw_parts(cred.CredentialBlob, cred.CredentialBlobSize as usize) };
        match dpapi_unprotect(cipher, &dpapi_entropy(profile)) {
            Some(bytes) => SecretLoad::Found(bytes),
            None => SecretLoad::NotFound,
        }
    }

    pub fn delete_secret(profile: &str) {
        let target = cred_target(profile);
        unsafe {
            let _ = CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None);
        }
    }
}

fn clear_sealed(profile: &str) {
    platform::delete_secret(profile);
    let mut rec = load_record(profile);
    if !rec.sealed.is_empty() || !rec.domain.is_empty() {
        rec.sealed.clear();
        rec.domain.clear();
        let _ = save_record(profile, &rec);
    }
}

/// `"unavailable" | "off" | "on" | "master_required"`
pub fn status() -> String {
    let profile = Registry::load().active;
    let domain = match platform::probe_biometry() {
        Probe::Unavailable => return "unavailable".into(),
        Probe::Ready(d) => d,
        Probe::Lockout => None,
    };
    let rec = load_record(&profile);
    if rec.sealed.is_empty() || rec.domain.is_empty() {
        return "off".into();
    }
    if let Some(cur) = domain {
        if B64.decode(&rec.domain).ok().as_deref() != Some(cur.as_slice()) {
            clear_sealed(&profile);
            return "off".into();
        }
    }
    if !master_fresh_at(&rec, now_secs(), platform::boot_time()) {
        return "master_required".into();
    }
    "on".into()
}

/// Records that the master password was just verified for the active profile.
pub fn master_unlocked() {
    let profile = Registry::load().active;
    let mut rec = load_record(&profile);
    rec.master_at = now_secs();
    rec.boot_time = platform::boot_time().unwrap_or(0);
    let _ = save_record(&profile, &rec);
}

/// Seals the active profile's unlocked vault key under OS biometrics and local secure storage.
pub fn enable(state: &AppState, reason: &str) -> Result<(), String> {
    if !state.is_unlocked() {
        return Err("locked".into());
    }
    let profile = Registry::load().active;
    let eval_domain = platform::evaluate_biometrics(reason)?;
    let domain = eval_domain.unwrap_or_else(|| SecretKey::random().as_bytes().to_vec());
    let raw_key = state.raw_key().ok_or("locked")?;
    let secret = SecretKey::random();
    platform::save_secret(&profile, secret.as_bytes()).map_err(|_| "failed".to_string())?;
    let wrap = derive_wrap_key(secret.as_bytes(), &domain, &profile);
    let sealed = seal_key(&wrap, &raw_key, &profile);
    let mut rec = load_record(&profile);
    rec.sealed = sealed;
    rec.domain = B64.encode(&domain);
    save_record(&profile, &rec)
}

/// Prompts biometric verification and unlocks the active profile's vault.
pub fn unlock(state: &AppState, reason: &str) -> Result<(), String> {
    let profile = Registry::load().active;
    let vault_path = state.vault_path();
    let rec = load_record(&profile);
    if rec.sealed.is_empty() || rec.domain.is_empty() {
        return Err("off".into());
    }
    let stored_domain = B64.decode(&rec.domain).map_err(|_| {
        clear_sealed(&profile);
        "invalidated".to_string()
    })?;
    if !master_fresh_at(&rec, now_secs(), platform::boot_time()) {
        return Err("master_required".into());
    }
    match platform::probe_biometry() {
        Probe::Unavailable => return Err("unavailable".into()),
        Probe::Lockout => return Err("lockout".into()),
        Probe::Ready(Some(cur)) if cur != stored_domain => {
            clear_sealed(&profile);
            return Err("invalidated".into());
        }
        Probe::Ready(_) => {}
    }

    let eval_domain = platform::evaluate_biometrics(reason)?;
    if let Some(ref cur) = eval_domain {
        if cur != &stored_domain {
            clear_sealed(&profile);
            return Err("invalidated".into());
        }
    }
    let domain = eval_domain.as_deref().unwrap_or(stored_domain.as_slice());
    if !master_fresh_at(&rec, now_secs(), platform::boot_time()) {
        return Err("master_required".into());
    }

    let secret = match platform::load_secret(&profile) {
        SecretLoad::Found(bytes) => bytes,
        SecretLoad::NotFound => {
            clear_sealed(&profile);
            return Err("invalidated".into());
        }
        SecretLoad::Error => return Err("failed".into()),
    };

    let wrap = derive_wrap_key(&secret, domain, &profile);
    let Some(raw_key) = open_key(&wrap, &rec.sealed, &profile) else {
        clear_sealed(&profile);
        return Err("invalidated".into());
    };
    let bytes = storage::read(&vault_path).map_err(|e| e.to_string())?;
    let vault = UnlockedVault::unlock_with_key(&bytes, &raw_key).map_err(|_| {
        clear_sealed(&profile);
        "invalidated".to_string()
    })?;

    if Registry::load().active != profile || state.vault_path() != vault_path {
        return Err("cancelled".into());
    }
    state.set_unlocked(vault);
    Ok(())
}

/// Disables biometric unlock for the active profile.
pub fn disable() {
    let profile = Registry::load().active;
    disable_for(&profile);
}

/// Disables biometric unlock and deletes any stored wrapping secret for `profile`.
pub fn disable_for(profile: &str) {
    clear_sealed(profile);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_key_roundtrip_and_domain_binding() {
        let secret = SecretKey::random();
        let domain = b"enclave-domain-state-v1";
        let raw_key = vec![0x42u8; 32];
        let wrap = derive_wrap_key(secret.as_bytes(), domain, "default");
        let sealed = seal_key(&wrap, &raw_key, "default");

        assert_eq!(open_key(&wrap, &sealed, "default").unwrap().as_slice(), raw_key.as_slice());

        // Different profile AAD or different Touch ID domain state must not unwrap the key.
        assert!(open_key(&wrap, &sealed, "other-profile").is_none());
        let other_domain_wrap = derive_wrap_key(secret.as_bytes(), b"enclave-domain-state-v2", "default");
        assert!(open_key(&other_domain_wrap, &sealed, "default").is_none());
    }

    #[test]
    fn master_fresh_requires_recent_unlock_and_same_boot() {
        let boot = Some(1_700_000_000i64);
        let now = 1_700_100_000u64;
        let fresh = Record {
            sealed: String::new(),
            domain: String::new(),
            master_at: now - 3600,
            boot_time: 1_700_000_000,
        };
        assert!(master_fresh_at(&fresh, now, boot));

        // After 7 days, master password is required again.
        assert!(!master_fresh_at(&fresh, fresh.master_at + MAX_SPAN_SECS, boot));

        // After a reboot (different boot_time), master password is required again.
        assert!(!master_fresh_at(&fresh, now, Some(1_700_000_500)));

        // Never verified.
        let unset = Record::default();
        assert!(!master_fresh_at(&unset, now, boot));
    }
}
