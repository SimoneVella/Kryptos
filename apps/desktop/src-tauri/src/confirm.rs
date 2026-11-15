//! Confirmation of every browser fill.
//!
//! The bridge thread that received a `credentials` request blocks in [`ask`].
//! Browser fills with Touch ID / Mac password or Windows Hello show
//! only the system prompt, which names the site, and leave the user in the
//! browser. The app window comes forward for the master password, if the system
//! prompt can't be used, and for AI and command line requests, which say who's
//! asking. The browser sees neither: the extension popup only gets the result.
//! Only one request can wait at a time; a second one is refused as "busy".

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// The app gives up a little before the native host stops waiting for it
/// (see `relay` in crates/native-host), so the browser gets a clean "timeout".
pub const TIMEOUT: Duration = Duration::from_secs(170);

#[derive(Clone, Serialize)]
pub struct FillRequest {
    pub id: u64,
    pub title: String,
    pub username: String,
    pub host: String,
    /// "system" (Touch ID / Mac password / Windows Hello) or "master".
    pub mode: &'static str,
    /// Who asks: "browser" (the extension) or "ai" (an assistant through the MCP server).
    pub source: &'static str,
    /// For "ai": the assistant's name, as it reports it.
    pub client: String,
}

struct Pending {
    id: u64,
    tx: mpsc::Sender<bool>,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
/// The system prompt's text in the UI's language, with `{host}` in it.
static REASON: Mutex<Option<String>> = Mutex::new(None);

pub fn set_reason(template: String) {
    *REASON.lock().unwrap() = Some(template);
}

fn reason(host: &str) -> String {
    REASON.lock().unwrap().as_deref().unwrap_or("fill your password on {host}").replace("{host}", host)
}

fn release(id: u64) {
    let mut pending = PENDING.lock().unwrap();
    if pending.as_ref().is_some_and(|p| p.id == id) {
        *pending = None;
    }
}

/// Whether Touch ID / Mac password or Windows Hello can confirm fills on this computer.
pub fn system_available() -> bool {
    #[cfg(target_os = "macos")]
    return macos::can_evaluate();
    #[cfg(target_os = "windows")]
    return winhello::can_evaluate();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    false
}

/// Asks the user and waits for the answer.
/// Ok when confirmed, else "denied", "timeout" or "busy".
pub fn ask(app: &AppHandle, title: String, username: String, host: String, mode: &str, source: &'static str, client: String) -> Result<(), String> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    {
        let mut pending = PENDING.lock().unwrap();
        if pending.is_some() {
            return Err("busy".into());
        }
        *pending = Some(Pending { id, tx });
    }
    let mode = if mode == "system" && system_available() { "system" } else { "master" };
    if source == "browser" && mode == "system" {
        match system_auth(&reason(&host)) {
            Ok(()) => {
                release(id);
                return Ok(());
            }
            Err(e) if e == "cancelled" || e == "timeout" => {
                release(id);
                return Err(if e == "cancelled" { "denied".into() } else { e });
            }
            // The prompt couldn't be shown: the app window asks instead.
            Err(_) => {}
        }
    }
    let window = app.get_webview_window("main");
    let was_focused = window.as_ref().is_some_and(|w| w.is_focused().unwrap_or(false));
    let was_minimized = window.as_ref().is_some_and(|w| w.is_minimized().unwrap_or(false));
    if let Some(w) = &window {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.emit("fill-confirm", FillRequest { id, title, username, host, mode, source, client });

    let answer = rx.recv_timeout(TIMEOUT);
    release(id);
    let _ = app.emit("fill-confirm-done", id);

    // Kryptos came forward only to ask: hand the screen back to the browser.
    if !was_focused {
        if let Some(w) = &window {
            if was_minimized {
                let _ = w.minimize();
            } else {
                #[cfg(target_os = "macos")]
                let _ = app.hide();
            }
        }
    }
    match answer {
        Ok(true) => Ok(()),
        Ok(false) => Err("denied".into()),
        Err(_) => Err("timeout".into()),
    }
}

/// Answers the request `id`, if it is still the one waiting.
pub fn answer(id: u64, ok: bool) {
    if let Some(p) = PENDING.lock().unwrap().as_ref().filter(|p| p.id == id) {
        let _ = p.tx.send(ok);
    }
}

/// Cancels any waiting confirmation (e.g. when the vault locks or switches profile).
pub fn cancel() {
    if let Some(p) = PENDING.lock().unwrap().take() {
        let _ = p.tx.send(false);
    }
}

pub fn is_pending(id: u64) -> bool {
    PENDING.lock().unwrap().as_ref().is_some_and(|p| p.id == id)
}

/// Shows the system prompt (Touch ID / Mac password, or Windows Hello) with
/// `reason` and blocks until it is answered, for `TIMEOUT` at most. Error
/// codes: "cancelled" (by the user), "interrupted" (by the system), "timeout",
/// "unavailable", "failed".
pub fn system_auth(reason: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return macos::evaluate(reason, TIMEOUT);
    #[cfg(target_os = "windows")]
    return winhello::evaluate(reason, TIMEOUT);
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = reason;
        Err("unavailable".into())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSError, NSString};
    use objc2_local_authentication::{LAContext, LAPolicy};

    // LAError codes (LocalAuthentication/LAError.h).
    const USER_CANCEL: isize = -2;
    const SYSTEM_CANCEL: isize = -4;
    const APP_CANCEL: isize = -9;

