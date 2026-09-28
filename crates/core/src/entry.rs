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
        }
    }
}

pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
