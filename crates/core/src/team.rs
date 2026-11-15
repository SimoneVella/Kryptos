//! Teams: a shared vault that people join with a personal key instead of a
//! shared password.
//!
//! Everyone keeps their own local vault, with their own master password. A team
//! has a separate random key, and the copy in the shared folder is encrypted with
//! that key and opens with no password at all. So nothing about a member's
//! password or fingerprint unlock is ever shared, and changing the team key
//! touches none of them.
//!
//! Each person has an X25519 identity, kept only in their local vault:
//! 1. the member sends the admin their public code (not secret);
//! 2. both compare the safety code by voice or in person, so that nobody who
//!    controls the channel can swap in their own key;
//! 3. the admin seals the team key to that member and puts it in the shared
//!    folder. The seal combines an ephemeral key with the admin's identity, so
//!    the member knows who it's from and pins the admin at join;
//! 4. removing someone makes a new team key, sealed only to those who stay.
//!    Key updates count only if sealed by the pinned admin, so a removed member,
//!    who still knows the old key, can't push a key of their own.
//!
//! Signatures: each identity also derives an Ed25519 key. It travels in the
//! member code (so the safety code covers it) and, for the admin, in the
//! invitation. Every member signs what they change in the shared copy, and the
//! admin signs the member list, so anyone can tell who changed an entry and a
//! member can't pass an edit off as someone else's.
//!
//! What someone already read can't be taken back: secrets they saw must be changed.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

use crate::crypto::{self, SecretKey, KEY_LEN};
use crate::error::{Error, Result};

const CODE_PREFIX: &str = "kryptos_member_";
const INVITE_INFO: &[u8] = b"kryptos/v1/team-invite";
const SIGN_INFO: &[u8] = b"kryptos/v1/team-sign";
const ITEM_DOMAIN: &[u8] = b"kryptos/v1/team-item";
const ROSTER_DOMAIN: &[u8] = b"kryptos/v1/team-roster";
/// Hex length of a public key.
const HEX_KEY: usize = KEY_LEN * 2;

/// 32 secret bytes, wiped on drop, hex in the (encrypted) vault.
#[derive(Clone)]
pub struct Key32(Zeroizing<[u8; KEY_LEN]>);

impl Key32 {
    pub fn random() -> Self {
        Self(Zeroizing::new(crypto::random_bytes::<KEY_LEN>()))
    }

    pub fn bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    pub fn secret_key(&self) -> Result<SecretKey> {
        SecretKey::from_slice(self.0.as_ref())
    }
}

impl std::fmt::Debug for Key32 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key32(<redacted>)")
    }
}

impl Serialize for Key32 {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&Zeroizing::new(hex(self.0.as_ref())))
    }
}

impl<'de> Deserialize<'de> for Key32 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = Zeroizing::new(String::deserialize(d)?);
        unhex(&s).map(|k| Self(Zeroizing::new(k))).ok_or_else(|| serde::de::Error::custom("bad key"))
    }
}

/// This vault's place in a team. Local: it never goes in the shared copy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: Uuid,
    pub name: String,
    pub key: Key32,
    /// Grows at each new team key.
    pub epoch: u64,
    /// The admin's public key (hex), pinned at join.
    pub admin: String,
    /// The admin's signing key (hex), pinned at join. Empty for a team made
    /// before signatures: then nothing is signed or checked.
    #[serde(default)]
    pub admin_sign: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// Public key, hex.
    pub public: String,
    pub name: String,
    pub added_at: u64,
    /// Signing key, hex (empty before signatures).
    #[serde(default)]
    pub sign: String,
    /// When the admin removed them, 0 while in the team. Removed members stay
    /// listed so that what they changed before still shows who did it.
    #[serde(default)]
    pub removed_at: u64,
}

/// Who signed a change to the shared copy, and the signature (both hex).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemSig {
    pub by: String,
    pub sig: String,
}

/// What an invitation carries, encrypted.
#[derive(Serialize, Deserialize)]
struct Payload {
    id: Uuid,
    name: String,
    key: Key32,
    epoch: u64,
    /// The admin's signing key.
    #[serde(default)]
    sign: String,
}

/// An opened invitation, before the user accepts it.
pub struct Invite {
    pub id: Uuid,
    pub name: String,
    pub key: Key32,
    pub epoch: u64,
    /// Who sealed it (hex).
    pub sender: String,
    /// The sender's signing key (hex), empty from versions before signatures.
    pub sign: String,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    sender: String,
    ephemeral: String,
    nonce: String,
    body: String,
}

