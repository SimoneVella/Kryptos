//! Profiles: separate vaults on one device (personal, work, a team), each with
//! its own file, master password, settings and cloud folder. Only the active
//! profile is ever open: switching locks it first, so the browser extension,
//! AI assistants, the command line and Android autofill reach one vault at most.
//!
//! The first vault stays where it always was, so existing installs just work.
//! Others live in `<data>/profiles/<id>/`. `profiles.json` holds names only.

use std::path::{Path, PathBuf};

use kryptos_core::{paths, storage};
use serde::{Deserialize, Serialize};

pub const DEFAULT: &str = "default";
const VAULT: &str = "vault.kryptos";
const SETTINGS: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    /// "" for the first profile until renamed: the UI shows its own word for it.
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub active: String,
    pub profiles: Vec<Profile>,
}

impl Default for Registry {
    fn default() -> Self {
        Self { active: DEFAULT.into(), profiles: vec![Profile { id: DEFAULT.into(), name: String::new() }] }
    }
}

impl Registry {
    fn file(dir: &Path) -> PathBuf {
        dir.join("profiles.json")
    }

    pub fn load_from(dir: &Path) -> Self {
        let mut r: Self = std::fs::read(Self::file(dir)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        if !r.profiles.iter().any(|p| p.id == r.active) {
            r.active = DEFAULT.into();
        }
        r
    }

    pub fn load() -> Self {
        Self::load_from(&paths::data_dir())
    }

    pub fn save(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        storage::write_atomic(&Self::file(&paths::data_dir()), &bytes).map_err(|e| e.to_string())
    }
}

/// A profile's folder. Ids are ours, but they're checked before touching disk.
pub fn dir_of(base: &Path, id: &str) -> Result<PathBuf, String> {
    if id == DEFAULT {
        return Ok(base.to_path_buf());
    }
    uuid::Uuid::parse_str(id).map_err(|_| "invalid_id")?;
    Ok(base.join("profiles").join(id))
}

/// (vault file, settings file) of a profile.
pub fn files(id: &str) -> Result<(PathBuf, PathBuf), String> {
    let dir = dir_of(&paths::data_dir(), id)?;
    Ok((dir.join(VAULT), dir.join(SETTINGS)))
}

/// The files to open at startup.
pub fn active_files() -> (PathBuf, PathBuf) {
    files(&Registry::load().active).unwrap_or_else(|_| (paths::vault_path(), crate::settings::Settings::path()))
}

/// The name for a profile, if it's usable.
pub fn clean_name(name: &str) -> Option<String> {
    let n = name.trim();
    (!n.is_empty() && n.chars().count() <= 40 && !n.chars().any(char::is_control)).then(|| n.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_installs_stay_where_they_are() {
        let base = Path::new("/data");
        assert_eq!(dir_of(base, DEFAULT).unwrap(), base);
        let id = uuid::Uuid::new_v4().to_string();
        assert_eq!(dir_of(base, &id).unwrap(), base.join("profiles").join(&id));
    }

    #[test]
    fn ids_cannot_leave_the_profiles_folder() {
        assert!(dir_of(Path::new("/data"), "../../etc").is_err());
    }

    #[test]
    fn a_missing_or_stale_registry_means_the_first_profile() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Registry::load_from(dir.path()).active, DEFAULT);
        std::fs::write(dir.path().join("profiles.json"), r#"{"active":"gone","profiles":[{"id":"default","name":""}]}"#).unwrap();
        assert_eq!(Registry::load_from(dir.path()).active, DEFAULT);
    }
}
