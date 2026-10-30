use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A stored login. Wiped from memory on drop.
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Entry {
    #[zeroize(skip)]
    pub id: Uuid,
    pub title: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub favorite: bool,
    /// 2FA secret: a base32 secret or an `otpauth://totp/...` link (see totp.rs).
    #[serde(default)]
    pub totp: String,
    /// One-time recovery codes the site gave when 2FA was turned on.
    #[serde(default)]
    pub backup_codes: Vec<BackupCode>,
    /// Earlier passwords, newest first (the last few, see vault.rs).
    #[serde(default)]
    pub history: Vec<PasswordChange>,
    /// The user allowed AI assistants to request this entry (see the MCP server).
    #[serde(default)]
    pub ai_access: bool,
    /// Unix seconds.
    pub created_at: u64,
    pub updated_at: u64,
}

/// What the UI sends when creating or editing an entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct EntryInput {
    pub title: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub totp: String,
    #[serde(default)]
    pub backup_codes: Vec<BackupCode>,
    #[serde(default)]
    pub ai_access: bool,
}

/// A password an entry used before, kept so a change can be undone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct PasswordChange {
    pub password: String,
    /// Unix seconds: when it stopped being the current one.
    pub replaced_at: u64,
}

/// A 2FA recovery code; each works once, so the user marks the ones spent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct BackupCode {
    pub code: String,
    #[serde(default)]
    pub used: bool,
}

/// Safe-to-list view: never carries the password or notes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntrySummary {
    pub id: Uuid,
    pub title: String,
    pub username: String,
    pub urls: Vec<String>,
    pub favorite: bool,
    pub updated_at: u64,
    /// Has a 2FA secret (the list offers to copy its code).
    #[serde(default)]
    pub has_totp: bool,
    #[serde(default)]
    pub ai_access: bool,
    /// Unix seconds of the last copy or fill, 0 if never (see `UnlockedVault::record_use`).
    #[serde(default)]
    pub last_used: u64,
}

impl From<&Entry> for EntrySummary {
    fn from(e: &Entry) -> Self {
        Self {
            id: e.id,
            title: e.title.clone(),
            username: e.username.clone(),
            urls: e.urls.clone(),
            favorite: e.favorite,
            updated_at: e.updated_at,
            has_totp: !e.totp.trim().is_empty(),
            ai_access: e.ai_access,
            last_used: 0,
        }
    }
}

pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
