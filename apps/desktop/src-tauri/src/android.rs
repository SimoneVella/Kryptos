//! Android glue: JNI entry points for the Kotlin side (KryptosBridge.kt) and
//! calls from Tauri commands into MainActivity.
//!
//! The AutofillService runs in the app's own process, so it reads the same
//! in-memory vault the UI unlocked. It can also start the process on its own
//! (Kryptos not open), which is why the vault state lives here and not only in
//! Tauri. Nothing is written to disk or sent anywhere: matching credentials are
//! handed to the Android autofill framework, which fills them into the
//! requesting app or browser.
//!
//! Biometric unlock: the vault key is sealed by an Android Keystore key that
//! only works right after a strong biometric check (see Biometric.kt). Rust
//! hands the key over once, when the user turns the feature on, and gets it
//! back through `unlockWithKey` after a successful fingerprint.

use std::path::PathBuf;
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::Duration;

use jni::objects::{JByteArray, JClass, JObject, JString};
use jni::sys::{jboolean, jbyteArray, jstring, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;
use kryptos_core::matching::entry_matches;
use kryptos_core::{storage, Entry, UnlockedVault};
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use zeroize::Zeroizing;

use crate::settings::Settings;
use crate::state::AppState;
use crate::watchdog;

static APP: OnceLock<AppHandle> = OnceLock::new();
static STATE: OnceLock<AppState> = OnceLock::new();
/// The Tauri command waiting for the current biometric prompt, if any.
static PENDING: Mutex<Option<mpsc::Sender<String>>> = Mutex::new(None);

/// How long a biometric prompt may stay open before the command gives up.
const PROMPT_TIMEOUT: Duration = Duration::from_secs(180);

/// The one vault state of the process, shared by the UI and the autofill side.
/// Whoever comes first (Tauri setup, or the autofill service on a cold start)
/// creates it and starts the auto-lock watchdog.
pub fn shared_state(data_dir: PathBuf) -> AppState {
    STATE
        .get_or_init(|| {
            kryptos_core::paths::set_data_dir(data_dir);
            let state = AppState::new(kryptos_core::paths::vault_path(), Settings::path());
            watchdog::start(state.clone(), |reason| emit("vault-locked", reason));
            state
        })
        .clone()
}

pub fn set_app(app: AppHandle) {
    let _ = APP.set(app);
}

fn state() -> Option<&'static AppState> {
    STATE.get()
}

/// Tells the UI (if it is running) that the lock state changed behind its back.
fn emit(event: &str, payload: &str) {
    if let Some(app) = APP.get() {
        let _ = app.emit(event, payload);
    }
}

// ───────── Tauri → MainActivity ─────────

/// Runs `f` with MainActivity on the Android UI thread and waits for its result.
/// Must not be called from the UI thread itself (use async commands).
fn with_activity<T: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce(&mut JNIEnv, &JObject) -> T + Send + 'static,
) -> Option<T> {
    let window = app.get_webview_window("main")?;
    let (tx, rx) = mpsc::channel();
    window
        .with_webview(move |w| {
            w.jni_handle().exec(move |env, activity, _| {
                let _ = tx.send(f(env, activity));
            })
        })
        .ok()?;
    rx.recv_timeout(Duration::from_secs(2)).ok()
}

fn call_void(app: &AppHandle, method: &'static str) {
    with_activity(app, move |env, activity| {
        let _ = env.call_method(activity, method, "()V", &[]);
    });
}

/// Whether Kryptos is the selected system autofill service.
pub fn autofill_enabled(app: &AppHandle) -> bool {
    with_activity(app, |env, activity| {
        env.call_method(activity, "isAutofillEnabled", "()Z", &[]).and_then(|v| v.z()).unwrap_or(false)
    })
    .unwrap_or(false)
}

/// Opens the system prompt that selects Kryptos as the autofill service.
pub fn open_autofill_settings(app: &AppHandle) {
    call_void(app, "openAutofillSettings");
}

/// "unavailable" | "off" | "on" | "master_required" (see Biometric.kt).
pub fn biometric_status(app: &AppHandle) -> String {
    with_activity(app, |env, activity| -> Option<String> {
        let obj = env.call_method(activity, "biometricStatus", "()Ljava/lang/String;", &[]).ok()?.l().ok()?;
        env.get_string(&JString::from(obj)).ok().map(String::from)
    })
    .flatten()
    .unwrap_or_else(|| "unavailable".into())
}

