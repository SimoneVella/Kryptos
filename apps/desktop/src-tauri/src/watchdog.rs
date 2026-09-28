//! Background thread that locks the vault on inactivity, system sleep and
//! (on macOS) when the screen is locked.

use std::time::{Duration, Instant, SystemTime};

use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

const TICK: Duration = Duration::from_secs(2);

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let (mut wall, mut mono) = (SystemTime::now(), Instant::now());
        loop {
            std::thread::sleep(TICK);
            // The monotonic clock stops while the machine sleeps; wall time does not.
            let slept = wall.elapsed().unwrap_or_default().saturating_sub(mono.elapsed()) > Duration::from_secs(10);
            (wall, mono) = (SystemTime::now(), Instant::now());

            let state = app.state::<AppState>();
            let reason = if state.lock_if_idle() {
                Some("idle")
            } else if state.settings().lock_on_sleep && (slept || screen_locked()) && state.lock() {
                Some(if slept { "sleep" } else { "screen" })
            } else {
                None
            };
            if let Some(r) = reason {
                let _ = app.emit("vault-locked", r);
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
