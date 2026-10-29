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
//!
//! Saving: when the user accepts Android's "Save to Kryptos?" sheet, the login
//! is saved at once if the vault is open. If it is locked, the login waits here,
//! in memory only and for a few minutes, until the fingerprint or the master
//! password opens the vault (`apply_pending_save`).

use std::path::PathBuf;
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use jni::objects::{JByteArray, JClass, JObject, JString, JValue};
use jni::sys::{jboolean, jbyteArray, jstring, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;
use kryptos_core::matching::target_matches;
use kryptos_core::{storage, SaveOutcome, UnlockedVault};
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

/// A login the user asked to save while the vault was locked.
struct PendingSave {
    target: String,
    title: String,
    username: Zeroizing<String>,
    password: Zeroizing<String>,
    at: Instant,
}
static PENDING_SAVE: Mutex<Option<PendingSave>> = Mutex::new(None);
/// A pending save is dropped if the vault is not opened within this time.
const SAVE_TTL: Duration = Duration::from_secs(300);

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

/// Copies `src` into the app cache as `name` and opens the system share sheet for it.
pub fn share_file(app: &AppHandle, src: String, name: String) {
    with_activity(app, move |env, activity| {
        let (Ok(src), Ok(name)) = (env.new_string(src), env.new_string(name)) else { return };
        let _ = env.call_method(
            activity,
            "shareFile",
            "(Ljava/lang/String;Ljava/lang/String;)V",
            &[JValue::Object(&src), JValue::Object(&name)],
        );
    });
}

/// The master password was just verified: restarts the 7-day biometric window.
pub fn master_unlocked(app: &AppHandle) {
    call_void(app, "onMasterUnlock");
}

// ───────── Saving logins ─────────

/// Saves into the open vault and tells the UI. Returns "added", "updated",
/// "unchanged" or an error code ("locked", ...).
fn save(state: &AppState, p: &PendingSave) -> Result<&'static str, String> {
    let outcome = state.mutate(|v| {
        v.save_login(&p.target, &p.title, &p.username, &p.password).map_err(crate::commands::code)
    })?;
    let word = match outcome {
        SaveOutcome::Added(_) => "added",
        SaveOutcome::Updated(_) => "updated",
        SaveOutcome::Unchanged(_) => "unchanged",
    };
    emit_json("login-saved", json!({ "title": p.title, "outcome": word }));
    Ok(word)
}

/// The vault was just opened: saves the login that was waiting for it, if any
/// and still fresh. Returns its outcome, or None.
pub fn apply_pending_save() -> Option<&'static str> {
    let state = state()?;
    let p = PENDING_SAVE.lock().unwrap().take().filter(|p| p.at.elapsed() < SAVE_TTL)?;
    save(state, &p).ok()
}

fn emit_json(event: &str, payload: serde_json::Value) {
    if let Some(app) = APP.get() {
        let _ = app.emit(event, payload);
    }
}

// ───────── Kotlin → Rust (KryptosBridge) ─────────

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

/// Returns a JSON array of `{id, title, username, password}` for entries matching `target`,
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
            .filter(|e| target_matches(e, &target))
            .map(|e| json!({ "id": e.id, "title": e.title, "username": e.username, "password": e.password }))
            .collect();
        Ok(serde_json::Value::Array(list).to_string())
    });
    match result {
        Ok(s) => env.new_string(s).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}

/// The user accepted Android's save sheet. `target` follows the same rules as
/// `fillData`. Returns "added" / "updated" / "unchanged", "locked" when the login
/// now waits for the vault to open (see `applyPendingSave`), or "failed".
#[no_mangle]
pub extern "system" fn Java_com_kryptos_vault_KryptosBridge_saveLogin<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    target: JString<'a>,
    title: JString<'a>,
    username: JString<'a>,
    password: JString<'a>,
) -> jstring {
    let mut text = |s: &JString| env.get_string(s).map(String::from).ok();
    let (Some(target), Some(title), Some(username), Some(password)) = (text(&target), text(&title), text(&username), text(&password))
    else {
        return std::ptr::null_mut();
    };
    let p = PendingSave {
        target,
        title,
        username: Zeroizing::new(username),
        password: Zeroizing::new(password),
        at: Instant::now(),
    };
    let result = match state() {
        Some(s) if s.is_unlocked() => save(s, &p).unwrap_or("failed"),
        Some(_) => {
            *PENDING_SAVE.lock().unwrap() = Some(p);
            "locked"
        }
        None => "failed",
    };
    env.new_string(result).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// After the fingerprint opened the vault: saves the waiting login. Returns its
/// outcome, or "none" when there was nothing (fresh) to save.
#[no_mangle]
pub extern "system" fn Java_com_kryptos_vault_KryptosBridge_applyPendingSave(env: JNIEnv, _class: JClass) -> jstring {
    let result = apply_pending_save().unwrap_or("none");
    env.new_string(result).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut())
}
