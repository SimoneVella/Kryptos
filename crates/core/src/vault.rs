use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{self, KdfParams, SecretKey, SALT_LEN};
use crate::entry::{now, BackupCode, Entry, EntryInput, EntrySummary};
use crate::error::{Error, Result};
use crate::format::{self, Header, WRAP_AAD};

#[derive(Debug, Default, Serialize, Deserialize)]
struct VaultData {
    #[serde(default)]
    entries: Vec<Entry>,
    /// Deleted entries (id and time only), so that merging with an older copy of
    /// the vault does not bring them back. Missing in vaults written before sync.
    #[serde(default)]
    tombstones: Vec<Tombstone>,
    /// When the master password last changed (Unix seconds), so that copies of
    /// the same vault synced through a cloud folder adopt the newest password.
    #[serde(default)]
    password_changed_at: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct Tombstone {
    id: Uuid,
    deleted_at: u64,
}

/// What a merge changed in the receiving vault.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MergeReport {
    /// Entries only the other copy had.
    pub added: usize,
    /// Entries the other copy had edited more recently.
    pub updated: usize,
    /// Entries the other copy had deleted after their last edit here.
    pub deleted: usize,
}

/// What [`UnlockedVault::sync_from`] did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SyncReport {
    #[serde(flatten)]
    pub merge: MergeReport,
    /// The other copy had a newer master password, now used here too.
    pub password_changed: bool,
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
        self.data.password_changed_at = now();
        Ok(())
    }

    /// Raw vault key, for sealing into the OS keychain / secure enclave.
    pub fn raw_key(&self) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(self.key.as_bytes().to_vec())
    }

    pub fn vault_id(&self) -> Uuid {
        self.header.vault_id
    }

    /// Whether `bytes` are a copy of this same vault (same id), which then opens
    /// with this vault's key, without the password.
    pub fn is_copy(&self, bytes: &[u8]) -> bool {
        format::decode(bytes).is_ok_and(|p| p.header.vault_id == self.header.vault_id)
    }

    /// Merges another copy of this same vault (cloud sync): no password needed,
    /// both share the vault key. If that copy got a newer master password, this
    /// one adopts it. Fails with `OtherVault` for a different vault (see `join`).
    pub fn sync_from(&mut self, bytes: &[u8]) -> Result<SyncReport> {
        if !self.is_copy(bytes) {
            return Err(Error::InvalidInput("other_vault"));
        }
        let other = Self::unlock_with_key(bytes, self.key.as_bytes())?;
        let merge = self.merge_from(&other);
        let password_changed = other.data.password_changed_at > self.data.password_changed_at;
        if password_changed {
            self.header = other.header.clone();
            self.data.password_changed_at = other.data.password_changed_at;
        }
        Ok(SyncReport { merge, password_changed })
    }

    /// Joins a different vault (the one already in the cloud folder): opens it with
    /// its own master password and brings this vault's entries into it. The result
    /// replaces this vault, so from then on both devices share one vault, one key
    /// and one master password. The report counts what this vault brought in.
    pub fn join(&self, bytes: &[u8], password: &str) -> Result<(UnlockedVault, MergeReport)> {
        let mut shared = Self::unlock(bytes, password)?;
        let report = shared.merge_from(self);
        Ok((shared, report))
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
            totp: input.totp.trim().to_owned(),
            backup_codes: clean_codes(&input.backup_codes),
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
        e.totp = input.totp.trim().to_owned();
        e.backup_codes = clean_codes(&input.backup_codes);
        e.updated_at = now();
        Ok(())
    }

    /// A login the user typed into a site or app and asked to keep (Android's
    /// "Save to Kryptos?"). Updates the password of the entry for that site with
    /// the same username, else adds a new entry titled `title`.
    pub fn save_login(&mut self, target: &str, title: &str, username: &str, password: &str) -> Result<SaveOutcome> {
        if password.is_empty() {
            return Err(Error::InvalidInput("password_required"));
        }
        let existing = self
            .data
            .entries
            .iter()
            .filter(|e| crate::matching::target_matches(e, target) && e.username.eq_ignore_ascii_case(username))
            .max_by_key(|e| e.updated_at);
        if let Some(e) = existing {
            if e.password == password {
                return Ok(SaveOutcome::Unchanged(e.id));
            }
            let id = e.id;
            let input = EntryInput {
                title: e.title.clone(),
                username: e.username.clone(),
                password: password.to_owned(),
                urls: e.urls.clone(),
                notes: e.notes.clone(),
                favorite: e.favorite,
                totp: e.totp.clone(),
                backup_codes: e.backup_codes.clone(),
            };
            self.update(id, input)?;
            return Ok(SaveOutcome::Updated(id));
        }
        let title = if title.trim().is_empty() { target } else { title };
        let id = self.add(EntryInput {
            title: title.to_owned(),
            username: username.to_owned(),
            password: password.to_owned(),
            urls: vec![target.to_owned()],
            notes: String::new(),
            favorite: false,
            totp: String::new(),
            backup_codes: Vec::new(),
        })?;
        Ok(SaveOutcome::Added(id))
    }

    /// Brings in the 2FA accounts of an authenticator export: each goes to the
    /// entry of the same service and account that has no 2FA yet, or becomes a
    /// new entry (no password). Keys Kryptos already has are skipped.
    pub fn import_otp(&mut self, accounts: &[crate::otp_import::OtpAccount]) -> OtpImportReport {
        use crate::totp::Totp;
        let norm = |s: &str| s.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_ascii_lowercase();
        let mut report = OtpImportReport::default();
        for acc in accounts {
            let Ok(new) = Totp::parse(&acc.uri) else { continue };
            let known = self.data.entries.iter().any(|e| Totp::parse(&e.totp).is_ok_and(|t| t.same_secret(&new)));
            if known {
                report.skipped += 1;
                continue;
            }
            let service = norm(&acc.issuer);
            // The issuer must be the entry's name or its site's name ("github" for
            // github.com, "google" for accounts.google.com), exactly: a lookalike or
            // short issuer must not attach its code to an unrelated login.
            let same_service = |e: &Entry| {
                !service.is_empty()
                    && (norm(&e.title) == service || e.urls.iter().any(|u| site_name(u).is_some_and(|n| norm(&n) == service)))
            };
            let same_account = |e: &Entry| acc.account.is_empty() || e.username.is_empty() || e.username.eq_ignore_ascii_case(&acc.account);
            if let Some(e) = self.data.entries.iter_mut().find(|e| e.totp.is_empty() && same_service(e) && same_account(e)) {
                e.totp = acc.uri.clone();
                e.updated_at = now();
                report.linked += 1;
                continue;
            }
            let title = [acc.issuer.trim(), acc.account.trim()].into_iter().find(|t| !t.is_empty()).unwrap_or("2FA");
            let added = self.add(EntryInput {
                title: title.to_owned(),
                username: acc.account.clone(),
                password: String::new(),
                urls: Vec::new(),
                notes: String::new(),
                favorite: false,
                totp: acc.uri.clone(),
                backup_codes: Vec::new(),
            });
            if added.is_ok() {
                report.added += 1;
            }
        }
        report
    }

    /// Marks backup code number `index` of an entry as used (or not). A change
    /// like any other: it updates the entry's time, so it syncs to other devices.
    pub fn set_backup_code_used(&mut self, id: Uuid, index: usize, used: bool) -> Result<()> {
        let e = self.data.entries.iter_mut().find(|e| e.id == id).ok_or(Error::NotFound)?;
        let c = e.backup_codes.get_mut(index).ok_or(Error::NotFound)?;
        if c.used != used {
            c.used = used;
            e.updated_at = now();
        }
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
        if self.data.entries.len() == before {
            return Err(Error::NotFound);
        }
        self.data.tombstones.retain(|t| t.id != id);
        self.data.tombstones.push(Tombstone { id, deleted_at: now() });
        Ok(())
    }

    // ---- sync ----

    /// Decrypts another copy of the vault (a backup, or one received from another
    /// device) with its own master password and merges it into this one.
    pub fn merge_bytes(&mut self, bytes: &[u8], password: &str) -> Result<MergeReport> {
        let other = UnlockedVault::unlock(bytes, password)?;
        Ok(self.merge_from(&other))
    }

    /// Merges another copy entry by entry: the most recent change wins, where a
    /// deletion counts as a change (at equal times the deletion wins). An entry
    /// edited after it was deleted elsewhere comes back. Entries that only differ
    /// by id but have the same site, username and password (e.g. the same CSV
    /// imported on both devices) are not duplicated.
    pub fn merge_from(&mut self, other: &UnlockedVault) -> MergeReport {
        let mut report = MergeReport::default();

        // Latest deletion time per id, from both sides.
        let mut deleted: std::collections::HashMap<Uuid, u64> = std::collections::HashMap::new();
        for t in self.data.tombstones.iter().chain(&other.data.tombstones) {
            let d = deleted.entry(t.id).or_insert(0);
            *d = (*d).max(t.deleted_at);
        }
        let is_deleted = |id: Uuid, updated_at: u64| deleted.get(&id).is_some_and(|&d| d >= updated_at);

        let before = self.data.entries.len();
        self.data.entries.retain(|e| !is_deleted(e.id, e.updated_at));
        report.deleted = before - self.data.entries.len();

        for theirs in &other.data.entries {
            if is_deleted(theirs.id, theirs.updated_at) {
                continue;
            }
            match self.data.entries.iter().position(|e| e.id == theirs.id) {
                Some(i) if theirs.updated_at > self.data.entries[i].updated_at => {
                    self.data.entries[i] = theirs.clone();
                    report.updated += 1;
                }
                Some(_) => {}
                None if self.data.entries.iter().any(|e| same_login(e, theirs)) => {}
                None => {
                    self.data.entries.push(theirs.clone());
                    report.added += 1;
                }
            }
        }

        let mut tombstones: Vec<_> = deleted.into_iter().map(|(id, deleted_at)| Tombstone { id, deleted_at }).collect();
        tombstones.sort_by_key(|t| t.deleted_at);
        self.data.tombstones = tombstones;
        report
    }
}

