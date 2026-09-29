//! IPC surface exposed to the UI. Errors are short machine-readable codes
//! ("locked", "wrong_password", ...) or a human message for validation errors.

use std::time::Duration;

use kryptos_core::crypto::KdfParams;
use kryptos_core::health::{self, HealthReport};
use kryptos_core::{generator, storage, Entry, EntryInput, EntrySummary, Error, GeneratorOptions, UnlockedVault};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use uuid::Uuid;
use zeroize::Zeroizing;

use std::io::Write;

use tauri_plugin_dialog::{DialogExt, FilePath};
use tauri_plugin_fs::{FsExt, OpenOptions};

use crate::browser;
use crate::settings::Settings;
use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn code(e: Error) -> String {
    match e {
        Error::WrongPassword => "wrong_password".into(),
        Error::NotFound => "not_found".into(),
        Error::InvalidInput(m) => m.into(),
        other => other.to_string(),
    }
}

fn parse_id(id: &str) -> CmdResult<Uuid> {
    Uuid::parse_str(id).map_err(|_| "invalid_id".into())
}

#[derive(Serialize)]
pub struct Status {
    exists: bool,
    unlocked: bool,
    platform: &'static str,
}

/// Argon2 cost: phones get 64 MiB (iOS extensions are capped around 120 MiB).
const KDF: KdfParams = if cfg!(mobile) { KdfParams::MOBILE } else { KdfParams::DESKTOP };

#[tauri::command]
pub fn status(state: State<'_, AppState>) -> Status {
    Status { exists: state.vault_exists(), unlocked: state.is_unlocked(), platform: std::env::consts::OS }
}

/// Argon2id takes ~1 s by design, so these run off the main thread.
#[tauri::command]
pub async fn create_vault(app: AppHandle, state: State<'_, AppState>, password: String) -> CmdResult<()> {
    if state.vault_exists() {
        return Err("vault_exists".into());
    }
    let password = Zeroizing::new(password);
    let path = state.vault_path.clone();
    let vault = tauri::async_runtime::spawn_blocking(move || -> CmdResult<UnlockedVault> {
        let v = UnlockedVault::create(&password, KDF).map_err(code)?;
        storage::write_atomic(&path, &v.to_bytes()).map_err(code)?;
        Ok(v)
    })
    .await
    .map_err(|e| e.to_string())??;
    state.set_unlocked(vault);
    master_verified(app).await;
    Ok(())
}

#[tauri::command]
pub async fn unlock(app: AppHandle, state: State<'_, AppState>, password: String) -> CmdResult<()> {
    let password = Zeroizing::new(password);
    let path = state.vault_path.clone();
    let vault = tauri::async_runtime::spawn_blocking(move || -> CmdResult<UnlockedVault> {
        let bytes = storage::read(&path).map_err(code)?;
        UnlockedVault::unlock(&bytes, &password).map_err(code)
    })
    .await
    .map_err(|e| e.to_string())??;
    state.set_unlocked(vault);
    master_verified(app).await;
    Ok(())
}

/// The master password was just typed correctly: biometric unlock may be used
/// again for the next 7 days (Android; nothing to do elsewhere yet).
async fn master_verified(app: AppHandle) {
    #[cfg(target_os = "android")]
    let _ = tauri::async_runtime::spawn_blocking(move || crate::android::master_unlocked(&app)).await;
    #[cfg(not(target_os = "android"))]
    let _ = app;
}

#[tauri::command]
pub fn lock(state: State<'_, AppState>) {
    state.lock();
}

/// The UI calls this on user interaction to postpone auto-lock.
#[tauri::command]
pub fn touch(state: State<'_, AppState>) {
    state.touch();
}

#[tauri::command]
pub fn list_entries(state: State<'_, AppState>) -> CmdResult<Vec<EntrySummary>> {
    state.with(|v| Ok(v.list()))
}

#[tauri::command]
pub fn get_entry(state: State<'_, AppState>, id: String) -> CmdResult<Entry> {
    let id = parse_id(&id)?;
    state.with(|v| v.get(id).cloned().map_err(code))
}

#[tauri::command]
pub fn add_entry(state: State<'_, AppState>, input: EntryInput) -> CmdResult<String> {
    state.mutate(|v| v.add(input).map(|id| id.to_string()).map_err(code))
}