pub fn public_of(secret: &Key32) -> String {
    hex(PublicKey::from(&StaticSecret::from(*secret.bytes())).as_bytes())
}

fn signing_key(secret: &Key32) -> Result<SigningKey> {
    let mut seed = Zeroizing::new([0u8; KEY_LEN]);
    Hkdf::<Sha256>::new(None, secret.bytes()).expand(SIGN_INFO, seed.as_mut()).map_err(|_| Error::Corrupt("key derivation"))?;
    Ok(SigningKey::from_bytes(&seed))
}

/// The signing key that goes with an identity, as hex.
pub fn sign_public_of(secret: &Key32) -> String {
    signing_key(secret).map(|k| hex(k.verifying_key().as_bytes())).unwrap_or_default()
}

/// The code a member sends the admin: both public keys. Nothing to protect.
pub fn member_code(public: &str, sign: &str) -> String {
    format!("{CODE_PREFIX}{public}{sign}")
}

/// The two public keys in a member code (exchange, signing), as hex. Codes
/// from before signatures carry only the first and are refused.
pub fn parse_code(code: &str) -> Result<(String, String)> {
    let bad = || Error::InvalidInput("team_bad_code");
    let keys = code.trim().strip_prefix(CODE_PREFIX).ok_or_else(bad)?;
    if keys.len() != HEX_KEY * 2 || !keys.is_ascii() {
        return Err(bad());
    }
    let (public, sign) = keys.split_at(HEX_KEY);
    let (public, sign) = (unhex(public).ok_or_else(bad)?, unhex(sign).ok_or_else(bad)?);
    // A weak (small-order) key would let others forge signatures in that name.
    if VerifyingKey::from_bytes(&sign).map_or(true, |k| k.is_weak()) {
        return Err(bad());
    }
    Ok((hex(&public), hex(&sign)))
}

/// Four groups of digits to read out loud (about 53 bits of a hash of both
/// keys, so swapping either one changes it).
pub fn safety_code(public: &str, sign: &str) -> String {
    let hash = Sha256::digest(format!("{public}{sign}").as_bytes());
    hash.chunks(2).take(4).map(|c| format!("{:04}", u16::from_be_bytes([c[0], c[1]]) % 10_000)).collect::<Vec<_>>().join(" ")
}

/// The file in the shared folder holding a member's sealed team key. Not named
/// `vault*`: those are taken for the provider's conflict copies.
pub fn invite_file(public: &str) -> String {
    format!("member-{}.key", &hex(&Sha256::digest(public.as_bytes()))[..32])
}

