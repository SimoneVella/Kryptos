//! Non-secret preferences, stored as plain JSON next to the vault.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub auto_lock_minutes: u64,
    pub lock_on_sleep: bool,
    pub clipboard_clear_secs: u64,
    pub browser_integration: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { auto_lock_minutes: 5, lock_on_sleep: true, clipboard_clear_secs: 30, browser_integration: false }
    }
}

impl Settings {
    pub fn path() -> PathBuf {
        kryptos_core::paths::data_dir().join("settings.json")
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        kryptos_core::storage::write_atomic(path, &bytes).map_err(|e| e.to_string())
    }

    pub fn validate(&self) -> Result<(), String> {
        if !(1..=240).contains(&self.auto_lock_minutes) {
            return Err("invalid_auto_lock".into());
        }
        if !(10..=300).contains(&self.clipboard_clear_secs) {
            return Err("invalid_clipboard_timeout".into());
        }
        Ok(())
    }
}
