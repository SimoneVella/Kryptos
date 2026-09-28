//! Background thread that locks the vault on inactivity, system sleep,
//! (on macOS) when the screen is locked and (on mobile) shortly after the
//! app leaves the screen. Android also locks on screen-off, see android.rs.

use std::time::{Duration, Instant, SystemTime};

use crate::state::AppState;

const TICK: Duration = Duration::from_secs(2);

/// How long the vault stays open once no app screen is visible (mobile). Long
/// enough for a two-step login (username page, then password page) to fill both.
pub const BACKGROUND_GRACE: Duration = Duration::from_secs(60);

/// `on_lock` receives the reason ("idle", "background", "sleep", "screen").
pub fn start(state: AppState, on_lock: impl Fn(&'static str) + Send + 'static) {
    std::thread::spawn(move || {
        let (mut wall, mut mono) = (SystemTime::now(), Instant::now());
        loop {
            std::thread::sleep(TICK);
            // The monotonic clock stops while the machine sleeps; wall time does not.
            let slept = wall.elapsed().unwrap_or_default().saturating_sub(mono.elapsed()) > Duration::from_secs(10);
            (wall, mono) = (SystemTime::now(), Instant::now());

            let reason = if state.lock_if_idle() {
                Some("idle")
            } else if state.lock_if_background(BACKGROUND_GRACE) {
                Some("background")
            } else if state.settings().lock_on_sleep && (slept || screen_locked()) && state.lock() {
                Some(if slept { "sleep" } else { "screen" })
            } else {
                None
            };
            if let Some(r) = reason {
                on_lock(r);
            }
        }
    });
}

#[cfg(target_os = "macos")]
fn screen_locked() -> bool {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::CFString;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
    }
    unsafe {
        let raw = CGSessionCopyCurrentDictionary();
        if raw.is_null() {
            return false;
        }
        let dict: CFDictionary<CFString, CFType> = CFDictionary::wrap_under_create_rule(raw);
        dict.find(CFString::from_static_string("CGSSessionScreenIsLocked"))
            .and_then(|v| v.downcast::<CFBoolean>())
            .map(bool::from)
            .unwrap_or(false)
    }
}

#[cfg(not(target_os = "macos"))]
fn screen_locked() -> bool {
    false
}
