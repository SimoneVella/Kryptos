use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use kryptos_core::{storage, MergeReport, SyncReport, UnlockedVault};
use zeroize::Zeroizing;

use crate::settings::Settings;

/// Cheap to clone: clones share the same vault. On Android the autofill side
/// holds one too, since it can run before (or without) the Tauri app.
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    /// The active profile's files (see profiles.rs).
    vault_path: PathBuf,
    settings_path: PathBuf,
    vault: Option<UnlockedVault>,
    last_activity: Activity,
    /// Set while no app screen is visible (mobile): the vault locks shortly after.
    background_since: Option<Activity>,
    settings: Settings,
    /// Counts unlocks, so a confirmation can't outlive the unlock it was given in.
    session: u64,
}

/// Tracks both clocks: the monotonic one can pause while the device sleeps
/// or the app is suspended, the wall clock can be changed by the user.
#[derive(Clone, Copy)]
struct Activity(Instant, SystemTime);

impl Activity {
    fn now() -> Self {
        Self(Instant::now(), SystemTime::now())
    }
    fn elapsed(&self) -> Duration {
        self.0.elapsed().max(self.1.elapsed().unwrap_or_default())
    }
}

impl AppState {
    pub fn new(vault_path: PathBuf, settings_path: PathBuf) -> Self {
        let settings = Settings::load(&settings_path);
        Self { inner: Arc::new(Mutex::new(Inner { vault_path, settings_path, vault: None, last_activity: Activity::now(), background_since: None, settings, session: 0 })) }
    }

    pub fn vault_path(&self) -> PathBuf {
        self.inner.lock().unwrap().vault_path.clone()
    }

    pub fn vault_exists(&self) -> bool {
        self.vault_path().exists()
    }

    /// Moves to another profile: the open vault is locked and dropped first.
    pub fn switch(&self, vault_path: PathBuf, settings_path: PathBuf) {
        let settings = Settings::load(&settings_path);
        let mut g = self.inner.lock().unwrap();
        g.vault = None;
        g.session += 1;
        g.vault_path = vault_path;
        g.settings_path = settings_path;
        g.settings = settings;
        g.background_since = None;
        g.last_activity = Activity::now();
    }

    pub fn is_unlocked(&self) -> bool {
        self.inner.lock().unwrap().vault.is_some()
    }

    pub fn set_unlocked(&self, v: UnlockedVault) {
        let mut g = self.inner.lock().unwrap();
        g.vault = Some(v);
        g.last_activity = Activity::now();
        g.session += 1;
    }

    #[cfg_attr(not(all(unix, desktop)), allow(dead_code))]
    pub fn session(&self) -> u64 {
        self.inner.lock().unwrap().session
    }

    /// Drops the decrypted vault; `UnlockedVault` zeroizes keys and entries on drop.
    /// Returns whether it was unlocked.
    pub fn lock(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        g.session += 1;
        g.vault.take().is_some()
    }

    pub fn touch(&self) {
        self.inner.lock().unwrap().last_activity = Activity::now();
    }

    pub fn settings(&self) -> Settings {
        self.inner.lock().unwrap().settings.clone()
    }

    pub fn set_settings(&self, s: Settings) -> Result<(), String> {
        s.validate()?;
        let mut g = self.inner.lock().unwrap();
        s.save(&g.settings_path)?;
        g.settings = s;
        Ok(())
    }

    // Only Android reports visibility and seals the key under biometrics so far.
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub fn set_foreground(&self, visible: bool) {
        let mut g = self.inner.lock().unwrap();
        g.background_since = if visible { None } else { Some(Activity::now()) };
    }

