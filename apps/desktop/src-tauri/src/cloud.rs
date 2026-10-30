//! Sync between devices through a cloud folder (Google Drive, iCloud Drive,
//! Dropbox, OneDrive). Kryptos never connects to the network: it keeps an
//! encrypted copy of the vault in `<folder>/Kryptos/vault.kryptos` and the
//! provider's own app moves that file between devices.
//!
//! All devices share one vault (same id, key and master password; a device that
//! starts with its own vault joins the shared one once, see `join`). So the copy
//! opens with the key already in memory and is merged silently: on unlock, when
//! the provider brings a newer file, and after every change made here.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use kryptos_core::{storage, Error, MergeReport};
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::state::AppState;

const TICK: Duration = Duration::from_secs(5);
const DIR: &str = "Kryptos";
const FILE: &str = "vault.kryptos";

/// Changes made here that the cloud copy doesn't have yet.
static DIRTY: AtomicBool = AtomicBool::new(true);
static STATUS: Mutex<Status> = Mutex::new(Status { state: "off", last_sync: 0 });
/// Modification time of the cloud copy when last read or written.
static SEEN: Mutex<Option<i64>> = Mutex::new(None);

#[derive(Clone, Copy, Serialize)]
pub struct Status {
    /// "off", "ok", "waiting" (vault locked or folder missing), "other_vault"
    /// (the folder holds a different vault: join it) or "error".
    pub state: &'static str,
    /// Unix seconds of the last successful sync, 0 if none yet.
    pub last_sync: u64,
}

/// A cloud folder found on this computer.
#[derive(Serialize)]
pub struct Provider {
    pub name: String,
    pub path: String,
    /// The folder already holds a Kryptos vault (set up from another device).
    pub has_vault: bool,
}

pub fn status() -> Status {
    *STATUS.lock().unwrap()
}

fn set_status(state: &'static str, synced: bool) {
    let mut s = STATUS.lock().unwrap();
    s.state = state;
    if synced {
        s.last_sync = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    }
}

/// Called after every change to the local vault.
pub fn mark_dirty() {
    DIRTY.store(true, Ordering::Relaxed);
}

pub fn vault_file(folder: &str) -> PathBuf {
    Path::new(folder).join(DIR).join(FILE)
}

/// Cloud folders kept in sync by their providers' apps on this Mac.
#[cfg(desktop)]
pub fn providers() -> Vec<Provider> {
    let Some(home) = dirs::home_dir() else { return vec![] };
    let mut found = Vec::new();
    let mut add = |name: &str, path: PathBuf| {
        if path.is_dir() && !found.iter().any(|p: &Provider| Path::new(&p.path) == path) {
            let has_vault = vault_file(&path.to_string_lossy()).exists();
            found.push(Provider { name: name.into(), path: path.to_string_lossy().into_owned(), has_vault });
        }
    };
    // Google Drive, Dropbox and OneDrive mount under ~/Library/CloudStorage.
    if let Ok(dir) = std::fs::read_dir(home.join("Library/CloudStorage")) {
        for e in dir.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("GoogleDrive") {
                if let Some(my_drive) = google_my_drive(&e.path()) {
                    add("Google Drive", my_drive);
                }
            } else if name.starts_with("Dropbox") {
                add("Dropbox", e.path());
            } else if name.starts_with("OneDrive") {
                add("OneDrive", e.path());
            }
        }
    }
    add("iCloud Drive", home.join("Library/Mobile Documents/com~apple~CloudDocs"));
    add("Dropbox", home.join("Dropbox"));
    found
}

/// "My Drive" inside a Google Drive mount; its name follows the account language.
#[cfg(desktop)]
fn google_my_drive(mount: &Path) -> Option<PathBuf> {
    const NAMES: &[&str] = &[
        "My Drive", "Il mio Drive", "Mi unidad", "Mon Drive", "Meine Ablage", "Meu Drive", "O meu disco", "Мой диск", "Мій диск", "我的云端硬盘",
    ];
    NAMES.iter().map(|n| mount.join(n)).find(|p| p.is_dir()).or_else(|| {
        // Unknown language: the first folder that isn't a shared drive or another computer.
        std::fs::read_dir(mount).ok()?.flatten().map(|e| e.path()).find(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            p.is_dir() && !n.starts_with('.') && !n.to_lowercase().contains("shared") && !n.to_lowercase().contains("computer")
        })
    })
}