fn same_login(a: &Entry, b: &Entry) -> bool {
    let host = |e: &Entry| e.urls.first().and_then(|u| crate::import::host_of(u)).unwrap_or_default();
    a.username == b.username && a.password == b.password && host(a) == host(b)
}

fn wrap_header(vault_id: Uuid, key: &SecretKey, password: &str, kdf: KdfParams) -> Result<Header> {
    let salt = crypto::random_bytes::<SALT_LEN>();
    let kek = crypto::derive_key(password.as_bytes(), &salt, &kdf)?;
    let (wrap_nonce, wrapped_key) = crypto::seal(&kek, key.as_bytes(), WRAP_AAD);
    Ok(Header { vault_id, kdf, salt: salt.to_vec(), wrap_nonce: wrap_nonce.to_vec(), wrapped_key })
}

fn check_password(pw: &str) -> Result<()> {
    if pw.chars().count() < 8 {
        return Err(Error::InvalidInput("password_too_short"));
    }
    Ok(())
}

/// What [`UnlockedVault::import_otp`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct OtpImportReport {
    /// Added to an existing entry.
    pub linked: usize,
    /// New entries.
    pub added: usize,
    /// Already in Kryptos.
    pub skipped: usize,
}

/// What [`UnlockedVault::save_login`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveOutcome {
    Added(Uuid),
    Updated(Uuid),
    Unchanged(Uuid),
}