/// Identifies one exact invitation, so that what's accepted is what was compared.
pub fn ticket(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn shared_key(ephemeral: &[u8], fixed: &[u8], sender: &str, recipient: &str, epub: &str) -> Result<SecretKey> {
    let mut ikm = Zeroizing::new(Vec::with_capacity(KEY_LEN * 2));
    ikm.extend_from_slice(ephemeral);
    ikm.extend_from_slice(fixed);
    let salt = format!("{sender}{recipient}{epub}");
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    Hkdf::<Sha256>::new(Some(salt.as_bytes()), &ikm).expand(INVITE_INFO, out.as_mut()).map_err(|_| Error::Corrupt("key derivation"))?;
    SecretKey::from_slice(out.as_ref())
}

fn public_key(hex_key: &str) -> Result<PublicKey> {
    unhex(hex_key).map(PublicKey::from).ok_or(Error::InvalidInput("team_bad_code"))
}

/// Seals `team` to `recipient`, from `sender` (see the module docs). The
/// signing key it carries is the team's pinned one: empty for a team made
/// before signatures, which stays unsigned for everyone.
pub fn seal(sender: &Key32, recipient: &str, team: &Team) -> Result<Vec<u8>> {
    let their = public_key(recipient)?;
    let ours = StaticSecret::from(*sender.bytes());
    let ephemeral = StaticSecret::from(crypto::random_bytes::<KEY_LEN>());
    let (eph_shared, fixed_shared) = (ephemeral.diffie_hellman(&their), ours.diffie_hellman(&their));
    if !eph_shared.was_contributory() || !fixed_shared.was_contributory() {
        return Err(Error::InvalidInput("team_bad_code"));
    }
    let (from, epub) = (public_of(sender), hex(PublicKey::from(&ephemeral).as_bytes()));
    let key = shared_key(eph_shared.as_bytes(), fixed_shared.as_bytes(), &from, recipient, &epub)?;
    let plain = Zeroizing::new(
        serde_json::to_vec(&Payload { id: team.id, name: team.name.clone(), key: team.key.clone(), epoch: team.epoch, sign: team.admin_sign.clone() }).map_err(|_| Error::Corrupt("invite"))?,
    );
    let (nonce, body) = crypto::seal(&key, &plain, INVITE_INFO);
    serde_json::to_vec(&Envelope { sender: from, ephemeral: epub, nonce: hex(&nonce), body: hex(&body) }).map_err(|_| Error::Corrupt("invite"))
}

/// Opens an invitation sealed to `me`. Who it's from is for the caller to judge.
pub fn open(me: &Key32, bytes: &[u8]) -> Result<Invite> {
    let bad = Error::InvalidInput("team_bad_invite");
    let env: Envelope = serde_json::from_slice(bytes).map_err(|_| Error::InvalidInput("team_bad_invite"))?;
    let (sender, ephemeral) = (public_key(&env.sender)?, public_key(&env.ephemeral)?);
    let ours = StaticSecret::from(*me.bytes());
    let (eph_shared, fixed_shared) = (ours.diffie_hellman(&ephemeral), ours.diffie_hellman(&sender));
    if !eph_shared.was_contributory() || !fixed_shared.was_contributory() {
        return Err(bad);
    }
    let key = shared_key(eph_shared.as_bytes(), fixed_shared.as_bytes(), &env.sender, &public_of(me), &env.ephemeral)?;
    let (nonce, body) = (unhex_vec(&env.nonce).ok_or(Error::InvalidInput("team_bad_invite"))?, unhex_vec(&env.body).ok_or(Error::InvalidInput("team_bad_invite"))?);
    let plain = crypto::open(&key, &nonce, &body, INVITE_INFO).ok_or(Error::InvalidInput("team_bad_invite"))?;
    let p: Payload = serde_json::from_slice(&plain).map_err(|_| bad)?;
    Ok(Invite { id: p.id, name: p.name, key: p.key, epoch: p.epoch, sender: env.sender, sign: p.sign })
}

fn message(domain: &[u8], team: Uuid, label: &str, content: &[u8]) -> Vec<u8> {
    let mut m = Vec::with_capacity(domain.len() + 16 + label.len() + 33);
    m.extend_from_slice(domain);
    m.extend_from_slice(team.as_bytes());
    m.extend_from_slice(label.as_bytes());
    m.push(0);
    m.extend_from_slice(&Sha256::digest(content));
    m
}

fn sign_message(secret: &Key32, msg: &[u8]) -> Result<ItemSig> {
    let key = signing_key(secret)?;
    Ok(ItemSig { by: hex(key.verifying_key().as_bytes()), sig: hex(&key.sign(msg).to_bytes()) })
}

fn verify_message(s: &ItemSig, msg: &[u8]) -> bool {
    let (Some(by), Some(sig)) = (unhex(&s.by), unhex_vec(&s.sig).and_then(|b| <[u8; 64]>::try_from(b).ok())) else { return false };
    VerifyingKey::from_bytes(&by).is_ok_and(|k| !k.is_weak() && k.verify_strict(msg, &Signature::from_bytes(&sig)).is_ok())
}

/// Signs one item of the shared copy: `key` names it ("entry:<id>"...), `content`
/// is its serialized form, timestamps included.
pub fn sign_item(secret: &Key32, team: Uuid, key: &str, content: &[u8]) -> Result<ItemSig> {
    sign_message(secret, &message(ITEM_DOMAIN, team, key, content))
}

/// Whether `s` is a valid signature of this item by the key it names. Whether
/// that key belongs to a member is for the caller to check.
pub fn verify_item(s: &ItemSig, team: Uuid, key: &str, content: &[u8]) -> bool {
    verify_message(s, &message(ITEM_DOMAIN, team, key, content))
}

/// The admin's signature of the member list, at a version that only grows.
pub fn sign_roster(secret: &Key32, team: Uuid, version: u64, members: &[Member]) -> Result<ItemSig> {
    sign_message(secret, &roster_message(team, version, members)?)
}

pub fn verify_roster(s: &ItemSig, admin_sign: &str, team: Uuid, version: u64, members: &[Member]) -> bool {
    s.by == admin_sign && roster_message(team, version, members).is_ok_and(|m| verify_message(s, &m))
}

fn roster_message(team: Uuid, version: u64, members: &[Member]) -> Result<Vec<u8>> {
    let content = serde_json::to_vec(members).map_err(|_| Error::Corrupt("members"))?;
    Ok(message(ROSTER_DOMAIN, team, &version.to_string(), &content))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex_vec(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

fn unhex(s: &str) -> Option<[u8; KEY_LEN]> {
    unhex_vec(s)?.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team(key: Key32, admin: &Key32) -> Team {
        Team { id: Uuid::new_v4(), name: "Oikos".into(), key, epoch: 1, admin: public_of(admin), admin_sign: sign_public_of(admin) }
    }

    #[test]
    fn an_invite_opens_only_for_its_member() {
        let (admin, marco, eve) = (Key32::random(), Key32::random(), Key32::random());
        let t = team(Key32::random(), &admin);
        let sealed = seal(&admin, &public_of(&marco), &t).unwrap();
        let got = open(&marco, &sealed).unwrap();
        assert_eq!(got.key.bytes(), t.key.bytes());
        assert_eq!(got.sender, public_of(&admin));
        assert_eq!(got.sign, sign_public_of(&admin));
        assert!(open(&eve, &sealed).is_err());
    }

    #[test]
    fn a_changed_invite_or_a_forged_sender_is_refused() {
        let (admin, marco, eve) = (Key32::random(), Key32::random(), Key32::random());
        let t = team(Key32::random(), &admin);
        let mut env: Envelope = serde_json::from_slice(&seal(&admin, &public_of(&marco), &t).unwrap()).unwrap();
        env.sender = public_of(&eve);
        assert!(open(&marco, &serde_json::to_vec(&env).unwrap()).is_err());
    }

    #[test]
    fn low_order_keys_are_refused() {
        let admin = Key32::random();
        let t = team(Key32::random(), &admin);
        assert!(seal(&admin, &"00".repeat(32), &t).is_err());
    }

    #[test]
    fn codes_round_trip_and_bad_ones_are_refused() {
        let id = Key32::random();
        let (k, s) = (public_of(&id), sign_public_of(&id));
        assert_eq!(parse_code(&format!("  {}\n", member_code(&k, &s))).unwrap(), (k.clone(), s.clone()));
        assert!(parse_code("kryptos_member_abc").is_err());
        assert!(parse_code(&k).is_err());
        // A code from before signatures has no signing key.
        assert!(parse_code(&format!("{CODE_PREFIX}{k}")).is_err());
        assert_eq!(safety_code(&k, &s).len(), 19);
        assert!(invite_file(&k).starts_with("member-") && !invite_file(&k).starts_with("vault"));
    }

    #[test]
    fn a_weak_signing_key_is_refused() {
        let k = public_of(&Key32::random());
        // The identity point: a small-order key anyone can "sign" for.
        let mut weak = [0u8; KEY_LEN];
        weak[0] = 1;
        assert!(parse_code(&member_code(&k, &hex(&weak))).is_err());
    }

    #[test]
    fn swapping_only_the_signing_key_changes_the_safety_code() {
        let (a, b) = (Key32::random(), Key32::random());
        assert_ne!(safety_code(&public_of(&a), &sign_public_of(&a)), safety_code(&public_of(&a), &sign_public_of(&b)));
    }

    #[test]
    fn item_signatures_bind_signer_team_item_and_content() {
        let (marco, eve) = (Key32::random(), Key32::random());
        let team = Uuid::new_v4();
        let s = sign_item(&marco, team, "entry:1", b"content").unwrap();
        assert_eq!(s.by, sign_public_of(&marco));
        assert!(verify_item(&s, team, "entry:1", b"content"));
        assert!(!verify_item(&s, team, "entry:1", b"changed"));
        assert!(!verify_item(&s, team, "entry:2", b"content"));
        assert!(!verify_item(&s, Uuid::new_v4(), "entry:1", b"content"));
        // Claiming someone else signed it doesn't verify.
        let forged = ItemSig { by: sign_public_of(&marco), ..sign_item(&eve, team, "entry:1", b"content").unwrap() };
        assert!(!verify_item(&forged, team, "entry:1", b"content"));
    }
}