/// Background loop: syncs whenever the cloud copy changed or this vault did.
pub fn start(app: AppHandle, state: AppState) {
    std::thread::spawn(move || loop {
        std::thread::sleep(TICK);
        let folder = state.settings().cloud_folder;
        if folder.is_empty() {
            set_status("off", false);
            continue;
        }
        if !state.is_unlocked() {
            // The first sync after the next unlock reads and writes again.
            *SEEN.lock().unwrap() = None;
            mark_dirty();
            set_status("waiting", false);
            continue;
        }
        match sync(&state, &folder) {
            Ok(Some(report)) => {
                set_status("ok", true);
                if report.merge.added + report.merge.updated + report.merge.deleted > 0 || report.password_changed {
                    let _ = app.emit("vault-synced", json!({ "report": report }));
                }
            }
            Ok(None) => {}
            Err(e) if e == "other_vault" => set_status("other_vault", false),
            Err(e) if e == "locked" || e == "no_folder" => set_status("waiting", false),
            Err(_) => set_status("error", false),
        }
    });
}

/// Where the cloud copy lives: a folder on disk (desktop) or a folder the user
/// picked with Android's system picker, reached through CloudFolder.kt.
enum Store {
    Fs(PathBuf),
    #[cfg(target_os = "android")]
    Saf(String),
}

impl Store {
    fn of(folder: &str) -> Store {
        #[cfg(target_os = "android")]
        if folder.starts_with("content://") {
            return Store::Saf(folder.to_owned());
        }
        Store::Fs(vault_file(folder))
    }

    /// The folder is reachable (provider app installed, permission not revoked).
    fn available(&self) -> bool {
        match self {
            Store::Fs(file) => file.parent().and_then(Path::parent).is_some_and(Path::is_dir),
            #[cfg(target_os = "android")]
            Store::Saf(tree) => crate::android::saf::usable(tree),
        }
    }

    /// Last-modified time of the copy in ms, None if there is no copy yet.
    fn modified(&self) -> Option<i64> {
        match self {
            Store::Fs(file) => std::fs::metadata(file)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64),
            #[cfg(target_os = "android")]
            Store::Saf(tree) => crate::android::saf::modified(tree),
        }
    }

    fn read(&self) -> Option<Vec<u8>> {
        match self {
            Store::Fs(file) => storage::read(file).ok(),
            #[cfg(target_os = "android")]
            Store::Saf(tree) => crate::android::saf::read(tree),
        }
    }

    fn write(&self, bytes: &[u8]) -> Result<(), String> {
        match self {
            Store::Fs(file) => storage::write_replace(file, bytes).map_err(|e| e.to_string()),
            #[cfg(target_os = "android")]
            Store::Saf(tree) => crate::android::saf::write(tree, bytes).then_some(()).ok_or_else(|| "cloud_write_failed".into()),
        }
    }

    /// Copies the provider made when two devices wrote at once: "vault (1).kryptos",
    /// "vault (conflict ...).kryptos" and the like, next to the main file.
    fn conflicts(&self) -> Vec<String> {
        match self {
            Store::Fs(file) => {
                let Some(dir) = file.parent() else { return vec![] };
                let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
                entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        let n = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        p != file && n.starts_with("vault") && n.ends_with(".kryptos")
                    })
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect()
            }
            #[cfg(target_os = "android")]
            Store::Saf(tree) => crate::android::saf::conflicts(tree),
        }
    }

    fn read_copy(&self, id: &str) -> Option<Vec<u8>> {
        match self {
            Store::Fs(_) => storage::read(Path::new(id)).ok(),
            #[cfg(target_os = "android")]
            Store::Saf(_) => crate::android::saf::read_doc(id),
        }
    }

    fn delete_copy(&self, id: &str) {
        match self {
            Store::Fs(_) => drop(std::fs::remove_file(id)),
            #[cfg(target_os = "android")]
            Store::Saf(_) => drop(crate::android::saf::delete(id)),
        }
    }
}