/// Shows the fingerprint prompt (`method` is "biometricEnable" or "biometricUnlock")
/// and blocks until the user is done. Ok on success, otherwise an error code.
pub fn biometric_prompt(app: &AppHandle, method: &'static str) -> Result<(), String> {
    let (tx, rx) = mpsc::channel();
    *PENDING.lock().unwrap() = Some(tx);
    call_void(app, method);
    let result = rx.recv_timeout(PROMPT_TIMEOUT).unwrap_or_else(|_| "timeout".into());
    PENDING.lock().unwrap().take();
    if result == "ok" { Ok(()) } else { Err(result) }
}

pub fn biometric_disable(app: &AppHandle) {
    call_void(app, "biometricDisable");
}

/// The master password was just verified: restarts the 7-day biometric window.
pub fn master_unlocked(app: &AppHandle) {
    call_void(app, "onMasterUnlock");
}

// ───────── Kotlin → Rust (KryptosBridge) ─────────

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

/// Called by the Kotlin side before anything else (it may run before Tauri).
#[no_mangle]
pub extern "system" fn Java_com_kryptos_vault_KryptosBridge_init(mut env: JNIEnv, _class: JClass, data_dir: JString) {
    if let Ok(dir) = env.get_string(&data_dir) {
        shared_state(PathBuf::from(String::from(dir)));
    }
}

#[no_mangle]
pub extern "system" fn Java_com_kryptos_vault_KryptosBridge_isUnlocked(_env: JNIEnv, _class: JClass) -> jboolean {
    if state().is_some_and(|s| s.is_unlocked()) { JNI_TRUE } else { JNI_FALSE }
}

/// Some app screen (Kryptos UI or the autofill unlock prompt) is visible or not.
#[no_mangle]
pub extern "system" fn Java_com_kryptos_vault_KryptosBridge_setForeground(_env: JNIEnv, _class: JClass, visible: jboolean) {
    if let Some(s) = state() {
        s.set_foreground(visible == JNI_TRUE);
    }
}

#[no_mangle]
pub extern "system" fn Java_com_kryptos_vault_KryptosBridge_screenOff(_env: JNIEnv, _class: JClass) {
    if let Some(s) = state() {
        if s.settings().lock_on_sleep && s.lock() {
            emit("vault-locked", "screen");
        }
    }
}

/// The raw vault key while unlocked (to seal it under biometrics), else null.
#[no_mangle]
pub extern "system" fn Java_com_kryptos_vault_KryptosBridge_vaultKey(env: JNIEnv, _class: JClass) -> jbyteArray {
    let Some(key) = state().and_then(|s| s.raw_key()) else { return std::ptr::null_mut() };
    env.byte_array_from_slice(&key).map(|a| a.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// Opens the vault with a key released by the biometric prompt.
#[no_mangle]
pub extern "system" fn Java_com_kryptos_vault_KryptosBridge_unlockWithKey(
    env: JNIEnv,
    _class: JClass,
    key: JByteArray,
) -> jboolean {
    let Some(s) = state() else { return JNI_FALSE };
    let Ok(key) = env.convert_byte_array(&key).map(Zeroizing::new) else { return JNI_FALSE };
    let vault = storage::read(&s.vault_path).ok().and_then(|bytes| UnlockedVault::unlock_with_key(&bytes, &key).ok());
    match vault {
        Some(v) => {
            s.set_unlocked(v);
            emit("vault-unlocked", "biometric");
            JNI_TRUE
        }
        None => JNI_FALSE,
    }
}

/// Result of a prompt started by `biometric_prompt`: "ok" or an error code.
#[no_mangle]
pub extern "system" fn Java_com_kryptos_vault_KryptosBridge_biometricDone(mut env: JNIEnv, _class: JClass, result: JString) {
    let result = env.get_string(&result).map(String::from).unwrap_or_else(|_| "failed".into());
    if let Some(tx) = PENDING.lock().unwrap().as_ref() {
        let _ = tx.send(result);
    }
}

/// Returns a JSON array of `{title, username, password}` for entries matching `target`,
/// or `null` when the vault is locked / the app has not started yet.
#[no_mangle]
pub extern "system" fn Java_com_kryptos_vault_KryptosBridge_fillData<'a>(
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
