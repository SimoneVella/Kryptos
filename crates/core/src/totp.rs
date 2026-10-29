//! Time-based one-time passwords (RFC 6238), the 6-digit "2FA codes".
//!
//! An entry stores what the site gave the user: either the bare base32 secret
//! or the `otpauth://totp/...` link inside its QR code. The code is computed
//! locally from the secret and the clock, so it needs no network.

use hmac::digest::core_api::BlockSizeUser;
use hmac::digest::Digest;
use hmac::{Mac, SimpleHmac};
use zeroize::Zeroizing;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Sha1,
    Sha256,
    Sha512,
}

pub struct Totp {
    secret: Zeroizing<Vec<u8>>,
    pub algorithm: Algorithm,
    pub digits: u32,
    /// Seconds each code lasts.
    pub period: u64,
}

const INVALID: Error = Error::InvalidInput("invalid_totp");

impl Totp {
    /// Parses a base32 secret (spaces, dashes and case are ignored) or an
    /// `otpauth://totp/...` link.
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        if !s.to_ascii_lowercase().starts_with("otpauth://") {
            return Self::new(base32_decode(s).ok_or(INVALID)?, Algorithm::Sha1, 6, 30);
        }
        let url = url::Url::parse(s).map_err(|_| INVALID)?;
        if !url.host_str().is_some_and(|h| h.eq_ignore_ascii_case("totp")) {
            return Err(INVALID); // HOTP (counter based) is not supported
        }
        let (mut secret, mut algorithm, mut digits, mut period) = (None, Algorithm::Sha1, 6, 30);
        for (k, v) in url.query_pairs() {
            match k.to_ascii_lowercase().as_str() {
                "secret" => secret = Some(base32_decode(&v).ok_or(INVALID)?),
                "algorithm" => {
                    algorithm = match v.to_ascii_uppercase().as_str() {
                        "SHA1" => Algorithm::Sha1,
                        "SHA256" => Algorithm::Sha256,
                        "SHA512" => Algorithm::Sha512,
                        _ => return Err(INVALID),
                    }
                }
                "digits" => digits = v.parse().map_err(|_| INVALID)?,
                "period" => period = v.parse().map_err(|_| INVALID)?,
                _ => {}
            }
        }
        Self::new(secret.ok_or(INVALID)?, algorithm, digits, period)
    }

    fn new(secret: Vec<u8>, algorithm: Algorithm, digits: u32, period: u64) -> Result<Self> {
        let secret = Zeroizing::new(secret);
        if secret.is_empty() || !(6..=8).contains(&digits) || !(1..=300).contains(&period) {
            return Err(INVALID);
        }
        Ok(Self { secret, algorithm, digits, period })
    }

    /// The code valid at `unix` seconds.
    pub fn code_at(&self, unix: u64) -> String {
        let counter = (unix / self.period).to_be_bytes();
        let mac = match self.algorithm {
            Algorithm::Sha1 => hmac::<sha1::Sha1>(&self.secret, &counter),
            Algorithm::Sha256 => hmac::<sha2::Sha256>(&self.secret, &counter),
            Algorithm::Sha512 => hmac::<sha2::Sha512>(&self.secret, &counter),
        };
        // Dynamic truncation (RFC 4226, section 5.3).
        let offset = (mac[mac.len() - 1] & 0x0f) as usize;
        let bin = u32::from_be_bytes([mac[offset] & 0x7f, mac[offset + 1], mac[offset + 2], mac[offset + 3]]);
        format!("{:0width$}", bin % 10u32.pow(self.digits), width = self.digits as usize)
    }

    /// The current code and how many seconds it stays valid.
    pub fn now(&self) -> (String, u64) {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        (self.code_at(t), self.period - t % self.period)
    }
}

fn hmac<D: Digest + BlockSizeUser>(key: &[u8], msg: &[u8]) -> Zeroizing<Vec<u8>> {
    let mut m = SimpleHmac::<D>::new_from_slice(key).expect("HMAC takes keys of any length");
    m.update(msg);
    Zeroizing::new(m.finalize().into_bytes().to_vec())
}

/// RFC 4648 base32, as used by authenticator apps: padding optional, case,
/// spaces and dashes ignored.
fn base32_decode(s: &str) -> Option<Vec<u8>> {
    let (mut out, mut buf, mut bits) = (Vec::new(), 0u64, 0u32);
    for c in s.chars().filter(|c| !matches!(c, ' ' | '-' | '=')) {
        let v = match c.to_ascii_uppercase() {
            c @ 'A'..='Z' => c as u64 - 'A' as u64,
            c @ '2'..='7' => c as u64 - '2' as u64 + 26,
            _ => return None,
        };
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b32(bytes: &[u8]) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
        let (mut out, mut buf, mut bits) = (String::new(), 0u32, 0);
        for &b in bytes {
            buf = (buf << 8) | b as u32;
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                out.push(A[((buf >> bits) & 31) as usize] as char);
            }
        }
        if bits > 0 {
            out.push(A[((buf << (5 - bits)) & 31) as usize] as char);
        }
        out
    }

    /// Test vectors from RFC 6238, appendix B (8 digits).
    #[test]
    fn rfc6238_vectors() {
        let cases = [
            ("SHA1", &b"12345678901234567890"[..], [(59, "94287082"), (1111111109, "07081804"), (20000000000, "65353130")]),
            ("SHA256", &b"12345678901234567890123456789012"[..], [(59, "46119246"), (1111111109, "68084774"), (20000000000, "77737706")]),
            (
                "SHA512",
                &b"1234567890123456789012345678901234567890123456789012345678901234"[..],
                [(59, "90693936"), (1111111109, "25091201"), (20000000000, "47863826")],
            ),
        ];
        for (alg, key, vectors) in cases {
            let t = Totp::parse(&format!("otpauth://totp/Test?secret={}&algorithm={alg}&digits=8", b32(key))).unwrap();
            for (time, code) in vectors {
                assert_eq!(t.code_at(time), code, "{alg} at {time}");
            }
        }
    }

    #[test]
    fn bare_secret_and_link() {
        // The secret most sites show, with spaces and lower case.
        let t = Totp::parse("jbsw y3dp ehpk 3pxp").unwrap();
        assert_eq!((t.digits, t.period, t.algorithm), (6, 30, Algorithm::Sha1));
        assert_eq!(t.code_at(59).len(), 6);
        let link = Totp::parse("otpauth://totp/GitHub:me?secret=JBSWY3DPEHPK3PXP&issuer=GitHub").unwrap();
        assert_eq!(link.code_at(1_700_000_000), t.code_at(1_700_000_000));
    }

    #[test]
    fn rejects_garbage() {
        for bad in ["", "not base32!", "otpauth://hotp/x?secret=JBSWY3DP", "otpauth://totp/x", "otpauth://totp/x?secret=JBSWY3DP&digits=12"] {
            assert!(Totp::parse(bad).is_err(), "{bad}");
        }
    }
}