/// One round: merges the cloud copy (and any conflict copies the provider made)
/// into the vault if they changed, then writes the cloud copy if this vault has
/// changes it lacks. Ok(None) when there was nothing to do.
pub fn sync(state: &AppState, folder: &str) -> Result<Option<kryptos_core::SyncReport>, String> {
    let store = Store::of(folder);
    if !store.available() {
        return Err("no_folder".into()); // provider app not installed, permission revoked, folder moved
    }
    let modified = store.modified();
    let changed = modified.is_some() && modified != *SEEN.lock().unwrap();
    let conflicts = store.conflicts();
    if !changed && conflicts.is_empty() && !DIRTY.load(Ordering::Relaxed) {
        return Ok(None);
    }

    let mut report = kryptos_core::SyncReport::default();
    let mut merge = |bytes: &[u8]| -> Result<(), String> {
        let r = state.sync_from(bytes)?;
        report.merge.added += r.merge.added;
        report.merge.updated += r.merge.updated;
        report.merge.deleted += r.merge.deleted;
        report.password_changed |= r.password_changed;
        Ok(())
    };
    if changed {
        merge(&store.read().ok_or("cloud_read_failed")?)?;
    }
    for copy in &conflicts {
        // Only copies of this vault are merged and removed; anything else is left alone.
        if let Some(bytes) = store.read_copy(copy) {
            if merge(&bytes).is_ok() {
                store.delete_copy(copy);
            }
        }
    }

    // Pull first, then push: the copy written below has everything from both sides.
    if DIRTY.swap(false, Ordering::Relaxed) || modified.is_none() || !conflicts.is_empty() || report.merge != MergeReport::default() {
        let bytes = state.read(|v| Ok(v.to_bytes()))?;
        if let Err(e) = store.write(&bytes) {
            mark_dirty();
            return Err(e);
        }
    }
    *SEEN.lock().unwrap() = store.modified();
    Ok(Some(report))
}

/// The folder already holds a Kryptos vault (set up from another device).
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub fn has_vault(folder: &str) -> bool {
    Store::of(folder).modified().is_some()
}

/// What turning sync on did.
#[derive(Serialize)]
pub struct Enabled {
    /// "created" (first device), "synced" (same vault already there) or
    /// "joined" (this device moved its entries into the shared vault).
    pub outcome: &'static str,
    pub added: usize,
}

/// Turns sync on for `folder`. `password` is this vault's master password (just
/// verified); it is also tried on a vault already in the folder, which opens
/// when the user kept the same master password on every device. Otherwise
/// "cloud_other_password": ask for that vault's password and call it again with
/// `cloud_password`.
pub fn enable(state: &AppState, folder: &str, password: &str, cloud_password: Option<&str>) -> Result<Enabled, String> {
    let store = Store::of(folder);
    if !store.available() {
        return Err("no_folder".into());
    }
    let outcome = match store.read() {
        None => {
            store.write(&state.read(|v| Ok(v.to_bytes()))?)?;
            Enabled { outcome: "created", added: 0 }
        }
        Some(bytes) if state.read(|v| Ok(v.is_copy(&bytes)))? => {
            let r = state.sync_from(&bytes)?;
            Enabled { outcome: "synced", added: r.merge.added }
        }
        Some(bytes) => {
            let pw = cloud_password.unwrap_or(password);
            let report = state.join_cloud(&bytes, pw).map_err(|e| match e {
                Error::WrongPassword => "cloud_other_password".to_string(),
                other => crate::commands::code(other),
            })?;
            Enabled { outcome: "joined", added: report.added }
        }
    };
    *SEEN.lock().unwrap() = None;
    mark_dirty();
    Ok(outcome)
}