    /// Locks once the app has been out of sight for `grace`.
    pub fn lock_if_background(&self, grace: Duration) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.vault.is_some() && g.background_since.is_some_and(|b| b.elapsed() >= grace) {
            g.vault = None;
            return true;
        }
        false
    }

    /// Raw vault key while unlocked, for sealing under biometrics.
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    pub fn raw_key(&self) -> Option<Zeroizing<Vec<u8>>> {
        self.inner.lock().unwrap().vault.as_ref().map(|v| v.raw_key())
    }

    pub fn lock_if_idle(&self) -> bool {
        let mut g = self.inner.lock().unwrap();
        let limit = Duration::from_secs(g.settings.auto_lock_minutes * 60);
        if g.vault.is_some() && g.last_activity.elapsed() >= limit {
            g.vault = None;
            return true;
        }
        false
    }

    /// Read access. Does not count as user activity (the browser bridge uses this).
    pub fn read<T>(&self, f: impl FnOnce(&UnlockedVault) -> Result<T, String>) -> Result<T, String> {
        let g = self.inner.lock().unwrap();
        f(g.vault.as_ref().ok_or("locked")?)
    }

    /// Read access triggered by the user in the app window.
    pub fn with<T>(&self, f: impl FnOnce(&UnlockedVault) -> Result<T, String>) -> Result<T, String> {
        let mut g = self.inner.lock().unwrap();
        g.last_activity = Activity::now();
        f(g.vault.as_ref().ok_or("locked")?)
    }

    /// Mutates the vault and persists it to disk before returning.
    pub fn mutate<T>(&self, f: impl FnOnce(&mut UnlockedVault) -> Result<T, String>) -> Result<T, String> {
        let mut g = self.inner.lock().unwrap();
        g.last_activity = Activity::now();
        let path = g.vault_path.clone();
        let v = g.vault.as_mut().ok_or("locked")?;
        let out = f(v)?;
        storage::write_atomic(&path, &v.to_bytes()).map_err(|e| e.to_string())?;
        crate::cloud::mark_dirty();
        Ok(out)
    }

    /// Merges a cloud copy of this vault (see cloud.rs); saved only if it changed
    /// something. Not user activity: it runs in the background.
    pub fn sync_from(&self, bytes: &[u8]) -> Result<SyncReport, String> {
        let mut g = self.inner.lock().unwrap();
        let path = g.vault_path.clone();
        let v = g.vault.as_mut().ok_or("locked")?;
        let r = v.sync_from(bytes).map_err(crate::commands::code)?;
        if r.merge != MergeReport::default() || r.password_changed {
            storage::write_atomic(&path, &v.to_bytes()).map_err(|e| e.to_string())?;
        }
        Ok(r)
    }

    /// Replaces this vault with the shared one in the cloud folder, after moving
    /// this vault's entries into it (see `UnlockedVault::join`). From then on this
    /// device's master password is the shared vault's.
    pub fn join_cloud(&self, bytes: &[u8], password: &str) -> Result<MergeReport, kryptos_core::Error> {
        let mut g = self.inner.lock().unwrap();
        let v = g.vault.as_ref().ok_or(kryptos_core::Error::InvalidInput("locked"))?;
        let (shared, report) = v.join(bytes, password)?;
        storage::write_atomic(&g.vault_path, &shared.to_bytes())?;
        g.vault = Some(shared);
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kryptos_core::crypto::KdfParams;

    #[test]
    fn switching_profile_closes_the_vault_and_keeps_files_apart() {
        let dir = tempfile::tempdir().unwrap();
        let file = |p: &str, f: &str| dir.path().join(p).join(f);
        std::fs::create_dir_all(dir.path().join("a")).unwrap();
        std::fs::create_dir_all(dir.path().join("b")).unwrap();
        let state = AppState::new(file("a", "vault.kryptos"), file("a", "settings.json"));
        let mut s = state.settings();
        s.cloud_folder = "/drive/personal".into();
        state.set_settings(s).unwrap();
        state.set_unlocked(UnlockedVault::create("password one", KdfParams::TEST).unwrap());
        let session = state.session();

        state.switch(file("b", "vault.kryptos"), file("b", "settings.json"));
        assert!(!state.is_unlocked(), "nothing of the old profile stays open");
        assert_ne!(state.session(), session, "earlier confirmations don't carry over");
        assert_eq!(state.settings().cloud_folder, "", "settings belong to the profile");

        // A write can't land in the other profile's file.
        state.set_unlocked(UnlockedVault::create("password two", KdfParams::TEST).unwrap());
        state.mutate(|_| Ok(())).unwrap();
        assert!(file("b", "vault.kryptos").exists());
        assert!(!file("a", "vault.kryptos").exists());
    }
}
