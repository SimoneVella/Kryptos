use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{self, KdfParams, SecretKey, SALT_LEN};
use crate::entry::{now, Entry, EntryInput, EntrySummary};
use crate::error::{Error, Result};
use crate::format::{self, Header, WRAP_AAD};

#[derive(Debug, Default, Serialize, Deserialize)]
struct VaultData {
    #[serde(default)]
    entries: Vec<Entry>,
}

/// A decrypted vault held in RAM. Dropping it wipes keys and entries.
pub struct UnlockedVault {
    header: Header,
    key: SecretKey,
    data: VaultData,
}

impl UnlockedVault {
    /// Creates a brand-new empty vault protected by `password`.
    pub fn create(password: &str, kdf: KdfParams) -> Result<Self> {
        check_password(password)?;
        let key = SecretKey::random();
        let header = wrap_header(Uuid::new_v4(), &key, password, kdf)?;
        Ok(Self { header, key, data: VaultData::default() })
    }

    /// Decrypts a vault file's bytes with the master password.
    pub fn unlock(bytes: &[u8], password: &str) -> Result<Self> {
        let parsed = format::decode(bytes)?;
        let h = &parsed.header;
        let kek = crypto::derive_key(password.as_bytes(), &h.salt, &h.kdf)?;
        let raw = crypto::open(&kek, &h.wrap_nonce, &h.wrapped_key, WRAP_AAD).ok_or(Error::WrongPassword)?;
        let key = SecretKey::from_slice(&raw)?;
        Self::open_body(parsed, key)
    }

    /// Unlocks with an already-unwrapped vault key (biometric path).
    pub fn unlock_with_key(bytes: &[u8], raw_key: &[u8]) -> Result<Self> {
        let parsed = format::decode(bytes)?;
        Self::open_body(parsed, SecretKey::from_slice(raw_key)?)
    }

    fn open_body(parsed: format::ParsedFile<'_>, key: SecretKey) -> Result<Self> {
        let plain = crypto::open(&key, parsed.nonce, parsed.body, parsed.aad)
            .ok_or(Error::Corrupt("body authentication failed"))?;
        let data: VaultData = serde_json::from_slice(&plain).map_err(|_| Error::Corrupt("bad body"))?;
        Ok(Self { header: parsed.header, key, data })
    }

    /// Serializes and encrypts the vault with a fresh nonce.
    pub fn to_bytes(&self) -> Vec<u8> {
        let plain = Zeroizing::new(serde_json::to_vec(&self.data).expect("vault serialization is infallible"));
        let aad = format::encode_prefix(&self.header);
        let (nonce, ct) = crypto::seal(&self.key, &plain, &aad);
        format::encode(&self.header, &nonce, &ct)
    }

    /// Re-wraps the vault key under a new password. The body key is unchanged.
    pub fn change_password(&mut self, new_password: &str, kdf: KdfParams) -> Result<()> {
        check_password(new_password)?;
        self.header = wrap_header(self.header.vault_id, &self.key, new_password, kdf)?;
        Ok(())
    }

    /// Raw vault key, for sealing into the OS keychain / secure enclave.
    pub fn raw_key(&self) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(self.key.as_bytes().to_vec())
    }

    pub fn vault_id(&self) -> Uuid {
        self.header.vault_id
    }

    // ---- entries ----

    pub fn list(&self) -> Vec<EntrySummary> {
        let mut v: Vec<_> = self.data.entries.iter().map(EntrySummary::from).collect();
        v.sort_by(|a, b| b.favorite.cmp(&a.favorite).then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())));
        v
    }

    pub fn entries(&self) -> &[Entry] {
        &self.data.entries
    }

    pub fn get(&self, id: Uuid) -> Result<&Entry> {
        self.data.entries.iter().find(|e| e.id == id).ok_or(Error::NotFound)
    }

    pub fn add(&mut self, input: EntryInput) -> Result<Uuid> {
        validate_input(&input)?;
        let t = now();
        let id = Uuid::new_v4();
        self.data.entries.push(Entry {
            id,
            title: input.title.trim().to_owned(),
            username: input.username.clone(),
            password: input.password.clone(),
            urls: clean_urls(&input.urls),
            notes: input.notes.clone(),
            favorite: input.favorite,
            created_at: t,
            updated_at: t,
        });
        Ok(id)
    }

    pub fn update(&mut self, id: Uuid, input: EntryInput) -> Result<()> {
        validate_input(&input)?;
        let e = self.data.entries.iter_mut().find(|e| e.id == id).ok_or(Error::NotFound)?;
        e.title = input.title.trim().to_owned();
        e.username = input.username.clone();
        e.password = input.password.clone();
        e.urls = clean_urls(&input.urls);
        e.notes = input.notes.clone();
        e.favorite = input.favorite;
        e.updated_at = now();
        Ok(())
    }

    /// Adds entries, skipping ones already present (same site, username and password).
    /// Returns (added, skipped).
    pub fn import(&mut self, inputs: Vec<EntryInput>) -> (usize, usize) {
        let key = |urls: &[String], user: &str, pw: &str| {
            let host = urls.first().and_then(|u| crate::import::host_of(u)).unwrap_or_default();
            (host, user.to_owned(), pw.to_owned())
        };
        let mut seen: std::collections::HashSet<_> =
            self.data.entries.iter().map(|e| key(&e.urls, &e.username, &e.password)).collect();
        let (mut added, mut skipped) = (0, 0);
        for input in inputs {
            if !seen.insert(key(&input.urls, &input.username, &input.password)) || self.add(input).is_err() {
                skipped += 1;
            } else {
                added += 1;
            }
        }
        (added, skipped)
    }

    pub fn delete(&mut self, id: Uuid) -> Result<()> {
        let before = self.data.entries.len();
        self.data.entries.retain(|e| e.id != id);
        if self.data.entries.len() == before { Err(Error::NotFound) } else { Ok(()) }
    }
}