fn validate_input(input: &EntryInput) -> Result<()> {
    if input.title.trim().is_empty() {
        return Err(Error::InvalidInput("title_required"));
    }
    if !input.totp.trim().is_empty() {
        crate::totp::Totp::parse(&input.totp)?;
    }
    Ok(())
}

/// The name of a URL's registrable domain: "google" for https://accounts.google.com,
/// "bbc" for bbc.co.uk.
fn site_name(url: &str) -> Option<String> {
    let host = crate::import::host_of(url)?.to_ascii_lowercase();
    let domain = psl::domain_str(&host)?;
    domain.split('.').next().map(str::to_owned)
}

/// Trimmed, without blanks or repeats (the first copy keeps its "used" mark).
fn clean_codes(codes: &[BackupCode]) -> Vec<BackupCode> {
    let mut out: Vec<BackupCode> = Vec::new();
    for c in codes {
        let code = c.code.trim();
        if !code.is_empty() && !out.iter().any(|o| o.code == code) {
            out.push(BackupCode { code: code.to_owned(), used: c.used });
        }
    }
    out
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
            totp: String::new(),
            backup_codes: Vec::new(),
        }
    }

    #[test]
    fn cloud_sync_same_vault_and_join() {
        // Phone and computer share one vault through the cloud folder.
        let mut phone = UnlockedVault::create("correct horse", KdfParams::TEST).unwrap();
        let mut computer = UnlockedVault::unlock(&phone.to_bytes(), "correct horse").unwrap();
        let a = phone.add(input("From phone")).unwrap();
        let mut b = input("From computer");
        b.urls = vec!["https://other.test".into()];
        computer.add(b).unwrap();
        let r = computer.sync_from(&phone.to_bytes()).unwrap();
        assert_eq!((r.merge.added, r.password_changed), (1, false));
        assert!(computer.get(a).is_ok());

        // A password change on the phone reaches the computer with the next sync.
        phone.change_password("battery staple", KdfParams::TEST).unwrap();
        assert!(computer.sync_from(&phone.to_bytes()).unwrap().password_changed);
        assert!(UnlockedVault::unlock(&computer.to_bytes(), "battery staple").is_ok());
        assert!(UnlockedVault::unlock(&computer.to_bytes(), "correct horse").is_err());

        // A separate vault can't be synced silently: it must join with its password.
        let mut laptop = UnlockedVault::create("laptop pass", KdfParams::TEST).unwrap();
        let mut c = input("From laptop");
        c.urls = vec!["https://laptop.test".into()];
        laptop.add(c).unwrap();
        assert!(matches!(laptop.sync_from(&phone.to_bytes()), Err(Error::InvalidInput("other_vault"))));
        let cloud = phone.to_bytes();
        assert!(laptop.join(&cloud, "wrong").is_err());
        let (joined, r) = laptop.join(&cloud, "battery staple").unwrap();
        assert_eq!(joined.vault_id(), phone.vault_id());
        assert_eq!(r.added, 1); // the laptop's own entry came along
        // The joined device now syncs silently with the others.
        assert!(phone.sync_from(&joined.to_bytes()).is_ok());
    }

    #[test]
    fn otp_import_links_adds_and_skips() {
        use crate::otp_import::OtpAccount;
        let mut v = UnlockedVault::create("correct horse", KdfParams::TEST).unwrap();
        let mut gh = input("GitHub");
        gh.urls = vec!["https://github.com".into()];
        let gh = v.add(gh).unwrap();
        let acc = |issuer: &str, account: &str, secret: &str| OtpAccount {
            issuer: issuer.into(),
            account: account.into(),
            uri: format!("otpauth://totp/x?secret={secret}"),
        };
        let accounts = [
            acc("GitHub", "me@example.com", "JBSWY3DPEHPK3PXP"),
            acc("Dropbox", "me@example.com", "GEZDGNBVGY3TQOJQ"),
            acc("GitHub", "me@example.com", "JBSWY3DPEHPK3PXP"),
        ];
        let r = v.import_otp(&accounts);
        assert_eq!(r, OtpImportReport { linked: 1, added: 1, skipped: 1 });
        assert!(v.get(gh).unwrap().totp.contains("JBSWY3DPEHPK3PXP"));
        assert!(v.entries().iter().any(|e| e.title == "Dropbox" && e.password.is_empty()));
        // Importing again changes nothing.
        assert_eq!(v.import_otp(&accounts), OtpImportReport { linked: 0, added: 0, skipped: 3 });

        // A short or lookalike issuer gets its own entry, never someone else's login.
        let mut dropbox = input("My files");
        dropbox.urls = vec!["https://www.dropbox.com".into()];
        dropbox.username = String::new();
        let dropbox = v.add(dropbox).unwrap();
        let tricky = [acc("X", "", "MFRGGZDFMZTWQ2LK"), acc("box", "", "ONSWG4TFOQYTEMZU"), acc("drop", "", "NBSWY3DPO5XXE3DE")];
        assert_eq!(v.import_otp(&tricky).linked, 0);
        assert!(v.get(dropbox).unwrap().totp.is_empty());
        // The real service still finds it through the site's name.
        assert_eq!(v.import_otp(&[acc("Dropbox", "", "GEZDGNBVGY3TQOJR")]).linked, 1);
        assert!(!v.get(dropbox).unwrap().totp.is_empty());
    }

    #[test]
    fn backup_codes_cleaned_and_marked() {
        let mut v = UnlockedVault::create("correct horse", KdfParams::TEST).unwrap();
        let mut i = input("Example");
        let code = |c: &str| BackupCode { code: c.into(), used: false };
        i.backup_codes = vec![code(" 1111-2222 "), code(""), code("3333 4444"), code("1111-2222")];
        let id = v.add(i).unwrap();
        let codes: Vec<_> = v.get(id).unwrap().backup_codes.iter().map(|c| c.code.clone()).collect();
        assert_eq!(codes, ["1111-2222", "3333 4444"]);

        let before = v.get(id).unwrap().updated_at;
        v.set_backup_code_used(id, 1, true).unwrap();
        assert!(v.get(id).unwrap().backup_codes[1].used);
        assert!(v.get(id).unwrap().updated_at >= before);
        assert!(v.set_backup_code_used(id, 5, true).is_err());
        // Survives a save and reopen.
        let v2 = UnlockedVault::unlock(&v.to_bytes(), "correct horse").unwrap();
        assert!(v2.get(id).unwrap().backup_codes[1].used);
    }

    #[test]
    fn save_login_adds_updates_or_keeps() {
        let mut v = UnlockedVault::create("correct horse", KdfParams::TEST).unwrap();
        let id = v.add(input("Example")).unwrap();

        // Same site and username, new password: updated in place.
        let out = v.save_login("https://www.example.com", "example.com", "ME@example.com", "n3w!").unwrap();
        assert_eq!(out, SaveOutcome::Updated(id));
        assert_eq!(v.get(id).unwrap().password, "n3w!");
        assert_eq!(v.get(id).unwrap().title, "Example");

        // Same password again: nothing to do.
        let out = v.save_login("https://example.com/login", "example.com", "me@example.com", "n3w!").unwrap();
        assert_eq!(out, SaveOutcome::Unchanged(id));

        // Another account on the same site, and an app: new entries.
        let SaveOutcome::Added(other) = v.save_login("https://example.com", "example.com", "you", "pw").unwrap() else {
            panic!("expected a new entry")
        };
        assert_eq!(v.get(other).unwrap().urls, vec!["https://example.com"]);
        let SaveOutcome::Added(app) = v.save_login("androidapp://com.example", "Example App", "me@example.com", "pw").unwrap() else {
            panic!("an app must not update the web entry")
        };
        assert_eq!(v.get(app).unwrap().title, "Example App");
        assert_eq!(v.entries().len(), 3);
        assert!(v.save_login("https://example.com", "x", "me", "").is_err());
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

    fn pair() -> (UnlockedVault, UnlockedVault) {
        let a = UnlockedVault::create("correct horse", KdfParams::TEST).unwrap();
        let b = UnlockedVault::unlock(&a.to_bytes(), "correct horse").unwrap();
        (a, b)
    }

    fn touch(v: &mut UnlockedVault, id: Uuid, title: &str, at: u64) {
        let e = v.data.entries.iter_mut().find(|e| e.id == id).unwrap();
        e.title = title.into();
        e.updated_at = at;
    }

    #[test]
    fn merge_adds_updates_and_keeps_newer() {
        let (mut a, mut b) = pair();
        let shared = a.add(input("Shared")).unwrap();
        let mut b2 = UnlockedVault::unlock(&a.to_bytes(), "correct horse").unwrap();
        std::mem::swap(&mut b, &mut b2);
        let mut only = input("Only B");
        only.urls = vec!["https://only-b.test".into()];
        let only_b = b.add(only).unwrap();
        touch(&mut b, shared, "Edited on B", 2_000_000_000);

        let r = a.merge_from(&b);
        assert_eq!(r, MergeReport { added: 1, updated: 1, deleted: 0 });
        assert_eq!(a.get(shared).unwrap().title, "Edited on B");
        assert!(a.get(only_b).is_ok());

        // Older copy does not overwrite the newer edit.
        touch(&mut b, shared, "Stale", 1);
        assert_eq!(a.merge_from(&b), MergeReport::default());
        assert_eq!(a.get(shared).unwrap().title, "Edited on B");
    }

    #[test]
    fn merge_propagates_deletions_both_ways() {
        let (mut a, _) = pair();
        let id = a.add(input("Doomed")).unwrap();
        let mut b = UnlockedVault::unlock(&a.to_bytes(), "correct horse").unwrap();

        b.delete(id).unwrap();
        assert_eq!(a.merge_from(&b).deleted, 1);
        assert!(a.get(id).is_err());

        // Merging back the other way must not resurrect it on B.
        let old_a = UnlockedVault::unlock(&a.to_bytes(), "correct horse").unwrap();
        assert_eq!(b.merge_from(&old_a), MergeReport::default());
        assert!(b.get(id).is_err());

        // Tombstones survive a save/unlock round trip.
        let reopened = UnlockedVault::unlock(&a.to_bytes(), "correct horse").unwrap();
        assert_eq!(reopened.data.tombstones.len(), 1);
    }

    #[test]
    fn edit_after_delete_wins() {
        let (mut a, _) = pair();
        let id = a.add(input("Phoenix")).unwrap();
        let mut b = UnlockedVault::unlock(&a.to_bytes(), "correct horse").unwrap();
        a.delete(id).unwrap();
        let deleted_at = a.data.tombstones[0].deleted_at;
        touch(&mut b, id, "Edited later", deleted_at + 10);

        assert_eq!(a.merge_from(&b).added, 1);
        assert_eq!(a.get(id).unwrap().title, "Edited later");
    }

    #[test]
    fn merge_skips_same_login_with_other_id() {
        let (mut a, mut b) = pair();
        a.add(input("Example")).unwrap();
        b.add(input("Example (imported twice)")).unwrap();
        assert_eq!(a.merge_from(&b), MergeReport::default());
        assert_eq!(a.entries().len(), 1);
    }

    #[test]
    fn merge_bytes_needs_the_other_password() {
        let (mut a, _) = pair();
        let mut other = UnlockedVault::create("another password", KdfParams::TEST).unwrap();
        other.add(input("From elsewhere")).unwrap();
        let bytes = other.to_bytes();
        assert!(matches!(a.merge_bytes(&bytes, "correct horse"), Err(Error::WrongPassword)));
        assert_eq!(a.merge_bytes(&bytes, "another password").unwrap().added, 1);
    }
}
