//! Kryptos core: everything that touches secrets lives here, shared by the
//! desktop app, the native-messaging host and (later) the mobile apps.
//!
//! Nothing in this crate performs network I/O.

pub mod ai_token;
pub mod crypto;
pub mod entry;
pub mod envs;
pub mod error;
pub mod format;
pub mod generator;
pub mod health;
pub mod import;
pub mod matching;
pub mod otp_import;
pub mod paths;
pub mod smart;
pub mod twofa;
pub mod peer;
pub mod storage;
pub mod team;
pub mod totp;
pub mod vault;

pub use entry::{BackupCode, Entry, EntryInput, EntrySummary, PasswordChange};
pub use envs::{EnvDiff, EnvFile, EnvSummary};
pub use error::{Error, Result};
pub use generator::GeneratorOptions;
pub use vault::{MergeReport, OtpImportReport, SaveOutcome, SyncReport, UnlockedVault};