fn wrap_header(vault_id: Uuid, key: &SecretKey, password: &str, kdf: KdfParams) -> Result<Header> {
    let salt = crypto::random_bytes::<SALT_LEN>();
    let kek = crypto::derive_key(password.as_bytes(), &salt, &kdf)?;
    let (wrap_nonce, wrapped_key) = crypto::seal(&kek, key.as_bytes(), WRAP_AAD);
    Ok(Header { vault_id, kdf, salt: salt.to_vec(), wrap_nonce: wrap_nonce.to_vec(), wrapped_key })
}

fn check_password(pw: &str) -> Result<()> {
    if pw.chars().count() < 8 {
        return Err(Error::InvalidInput("master password must be at least 8 characters"));
    }
    Ok(())
}

fn validate_input(input: &EntryInput) -> Result<()> {
    if input.title.trim().is_empty() {
        return Err(Error::InvalidInput("title is required"));
    }
    Ok(())
}

fn clean_urls(urls: &[String]) -> Vec<String> {
    urls.iter().map(|u| u.trim()).filter(|u| !u.is_empty()).map(str::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(title: &str) -> EntryInput {
        EntryInput {
            title: title.into(),
            username: "me@example.com".into(),
            password: "s3cret!".into(),
            urls: vec!["https://example.com".into(), " ".into()],
            notes: String::new(),
            favorite: false,
        }
    }

    #[test]
    fn create_save_unlock_roundtrip() {
        let mut v = UnlockedVault::create("correct horse", KdfParams::TEST).unwrap();
        let id = v.add(input("Example")).unwrap();
        let bytes = v.to_bytes();

        let v2 = UnlockedVault::unlock(&bytes, "correct horse").unwrap();
        let e = v2.get(id).unwrap();
        assert_eq!(e.password, "s3cret!");
        assert_eq!(e.urls, vec!["https://example.com"]);
        assert!(matches!(UnlockedVault::unlock(&bytes, "wrong password"), Err(Error::WrongPassword)));
    }

    #[test]
    fn ciphertext_does_not_leak_plaintext() {
        let mut v = UnlockedVault::create("correct horse", KdfParams::TEST).unwrap();
        v.add(input("VerySpecificTitle")).unwrap();
        let bytes = v.to_bytes();
        let hay = String::from_utf8_lossy(&bytes);
        assert!(!hay.contains("VerySpecificTitle"));
        assert!(!hay.contains("s3cret!"));
    }

    #[test]
    fn header_tampering_is_detected() {
        let v = UnlockedVault::create("correct horse", KdfParams::TEST).unwrap();
        let bytes = v.to_bytes();
        let raw_key = v.raw_key();
        // Flip a byte inside the JSON header (the vault_id); the key still unwraps
        // with the right key but the body AAD no longer matches.
        let mut t = bytes.clone();
        let pos = String::from_utf8_lossy(&t).find("vault_id").unwrap() + 12;
        t[pos] = if t[pos] == b'a' { b'b' } else { b'a' };
        assert!(UnlockedVault::unlock_with_key(&t, &raw_key).is_err());
    }

    #[test]
    fn change_password_keeps_data() {
        let mut v = UnlockedVault::create("old password", KdfParams::TEST).unwrap();
        let id = v.add(input("X")).unwrap();
        v.change_password("new password", KdfParams::TEST).unwrap();
        let bytes = v.to_bytes();
        assert!(UnlockedVault::unlock(&bytes, "old password").is_err());
        assert_eq!(UnlockedVault::unlock(&bytes, "new password").unwrap().get(id).unwrap().title, "X");
    }

    #[test]
    fn import_skips_duplicates() {
        let mut v = UnlockedVault::create("correct horse", KdfParams::TEST).unwrap();
        v.add(input("Example")).unwrap();
        let mut fresh = EntryInput::default();
        fresh.title = "New".into();
        fresh.password = "x".into();
        let (added, skipped) = v.import(vec![input("Dup"), input("Dup again"), fresh]);
        assert_eq!((added, skipped), (1, 2));
        assert_eq!(v.list().len(), 2);
    }

    #[test]
    fn update_and_delete() {
        let mut v = UnlockedVault::create("correct horse", KdfParams::TEST).unwrap();
        let id = v.add(input("A")).unwrap();
        v.update(id, input("B")).unwrap();
        assert_eq!(v.list()[0].title, "B");
        v.delete(id).unwrap();
        assert!(v.list().is_empty());
        assert!(matches!(v.delete(id), Err(Error::NotFound)));
    }
}
