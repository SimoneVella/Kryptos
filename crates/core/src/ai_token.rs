//! Access codes for AI assistants (the MCP server). The app shows a code once
//! and keeps only its SHA-256, so the settings file never holds a usable code.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};

const PREFIX: &str = "kryptos_ai_";

/// A new random code: 256 bits from the OS CSPRNG.
pub fn generate() -> String {
    let mut b = [0u8; 32];
    OsRng.fill_bytes(&mut b);
    format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(b))
}

/// What the settings keep: the code's SHA-256, hex.
pub fn hash(code: &str) -> String {
    Sha256::digest(code.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether `code` matches the stored `hash`, in constant time.
pub fn verify(code: &str, stored: &str) -> bool {
    let h = hash(code);
    h.len() == stored.len() && stored.len() == 64 && h.bytes().zip(stored.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes() {
        let a = generate();
        assert!(a.starts_with(PREFIX) && a.len() > 50);
        assert_ne!(a, generate());
        let h = hash(&a);
        assert!(verify(&a, &h));
        assert!(!verify(&generate(), &h));
        assert!(!verify(&a, ""));
    }
}
