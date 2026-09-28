//! JNI entry points for the Android AutofillService (KryptosAutofillService.kt).
//!
//! The service runs in the app's own process, so it reads the same in-memory
//! vault the UI unlocked. Nothing is written to disk or sent anywhere: the
//! matching credentials are handed to the Android autofill framework, which
//! fills them into the requesting app or browser.

use std::sync::OnceLock;

use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jstring, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;
use kryptos_core::matching::entry_matches;
use kryptos_core::Entry;
use serde_json::json;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

static APP: OnceLock<AppHandle> = OnceLock::new();

pub fn set_app(app: AppHandle) {
    let _ = APP.set(app);
}

fn state() -> Option<tauri::State<'static, AppState>> {
    APP.get().map(|a| a.state::<AppState>())
}

/// `target` is either a web URL (browsers report the page domain) or
/// `androidapp://<package>` for native apps, which only match entries the user
/// explicitly linked to that package (package names are not proof of identity).
fn matches(e: &Entry, target: &str) -> bool {
    if target.starts_with("androidapp://") {
        e.urls.iter().any(|u| u.trim().eq_ignore_ascii_case(target))
    } else {
        entry_matches(e, target)
    }
}

#[no_mangle]
pub extern "system" fn Java_com_kryptos_app_KryptosBridge_isUnlocked(_env: JNIEnv, _class: JClass) -> jboolean {
    if state().is_some_and(|s| s.is_unlocked()) { JNI_TRUE } else { JNI_FALSE }
}

/// Returns a JSON array of `{title, username, password}` for entries matching `target`,
/// or `null` when the vault is locked / the app has not started yet.
#[no_mangle]
pub extern "system" fn Java_com_kryptos_app_KryptosBridge_fillData<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    target: JString<'a>,
) -> jstring {
    let Ok(target) = env.get_string(&target).map(String::from) else { return std::ptr::null_mut() };
    let Some(state) = state() else { return std::ptr::null_mut() };
    let result = state.read(|v| {
        let list: Vec<_> = v
            .entries()
            .iter()
            .filter(|e| matches(e, &target))
            .map(|e| json!({ "title": e.title, "username": e.username, "password": e.password }))
            .collect();
        Ok(serde_json::Value::Array(list).to_string())
    });
    match result {
        Ok(s) => env.new_string(s).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}
