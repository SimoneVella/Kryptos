//! IPC surface exposed to the UI. Errors are short machine-readable codes
//! ("locked", "wrong_password", ...) or a human message for validation errors.

use std::time::Duration;

use kryptos_core::crypto::KdfParams;
use kryptos_core::health::{self, HealthReport};
use kryptos_core::totp::Totp;
use kryptos_core::{generator, storage, Entry, EntryInput, EntrySummary, Error, GeneratorOptions, UnlockedVault};
use serde::{Deserialize, Serialize};
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

pub(crate) type CmdResult<T> = Result<T, String>;

pub(crate) fn code(e: Error) -> String {
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
    /// The active profile's id: the UI starts afresh when it changes.
    profile: String,
}

/// Argon2 cost: phones get 64 MiB (iOS extensions are capped around 120 MiB).
const KDF: KdfParams = if cfg!(mobile) { KdfParams::MOBILE } else { KdfParams::DESKTOP };

#[tauri::command]
pub fn status(state: State<'_, AppState>) -> Status {
    Status { exists: state.vault_exists(), unlocked: state.is_unlocked(), platform: std::env::consts::OS, profile: crate::profiles::Registry::load().active }
}

/// Argon2id takes ~1 s by design, so these run off the main thread.
#[tauri::command]
pub async fn create_vault(app: AppHandle, state: State<'_, AppState>, password: String) -> CmdResult<()> {
    if state.vault_exists() {
        return Err("vault_exists".into());
    }
    let password = Zeroizing::new(password);
    let path = state.vault_path();
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
    let path = state.vault_path();
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

/// Marks one of an entry's 2FA backup codes as spent (or not).
#[tauri::command]
pub fn mark_backup_code(state: State<'_, AppState>, id: String, index: usize, used: bool) -> CmdResult<()> {
    let id = parse_id(&id)?;
    state.mutate(|v| v.set_backup_code_used(id, index, used).map_err(code))
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
            "totp" => Ok(Zeroizing::new(Totp::parse(&e.totp).map_err(code)?.now().0)),
            _ => Err("invalid_field".into()),
        }
    })?;
    app.clipboard().write_text(secret.as_str()).map_err(|e| e.to_string())?;
    // "Recent" ordering; failing to note it must not fail the copy.
    let _ = state.mutate(|v| {
        v.record_use(id);
        Ok(())
    });
    if field == "password" || field == "totp" {
        schedule_clear(app, secret, state.settings().clipboard_clear_secs);
    }
    Ok(())
}

#[derive(Serialize)]
pub struct TotpCode {
    code: String,
    /// Seconds before the code changes.
    remaining: u64,
    period: u64,
}

/// The entry's current 2FA code. A read, not activity: the code view polls it
/// every second and must not keep the vault from auto-locking.
#[tauri::command]
pub fn totp_code(state: State<'_, AppState>, id: String) -> CmdResult<TotpCode> {
    let id = parse_id(&id)?;
    state.read(|v| {
        let t = Totp::parse(&v.get(id).map_err(code)?.totp).map_err(code)?;
        let (code, remaining) = t.now();
        Ok(TotpCode { code, remaining, period: t.period })
    })
}

