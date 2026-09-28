use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    /// Key unwrap failed. Deliberately indistinguishable from a tampered key blob.
    #[error("wrong master password")]
    WrongPassword,
    #[error("vault file is corrupted or has been tampered with: {0}")]
    Corrupt(&'static str),
    #[error("unsupported vault format version {0}")]
    UnsupportedVersion(u16),
    #[error("key derivation parameters out of accepted range")]
    InvalidKdfParams,
    #[error("entry not found")]
    NotFound,
    #[error("invalid input: {0}")]
    InvalidInput(&'static str),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}