    /// Owner authentication: Touch ID when available, otherwise the account password.
    const POLICY: LAPolicy = LAPolicy::DeviceOwnerAuthentication;

    pub fn can_evaluate() -> bool {
        let ctx = unsafe { LAContext::new() };
        unsafe { ctx.canEvaluatePolicy_error(POLICY) }.is_ok()
    }

    pub fn evaluate(reason: &str, timeout: Duration) -> Result<(), String> {
        let ctx = unsafe { LAContext::new() };
        if unsafe { ctx.canEvaluatePolicy_error(POLICY) }.is_err() {
            return Err("unavailable".into());
        }
        let (tx, rx) = mpsc::channel::<Result<(), isize>>();
        let reply = RcBlock::new(move |ok: Bool, error: *mut NSError| {
            let result = if ok.as_bool() {
                Ok(())
            } else {
                // SAFETY: LocalAuthentication passes a valid NSError (or null) for the duration of the call.
                Err(unsafe { error.as_ref() }.map_or(0, |e| e.code()))
            };
            let _ = tx.send(result);
        });
        unsafe { ctx.evaluatePolicy_localizedReason_reply(POLICY, &NSString::from_str(reason), &reply) };
        match rx.recv_timeout(timeout) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(USER_CANCEL | APP_CANCEL)) => Err("cancelled".into()),
            Ok(Err(SYSTEM_CANCEL)) => Err("interrupted".into()),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                unsafe { ctx.invalidate() };
                Err("timeout".into())
            }
            _ => Err("failed".into()),
        }
    }
}

#[cfg(target_os = "windows")]
pub(crate) mod winhello {
    use std::sync::mpsc;
    use std::time::Duration;

    use windows::core::{factory, HSTRING};
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
    };
    use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
    use windows::Win32::System::WinRT::{
        IUserConsentVerifierInterop, RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Availability {
        Available,
        Busy,
        Unavailable,
    }

    struct RoGuard(bool);

    impl RoGuard {
        fn init() -> Result<Self, ()> {
            match unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
                Ok(()) => Ok(Self(true)),
                Err(e) if e.code() == RPC_E_CHANGED_MODE => Ok(Self(false)),
                Err(_) => Err(()),
            }
        }
    }

    impl Drop for RoGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { RoUninitialize() };
            }
        }
    }

    pub fn check_availability() -> Availability {
        let Ok(_ro) = RoGuard::init() else {
            return Availability::Unavailable;
        };
        let Ok(op) = UserConsentVerifier::CheckAvailabilityAsync() else {
            return Availability::Unavailable;
        };
        match op.join() {
            Ok(v) if v == UserConsentVerifierAvailability::Available => Availability::Available,
            Ok(v) if v == UserConsentVerifierAvailability::DeviceBusy => Availability::Busy,
            _ => Availability::Unavailable,
        }
    }

    pub fn can_evaluate() -> bool {
        check_availability() == Availability::Available
    }

    pub fn evaluate(reason: &str, timeout: Duration) -> Result<(), String> {
        let msg = if reason.trim().is_empty() { "Windows Hello" } else { reason }.to_string();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(evaluate_sync(&msg));
        });
        match rx.recv_timeout(timeout) {
            Ok(r) => r,
            Err(mpsc::RecvTimeoutError::Timeout) => Err("timeout".into()),
            Err(_) => Err("failed".into()),
        }
    }

    fn evaluate_sync(msg: &str) -> Result<(), String> {
        let Ok(_ro) = RoGuard::init() else {
            return Err("unavailable".into());
        };
        match UserConsentVerifier::CheckAvailabilityAsync().and_then(|op| op.join()) {
            Ok(v) if v == UserConsentVerifierAvailability::Available => {}
            Ok(v) if v == UserConsentVerifierAvailability::DeviceBusy => return Err("interrupted".into()),
            _ => return Err("unavailable".into()),
        }
        let hmsg = HSTRING::from(msg);
        let hwnd = unsafe { GetForegroundWindow() };
        let attempt = |use_window: bool| -> windows::core::Result<UserConsentVerificationResult> {
            let op = if use_window {
                let interop = factory::<UserConsentVerifier, IUserConsentVerifierInterop>()?;
                unsafe { interop.RequestVerificationForWindowAsync(hwnd, &hmsg)? }
            } else {
                UserConsentVerifier::RequestVerificationAsync(&hmsg)?
            };
            op.join()
        };
        let res = (if !hwnd.0.is_null() {
            attempt(true).or_else(|_| attempt(false))
        } else {
            attempt(false)
        })
        .map_err(|_| "failed".to_string())?;
        match res {
            r if r == UserConsentVerificationResult::Verified => Ok(()),
            r if r == UserConsentVerificationResult::Canceled => Err("cancelled".into()),
            r if r == UserConsentVerificationResult::RetriesExhausted => Err("lockout".into()),
            r if r == UserConsentVerificationResult::DeviceBusy => Err("interrupted".into()),
            r if r == UserConsentVerificationResult::DeviceNotPresent
                || r == UserConsentVerificationResult::NotConfiguredForUser
                || r == UserConsentVerificationResult::DisabledByPolicy =>
            {
                Err("unavailable".into())
            }
            _ => Err("failed".into()),
        }
    }
}
