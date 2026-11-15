//! macOS Touch ID vault unlock (`#[cfg(target_os = "macos")]`).
//!
//! Matches the Android Keystore design (`Biometric.kt`):
//! - Requires a Developer ID signed binary (`own_team_id().is_some()`), so the
//!   macOS Login Keychain ACL binds the secret to the signed app identity.
//! - Generates a random 256-bit wrapping secret stored in the macOS Keychain,
//!   combines it with `LAContext::evaluatedPolicyDomainState()` via HKDF-SHA256,
//!   and wraps `vault.raw_key()` with XChaCha20-Poly1305 (AAD = profile id).
//! - Any change to enrolled Touch ID fingerprints changes the domain state,
//!   immediately invalidating the sealed key.
//! - Enforces the master password after every reboot (`kern.boottime`) and at
//!   least every 7 days.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use block2::RcBlock;
use kryptos_core::crypto::{self, SecretKey};
use kryptos_core::{paths, peer, storage, UnlockedVault};
use objc2::runtime::Bool;
use objc2_foundation::{NSError, NSString};
use objc2_local_authentication::{LAContext, LAError, LAPolicy};
use security_framework::passwords::{delete_generic_password, generic_password, set_generic_password, PasswordOptions};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::profiles::{self, Registry};
use crate::state::AppState;

const SERVICE: &str = "com.kryptos.vault.biometric";
const FILE: &str = "biometric.json";
const MAX_SPAN_SECS: u64 = 7 * 24 * 3600;
const TIMEOUT: Duration = Duration::from_secs(120);
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
const POLICY: LAPolicy = LAPolicy::DeviceOwnerAuthenticationWithBiometrics;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Record {
    /// Base64 of `nonce (24) || XChaCha20-Poly1305 ciphertext` wrapping `vault.raw_key()`.
    sealed: String,
    /// Base64 of `LAContext::evaluatedPolicyDomainState` at the time Touch ID was enabled.
    domain: String,
    /// Unix seconds when the master password was last verified for this profile.
    master_at: u64,
    /// Kernel `kern.boottime.tv_sec` when the master password was verified.
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

fn boot_time() -> Option<i64> {
    let mut tv: libc::timeval = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of::<libc::timeval>();
    let r = unsafe { libc::sysctlbyname(c"kern.boottime".as_ptr(), (&mut tv as *mut libc::timeval).cast(), &mut size, std::ptr::null_mut(), 0) };
    (r == 0 && size == std::mem::size_of::<libc::timeval>() && tv.tv_sec > 0).then_some(tv.tv_sec as i64)
}

fn master_fresh_at(rec: &Record, now: u64, current_boot: Option<i64>) -> bool {
    rec.master_at > 0 && now >= rec.master_at && now - rec.master_at < MAX_SPAN_SECS && current_boot.is_some() && Some(rec.boot_time) == current_boot
}

fn derive_wrap_key(keychain_secret: &[u8], domain: &[u8], profile: &str) -> SecretKey {
    let info = format!("kryptos-macos-touchid-v1:{profile}");
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

#[allow(deprecated)]
fn context_domain(ctx: &LAContext) -> Option<Vec<u8>> {
    let data = unsafe { ctx.evaluatedPolicyDomainState() }?;
    let bytes = data.to_vec();
    (!bytes.is_empty()).then_some(bytes)
}

enum Probe {
    Ready(Vec<u8>),
    Lockout,
    Unavailable,
}

fn probe_biometry() -> Probe {
    if peer::own_team_id().is_none() {
        return Probe::Unavailable;
    }
    let ctx = unsafe { LAContext::new() };
    match unsafe { ctx.canEvaluatePolicy_error(POLICY) } {
        Ok(()) => match context_domain(&ctx) {
            Some(domain) => Probe::Ready(domain),
            None => Probe::Unavailable,
        },
        Err(err) if err.code() == LAError::BiometryLockout.0 => Probe::Lockout,
        Err(_) => Probe::Unavailable,
    }
}

fn evaluate_biometrics(reason: &str) -> Result<Vec<u8>, String> {
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
        Ok(Ok(())) => context_domain(&ctx).ok_or_else(|| "failed".to_string()),
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

fn clear_sealed(profile: &str) {
    let _ = delete_generic_password(SERVICE, profile);
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
    let domain = match probe_biometry() {
        Probe::Unavailable => return "unavailable".into(),
        Probe::Ready(d) => Some(d),
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
    if !master_fresh_at(&rec, now_secs(), boot_time()) {
        return "master_required".into();
    }
    "on".into()
}

/// Records that the master password was just verified for the active profile.
pub fn master_unlocked() {
    let profile = Registry::load().active;
    let mut rec = load_record(&profile);
    rec.master_at = now_secs();
    rec.boot_time = boot_time().unwrap_or(0);
    let _ = save_record(&profile, &rec);
}

/// Seals the active profile's unlocked vault key under Touch ID + Keychain.
pub fn enable(state: &AppState, reason: &str) -> Result<(), String> {
    if !state.is_unlocked() {
        return Err("locked".into());
    }
    let profile = Registry::load().active;
    let domain = evaluate_biometrics(reason)?;
    let raw_key = state.raw_key().ok_or("locked")?;
    let secret = SecretKey::random();
    set_generic_password(SERVICE, &profile, secret.as_bytes()).map_err(|_| "failed".to_string())?;
    let wrap = derive_wrap_key(secret.as_bytes(), &domain, &profile);
    let sealed = seal_key(&wrap, &raw_key, &profile);
    let mut rec = load_record(&profile);
    rec.sealed = sealed;
    rec.domain = B64.encode(&domain);
    save_record(&profile, &rec)
}

/// Prompts Touch ID and unlocks the active profile's vault using the Keychain-backed key.
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
    if !master_fresh_at(&rec, now_secs(), boot_time()) {
        return Err("master_required".into());
    }
    match probe_biometry() {
        Probe::Unavailable => return Err("unavailable".into()),
        Probe::Lockout => return Err("lockout".into()),
        Probe::Ready(cur) if cur != stored_domain => {
            clear_sealed(&profile);
            return Err("invalidated".into());
        }
        Probe::Ready(_) => {}
    }

    let eval_domain = evaluate_biometrics(reason)?;
    if eval_domain != stored_domain {
        clear_sealed(&profile);
        return Err("invalidated".into());
    }
    if !master_fresh_at(&rec, now_secs(), boot_time()) {
        return Err("master_required".into());
    }

    let secret = match generic_password(PasswordOptions::new_generic_password(SERVICE, &profile)) {
        Ok(bytes) => Zeroizing::new(bytes),
        Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => {
            clear_sealed(&profile);
            return Err("invalidated".into());
        }
        Err(_) => return Err("failed".into()),
    };

    let wrap = derive_wrap_key(&secret, &eval_domain, &profile);
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

/// Disables Touch ID unlock for the active profile.
pub fn disable() {
    let profile = Registry::load().active;
    disable_for(&profile);
}

/// Disables Touch ID unlock and deletes any stored Keychain secret for `profile`.
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