/// The vault this device ended up with must reach the folder before sync is on.
pub fn push_now(state: &AppState, folder: &str) -> Result<(), String> {
    sync(state, folder).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kryptos_core::crypto::KdfParams;
    use kryptos_core::{EntryInput, UnlockedVault};

    /// The sync state is per process (one device): run these one at a time.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn entry(title: &str) -> EntryInput {
        EntryInput {
            title: title.into(),
            username: "me".into(),
            password: "pw".into(),
            urls: vec![format!("https://{}.test", title.to_lowercase())],
            notes: String::new(),
            favorite: false,
            totp: String::new(),
            backup_codes: Vec::new(),
        }
    }

    fn titles(v: &UnlockedVault) -> Vec<String> {
        let mut t: Vec<_> = v.entries().iter().map(|e| e.title.clone()).collect();
        t.sort();
        t
    }

    /// This device against a second one that only touches the cloud folder.
    #[test]
    fn syncs_both_ways_and_merges_conflict_copies() {
        let _serial = SERIAL.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Drive");
        std::fs::create_dir(&folder).unwrap();
        let folder = folder.to_string_lossy().into_owned();
        let state = AppState::new(dir.path().join("local/vault.kryptos"), dir.path().join("local/settings.json"));
        let mut v = UnlockedVault::create("same password", KdfParams::TEST).unwrap();
        v.add(entry("Here")).unwrap();
        state.set_unlocked(v);

        // First device: the folder gets this vault.
        assert_eq!(enable(&state, &folder, "same password", None).unwrap().outcome, "created");
        sync(&state, &folder).unwrap();
        let file = vault_file(&folder);
        let mut other = UnlockedVault::unlock(&storage::read(&file).unwrap(), "same password").unwrap();

        // A change from the other device arrives here...
        other.add(entry("There")).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        storage::write_replace(&file, &other.to_bytes()).unwrap();
        let r = sync(&state, &folder).unwrap().unwrap();
        assert_eq!(r.merge.added, 1);
        assert_eq!(state.read(|v| Ok(titles(v))).unwrap(), ["Here", "There"]);
        // ...and was saved to this device's own file.
        let local = UnlockedVault::unlock(&storage::read(&state.vault_path).unwrap(), "same password").unwrap();
        assert_eq!(titles(&local), ["Here", "There"]);

        // A change made here reaches the folder.
        state.mutate(|v| v.add(entry("Later")).map(drop).map_err(|e| e.to_string())).unwrap();
        sync(&state, &folder).unwrap();
        let cloud = UnlockedVault::unlock(&storage::read(&file).unwrap(), "same password").unwrap();
        assert_eq!(titles(&cloud), ["Here", "Later", "There"]);

        // A conflict copy left by the provider is merged and removed.
        other.add(entry("Conflict")).unwrap();
        let copy = file.with_file_name("vault (1).kryptos");
        storage::write_replace(&copy, &other.to_bytes()).unwrap();
        sync(&state, &folder).unwrap();
        assert!(!copy.exists());
        assert!(state.read(|v| Ok(titles(v))).unwrap().contains(&"Conflict".to_string()));
        assert!(nothing_to_do(&state, &folder));
    }

    fn nothing_to_do(state: &AppState, folder: &str) -> bool {
        matches!(sync(state, folder), Ok(None))
    }

    /// A device that started with its own vault joins the one in the folder.
    #[test]
    fn joins_a_different_vault() {
        let _serial = SERIAL.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().to_string_lossy().into_owned();
        let mut shared = UnlockedVault::create("phone password", KdfParams::TEST).unwrap();
        shared.add(entry("Phone")).unwrap();
        storage::write_replace(&vault_file(&folder), &shared.to_bytes()).unwrap();

        let state = AppState::new(dir.path().join("local/vault.kryptos"), dir.path().join("local/settings.json"));
        let mut mine = UnlockedVault::create("computer password", KdfParams::TEST).unwrap();
        mine.add(entry("Computer")).unwrap();
        state.set_unlocked(mine);

        // Different master passwords: the shared vault's one is needed.
        assert_eq!(enable(&state, &folder, "computer password", None).err().as_deref(), Some("cloud_other_password"));
        assert_eq!(state.read(|v| Ok(titles(v))).unwrap(), ["Computer"]); // nothing lost on the way
        let done = enable(&state, &folder, "computer password", Some("phone password")).unwrap();
        assert_eq!((done.outcome, done.added), ("joined", 1));
        assert_eq!(state.read(|v| Ok(titles(v))).unwrap(), ["Computer", "Phone"]);
        // This device now uses the shared vault and its password.
        let local = storage::read(&state.vault_path).unwrap();
        assert!(UnlockedVault::unlock(&local, "phone password").is_ok());
        assert_eq!(state.read(|v| Ok(v.vault_id())).unwrap(), shared.vault_id());
    }
}