#[tauri::command]
pub fn update_entry(state: State<'_, AppState>, id: String, input: EntryInput) -> CmdResult<()> {
    let id = parse_id(&id)?;
    state.mutate(|v| v.update(id, input).map_err(code))
}

#[tauri::command]
pub fn delete_entry(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let id = parse_id(&id)?;
    state.mutate(|v| v.delete(id).map_err(code))
}

/// Copies a field to the clipboard from Rust (the password never needs to reach JS)
/// and clears it after 30 s if the user hasn't copied something else meanwhile.
#[tauri::command]
pub fn copy_field(app: AppHandle, state: State<'_, AppState>, id: String, field: String) -> CmdResult<()> {
    let id = parse_id(&id)?;
    let secret = state.with(|v| {
        let e = v.get(id).map_err(code)?;
        match field.as_str() {
            "password" => Ok(Zeroizing::new(e.password.clone())),
            "username" => Ok(Zeroizing::new(e.username.clone())),
            _ => Err("invalid_field".into()),
        }
    })?;
    app.clipboard().write_text(secret.as_str()).map_err(|e| e.to_string())?;
    if field == "password" {
        schedule_clear(app, secret, state.settings().clipboard_clear_secs);
    }
    Ok(())
}

/// Copies arbitrary text (e.g. a freshly generated password) with the same auto-clear.
#[tauri::command]
pub fn copy_text(app: AppHandle, state: State<'_, AppState>, text: String) -> CmdResult<()> {
    state.touch();
    let text = Zeroizing::new(text);
    app.clipboard().write_text(text.as_str()).map_err(|e| e.to_string())?;
    schedule_clear(app, text, state.settings().clipboard_clear_secs);
    Ok(())
}

fn schedule_clear(app: AppHandle, secret: Zeroizing<String>, secs: u64) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(secs));
        if app.clipboard().read_text().ok().as_deref() == Some(secret.as_str()) {
            let _ = app.clipboard().write_text(String::new());
        }
    });
}

#[tauri::command]
pub fn security_report(state: State<'_, AppState>) -> CmdResult<HealthReport> {
    state.with(|v| Ok(health::analyze(v.entries())))
}

#[derive(Serialize)]
pub struct Generated {
    password: String,
    entropy_bits: f64,
}

#[tauri::command]
pub fn generate_password(options: Option<GeneratorOptions>) -> CmdResult<Generated> {
    let o = options.unwrap_or_default();
    Ok(Generated { password: generator::generate(&o).map_err(code)?, entropy_bits: generator::entropy_bits(&o) })
}

#[tauri::command]
pub async fn change_master_password(state: State<'_, AppState>, current: String, new: String) -> CmdResult<()> {
    let (current, new) = (Zeroizing::new(current), Zeroizing::new(new));
    let path = state.vault_path.clone();
    // Verify the current password against the file, not just "is unlocked".
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<()> {
        let bytes = storage::read(&path).map_err(code)?;
        UnlockedVault::unlock(&bytes, &current).map(drop).map_err(code)
    })
    .await
    .map_err(|e| e.to_string())??;
    // Re-wrapping runs Argon2 while holding the state lock; acceptable for a rare, explicit action.
    state.mutate(|v| v.change_password(&new, KDF).map_err(code))
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn update_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<()> {
    state.touch();
    state.set_settings(settings)
}

#[derive(Serialize)]
pub struct ImportResult {
    added: usize,
    skipped: usize,
    file: String,
}

