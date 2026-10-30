use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{self, KdfParams, SecretKey, SALT_LEN};
use crate::entry::{now, BackupCode, Entry, EntryInput, EntrySummary, PasswordChange};
use crate::envs::{self, EnvFile, EnvSummary, EnvVersion};
use crate::error::{Error, Result};
use crate::format::{self, Header, WRAP_AAD};
use crate::team::{self, Invite, Key32, Member, Team};

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
    /// Deleted entries kept for `TRASH_DAYS`, so a deletion can be undone.
    #[serde(default)]
    trash: Vec<Trashed>,
    /// When each entry was last copied or filled. Kept apart from the entries so
    /// using one doesn't count as an edit (which would win sync conflicts).
    #[serde(default)]
    usage: std::collections::HashMap<Uuid, u64>,
    /// Projects' `.env` files (see envs.rs).
    #[serde(default)]
    envs: Vec<EnvFile>,
    /// This person's team identity. Local: never in a team's shared copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    identity: Option<Key32>,
    /// The team this vault syncs with, if any. Local too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    team: Option<Team>,
    /// Who's in the team, as the admin last wrote it.
    #[serde(default)]
    members: Vec<Member>,
}

/// What goes in a team's shared copy: everyone's data, nobody's keys.
#[derive(Serialize)]
struct SharedData<'a> {
    entries: &'a [Entry],
    tombstones: &'a [Tombstone],
    trash: &'a [Trashed],
    usage: &'a std::collections::HashMap<Uuid, u64>,
    envs: &'a [EnvFile],
    members: &'a [Member],
}

/// How far ahead of this clock a timestamp from a merge may be. Last edit wins
/// on merges, so a date in the future would beat every later edit forever.
const MAX_SKEW: u64 = 24 * 3600;

/// A date past `limit` is impossible: it counts as no date, so it loses to any
/// real edit on every device, whenever each of them happens to see it.
fn sane(t: u64, limit: u64) -> u64 {
    if t > limit { 0 } else { t }
}

/// The timestamp for an edit: always newer than what's being replaced.
fn bump(previous: u64) -> u64 {
    now().max(previous.saturating_add(1))
}

/// A sealed team key: the file name in the shared folder, and its content.
pub type SealedFile = (String, Vec<u8>);

/// How long the trash keeps deleted entries.
pub const TRASH_DAYS: u64 = 30;
/// Earlier passwords kept per entry.
pub const HISTORY_LEN: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Trashed {
    entry: Entry,
    deleted_at: u64,
}

