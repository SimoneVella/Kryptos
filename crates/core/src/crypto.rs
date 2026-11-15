//! Primitives: Argon2id for password -> key, XChaCha20-Poly1305 for AEAD.
//!
//! XChaCha20 uses 192-bit nonces, so random nonces are safe for the lifetime
//! of a vault without any counter state.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{Error, Result};

pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 24;
pub const SALT_LEN: usize = 16;

/// Argon2id cost parameters, stored in the vault header so they can be raised later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl KdfParams {
    /// Desktop default: 256 MiB, 3 passes, 4 lanes (~0.5-1 s on a modern laptop).
    pub const DESKTOP: Self = Self { m_cost_kib: 256 * 1024, t_cost: 3, p_cost: 4 };
    /// Mobile default: 64 MiB keeps us clear of iOS extension memory limits.
    pub const MOBILE: Self = Self { m_cost_kib: 64 * 1024, t_cost: 3, p_cost: 4 };
    /// Only for tests: fast, and still accepted by `validate`.
    #[doc(hidden)]
    pub const TEST: Self = Self { m_cost_kib: 19 * 1024, t_cost: 2, p_cost: 1 };

    /// Rejects parameters below the OWASP floor (19 MiB, t=2) or absurdly high
    /// ones (a crafted header must not be able to OOM the process).
    pub fn validate(&self) -> Result<()> {
        let ok = (19 * 1024..=4 * 1024 * 1024).contains(&self.m_cost_kib)
            && (2..=64).contains(&self.t_cost)
            && (1..=16).contains(&self.p_cost);
        if ok { Ok(()) } else { Err(Error::InvalidKdfParams) }
    }
}

impl Default for KdfParams {
    fn default() -> Self {
        Self::DESKTOP
    }
}

/// A 256-bit key that is wiped from memory when dropped.
pub struct SecretKey(Zeroizing<[u8; KEY_LEN]>);

impl SecretKey {
    pub fn random() -> Self {
        let mut k = Zeroizing::new([0u8; KEY_LEN]);
        OsRng.fill_bytes(k.as_mut());
        Self(k)
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let arr: [u8; KEY_LEN] = bytes.try_into().map_err(|_| Error::Corrupt("bad key length"))?;
        Ok(Self(Zeroizing::new(arr)))
    }

    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey(<redacted>)")
    }
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    OsRng.fill_bytes(&mut b);
    b
}

pub fn derive_key(password: &[u8], salt: &[u8], params: &KdfParams) -> Result<SecretKey> {
    params.validate()?;
    let p = Params::new(params.m_cost_kib, params.t_cost, params.p_cost, Some(KEY_LEN))
        .map_err(|_| Error::InvalidKdfParams)?;
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, p)
        .hash_password_into(password, salt, out.as_mut())
        .map_err(|_| Error::InvalidKdfParams)?;
    Ok(SecretKey(out))
}

/// Derives a 256-bit key from high-entropy input material via HKDF-SHA256.
pub fn hkdf_key(ikm: &[u8], salt: &[u8], info: &[u8]) -> SecretKey {
    use hkdf::Hkdf;
    use sha2::Sha256;
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    hk.expand(info, out.as_mut()).expect("KEY_LEN is a valid HKDF-SHA256 length");
    SecretKey(out)
}

pub fn seal(key: &SecretKey, plaintext: &[u8], aad: &[u8]) -> ([u8; NONCE_LEN], Vec<u8>) {
    let cipher = XChaCha20Poly1305::new(key.as_bytes().into());
    let nonce = random_bytes::<NONCE_LEN>();
    let ct = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plaintext, aad })
        .expect("XChaCha20-Poly1305 encryption cannot fail for in-memory buffers");
    (nonce, ct)
}

/// Returns `None` on authentication failure; callers map that to the right error.
pub fn open(key: &SecretKey, nonce: &[u8], ciphertext: &[u8], aad: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
    if nonce.len() != NONCE_LEN {
        return None;
    }
    let cipher = XChaCha20Poly1305::new(key.as_bytes().into());
    cipher
        .decrypt(XNonce::from_slice(nonce), Payload { msg: ciphertext, aad })
        .ok()
        .map(Zeroizing::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip_and_tamper() {
        let k = SecretKey::random();
        let (n, mut ct) = seal(&k, b"hello", b"aad");
        assert_eq!(open(&k, &n, &ct, b"aad").unwrap().as_slice(), b"hello");
        assert!(open(&k, &n, &ct, b"other").is_none());
        ct[0] ^= 1;
        assert!(open(&k, &n, &ct, b"aad").is_none());
    }

    #[test]
    fn kdf_is_deterministic_and_salted() {
        let a = derive_key(b"pw", &[1; SALT_LEN], &KdfParams::TEST).unwrap();
        let b = derive_key(b"pw", &[1; SALT_LEN], &KdfParams::TEST).unwrap();
        let c = derive_key(b"pw", &[2; SALT_LEN], &KdfParams::TEST).unwrap();
        assert_eq!(a.as_bytes(), b.as_bytes());
        assert_ne!(a.as_bytes(), c.as_bytes());
    }

    #[test]
    fn rejects_weak_params() {
        let weak = KdfParams { m_cost_kib: 1024, t_cost: 1, p_cost: 1 };
        assert!(matches!(derive_key(b"pw", &[0; SALT_LEN], &weak), Err(Error::InvalidKdfParams)));
    }

    #[test]
    fn hkdf_key_is_domain_separated() {
        let a = hkdf_key(b"ikm1", b"salt1", b"info1");
        let a2 = hkdf_key(b"ikm1", b"salt1", b"info1");
        assert_eq!(a.as_bytes(), a2.as_bytes());
        assert_ne!(a.as_bytes(), hkdf_key(b"ikm2", b"salt1", b"info1").as_bytes());
        assert_ne!(a.as_bytes(), hkdf_key(b"ikm1", b"salt2", b"info1").as_bytes());
        assert_ne!(a.as_bytes(), hkdf_key(b"ikm1", b"salt1", b"info2").as_bytes());
    }
}