/// Opens a native file picker from Rust (the UI never gets filesystem access),
/// parses the CSV and merges it into the vault. Returns None if cancelled.
#[tauri::command]
pub async fn import_csv(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Option<ImportResult>> {
    let dialog = app.dialog().file().set_title("Importa password (CSV)");
    // Android maps extensions to MIME types, and exported CSVs are often not "text/csv":
    // filtering there would grey out the very file the user is looking for.
    #[cfg(not(target_os = "android"))]
    let dialog = dialog.add_filter("CSV", &["csv"]);
    let Some(picked) = dialog.blocking_pick_file() else { return Ok(None) };
    let file = file_name(&picked);
    // The fs plugin also reads Android `content://` URIs. It is used from Rust only:
    // the UI gets no file-system permission.
    let data = Zeroizing::new(app.fs().read(picked).map_err(|e| e.to_string())?);
    let inputs = kryptos_core::import::parse_csv(&data).map_err(code)?;
    let (added, skipped) = state.mutate(|v| Ok(v.import(inputs)))?;
    Ok(Some(ImportResult { added, skipped, file }))
}

/// Display name of a picked file; Android only gives an opaque, percent-encoded URI.
fn file_name(picked: &FilePath) -> String {
    match picked {
        FilePath::Path(p) => p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        FilePath::Url(u) => {
            let last = u.path_segments().and_then(|mut s| s.next_back()).unwrap_or_default();
            let decoded = percent_decode(last);
            decoded.rsplit(['/', ':']).next().unwrap_or_default().to_string()
        }
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match (b[i], b.get(i + 1).copied().and_then(hex), b.get(i + 2).copied().and_then(hex)) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (c, _, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Copies the encrypted vault file to a user-chosen location.
#[tauri::command]
pub async fn export_backup(app: AppHandle, state: State<'_, AppState>) -> CmdResult<bool> {
    state.touch();
    let name = format!("Kryptos-backup-{}.kryptos", today());
    let picked = app.dialog().file().set_title("Salva backup cifrato").set_file_name(name).blocking_save_file();
    let Some(picked) = picked else { return Ok(false) };
    // The vault file is already encrypted: the backup is a byte-for-byte copy.
    let bytes = std::fs::read(&state.vault_path).map_err(|e| e.to_string())?;
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    let mut out = app.fs().open(picked, opts).map_err(|e| e.to_string())?;
    out.write_all(&bytes).and_then(|_| out.sync_all()).map_err(|e| e.to_string())?;
    Ok(true)
}

fn today() -> String {
    // YYYY-MM-DD without pulling in a date crate (civil-from-days, Howard Hinnant).
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86400).unwrap_or(0) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[tauri::command]
pub fn connect_browser(state: State<'_, AppState>) -> CmdResult<browser::Registered> {
    let r = browser::register()?;
    let mut s = state.settings();
    s.browser_integration = true;
    state.set_settings(s)?;
    Ok(r)
}

#[tauri::command]
pub fn disconnect_browser(state: State<'_, AppState>) -> CmdResult<()> {
    browser::unregister();
    let mut s = state.settings();
    s.browser_integration = false;
    state.set_settings(s)
}

// Async so they run off the UI thread, which the Android side needs to be free.

/// Whether Kryptos is the system autofill service (Android only; false elsewhere).
#[tauri::command]
pub async fn autofill_status(app: AppHandle) -> bool {
    #[cfg(target_os = "android")]
    return crate::android::autofill_enabled(&app);
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        false
    }
}

#[tauri::command]
pub async fn open_autofill_settings(app: AppHandle) {
    #[cfg(target_os = "android")]
    crate::android::open_autofill_settings(&app);
    #[cfg(not(target_os = "android"))]
    let _ = app;
}

// Biometric unlock (Android fingerprint for now). Status is one of
// "unavailable" | "off" | "on" | "master_required". The prompt commands block
// until the user answers, so they run on a blocking thread.

#[tauri::command]
pub async fn biometric_status(app: AppHandle) -> String {
    #[cfg(target_os = "android")]
    return tauri::async_runtime::spawn_blocking(move || crate::android::biometric_status(&app))
        .await
        .unwrap_or_else(|_| "unavailable".into());
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        "unavailable".into()
    }
}

/// Seals the vault key under the fingerprint. The vault must be unlocked.
#[tauri::command]
pub async fn biometric_enable(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    if !state.is_unlocked() {
        return Err("locked".into());
    }
    biometric_prompt(app, "biometricEnable").await
}

#[tauri::command]
pub async fn biometric_unlock(app: AppHandle) -> CmdResult<()> {
    biometric_prompt(app, "biometricUnlock").await
}

#[tauri::command]
pub async fn biometric_disable(app: AppHandle) {
    #[cfg(target_os = "android")]
    let _ = tauri::async_runtime::spawn_blocking(move || crate::android::biometric_disable(&app)).await;
    #[cfg(not(target_os = "android"))]
    let _ = app;
}

async fn biometric_prompt(app: AppHandle, method: &'static str) -> CmdResult<()> {
    #[cfg(target_os = "android")]
    return tauri::async_runtime::spawn_blocking(move || crate::android::biometric_prompt(&app, method))
        .await
        .map_err(|e| e.to_string())?;
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, method);
        Err("unavailable".into())
    }
}