/// An entry in the trash, as listed (no password).
#[derive(Debug, Clone, Serialize)]
pub struct TrashedSummary {
    #[serde(flatten)]
    pub entry: EntrySummary,
    pub deleted_at: u64,
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
        let mut vault = Self { header: parsed.header, key, data };
        // The trash also has to expire on devices that never sync.
        vault.prune_trash();
        Ok(vault)
    }

    /// Serializes and encrypts the vault with a fresh nonce.
    /// What to put in the cloud folder: this vault, or the team's shared copy.
    pub fn to_cloud_bytes(&self) -> Result<Vec<u8>> {
        let Some(t) = &self.data.team else { return Ok(self.to_bytes()) };
        let d = &self.data;
        let shared = SharedData { entries: &d.entries, tombstones: &d.tombstones, trash: &d.trash, usage: &d.usage, envs: &d.envs, members: &d.members };
        let plain = Zeroizing::new(serde_json::to_vec(&shared).expect("vault serialization is infallible"));
        // No wrapped key: no password can open this file (format::is_team_file).
        let header = Header { vault_id: t.id, kdf: self.header.kdf, salt: crypto::random_bytes::<SALT_LEN>().to_vec(), wrap_nonce: Vec::new(), wrapped_key: Vec::new() };
        let (nonce, ct) = crypto::seal(&t.key.secret_key()?, &plain, &format::encode_prefix(&header));
        Ok(format::encode(&header, &nonce, &ct))
    }

    /// Merges a team's shared copy, opened with the team key.
    fn sync_team(&mut self, bytes: &[u8]) -> Result<MergeReport> {
        let (key, id, admin) = match &self.data.team {
            Some(t) => (t.key.secret_key()?, t.id, t.admin.clone()),
            None => return Err(Error::InvalidInput("other_vault")),
        };
        let parsed = format::decode(bytes)?;
        if parsed.header.vault_id != id {
            return Err(Error::InvalidInput("other_vault"));
        }
        let plain = crypto::open(&key, parsed.nonce, parsed.body, parsed.aad).ok_or(Error::InvalidInput("team_key_outdated"))?;
        let data: VaultData = serde_json::from_slice(&plain).map_err(|_| Error::Corrupt("bad body"))?;
        let other = Self { header: parsed.header, key, data };
        let report = self.merge_from(&other);
        // The list is the admin's: only they change it, the others just follow.
        if self.identity_public().as_deref() != Some(admin.as_str()) && !other.data.members.is_empty() {
            self.data.members = other.data.members.clone();
        }
        Ok(report)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let plain = Zeroizing::new(serde_json::to_vec(&self.data).expect("vault serialization is infallible"));
        let aad = format::encode_prefix(&self.header);
        let (nonce, ct) = crypto::seal(&self.key, &plain, &aad);
        format::encode(&self.header, &nonce, &ct)
    }

    /// Changes the master password and the vault key with it: whoever had the old
    /// password and a copy of this vault can't read the copies made from now on.
    /// The other devices join again with the new password (see `sync_from`).
    pub fn change_password(&mut self, new_password: &str, kdf: KdfParams) -> Result<()> {
        check_password(new_password)?;
        let key = SecretKey::random();
        self.header = wrap_header(self.header.vault_id, &key, new_password, kdf)?;
        self.key = key;
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
        if let Some(t) = &self.data.team {
            return format::decode(bytes).is_ok_and(|p| p.header.vault_id == t.id);
        }
        format::decode(bytes).is_ok_and(|p| p.header.vault_id == self.header.vault_id)
    }

    /// Whether a copy of this vault opens with the key in memory.
    pub fn opens(&self, bytes: &[u8]) -> bool {
        self.data.team.is_some() || Self::unlock_with_key(bytes, self.key.as_bytes()).is_ok()
    }

    /// Merges another copy of this same vault (cloud sync): no password needed,
    /// both share the vault key. A copy under another key is "other_vault": a
    /// different vault, or a master password changed on another device, which
    /// takes that password (see `join`). A password is never adopted silently.
    pub fn sync_from(&mut self, bytes: &[u8]) -> Result<SyncReport> {
        // Passwords are personal in a team: nothing about them is adopted here.
        if self.data.team.is_some() {
            return Ok(SyncReport { merge: self.sync_team(bytes)?, password_changed: false });
        }
        if !self.is_copy(bytes) {
            return Err(Error::InvalidInput("other_vault"));
        }
        let other = Self::unlock_with_key(bytes, self.key.as_bytes()).map_err(|e| match e {
            Error::Corrupt("body authentication failed") => Error::InvalidInput("other_vault"),
            other => other,
        })?;
        let merge = self.merge_from(&other);
        Ok(SyncReport { merge, password_changed: false })
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
        let mut v: Vec<_> = self
            .data
            .entries
            .iter()
            .map(|e| EntrySummary { last_used: self.data.usage.get(&e.id).copied().unwrap_or(0), ..EntrySummary::from(e) })
            .collect();
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
            history: Vec::new(),
            ai_access: input.ai_access,
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
        if e.password != input.password && !e.password.is_empty() {
            let old = std::mem::take(&mut e.password);
            e.history.insert(0, PasswordChange { password: old, replaced_at: now() });
            e.history.truncate(HISTORY_LEN);
        }
        e.password = input.password.clone();
        e.urls = clean_urls(&input.urls);
        e.notes = input.notes.clone();
        e.favorite = input.favorite;
        e.totp = input.totp.trim().to_owned();
        e.backup_codes = clean_codes(&input.backup_codes);
        e.ai_access = input.ai_access;
        e.updated_at = bump(e.updated_at);
        Ok(())
    }

    /// Renames an entry (smart title suggestions).
    pub fn rename(&mut self, id: Uuid, title: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(Error::InvalidInput("title_required"));
        }
        let e = self.data.entries.iter_mut().find(|e| e.id == id).ok_or(Error::NotFound)?;
        if e.title != title {
            e.title = title.to_owned();
            e.updated_at = bump(e.updated_at);
        }
        Ok(())
    }

    /// Notes that an entry was just used (copied or filled), for "recent" ordering.
    pub fn record_use(&mut self, id: Uuid) {
        if self.data.entries.iter().any(|e| e.id == id) {
            self.data.usage.insert(id, now());
        }
    }

    // ---- trash ----

    /// Entries deleted in the last `TRASH_DAYS`, newest first.
    pub fn trash(&self) -> Vec<TrashedSummary> {
        let mut v: Vec<_> = self
            .data
            .trash
            .iter()
            .map(|t| TrashedSummary { entry: EntrySummary::from(&t.entry), deleted_at: t.deleted_at })
            .collect();
        v.sort_by_key(|t| std::cmp::Reverse(t.deleted_at));
        v
    }

    /// Brings an entry back from the trash. It counts as an edit made now, so the
    /// restore wins over the deletion on other devices too.
    pub fn restore(&mut self, id: Uuid) -> Result<()> {
        let i = self.data.trash.iter().position(|t| t.entry.id == id).ok_or(Error::NotFound)?;
        let mut entry = self.data.trash.remove(i).entry;
        entry.updated_at = bump(entry.updated_at);
        self.data.tombstones.retain(|t| t.id != id);
        self.data.entries.push(entry);
        Ok(())
    }

    /// Deletes an entry from the trash for good.
    pub fn purge(&mut self, id: Uuid) -> Result<()> {
        let before = self.data.trash.len();
        self.data.trash.retain(|t| t.entry.id != id);
        if self.data.trash.len() == before {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    pub fn empty_trash(&mut self) {
        self.data.trash.clear();
    }

    fn prune_trash(&mut self) {
        let cutoff = now().saturating_sub(TRASH_DAYS * 24 * 3600);
        self.data.trash.retain(|t| t.deleted_at >= cutoff);
        let expired: Vec<Tombstone> =
            self.data.envs.iter().filter(|e| e.deleted_at != 0 && e.deleted_at < cutoff).map(|e| Tombstone { id: e.id, deleted_at: e.updated_at }).collect();
        self.data.envs.retain(|e| e.deleted_at == 0 || e.deleted_at >= cutoff);
        self.data.tombstones.extend(expired);
    }

    // ---- team ----

    /// This person's public key, once they have an identity.
    pub fn identity_public(&self) -> Option<String> {
        self.data.identity.as_ref().map(team::public_of)
    }

    /// Makes the identity on first use. Returns the public key.
    pub fn ensure_identity(&mut self) -> String {
        team::public_of(self.data.identity.get_or_insert_with(Key32::random))
    }

    pub fn team(&self) -> Option<&Team> {
        self.data.team.as_ref()
    }

    pub fn members(&self) -> &[Member] {
        &self.data.members
    }

    pub fn is_admin(&self) -> bool {
        self.data.team.as_ref().is_some_and(|t| self.identity_public().as_deref() == Some(t.admin.as_str()))
    }

    /// Starts a team, with this person as its admin and only member. Everything
    /// in this vault becomes the team's.
    pub fn team_create(&mut self, name: &str, me: &str) -> Result<()> {
        if self.data.team.is_some() {
            return Err(Error::InvalidInput("team_exists"));
        }
        let name = envs::clean_name(name).ok_or(Error::InvalidInput("invalid_name"))?;
        let public = self.ensure_identity();
        // What was deleted before the team isn't the team's.
        self.data.trash.clear();
        self.data.members = vec![Member { public: public.clone(), name: me.trim().to_owned(), added_at: now() }];
        self.data.team = Some(Team { id: Uuid::new_v4(), name, key: Key32::random(), epoch: 1, admin: public });
        Ok(())
    }

    /// Admin: seals the team key to a new member. Returns (file name, content).
    pub fn team_invite(&mut self, code: &str, name: &str) -> Result<(String, Vec<u8>)> {
        let public = team::parse_code(code)?;
        let name = envs::clean_name(name).ok_or(Error::InvalidInput("invalid_name"))?;
        if !self.is_admin() {
            return Err(Error::InvalidInput("team_not_admin"));
        }
        if self.data.members.iter().any(|m| m.public == public) {
            return Err(Error::InvalidInput("team_member_exists"));
        }
        let sealed = self.seal_for(&public)?;
        self.data.members.push(Member { public: public.clone(), name, added_at: now() });
        Ok((team::invite_file(&public), sealed))
    }

    fn seal_for(&self, public: &str) -> Result<Vec<u8>> {
        let (Some(me), Some(t)) = (&self.data.identity, &self.data.team) else { return Err(Error::InvalidInput("team_not_admin")) };
        team::seal(me, public, t)
    }

    /// Admin: removes a member and changes the team key. Returns the sealed key
    /// file of each member who stays, to write before the shared copy, and the
    /// removed member's file, to delete.
    pub fn team_remove(&mut self, public: &str) -> Result<(Vec<SealedFile>, String)> {
        if !self.is_admin() {
            return Err(Error::InvalidInput("team_not_admin"));
        }
        if self.identity_public().as_deref() == Some(public) || !self.data.members.iter().any(|m| m.public == public) {
            return Err(Error::InvalidInput("not_found"));
        }
        self.data.members.retain(|m| m.public != public);
        let t = self.data.team.as_mut().ok_or(Error::InvalidInput("team_not_admin"))?;
        t.key = Key32::random();
        t.epoch += 1;
        let me = self.identity_public();
        let others: Vec<String> = self.data.members.iter().map(|m| m.public.clone()).filter(|p| Some(p) != me.as_ref()).collect();
        let files = others.iter().map(|p| Ok((team::invite_file(p), self.seal_for(p)?))).collect::<Result<Vec<_>>>()?;
        Ok((files, team::invite_file(public)))
    }

    /// Opens an invitation sealed to this person, to show it before accepting.
    pub fn team_peek(&self, bytes: &[u8]) -> Result<Invite> {
        team::open(self.data.identity.as_ref().ok_or(Error::InvalidInput("team_bad_invite"))?, bytes)
    }

    /// Joins the team of an invitation. Only into an empty vault, since all it
    /// holds would go to the team.
    pub fn team_join(&mut self, bytes: &[u8]) -> Result<()> {
        if self.data.team.is_some() {
            return Err(Error::InvalidInput("team_exists"));
        }
        if !self.data.entries.is_empty() || !self.data.envs.is_empty() {
            return Err(Error::InvalidInput("team_vault_not_empty"));
        }
        let i = self.team_peek(bytes)?;
        // Logins deleted to empty the vault must not reach the team from the trash.
        self.data.trash.clear();
        self.data.tombstones.clear();
        self.data.usage.clear();
        self.data.team = Some(Team { id: i.id, name: i.name, key: i.key, epoch: i.epoch, admin: i.sender });
        Ok(())
    }

    /// Takes a new team key after a removal. Only from the admin pinned at join,
    /// for this team, and newer than the current one. Returns whether it applied.
    pub fn team_update(&mut self, bytes: &[u8]) -> Result<bool> {
        let i = self.team_peek(bytes)?;
        let Some(t) = self.data.team.as_mut() else { return Ok(false) };
        if i.sender != t.admin || i.id != t.id || i.epoch <= t.epoch {
            return Ok(false);
        }
        t.key = i.key;
        t.epoch = i.epoch;
        Ok(true)
    }

    /// Stops syncing with the team. What's in this vault stays here. The admin
    /// can't leave: nobody else could invite or remove anyone.
    pub fn team_leave(&mut self) -> Result<()> {
        if self.is_admin() {
            return Err(Error::InvalidInput("team_not_admin"));
        }
        self.data.team = None;
        self.data.members.clear();
        Ok(())
    }

    // ---- projects ----

    /// Every environment, the trashed ones included (`deleted_at` tells them apart).
    pub fn envs(&self) -> Vec<EnvSummary> {
        let mut list: Vec<EnvSummary> = self.data.envs.iter().map(EnvSummary::from).collect();
        list.sort_by_key(|e| (e.project.to_lowercase(), e.name.to_lowercase()));
        list
    }

    pub fn env(&self, id: Uuid) -> Result<&EnvFile> {
        self.data.envs.iter().find(|e| e.id == id).ok_or(Error::NotFound)
    }

    fn env_mut(&mut self, id: Uuid) -> Result<&mut EnvFile> {
        self.data.envs.iter_mut().find(|e| e.id == id).ok_or(Error::NotFound)
    }

    /// The live environment with these names (case doesn't matter).
    pub fn env_named(&self, project: &str, name: &str) -> Option<&EnvFile> {
        let same = |a: &str, b: &str| a.trim().to_lowercase() == b.trim().to_lowercase();
        self.data.envs.iter().find(|e| e.deleted_at == 0 && same(&e.project, project) && same(&e.name, name))
    }

    /// Creates the environment, or replaces its content if it exists (the old
    /// content goes to its history). Returns its id.
    pub fn env_put(&mut self, project: &str, name: &str, content: &str) -> Result<Uuid> {
        let project = envs::clean_name(project).ok_or(Error::InvalidInput("invalid_name"))?;
        let name = envs::clean_name(name).ok_or(Error::InvalidInput("invalid_name"))?;
        if content.len() > envs::MAX_LEN {
            return Err(Error::InvalidInput("env_too_long"));
        }
        if let Some(id) = self.env_named(&project, &name).map(|e| e.id) {
            self.env_update(id, content)?;
            return Ok(id);
        }
        let t = now();
        let env = EnvFile { id: Uuid::new_v4(), project, name, content: content.to_owned(), history: Vec::new(), ai_access: false, created_at: t, updated_at: t, deleted_at: 0 };
        let id = env.id;
        self.data.envs.push(env);
        Ok(id)
    }

    pub fn env_update(&mut self, id: Uuid, content: &str) -> Result<()> {
        if content.len() > envs::MAX_LEN {
            return Err(Error::InvalidInput("env_too_long"));
        }
        let e = self.env_mut(id)?;
        if e.content == content {
            return Ok(());
        }
        let t = bump(e.updated_at);
        let old = std::mem::replace(&mut e.content, content.to_owned());
        e.history.insert(0, EnvVersion { content: old, replaced_at: t });
        e.history.truncate(HISTORY_LEN);
        e.updated_at = t;
        Ok(())
    }

    /// Renames the environment, or moves it to another project.
    pub fn env_rename(&mut self, id: Uuid, project: &str, name: &str) -> Result<()> {
        let project = envs::clean_name(project).ok_or(Error::InvalidInput("invalid_name"))?;
        let name = envs::clean_name(name).ok_or(Error::InvalidInput("invalid_name"))?;
        if self.env_named(&project, &name).is_some_and(|e| e.id != id) {
            return Err(Error::InvalidInput("env_exists"));
        }
        let e = self.env_mut(id)?;
        e.ai_access &= !envs::is_production(&name);
        e.project = project;
        e.name = name;
        e.updated_at = bump(e.updated_at);
        Ok(())
    }

    /// Renames a whole project (all its environments, trashed ones too).
    pub fn env_rename_project(&mut self, project: &str, new_name: &str) -> Result<()> {
        let new_name = envs::clean_name(new_name).ok_or(Error::InvalidInput("invalid_name"))?;
        for e in self.data.envs.iter_mut().filter(|e| e.project == project) {
            e.project = new_name.clone();
            e.updated_at = bump(e.updated_at);
        }
        Ok(())
    }

    /// Shares an environment with AI assistants. Production never is.
    pub fn env_set_ai(&mut self, id: Uuid, on: bool) -> Result<()> {
        let e = self.env_mut(id)?;
        if on && envs::is_production(&e.name) {
            return Err(Error::InvalidInput("env_production"));
        }
        e.ai_access = on;
        e.updated_at = bump(e.updated_at);
        Ok(())
    }

    /// Moves the environment to the trash for `TRASH_DAYS`.
    pub fn env_delete(&mut self, id: Uuid) -> Result<()> {
        let e = self.env_mut(id)?;
        let t = bump(e.updated_at);
        e.deleted_at = t;
        e.updated_at = t;
        Ok(())
    }

    pub fn env_restore(&mut self, id: Uuid) -> Result<()> {
        let e = self.env(id)?;
        if self.env_named(&e.project, &e.name).is_some() {
            return Err(Error::InvalidInput("env_exists"));
        }
        let e = self.env_mut(id)?;
        e.deleted_at = 0;
        e.updated_at = bump(e.updated_at);
        Ok(())
    }

    /// Merges the other copy's environments, newest edit winning. The same
    /// project and environment created on two devices has two ids: the older
    /// one's content moves into the newer one's history, so nothing is lost.
    fn merge_envs(&mut self, other: &[EnvFile], deleted: &std::collections::HashMap<Uuid, u64>, limit: u64, report: &mut MergeReport) {
        let gone = |e: &EnvFile| deleted.get(&e.id).is_some_and(|&d| d >= sane(e.updated_at, limit));
        let clamped = |e: &EnvFile| {
            let mut e = e.clone();
            e.updated_at = sane(e.updated_at, limit);
            e.deleted_at = sane(e.deleted_at, limit);
            e
        };
        let before = self.data.envs.len();
        self.data.envs.retain(|e| !gone(e));
        report.deleted += before - self.data.envs.len();
        for theirs in other.iter().filter(|e| !gone(e)) {
            match self.data.envs.iter().position(|e| e.id == theirs.id) {
                Some(i) if sane(theirs.updated_at, limit) > self.data.envs[i].updated_at => {
                    self.data.envs[i] = clamped(theirs);
                    report.updated += 1;
                }
                Some(_) => {}
                None => {
                    self.data.envs.push(clamped(theirs));
                    report.added += 1;
                }
            }
        }
        // Newest first, ties by id: both devices pick the same survivor.
        self.data.envs.sort_by_key(|e| (e.deleted_at != 0, std::cmp::Reverse(e.updated_at), e.id));
        let mut kept: Vec<EnvFile> = Vec::with_capacity(self.data.envs.len());
        for e in std::mem::take(&mut self.data.envs) {
            let twin = kept.iter_mut().find(|k| {
                e.deleted_at == 0 && k.deleted_at == 0 && k.project.to_lowercase() == e.project.to_lowercase() && k.name.to_lowercase() == e.name.to_lowercase()
            });
            match twin {
                Some(k) => {
                    if k.content != e.content && !k.history.iter().any(|h| h.content == e.content) {
                        k.history.push(EnvVersion { content: e.content, replaced_at: e.updated_at });
                        k.history.sort_by_key(|h| std::cmp::Reverse(h.replaced_at));
                        k.history.truncate(HISTORY_LEN);
                    }
                    self.data.tombstones.push(Tombstone { id: e.id, deleted_at: e.updated_at });
                }
                None => kept.push(e),
            }
        }
        self.data.envs = kept;
    }

    // ---- smart cleanup ----

    /// Merges a group of duplicate entries into the most recently edited one:
    /// the others' different passwords go to its history and their sites are
    /// added to it; then they move to the trash. Returns the kept entry's id.
    pub fn merge_duplicates(&mut self, ids: &[Uuid]) -> Result<Uuid> {
        let keep = ids
            .iter()
            .filter_map(|id| self.data.entries.iter().find(|e| e.id == *id))
            .max_by_key(|e| e.updated_at)
            .map(|e| e.id)
            .ok_or(Error::NotFound)?;
        let others: Vec<Entry> = self.data.entries.iter().filter(|e| e.id != keep && ids.contains(&e.id)).cloned().collect();
        if others.is_empty() {
            return Ok(keep);
        }
        let k = self.data.entries.iter_mut().find(|e| e.id == keep).ok_or(Error::NotFound)?;
        for o in &others {
            if !o.password.is_empty() && o.password != k.password && !k.history.iter().any(|h| h.password == o.password) {
                k.history.push(PasswordChange { password: o.password.clone(), replaced_at: o.updated_at });
            }
            for u in &o.urls {
                if !k.urls.contains(u) {
                    k.urls.push(u.clone());
                }
            }
            if k.totp.is_empty() && !o.totp.is_empty() {
                k.totp = o.totp.clone();
            }
            k.favorite |= o.favorite;
        }
        k.history.sort_by_key(|h| std::cmp::Reverse(h.replaced_at));
        k.history.truncate(HISTORY_LEN);
        k.updated_at = bump(k.updated_at);
        for o in others {
            self.delete(o.id)?;
        }
        Ok(keep)
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
                ai_access: e.ai_access,
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
            ai_access: false,
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
                e.updated_at = bump(e.updated_at);
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
                ai_access: false,
            });
            if added.is_ok() {
                report.added += 1;
            }
        }
        report
    }

    /// Sets (or replaces) an entry's 2FA secret, e.g. read from a site's QR code.
    pub fn set_totp(&mut self, id: Uuid, uri: &str) -> Result<()> {
        crate::totp::Totp::parse(uri)?;
        let e = self.data.entries.iter_mut().find(|e| e.id == id).ok_or(Error::NotFound)?;
        e.totp = uri.trim().to_owned();
        e.updated_at = bump(e.updated_at);
        Ok(())
    }

    /// Marks backup code number `index` of an entry as used (or not). A change
    /// like any other: it updates the entry's time, so it syncs to other devices.
    pub fn set_backup_code_used(&mut self, id: Uuid, index: usize, used: bool) -> Result<()> {
        let e = self.data.entries.iter_mut().find(|e| e.id == id).ok_or(Error::NotFound)?;
        let c = e.backup_codes.get_mut(index).ok_or(Error::NotFound)?;
        if c.used != used {
            c.used = used;
            e.updated_at = bump(e.updated_at);
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

    /// Moves an entry to the trash (and leaves a tombstone for sync).
    pub fn delete(&mut self, id: Uuid) -> Result<()> {
        let i = self.data.entries.iter().position(|e| e.id == id).ok_or(Error::NotFound)?;
        let entry = self.data.entries.remove(i);
        let t = bump(entry.updated_at);
        self.data.tombstones.retain(|t| t.id != id);
        self.data.tombstones.push(Tombstone { id, deleted_at: t });
        self.data.usage.remove(&id);
        self.data.trash.retain(|x| x.entry.id != id);
        self.data.trash.push(Trashed { entry, deleted_at: t });
        self.prune_trash();
        Ok(())
    }

    // ---- sync ----

    /// Clears any date past `limit`, also on what an earlier merge let in.
    fn clamp_times(&mut self, limit: u64) {
        for e in &mut self.data.entries {
            e.updated_at = sane(e.updated_at, limit);
        }
        for e in &mut self.data.envs {
            e.updated_at = sane(e.updated_at, limit);
            e.deleted_at = sane(e.deleted_at, limit);
        }
        for t in &mut self.data.tombstones {
            t.deleted_at = sane(t.deleted_at, limit);
        }
        for t in &mut self.data.trash {
            t.deleted_at = sane(t.deleted_at, limit);
        }
    }

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
        let limit = now() + MAX_SKEW;
        let stamp = |t: u64| sane(t, limit);
        self.clamp_times(limit);

        // Latest deletion time per id, from both sides.
        let mut deleted: std::collections::HashMap<Uuid, u64> = std::collections::HashMap::new();
        for t in self.data.tombstones.iter().chain(&other.data.tombstones) {
            let d = deleted.entry(t.id).or_insert(0);
            *d = (*d).max(stamp(t.deleted_at));
        }
        let is_deleted = |id: Uuid, updated_at: u64| deleted.get(&id).is_some_and(|&d| d >= updated_at);

        let before = self.data.entries.len();
        self.data.entries.retain(|e| !is_deleted(e.id, e.updated_at));
        report.deleted = before - self.data.entries.len();

        for theirs in &other.data.entries {
            let updated_at = stamp(theirs.updated_at);
            if is_deleted(theirs.id, updated_at) {
                continue;
            }
            let incoming = || {
                let mut e = theirs.clone();
                e.updated_at = updated_at;
                e
            };
            match self.data.entries.iter().position(|e| e.id == theirs.id) {
                Some(i) if updated_at > self.data.entries[i].updated_at => {
                    self.data.entries[i] = incoming();
                    report.updated += 1;
                }
                Some(_) => {}
                None if self.data.entries.iter().any(|e| same_login(e, theirs)) => {}
                None => {
                    self.data.entries.push(incoming());
                    report.added += 1;
                }
            }
        }

        self.merge_envs(&other.data.envs, &deleted, limit, &mut report);
        let extra = self.data.tombstones.iter().filter(|t| !deleted.contains_key(&t.id)).copied().collect::<Vec<_>>();
        let mut tombstones: Vec<_> = deleted.into_iter().map(|(id, deleted_at)| Tombstone { id, deleted_at }).chain(extra).collect();
        tombstones.sort_by_key(|t| t.deleted_at);
        self.data.tombstones = tombstones;

        // Trash: the union of both, minus anything alive again on either side.
        for t in &other.data.trash {
            if !self.data.trash.iter().any(|x| x.entry.id == t.entry.id) {
                let mut t = t.clone();
                t.deleted_at = stamp(t.deleted_at);
                self.data.trash.push(t);
            }
        }
        let alive: std::collections::HashSet<Uuid> = self.data.entries.iter().map(|e| e.id).collect();
        self.data.trash.retain(|t| !alive.contains(&t.entry.id));
        self.prune_trash();
        // Usage: the latest on either side.
        for (id, t) in &other.data.usage {
            let mine = self.data.usage.entry(*id).or_insert(0);
            *mine = (*mine).max(stamp(*t));
        }
        self.data.usage.retain(|id, _| alive.contains(id));
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

    fn vault() -> UnlockedVault {
        UnlockedVault::create("correct horse", KdfParams::TEST).unwrap()
    }

    fn login(v: &mut UnlockedVault, title: &str) {
        let mut input = EntryInput::default();
        input.title = title.into();
        input.password = title.into();
        v.add(input).unwrap();
    }

    /// An admin with a vault, and Marco joined through an invitation.
    fn team_of_two() -> (UnlockedVault, UnlockedVault) {
        let mut admin = vault();
        login(&mut admin, "server");
        admin.team_create("Oikos", "Simone").unwrap();
        let mut marco = UnlockedVault::create("marco's password", KdfParams::TEST).unwrap();
        let code = crate::team::member_code(&marco.ensure_identity());
        let (_, sealed) = admin.team_invite(&code, "Marco").unwrap();
        marco.team_join(&sealed).unwrap();
        marco.sync_from(&admin.to_cloud_bytes().unwrap()).unwrap();
        (admin, marco)
    }

    #[test]
    fn a_member_joins_without_any_shared_password() {
        let (admin, marco) = team_of_two();
        assert_eq!(marco.list()[0].title, "server");
        assert_eq!(marco.members().len(), 2);

        // The shared copy has no password path, not even the admin's.
        let shared = admin.to_cloud_bytes().unwrap();
        assert!(format::is_team_file(&shared));
        assert!(UnlockedVault::unlock(&shared, "correct horse").is_err());
        assert!(UnlockedVault::unlock(&shared, "marco's password").is_err());
    }

    #[test]
    fn the_shared_copy_carries_no_keys() {
        let (admin, _) = team_of_two();
        let t = admin.team().unwrap();
        let key = t.key.secret_key().unwrap();
        let shared = admin.to_cloud_bytes().unwrap();
        let parsed = format::decode(&shared).unwrap();
        let plain = crypto::open(&key, parsed.nonce, parsed.body, parsed.aad).unwrap();
        let text = String::from_utf8_lossy(&plain);
        assert!(!text.contains("identity") && !text.contains("\"team\"") && !text.contains("password_changed_at"));
    }

    #[test]
    fn changes_flow_both_ways_and_local_passwords_stay_personal() {
        let (mut admin, mut marco) = team_of_two();
        marco.change_password("marco's new password", KdfParams::TEST).unwrap();
        login(&mut marco, "from marco");
        let r = admin.sync_from(&marco.to_cloud_bytes().unwrap()).unwrap();
        assert_eq!(r.merge.added, 1);
        assert!(!r.password_changed);
        // Each local file still opens with its own password.
        assert!(UnlockedVault::unlock(&admin.to_bytes(), "correct horse").is_ok());
        assert!(UnlockedVault::unlock(&marco.to_bytes(), "marco's new password").is_ok());
    }

    #[test]
    fn a_removed_member_is_locked_out_and_the_others_follow() {
        let (mut admin, mut marco) = team_of_two();
        let mut anna = UnlockedVault::create("anna's password", KdfParams::TEST).unwrap();
        let code = crate::team::member_code(&anna.ensure_identity());
        let (_, sealed) = admin.team_invite(&code, "Anna").unwrap();
        anna.team_join(&sealed).unwrap();
        anna.sync_from(&admin.to_cloud_bytes().unwrap()).unwrap();
        login(&mut anna, "anna's edit made before the new key");

        let marco_public = marco.identity_public().unwrap();
        let (files, gone) = admin.team_remove(&marco_public).unwrap();
        assert_eq!(gone, crate::team::invite_file(&marco_public));
        assert_eq!(files.len(), 1, "only Anna gets the new key");
        login(&mut admin, "after removal");
        let shared = admin.to_cloud_bytes().unwrap();

        // Marco can't read it, and has no new key to take.
        assert!(matches!(marco.sync_from(&shared), Err(Error::InvalidInput("team_key_outdated"))));
        assert!(marco.team_update(&files[0].1).is_err());

        // Anna updates, and her offline edit isn't lost.
        assert!(matches!(anna.sync_from(&shared), Err(Error::InvalidInput("team_key_outdated"))));
        assert!(anna.team_update(&files[0].1).unwrap());
        anna.sync_from(&shared).unwrap();
        assert!(anna.list().iter().any(|e| e.title == "after removal"));
        assert_eq!(anna.members().len(), 2);
        admin.sync_from(&anna.to_cloud_bytes().unwrap()).unwrap();
        assert!(admin.list().iter().any(|e| e.title.starts_with("anna's edit")));
        // Her local file never depended on the team key.
        assert!(UnlockedVault::unlock(&anna.to_bytes(), "anna's password").is_ok());
    }

    #[test]
    fn only_the_pinned_admin_can_change_the_key() {
        let (admin, mut marco) = team_of_two();
        // Marco knows the current team key and forges a newer epoch for Anna.
        let mut anna = UnlockedVault::create("anna's password", KdfParams::TEST).unwrap();
        let anna_public = anna.ensure_identity();
        let mut admin = admin;
        let (_, sealed) = admin.team_invite(&crate::team::member_code(&anna_public), "Anna").unwrap();
        anna.team_join(&sealed).unwrap();
        let before = anna.team().unwrap().key.bytes().to_owned();

        let mut forged = marco.team().unwrap().clone();
        forged.epoch += 5;
        marco.ensure_identity();
        let bytes = crate::team::seal(marco.data.identity.as_ref().unwrap(), &anna_public, &forged).unwrap();
        assert!(!anna.team_update(&bytes).unwrap());
        assert_eq!(anna.team().unwrap().key.bytes(), &before);
        // An old key from the real admin doesn't apply either.
        assert!(!anna.team_update(&sealed).unwrap());
        assert!(marco.team_invite(&crate::team::member_code(&anna_public), "x").is_err());
    }

    #[test]
    fn joining_needs_an_empty_vault_and_the_admin_cannot_leave() {
        let (mut admin, mut marco) = team_of_two();
        let mut full = vault();
        login(&mut full, "personal");
        let code = crate::team::member_code(&full.ensure_identity());
        let (_, sealed) = admin.team_invite(&code, "Full").unwrap();
        assert!(matches!(full.team_join(&sealed), Err(Error::InvalidInput("team_vault_not_empty"))));
        assert!(admin.team_leave().is_err());
        marco.team_leave().unwrap();
        assert!(marco.team().is_none());
        assert_eq!(marco.list()[0].title, "server");
    }

    #[test]
    fn the_old_password_stops_working_on_new_copies() {
        let mut v = vault();
        let old_copy = v.to_bytes();
        let old = UnlockedVault::unlock(&old_copy, "correct horse").unwrap();
        v.change_password("a brand new password", KdfParams::TEST).unwrap();
        login(&mut v, "after the change");
        let new_copy = v.to_bytes();
        assert!(UnlockedVault::unlock(&new_copy, "correct horse").is_err());
        assert!(UnlockedVault::unlock_with_key(&new_copy, &old.raw_key()).is_err());
        // Nor can a copy made with the old key be pushed onto this device.
        assert!(matches!(v.sync_from(&old_copy), Err(Error::InvalidInput("other_vault"))));
        assert!(UnlockedVault::unlock(&v.to_bytes(), "a brand new password").is_ok());
    }

    #[test]
    fn deleted_logins_never_reach_the_team() {
        let mut admin = vault();
        admin.team_create("Oikos", "Simone").unwrap();
        let mut marco = UnlockedVault::create("marco's password", KdfParams::TEST).unwrap();
        login(&mut marco, "marco's bank");
        let id = marco.list()[0].id;
        marco.delete(id).unwrap();
        let code = crate::team::member_code(&marco.ensure_identity());
        let (_, sealed) = admin.team_invite(&code, "Marco").unwrap();
        marco.team_join(&sealed).unwrap();
        assert!(marco.trash().is_empty());
        admin.sync_from(&marco.to_cloud_bytes().unwrap()).unwrap();
        assert!(admin.trash().is_empty());

        let mut owner = vault();
        login(&mut owner, "old");
        let id = owner.list()[0].id;
        owner.delete(id).unwrap();
        owner.team_create("Other", "Anna").unwrap();
        assert!(owner.trash().is_empty());
    }

    #[test]
    fn a_date_in_the_future_cannot_undo_later_edits() {
        let (mut admin, mut marco) = team_of_two();
        let id = marco.list()[0].id;
        // Marco's modified client plants a far-future date.
        let e = marco.data.entries.iter_mut().find(|e| e.id == id).unwrap();
        e.updated_at = u64::MAX;
        e.password = "planted".into();
        admin.sync_from(&marco.to_cloud_bytes().unwrap()).unwrap();
        assert!(admin.get(id).unwrap().updated_at <= now() + MAX_SKEW);

        // The next edit wins, on both sides.
        let mut input = EntryInput::default();
        input.title = "server".into();
        input.password = "rotated".into();
        admin.update(id, input).unwrap();
        marco.sync_from(&admin.to_cloud_bytes().unwrap()).unwrap();
        admin.sync_from(&marco.to_cloud_bytes().unwrap()).unwrap();
        assert_eq!(admin.get(id).unwrap().password, "rotated");
        assert_eq!(marco.get(id).unwrap().password, "rotated");
    }

    #[test]
    fn identity_survives_a_save() {
        let mut v = vault();
        let public = v.ensure_identity();
        let v = UnlockedVault::unlock(&v.to_bytes(), "correct horse").unwrap();
        assert_eq!(v.identity_public().as_deref(), Some(public.as_str()));
    }


    #[test]
    fn last_used_survives_a_save() {
        let mut v = UnlockedVault::create("correct horse", KdfParams::TEST).unwrap();
        let mut input = EntryInput::default();
        input.title = "a".into();
        let id = v.add(input).unwrap();
        v.record_use(id);
        let v = UnlockedVault::unlock(&v.to_bytes(), "correct horse").unwrap();
        assert!(v.list()[0].last_used > 0);
    }

    #[test]
    fn envs_keep_versions_and_never_share_production_with_ai() {
        let mut v = UnlockedVault::create("correct horse", KdfParams::TEST).unwrap();
        let id = v.env_put("Oikos", "production", "A=1").unwrap();
        assert_eq!(v.env_put("oikos", "PRODUCTION", "A=2").unwrap(), id);
        assert_eq!(v.env(id).unwrap().history[0].content, "A=1");
        assert!(v.env_set_ai(id, true).is_err());

        let dev = v.env_put("Oikos", "development", "B=1").unwrap();
        v.env_set_ai(dev, true).unwrap();
        v.env_rename(dev, "Oikos", "prod-2").unwrap();
        assert!(!v.env(dev).unwrap().ai_access);

        v.env_delete(id).unwrap();
        assert!(v.env_named("oikos", "production").is_none());
        v.env_put("oikos", "production", "C=1").unwrap();
        assert!(v.env_restore(id).is_err(), "the name is taken again");

        let v = UnlockedVault::unlock(&v.to_bytes(), "correct horse").unwrap();
        assert_eq!(v.envs().len(), 3);
    }

    #[test]
    fn envs_sync_and_twins_keep_both_contents() {
        let mut a = UnlockedVault::create("correct horse", KdfParams::TEST).unwrap();
        let mut b = UnlockedVault::unlock_with_key(&a.to_bytes(), &a.raw_key()).unwrap();
        a.env_put("Oikos", "staging", "OLD=1").unwrap();
        let mine = b.env_put("oikos", "staging", "NEW=1").unwrap();
        b.env_mut(mine).unwrap().updated_at += 5;

        let r = a.merge_from(&b);
        assert_eq!(r.added, 1);
        let kept = a.env_named("oikos", "staging").unwrap();
        assert_eq!(kept.content, "NEW=1");
        assert_eq!(kept.history[0].content, "OLD=1");

        // The loser's id doesn't come back when the two copies meet again.
        b.merge_from(&a);
        a.merge_from(&b);
        assert_eq!(a.envs().len(), 1);
        assert_eq!(b.envs().len(), 1);

        // A trashed environment syncs as an edit.
        b.env_delete(mine).unwrap();
        b.env_mut(mine).unwrap().updated_at += 10;
        let r = a.merge_from(&b);
        assert_eq!(r.updated, 1);
        assert!(a.env_named("oikos", "staging").is_none());
    }

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
            ai_access: false,
        }
    }

    #[test]
    fn trash_history_usage_and_duplicates() {
        let mut v = UnlockedVault::create("correct horse", KdfParams::TEST).unwrap();
        let id = v.add(input("Example")).unwrap();

        // A changed password goes to the history.
        let mut i = input("Example");
        i.password = "n3w".into();
        v.update(id, i).unwrap();
        assert_eq!(v.get(id).unwrap().history[0].password, "s3cret!");

        // Deleting goes to the trash; restoring brings it back, history included.
        v.record_use(id);
        assert!(v.list()[0].last_used > 0);
        v.delete(id).unwrap();
        assert!(v.get(id).is_err());
        assert_eq!(v.trash().len(), 1);
        v.restore(id).unwrap();
        assert_eq!(v.get(id).unwrap().history.len(), 1);
        assert!(v.trash().is_empty());
        v.delete(id).unwrap();
        v.purge(id).unwrap();
        assert!(v.trash().is_empty());

        // Duplicates: the newest stays, the other's password joins its history.
        let a = v.add(input("Dup")).unwrap();
        let mut b = input("Dup");
        b.password = "other".into();
        let b = v.add(b).unwrap();
        v.data.entries.iter_mut().find(|e| e.id == b).unwrap().updated_at += 10;
        assert_eq!(v.merge_duplicates(&[a, b]).unwrap(), b);
        assert!(v.get(a).is_err());
        assert!(v.get(b).unwrap().history.iter().any(|h| h.password == "s3cret!"));
        assert_eq!(v.trash().len(), 1);

        // The trash survives a save and travels with sync.
        let copy = UnlockedVault::unlock(&v.to_bytes(), "correct horse").unwrap();
        assert_eq!(copy.trash().len(), 1);
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

        // A password change makes a new key: the computer has to join with it.
        phone.change_password("battery staple", KdfParams::TEST).unwrap();
        assert!(matches!(computer.sync_from(&phone.to_bytes()), Err(Error::InvalidInput("other_vault"))));
        let (computer, _) = computer.join(&phone.to_bytes(), "battery staple").unwrap();
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
        touch(&mut b, shared, "Edited on B", now() + 60);

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
