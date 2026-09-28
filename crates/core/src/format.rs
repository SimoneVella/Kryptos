//! On-disk layout of a `.kryptos` vault file:
//!
//! ```text
//! magic      8 bytes   "KRYPTOS\0"
//! version    u16 LE
//! hdr_len    u32 LE
//! header     hdr_len bytes of JSON (`Header`)
//! nonce      24 bytes
//! body       XChaCha20-Poly1305 ciphertext of the JSON `VaultData`
//! ```
//!
//! Everything before `nonce` is the AAD for `body`, so any change to the
//! header (KDF params, salt, wrapped key) makes decryption fail.
//!
//! Key hierarchy: Argon2id(master password, salt) = KEK, which wraps a random
//! vault key (DEK). The DEK encrypts the body. Changing the master password
//! only re-wraps the DEK; biometric unlock stores the DEK in Keychain/Keystore.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::crypto::{KdfParams, NONCE_LEN};
use crate::error::{Error, Result};

pub const MAGIC: &[u8; 8] = b"KRYPTOS\0";
pub const VERSION: u16 = 1;
pub const WRAP_AAD: &[u8] = b"kryptos/v1/wrapped-vault-key";
const MAX_HEADER: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub vault_id: Uuid,
    pub kdf: KdfParams,
    #[serde(with = "b64")]
    pub salt: Vec<u8>,
    #[serde(with = "b64")]
    pub wrap_nonce: Vec<u8>,
    #[serde(with = "b64")]
    pub wrapped_key: Vec<u8>,
}

pub struct ParsedFile<'a> {
    pub header: Header,
    /// Bytes covered by the body's AAD (magic..end of header).
    pub aad: &'a [u8],
    pub nonce: &'a [u8],
    pub body: &'a [u8],
}

pub fn encode(header: &Header, nonce: &[u8; NONCE_LEN], body: &[u8]) -> Vec<u8> {
    let prefix = encode_prefix(header);
    let mut out = Vec::with_capacity(prefix.len() + NONCE_LEN + body.len());
    out.extend_from_slice(&prefix);
    out.extend_from_slice(nonce);
    out.extend_from_slice(body);
    out
}

/// magic + version + header; also the body's AAD.
pub fn encode_prefix(header: &Header) -> Vec<u8> {
    let hdr = serde_json::to_vec(header).expect("header serialization is infallible");
    let mut out = Vec::with_capacity(14 + hdr.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(hdr.len() as u32).to_le_bytes());
    out.extend_from_slice(&hdr);
    out
}

pub fn decode(bytes: &[u8]) -> Result<ParsedFile<'_>> {
    if bytes.len() < 14 || &bytes[..8] != MAGIC {
        return Err(Error::Corrupt("not a Kryptos vault"));
    }
    let version = u16::from_le_bytes([bytes[8], bytes[9]]);
    if version != VERSION {
        return Err(Error::UnsupportedVersion(version));
    }
    let hdr_len = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    if hdr_len > MAX_HEADER {
        return Err(Error::Corrupt("header too large"));
    }
    let hdr_end = 14usize
        .checked_add(hdr_len)
        .filter(|&e| e + NONCE_LEN <= bytes.len())
        .ok_or(Error::Corrupt("truncated file"))?;
    let header: Header =
        serde_json::from_slice(&bytes[14..hdr_end]).map_err(|_| Error::Corrupt("bad header"))?;
    Ok(ParsedFile {
        header,
        aad: &bytes[..hdr_end],
        nonce: &bytes[hdr_end..hdr_end + NONCE_LEN],
        body: &bytes[hdr_end + NONCE_LEN..],
    })
}

mod b64 {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&B64.encode(v))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        B64.decode(s).map_err(serde::de::Error::custom)
    }
}
