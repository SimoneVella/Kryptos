use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use kryptos_core::{storage, UnlockedVault};

use crate::settings::Settings;

pub struct AppState {
    pub vault_path: PathBuf,
    settings_path: PathBuf,
    inner: Mutex<Inner>,
}

struct Inner {
    vault: Option<UnlockedVault>,
    last_activity: Activity,
    settings: Settings,
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
        Self { vault_path, settings_path, inner: Mutex::new(Inner { vault: None, last_activity: Activity::now(), settings }) }
    }

    pub fn vault_exists(&self) -> bool {
        self.vault_path.exists()
    }

    pub fn is_unlocked(&self) -> bool {
        self.inner.lock().unwrap().vault.is_some()
    }

    pub fn set_unlocked(&self, v: UnlockedVault) {
        let mut g = self.inner.lock().unwrap();
        g.vault = Some(v);
        g.last_activity = Activity::now();
    }

    /// Drops the decrypted vault; `UnlockedVault` zeroizes keys and entries on drop.
    /// Returns whether it was unlocked.
    pub fn lock(&self) -> bool {
        self.inner.lock().unwrap().vault.take().is_some()
    }

    pub fn touch(&self) {
        self.inner.lock().unwrap().last_activity = Activity::now();
    }

    pub fn settings(&self) -> Settings {
        self.inner.lock().unwrap().settings.clone()
    }

    pub fn set_settings(&self, s: Settings) -> Result<(), String> {
        s.validate()?;
        s.save(&self.settings_path)?;
        self.inner.lock().unwrap().settings = s;
        Ok(())
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
        let v = g.vault.as_mut().ok_or("locked")?;
        let out = f(v)?;
        storage::write_atomic(&self.vault_path, &v.to_bytes()).map_err(|e| e.to_string())?;
        Ok(out)
    }
}