/// Copies one of an entry's earlier passwords (`index` in its history).
#[tauri::command]
pub fn copy_history(app: AppHandle, state: State<'_, AppState>, id: String, index: usize) -> CmdResult<()> {
    let id = parse_id(&id)?;
    let secret = state.with(|v| {
        let e = v.get(id).map_err(code)?;
        e.history.get(index).map(|h| Zeroizing::new(h.password.clone())).ok_or_else(|| "not_found".to_string())
    })?;
    app.clipboard().write_text(secret.as_str()).map_err(|e| e.to_string())?;
    schedule_clear(app, secret, state.settings().clipboard_clear_secs);
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
pub async fn change_master_password(app: AppHandle, state: State<'_, AppState>, current: String, new: String) -> CmdResult<()> {
    let (current, new) = (Zeroizing::new(current), Zeroizing::new(new));
    let path = state.vault_path();
    // Verify the current password against the file, not just "is unlocked".
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<()> {
        let bytes = storage::read(&path).map_err(code)?;
        UnlockedVault::unlock(&bytes, &current).map(drop).map_err(code)
    })
    .await
    .map_err(|e| e.to_string())??;
    // A new password makes a new vault key, and the copy in the cloud folder will
    // stop opening here: bring in what it has first, then replace it.
    let folder = state.settings().cloud_folder;
    if !folder.is_empty() {
        let _ = crate::cloud::sync(&state, &folder);
    }
    // Re-wrapping runs Argon2 while holding the state lock; acceptable for a rare, explicit action.
    state.mutate(|v| v.change_password(&new, KDF).map_err(code))?;
    if !folder.is_empty() {
        let _ = crate::cloud::force_push(&state, &folder);
    }
    // The fingerprint sealed the old key.
    #[cfg(target_os = "android")]
    let _ = tauri::async_runtime::spawn_blocking(move || crate::android::biometric_disable(&app)).await;
    #[cfg(not(target_os = "android"))]
    let _ = app;
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn update_settings(state: State<'_, AppState>, mut settings: Settings) -> CmdResult<()> {
    state.touch();
    // Only cloud_enable / cloud_disable change the sync folder, and only the ai_*
    // commands change AI access: a settings screen loaded earlier must not undo them.
    let current = state.settings();
    settings.cloud_folder = current.cloud_folder;
    settings.ai_enabled = current.ai_enabled;
    settings.ai_code_hash = current.ai_code_hash;
    settings.ai_code_expires = current.ai_code_expires;
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

/// The user's answer to "add this 2FA code to…" (otp_add.rs): an entry id,
/// "new", or None to cancel.
#[tauri::command]
pub fn otp_add_answer(id: u64, choice: Option<String>) {
    #[cfg(desktop)]
    crate::otp_add::answer(id, choice);
    #[cfg(mobile)]
    let _ = (id, choice);
}

// ───────── Profiles ─────────

#[tauri::command]
pub fn profile_list() -> crate::profiles::Registry {
    crate::profiles::Registry::load()
}

fn activate(state: &AppState, mut reg: crate::profiles::Registry, id: String) -> CmdResult<()> {
    let (vault, settings) = crate::profiles::files(&id)?;
    reg.active = id;
    reg.save()?;
    state.switch(vault, settings);
    crate::cloud::reset();
    Ok(())
}

/// Locks the current vault and moves to another profile.
#[tauri::command]
pub fn profile_switch(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let reg = crate::profiles::Registry::load();
    if !reg.profiles.iter().any(|p| p.id == id) {
        return Err("not_found".into());
    }
    if reg.active == id {
        return Ok(());
    }
    activate(&state, reg, id)
}

/// A new, empty vault with its own master password. It opens right away. Device
/// preferences carry over; the cloud folder and AI access start off.
#[tauri::command]
pub async fn profile_create(app: AppHandle, state: State<'_, AppState>, name: String, password: String) -> CmdResult<()> {
    let name = crate::profiles::clean_name(&name).ok_or("invalid_name")?;
    let mut reg = crate::profiles::Registry::load();
    if reg.profiles.iter().any(|p| p.name.to_lowercase() == name.to_lowercase()) {
        return Err("profile_exists".into());
    }
    let id = Uuid::new_v4().to_string();
    let (path, settings_path) = crate::profiles::files(&id)?;
    let password = Zeroizing::new(password);
    let vault = tauri::async_runtime::spawn_blocking(move || -> CmdResult<UnlockedVault> {
        let v = UnlockedVault::create(&password, KDF).map_err(code)?;
        if let Some(dir) = path.parent() {
            storage::ensure_private_dir(dir).map_err(code)?;
        }
        storage::write_atomic(&path, &v.to_bytes()).map_err(code)?;
        Ok(v)
    })
    .await
    .map_err(|e| e.to_string())??;
    let inherited = Settings { cloud_folder: String::new(), ai_enabled: false, ai_code_hash: String::new(), ai_code_expires: 0, ..state.settings() };
    inherited.save(&settings_path)?;
    reg.profiles.push(crate::profiles::Profile { id: id.clone(), name });
    activate(&state, reg, id)?;
    state.set_unlocked(vault);
    master_verified(app).await;
    Ok(())
}

#[tauri::command]
pub fn profile_rename(id: String, name: String) -> CmdResult<()> {
    let name = crate::profiles::clean_name(&name).ok_or("invalid_name")?;
    let mut reg = crate::profiles::Registry::load();
    reg.profiles.iter_mut().find(|p| p.id == id).ok_or("not_found")?.name = name;
    reg.save()
}

/// Deletes a profile's vault from this device for good. It takes that profile's
/// master password, and the first profile or the open one can't be deleted.
#[tauri::command]
pub async fn profile_delete(state: State<'_, AppState>, id: String, password: String) -> CmdResult<()> {
    let mut reg = crate::profiles::Registry::load();
    if id == crate::profiles::DEFAULT || id == reg.active {
        return Err("profile_in_use".into());
    }
    if !reg.profiles.iter().any(|p| p.id == id) {
        return Err("not_found".into());
    }
    state.touch();
    let (path, _) = crate::profiles::files(&id)?;
    let password = Zeroizing::new(password);
    let dir = path.parent().ok_or("invalid_id")?.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<()> {
        if path.exists() {
            UnlockedVault::unlock(&storage::read(&path).map_err(code)?, &password).map_err(code)?;
        }
        std::fs::remove_dir_all(&dir).or_else(|e| if e.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(e) }).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    reg.profiles.retain(|p| p.id != id);
    reg.save()
}

/// Whether another profile already syncs through this folder: two vaults can't
/// share one file.
fn folder_in_use(folder: &str, active: &str) -> bool {
    crate::profiles::Registry::load().profiles.iter().filter(|p| p.id != active).any(|p| {
        crate::profiles::files(&p.id).is_ok_and(|(_, settings)| Settings::load(&settings).cloud_folder == folder)
    })
}

// ───────── Teams ─────────

#[derive(Serialize)]
pub struct TeamMember {
    public: String,
    name: String,
    safety: String,
    added_at: u64,
    me: bool,
    admin: bool,
}

/// This person's public key, made on first use (only then is the vault saved).
fn identity(state: &AppState) -> CmdResult<String> {
    match state.read(|v| Ok(v.identity_public()))? {
        Some(me) => Ok(me),
        None => state.mutate(|v| Ok(v.ensure_identity())),
    }
}

/// The safety code of a pasted member code, to compare with that person.
#[tauri::command]
pub fn team_code_safety(code_text: String) -> CmdResult<String> {
    kryptos_core::team::parse_code(&code_text).map(|p| kryptos_core::team::safety_code(&p)).map_err(code)
}

/// This person's code, and their team if they're in one.
#[tauri::command]
pub fn team_status(state: State<'_, AppState>) -> CmdResult<serde_json::Value> {
    use kryptos_core::team;
    let me = identity(&state)?;
    state.with(|v| {
        let t = v.team().map(|t| {
            let members: Vec<TeamMember> = v
                .members()
                .iter()
                .map(|m| TeamMember { public: m.public.clone(), name: m.name.clone(), safety: team::safety_code(&m.public), added_at: m.added_at, me: m.public == me, admin: m.public == t.admin })
                .collect();
            serde_json::json!({ "name": t.name, "is_admin": t.admin == me, "admin_safety": team::safety_code(&t.admin), "members": members })
        });
        Ok(serde_json::json!({ "code": team::member_code(&me), "safety": team::safety_code(&me), "team": t }))
    })
}

/// A folder for a team. Picking the `Kryptos` folder itself means its parent.
#[tauri::command]
pub async fn team_pick_folder(app: AppHandle) -> Option<crate::cloud::Provider> {
    #[cfg(target_os = "android")]
    return cloud_pick_folder(app).await;
    #[cfg(not(target_os = "android"))]
    {
        let dir = app.dialog().file().set_title("Kryptos").blocking_pick_folder()?.into_path().ok()?;
        let dir = if dir.file_name().is_some_and(|n| n == "Kryptos") { dir.parent()?.to_path_buf() } else { dir };
        let path = dir.to_string_lossy().into_owned();
        let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Some(crate::cloud::Provider { has_vault: crate::cloud::has_vault(&path), name, path })
    }
}

/// Starts a team in this vault, synced through an unused folder.
#[tauri::command]
pub fn team_create(state: State<'_, AppState>, name: String, me: String, folder: String) -> CmdResult<()> {
    if !state.settings().cloud_folder.is_empty() {
        return Err("team_cloud_on".into());
    }
    if folder_in_use(&folder, &crate::profiles::Registry::load().active) {
        return Err("cloud_folder_in_use".into());
    }
    let store = crate::cloud::Store::of(&folder);
    if !store.available() {
        return Err("no_folder".into());
    }
    if store.read().is_some() {
        return Err("team_folder_taken".into());
    }
    state.mutate(|v| v.team_create(&name, &me).map_err(code))?;
    let bytes = state.read(|v| v.to_cloud_bytes().map_err(code))?;
    store.write(&bytes)?;
    let mut s = state.settings();
    s.cloud_folder = folder;
    state.set_settings(s)?;
    crate::cloud::reset();
    Ok(())
}

/// Admin: adds a member. The caller has compared the safety code with them.
#[tauri::command]
pub fn team_invite(state: State<'_, AppState>, code_text: String, name: String) -> CmdResult<()> {
    let store = crate::cloud::Store::of(&state.settings().cloud_folder);
    let (file, sealed) = state.mutate(|v| v.team_invite(&code_text, &name).map_err(code))?;
    store.write_named(&file, &sealed)?;
    crate::cloud::mark_dirty();
    Ok(())
}

/// Admin: removes a member and changes the team key. Everyone else's new key is
/// written before the shared copy, so nobody finds a file they can't open.
#[tauri::command]
pub fn team_remove(state: State<'_, AppState>, public: String) -> CmdResult<()> {
    let store = crate::cloud::Store::of(&state.settings().cloud_folder);
    let (files, gone) = state.mutate(|v| v.team_remove(&public).map_err(code))?;
    for (file, sealed) in &files {
        store.write_named(file, sealed)?;
    }
    store.write(&state.read(|v| v.to_cloud_bytes().map_err(code))?)?;
    store.delete_named(&gone);
    crate::cloud::reset();
    Ok(())
}

fn invite_in(state: &AppState, folder: &str) -> CmdResult<Vec<u8>> {
    let store = crate::cloud::Store::of(folder);
    if !store.available() {
        return Err("no_folder".into());
    }
    let me = identity(state)?;
    store.read_named(&kryptos_core::team::invite_file(&me)).ok_or_else(|| "team_no_invite".into())
}

/// Who an invitation in `folder` is from, to compare before accepting.
#[tauri::command]
pub fn team_peek(state: State<'_, AppState>, folder: String) -> CmdResult<serde_json::Value> {
    let sealed = invite_in(&state, &folder)?;
    state.with(|v| {
        let i = v.team_peek(&sealed).map_err(code)?;
        Ok(serde_json::json!({ "name": i.name, "admin_safety": kryptos_core::team::safety_code(&i.sender), "ticket": kryptos_core::team::ticket(&sealed) }))
    })
}

/// Accepts the exact invitation shown by `team_peek`: if the file changed since
/// the safety code was compared, it's refused.
#[tauri::command]
pub fn team_join(state: State<'_, AppState>, folder: String, ticket: String) -> CmdResult<()> {
    if !state.settings().cloud_folder.is_empty() {
        return Err("team_cloud_on".into());
    }
    if folder_in_use(&folder, &crate::profiles::Registry::load().active) {
        return Err("cloud_folder_in_use".into());
    }
    let sealed = invite_in(&state, &folder)?;
    if kryptos_core::team::ticket(&sealed) != ticket {
        return Err("team_bad_invite".into());
    }
    state.mutate(|v| v.team_join(&sealed).map_err(code))?;
    let mut s = state.settings();
    s.cloud_folder = folder;
    state.set_settings(s)?;
    crate::cloud::reset();
    Ok(())
}

/// Stops syncing with the team. What's already here stays in this vault.
#[tauri::command]
pub fn team_leave(state: State<'_, AppState>) -> CmdResult<()> {
    state.mutate(|v| v.team_leave().map_err(code))?;
    let mut s = state.settings();
    s.cloud_folder.clear();
    state.set_settings(s)?;
    crate::cloud::reset();
    Ok(())
}

// ───────── Projects (.env files) ─────────

#[tauri::command]
pub fn env_list(state: State<'_, AppState>) -> CmdResult<Vec<kryptos_core::EnvSummary>> {
    state.with(|v| Ok(v.envs()))
}

#[tauri::command]
pub fn env_get(state: State<'_, AppState>, id: String) -> CmdResult<kryptos_core::EnvFile> {
    let id = parse_id(&id)?;
    state.with(|v| v.env(id).cloned().map_err(code))
}

#[tauri::command]
pub fn env_put(state: State<'_, AppState>, project: String, name: String, content: String) -> CmdResult<String> {
    state.mutate(|v| v.env_put(&project, &name, &content).map(|id| id.to_string()).map_err(code))
}

#[tauri::command]
pub fn env_update(state: State<'_, AppState>, id: String, content: String) -> CmdResult<()> {
    let id = parse_id(&id)?;
    state.mutate(|v| v.env_update(id, &content).map_err(code))
}

#[tauri::command]
pub fn env_rename(state: State<'_, AppState>, id: String, project: String, name: String) -> CmdResult<()> {
    let id = parse_id(&id)?;
    state.mutate(|v| v.env_rename(id, &project, &name).map_err(code))
}

#[tauri::command]
pub fn env_rename_project(state: State<'_, AppState>, project: String, name: String) -> CmdResult<()> {
    state.mutate(|v| v.env_rename_project(&project, &name).map_err(code))
}

#[tauri::command]
pub fn env_set_ai(state: State<'_, AppState>, id: String, on: bool) -> CmdResult<()> {
    let id = parse_id(&id)?;
    state.mutate(|v| v.env_set_ai(id, on).map_err(code))
}

#[tauri::command]
pub fn env_delete(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let id = parse_id(&id)?;
    state.mutate(|v| v.env_delete(id).map_err(code))
}

#[tauri::command]
pub fn env_restore(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let id = parse_id(&id)?;
    state.mutate(|v| v.env_restore(id).map_err(code))
}

/// Which variables differ between two environments (names only).
#[tauri::command]
pub fn env_diff(state: State<'_, AppState>, a: String, b: String) -> CmdResult<kryptos_core::EnvDiff> {
    let (a, b) = (parse_id(&a)?, parse_id(&b)?);
    state.with(|v| Ok(kryptos_core::envs::diff(&v.env(a).map_err(code)?.content, &v.env(b).map_err(code)?.content)))
}

#[tauri::command]
pub fn env_example(state: State<'_, AppState>, id: String) -> CmdResult<String> {
    let id = parse_id(&id)?;
    state.with(|v| Ok(kryptos_core::envs::example(&v.env(id).map_err(code)?.content)))
}

/// A file picked by the user, as text. Nothing is saved until they confirm.
#[tauri::command]
pub async fn env_pick_file(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Option<PickedEnv>> {
    state.touch();
    let Some(picked) = app.dialog().file().set_title("Import .env").blocking_pick_file() else { return Ok(None) };
    let file = file_name(&picked);
    let data = Zeroizing::new(app.fs().read(picked).map_err(|e| e.to_string())?);
    if data.len() > kryptos_core::envs::MAX_LEN {
        return Err("env_too_long".into());
    }
    let content = String::from_utf8(data.to_vec()).map_err(|_| "env_not_text")?;
    Ok(Some(PickedEnv { name: kryptos_core::envs::env_name(&file), content }))
}

#[derive(Serialize)]
pub struct PickedEnv {
    name: String,
    content: String,
}

/// Saves the environment as a file where the user chooses.
#[tauri::command]
pub async fn env_export(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<bool> {
    let id = parse_id(&id)?;
    let content = state.with(|v| Ok(Zeroizing::new(v.env(id).map_err(code)?.content.clone())))?;
    let Some(picked) = app.dialog().file().set_title("Export .env").set_file_name(".env").blocking_save_file() else { return Ok(false) };
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    let mut out = app.fs().open(picked, opts).map_err(|e| e.to_string())?;
    out.write_all(content.as_bytes()).and_then(|_| out.sync_all()).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Looks for `.env` files under a folder the user picked (desktop).
#[tauri::command]
pub async fn env_scan(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Option<Vec<kryptos_core::envs::FoundEnv>>> {
    state.touch();
    #[cfg(desktop)]
    {
        let Some(dir) = app.dialog().file().set_title("Kryptos").blocking_pick_folder() else { return Ok(None) };
        let dir = dir.into_path().map_err(|e| e.to_string())?;
        let found = tauri::async_runtime::spawn_blocking(move || kryptos_core::envs::scan(&dir)).await.map_err(|e| e.to_string())?;
        *SCANNED.lock().unwrap() = found.iter().map(|f| f.path.clone()).collect();
        Ok(Some(found))
    }
    #[cfg(mobile)]
    {
        let _ = app;
        Ok(None)
    }
}

/// What the last `env_scan` found: the UI can import these and no other file.
static SCANNED: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Imports files found by `env_scan`.
#[tauri::command]
pub fn env_import_found(state: State<'_, AppState>, items: Vec<FoundItem>) -> CmdResult<usize> {
    let mut read = Vec::with_capacity(items.len());
    for i in items {
        let path = std::path::PathBuf::from(&i.path);
        if !SCANNED.lock().unwrap().contains(&path) {
            return Err("invalid_id".into());
        }
        read.push((i.project, i.name, Zeroizing::new(std::fs::read_to_string(&path).map_err(|e| e.to_string())?)));
    }
    state.mutate(|v| {
        for (project, name, content) in &read {
            v.env_put(project, name, content).map_err(code)?;
        }
        Ok(read.len())
    })
}

#[derive(Deserialize)]
pub struct FoundItem {
    path: String,
    project: String,
    name: String,
}

#[derive(Serialize, Default)]
pub struct CliStatus {
    installed: bool,
    /// The folder isn't in `~/.zshrc` yet, or was added just now: open a new terminal.
    new_terminal: bool,
}

#[cfg(all(unix, desktop))]
mod cli {
    use std::path::{Path, PathBuf};

    pub fn link() -> Option<PathBuf> {
        Some(dirs::home_dir()?.join(".local/bin/kryptos"))
    }

    pub fn linked(link: &Path, target: &Path) -> bool {
        std::fs::read_link(link).is_ok_and(|p| p == target)
    }

    pub fn on_path(rc: &str) -> bool {
        rc.lines().any(|l| !l.trim_start().starts_with('#') && l.contains("PATH") && l.contains(".local/bin"))
    }
}

/// Whether `kryptos` is ready to use in a terminal.
#[tauri::command]
pub fn cli_status() -> CliStatus {
    #[cfg(all(unix, desktop))]
    if let (Some(link), Ok(target)) = (cli::link(), crate::browser::cli_binary()) {
        return CliStatus { installed: cli::linked(&link, &target), new_terminal: false };
    }
    CliStatus::default()
}

/// Links the bundled `kryptos` into `~/.local/bin` and adds that folder to
/// `~/.zshrc` if it's missing. Safe to run again.
#[tauri::command]
pub fn cli_install() -> CmdResult<CliStatus> {
    #[cfg(all(unix, desktop))]
    {
        let (link, target) = (cli::link().ok_or("no_home")?, crate::browser::cli_binary()?);
        let dir = link.parent().ok_or("no_home")?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        if !cli::linked(&link, &target) {
            match std::fs::remove_file(&link) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
                _ => {}
            }
            std::os::unix::fs::symlink(&target, &link).map_err(|e| e.to_string())?;
        }
        let rc = dirs::home_dir().ok_or("no_home")?.join(".zshrc");
        let text = std::fs::read_to_string(&rc).unwrap_or_default();
        let add = !cli::on_path(&text);
        if add {
            let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&rc).map_err(|e| e.to_string())?;
            writeln!(file, "\n# Kryptos command line\nexport PATH=\"$HOME/.local/bin:$PATH\"").map_err(|e| e.to_string())?;
        }
        Ok(CliStatus { installed: true, new_terminal: add })
    }
    #[cfg(not(all(unix, desktop)))]
    Err("unsupported_on_this_device".into())
}

// ───────── Trash ─────────

#[tauri::command]
pub fn trash_list(state: State<'_, AppState>) -> CmdResult<Vec<kryptos_core::vault::TrashedSummary>> {
    state.with(|v| Ok(v.trash()))
}

#[tauri::command]
pub fn trash_restore(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let id = parse_id(&id)?;
    state.mutate(|v| v.restore(id).map_err(code))
}

#[tauri::command]
pub fn trash_purge(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let id = parse_id(&id)?;
    state.mutate(|v| v.purge(id).map_err(code))
}

#[tauri::command]
pub fn trash_empty(state: State<'_, AppState>) -> CmdResult<()> {
    state.mutate(|v| {
        v.empty_trash();
        Ok(())
    })
}

// ───────── Smart cleanup (smart.rs) ─────────

#[derive(Serialize)]
pub struct SmartSuggestions {
    titles: Vec<kryptos_core::smart::TitleSuggestion>,
    duplicates: Vec<kryptos_core::smart::DuplicateGroup>,
}

/// Readable titles for entries named after a host, and groups of duplicates.
#[tauri::command]
pub fn smart_suggestions(state: State<'_, AppState>) -> CmdResult<SmartSuggestions> {
    state.with(|v| {
        Ok(SmartSuggestions {
            titles: kryptos_core::smart::title_suggestions(v.entries()),
            duplicates: kryptos_core::smart::duplicates(v.entries()),
        })
    })
}

#[derive(serde::Deserialize)]
pub struct Rename {
    id: String,
    title: String,
}

#[tauri::command]
pub fn apply_titles(state: State<'_, AppState>, titles: Vec<Rename>) -> CmdResult<usize> {
    let renames = titles.iter().map(|r| Ok((parse_id(&r.id)?, r.title.as_str()))).collect::<CmdResult<Vec<_>>>()?;
    state.mutate(|v| {
        for (id, title) in &renames {
            v.rename(*id, title).map_err(code)?;
        }
        Ok(renames.len())
    })
}

/// Merges a duplicate group into its newest entry; the rest go to the trash.
#[tauri::command]
pub fn merge_duplicates(state: State<'_, AppState>, ids: Vec<String>) -> CmdResult<String> {
    let ids = ids.iter().map(|i| parse_id(i)).collect::<CmdResult<Vec<_>>>()?;
    state.mutate(|v| v.merge_duplicates(&ids).map(|id| id.to_string()).map_err(code))
}

#[tauri::command]
pub fn generate_passphrase(words: usize, separator: String, capitalize: bool, digit: bool) -> CmdResult<Generated> {
    let password = generator::passphrase(words, &separator, capitalize, digit).map_err(code)?;
    Ok(Generated { password, entropy_bits: generator::passphrase_bits(words, digit) })
}

/// Opens the site's "change password" page in the browser (the standard
/// `/.well-known/change-password` address, which sites redirect to the right
/// page). Built here from the entry's own site: the UI can't open arbitrary URLs.
#[tauri::command]
pub fn open_change_password(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let id = parse_id(&id)?;
    let host = state.with(|v| {
        let e = v.get(id).map_err(code)?;
        e.urls.iter().find_map(|u| kryptos_core::import::host_of(u)).ok_or_else(|| "no_site".to_string())
    })?;
    if !host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return Err("no_site".into());
    }
    app.opener().open_url(format!("https://{host}/.well-known/change-password"), None::<&str>).map_err(|e| e.to_string())
}

// ───────── AI assistants (ai.rs, crates/mcp-server) ─────────

#[derive(Serialize)]
pub struct AiStatus {
    enabled: bool,
    has_code: bool,
    expires: u64,
    /// Where kryptos-mcp is, for the AI app's config ("" if not bundled).
    binary: String,
    shared: usize,
    log: serde_json::Value,
}

#[tauri::command]
pub fn ai_status(state: State<'_, AppState>) -> CmdResult<AiStatus> {
    let s = state.settings();
    // A read, not activity: Settings polls this every few seconds and must not
    // keep the vault from auto-locking.
    let shared = state.read(|v| Ok(v.entries().iter().filter(|e| e.ai_access).count() + v.envs().iter().filter(|e| e.ai_access && e.deleted_at == 0).count()))?;
    #[cfg(desktop)]
    let (binary, log) = (
        crate::browser::mcp_binary().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
        serde_json::to_value(crate::ai::log()).unwrap_or_default(),
    );
    #[cfg(mobile)]
    let (binary, log) = (String::new(), serde_json::Value::Array(vec![]));
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    Ok(AiStatus {
        enabled: s.ai_enabled,
        has_code: !s.ai_code_hash.is_empty() && s.ai_code_expires > now,
        expires: s.ai_code_expires,
        binary,
        shared,
        log,
    })
}

#[tauri::command]
pub fn ai_set_enabled(state: State<'_, AppState>, enabled: bool) -> CmdResult<()> {
    state.with(|_| Ok(()))?; // only while unlocked
    let mut s = state.settings();
    s.ai_enabled = enabled;
    state.set_settings(s)
}

/// A new access code, valid for `hours` (1 hour to 30 days); the previous one
/// stops working. Returned once, with a ready-to-paste MCP config.
#[tauri::command]
pub fn ai_new_code(state: State<'_, AppState>, hours: u64) -> CmdResult<serde_json::Value> {
    state.with(|_| Ok(()))?;
    if !(1..=720).contains(&hours) {
        return Err("bad_request".into());
    }
    let code = Zeroizing::new(kryptos_core::ai_token::generate());
    let mut s = state.settings();
    s.ai_code_hash = kryptos_core::ai_token::hash(&code);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    s.ai_code_expires = now + hours * 3600;
    state.set_settings(s)?;
    #[cfg(desktop)]
    let binary = crate::browser::mcp_binary().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "kryptos-mcp".into());
    #[cfg(mobile)]
    let binary = String::from("kryptos-mcp");
    let config = serde_json::json!({ "mcpServers": { "kryptos": { "command": binary, "env": { "KRYPTOS_MCP_TOKEN": code.as_str() } } } });
    Ok(serde_json::json!({ "code": code.as_str(), "config": serde_json::to_string_pretty(&config).unwrap_or_default() }))
}

#[tauri::command]
pub fn ai_revoke(state: State<'_, AppState>) -> CmdResult<()> {
    let mut s = state.settings();
    s.ai_code_hash.clear();
    s.ai_code_expires = 0;
    state.set_settings(s)
}

// ───────── Cloud folder sync (cloud.rs) ─────────

/// Cloud folders found on this device (none on mobile: there the folder is picked).
#[tauri::command]
pub fn cloud_providers() -> Vec<crate::cloud::Provider> {
    #[cfg(desktop)]
    return crate::cloud::providers();
    #[cfg(mobile)]
    Vec::new()
}

/// Android: the system folder picker (Google Drive, Dropbox... appear there).
/// The picked folder, or None if cancelled.
#[tauri::command]
pub async fn cloud_pick_folder(app: AppHandle) -> Option<crate::cloud::Provider> {
    #[cfg(target_os = "android")]
    return tauri::async_runtime::spawn_blocking(move || {
        let tree = crate::android::pick_cloud_folder(&app)?;
        let name = crate::android::saf::name(&tree);
        Some(crate::cloud::Provider { has_vault: crate::cloud::has_vault(&tree), name, path: tree })
    })
    .await
    .ok()
    .flatten();
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        None
    }
}

#[tauri::command]
pub fn cloud_status(state: State<'_, AppState>) -> serde_json::Value {
    let folder = state.settings().cloud_folder;
    // Android folders are opaque URIs: their display name comes from the provider.
    #[cfg(target_os = "android")]
    let name = if folder.is_empty() { String::new() } else { crate::android::saf::name(&folder) };
    #[cfg(not(target_os = "android"))]
    let name = String::new();
    serde_json::json!({ "folder": folder, "name": name, "status": crate::cloud::status() })
}

/// Turns on sync through `folder`. `password` is this vault's master password,
/// verified here (the UI also requires it to be very strong). If the folder
/// already holds a different vault with another password, fails with
/// "cloud_other_password": call again with `cloud_password`.
#[tauri::command]
pub async fn cloud_enable(
    app: AppHandle,
    state: State<'_, AppState>,
    folder: String,
    password: String,
    cloud_password: Option<String>,
) -> CmdResult<crate::cloud::Enabled> {
    let (password, cloud_password) = (Zeroizing::new(password), cloud_password.map(Zeroizing::new));
    if folder_in_use(&folder, &crate::profiles::Registry::load().active) {
        return Err("cloud_folder_in_use".into());
    }
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> CmdResult<crate::cloud::Enabled> {
        let bytes = storage::read(&st.vault_path()).map_err(code)?;
        UnlockedVault::unlock(&bytes, &password).map(drop).map_err(code)?;
        let done = crate::cloud::enable(&st, &folder, &password, cloud_password.as_deref().map(String::as_str))?;
        let mut s = st.settings();
        s.cloud_folder = folder.clone();
        st.set_settings(s)?;
        crate::cloud::push_now(&st, &folder)?;
        // Joining replaced the vault key: the fingerprint sealed the old one.
        #[cfg(target_os = "android")]
        if done.outcome == "joined" {
            crate::android::biometric_disable(&app);
        }
        #[cfg(not(target_os = "android"))]
        let _ = app;
        Ok(done)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Stops syncing. The copy in the cloud folder stays where it is.
#[tauri::command]
pub fn cloud_disable(state: State<'_, AppState>) -> CmdResult<()> {
    let mut s = state.settings();
    s.cloud_folder.clear();
    state.set_settings(s)
}

#[derive(Serialize)]
pub struct OtpImportResult {
    #[serde(flatten)]
    report: kryptos_core::OtpImportReport,
    file: String,
}

/// Imports the 2FA accounts of an authenticator app's export (Aegis, 2FAS,
/// Google Authenticator transfer link, otpauth:// lists). None if cancelled.
#[tauri::command]
pub async fn import_otp(app: AppHandle, state: State<'_, AppState>) -> CmdResult<Option<OtpImportResult>> {
    let Some(picked) = app.dialog().file().set_title("Importa codici 2FA").blocking_pick_file() else { return Ok(None) };
    let file = file_name(&picked);
    let data = Zeroizing::new(app.fs().read(picked).map_err(|e| e.to_string())?);
    let accounts = kryptos_core::otp_import::parse_export(&data).map_err(code)?;
    let report = state.mutate(|v| Ok(v.import_otp(&accounts)))?;
    Ok(Some(OtpImportResult { report, file }))
}

/// Reads the backup-code file a site lets you download and returns the codes
/// in it (the editor adds them to the entry). None if cancelled.
#[tauri::command]
pub async fn read_backup_codes(app: AppHandle) -> CmdResult<Option<Vec<String>>> {
    let Some(picked) = app.dialog().file().set_title("Codici di backup").blocking_pick_file() else { return Ok(None) };
    let data = Zeroizing::new(app.fs().read(picked).map_err(|e| e.to_string())?);
    if data.len() > 256 * 1024 {
        return Err("backup_codes_not_found".into());
    }
    let codes = kryptos_core::otp_import::parse_backup_codes(&String::from_utf8_lossy(&data));
    if codes.is_empty() {
        return Err("backup_codes_not_found".into());
    }
    Ok(Some(codes))
}

/// Display name of a picked file; Android only gives an opaque, percent-encoded URI.
pub(crate) fn file_name(picked: &FilePath) -> String {
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
    let bytes = std::fs::read(state.vault_path()).map_err(|e| e.to_string())?;
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    let mut out = app.fs().open(picked, opts).map_err(|e| e.to_string())?;
    out.write_all(&bytes).and_then(|_| out.sync_all()).map_err(|e| e.to_string())?;
    Ok(true)
}

pub(crate) fn today() -> String {
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

/// Android: the vault was just opened in the app; saves a login the user asked
/// to keep while it was locked (see android.rs). Its outcome, or None.
#[tauri::command]
pub fn apply_pending_save() -> Option<&'static str> {
    #[cfg(target_os = "android")]
    return crate::android::apply_pending_save();
    #[cfg(not(target_os = "android"))]
    None
}

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

// Confirmation of a browser fill (desktop), answered in the app window. `id` is
// the request announced by the "fill-confirm" event.

/// The system prompt's text in the UI's language, for fills that ask from Rust.
#[tauri::command]
pub fn fill_reason_template(template: String) {
    #[cfg(desktop)]
    crate::confirm::set_reason(template);
    #[cfg(not(desktop))]
    let _ = template;
}

/// Touch ID or the Mac password. `reason` is shown in the system prompt.
#[tauri::command]
pub async fn fill_confirm_system(id: u64, reason: String) -> CmdResult<()> {
    #[cfg(desktop)]
    {
        if !crate::confirm::is_pending(id) {
            return Err("timeout".into());
        }
        tauri::async_runtime::spawn_blocking(move || crate::confirm::system_auth(&reason))
            .await
            .map_err(|e| e.to_string())??;
        crate::confirm::answer(id, true);
        Ok(())
    }
    #[cfg(not(desktop))]
    {
        let _ = (id, reason);
        Err("unsupported_on_this_device".into())
    }
}

/// The master password, checked against the vault file (not just "is unlocked").
#[tauri::command]
pub async fn fill_confirm_master(state: State<'_, AppState>, id: u64, password: String) -> CmdResult<()> {
    #[cfg(desktop)]
    {
        if !crate::confirm::is_pending(id) {
            return Err("timeout".into());
        }
        let password = Zeroizing::new(password);
        let path = state.vault_path();
        tauri::async_runtime::spawn_blocking(move || -> CmdResult<()> {
            let bytes = storage::read(&path).map_err(code)?;
            UnlockedVault::unlock(&bytes, &password).map(drop).map_err(code)
        })
        .await
        .map_err(|e| e.to_string())??;
        crate::confirm::answer(id, true);
        Ok(())
    }
    #[cfg(not(desktop))]
    {
        let _ = (state, id, password);
        Err("unsupported_on_this_device".into())
    }
}

#[tauri::command]
pub fn fill_confirm_cancel(id: u64) {
    #[cfg(desktop)]
    crate::confirm::answer(id, false);
    #[cfg(not(desktop))]
    let _ = id;
}
