// Typed wrapper around the Rust commands. This is the only file that talks to
// the backend: screens/components can be redesigned freely on top of it.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Status = { exists: boolean; unlocked: boolean; platform: string; profile: string };
export type Profile = { id: string; name: string };
export type Profiles = { active: string; profiles: Profile[] };

export type EntrySummary = {
  id: string;
  title: string;
  username: string;
  urls: string[];
  favorite: boolean;
  updated_at: number;
  has_totp: boolean;
  ai_access: boolean;
  /** Unix seconds of the last copy or fill, 0 if never. */
  last_used: number;
};

export type Entry = EntrySummary & {
  password: string;
  notes: string;
  /** 2FA secret or otpauth:// link ("" when none). */
  totp: string;
  backup_codes: BackupCode[];
  /** Earlier passwords, newest first. */
  history: { password: string; replaced_at: number }[];
  created_at: number;
};

export type EntryInput = {
  title: string;
  username: string;
  password: string;
  urls: string[];
  notes: string;
  favorite: boolean;
  totp: string;
  backup_codes: BackupCode[];
  /** AI assistants may request this entry (MCP server, with confirmation). */
  ai_access: boolean;
};

/** A cloud folder found on this computer (Google Drive, iCloud Drive, Dropbox, OneDrive). */
export type OtpAddRequest = {
  id: number;
  host: string;
  issuer: string;
  account: string;
  candidates: { id: string; title: string; username: string }[];
};

export type TeamMember = { public: string; name: string; safety: string; added_at: number; me: boolean; admin: boolean };
export type TeamStatus = { code: string; safety: string; team: { name: string; is_admin: boolean; admin_safety: string; members: TeamMember[] } | null };

export type EnvSummary = {
  id: string;
  project: string;
  name: string;
  vars: number;
  versions: number;
  ai_access: boolean;
  production: boolean;
  updated_at: number;
  /** Unix seconds, 0 unless in the trash. */
  deleted_at: number;
};
export type EnvFile = { id: string; project: string; name: string; content: string; ai_access: boolean; history: { content: string; replaced_at: number }[] };
export type EnvDiff = { only_a: string[]; only_b: string[]; different: string[] };
export type FoundEnv = { path: string; project: string; name: string; vars: number };

export type CloudProvider = { name: string; path: string; has_vault: boolean };
export type CloudStatus = {
  /** "" when sync is off. */
  folder: string;
  /** The folder's display name on Android ("" elsewhere). */
  name: string;
  status: { state: "off" | "ok" | "waiting" | "other_vault" | "team_removed" | "error"; last_sync: number };
};
export type SyncReport = { added: number; updated: number; deleted: number; password_changed: boolean };

/** A 2FA recovery code; each works once. */
export type BackupCode = { code: string; used: boolean };

export type TotpCode = { code: string; remaining: number; period: number };

export type GeneratorOptions = {
  length: number;
  lowercase: boolean;
  uppercase: boolean;
  digits: boolean;
  symbols: boolean;
  exclude_ambiguous: boolean;
};

export const defaultGenerator: GeneratorOptions = {
  length: 20,
  lowercase: true,
  uppercase: true,
  digits: true,
  symbols: true,
  exclude_ambiguous: true,
};

export type HealthReport = {
  total: number;
  weak: string[];
  reused: string[];
  old: string[];
  /** Sites offering authenticator 2FA where the entry has no code yet. */
  no_2fa: string[];
  score: number;
};

export type TrashedEntry = EntrySummary & { deleted_at: number };
export type TitleSuggestion = { id: string; title: string; suggested: string };
export type DuplicateGroup = { ids: string[]; title: string; username: string; same_password: boolean };

export type Settings = {
  auto_lock_minutes: number;
  lock_on_sleep: boolean;
  clipboard_clear_secs: number;
  browser_integration: boolean;
  language: string;
  /** How each browser fill is confirmed (desktop). */
  fill_confirm: "system" | "master";
  /** Set only through cloudEnable / cloudDisable. */
  cloud_folder: string;
};

export type MergeReport = { added: number; updated: number; deleted: number };
/** A copy of a vault waiting for its master password before being merged. */
export type StagedCopy = { file: string; same_vault: boolean };

/** A browser fill waiting for the user's OK in the app window (desktop). */
export type FillRequest = {
  id: number;
  title: string;
  username: string;
  host: string;
  mode: "system" | "master";
  /** Who asks: the browser extension, or an AI assistant through the MCP server. */
  source: "browser" | "ai" | "env";
  /** The assistant's name, as it reports it (source "ai"). */
  client: string;
};

export type AiStatus = {
  enabled: boolean;
  has_code: boolean;
  expires: number;
  binary: string;
  shared: number;
  log: { at: number; client: string; title: string; op: "credential" | "totp" | "env"; outcome: string }[];
};

export type BiometricStatus = "unavailable" | "off" | "on" | "master_required";

/** Backend error codes worth special-casing in the UI. */
export type ErrorCode = "locked" | "wrong_password" | "not_found" | "vault_exists" | string;

export const api = {
  status: () => invoke<Status>("status"),
  teamStatus: () => invoke<TeamStatus>("team_status"),
  teamCodeSafety: (codeText: string) => invoke<string>("team_code_safety", { codeText }),
  teamPickFolder: () => invoke<CloudProvider | null>("team_pick_folder"),
  teamCreate: (name: string, me: string, folder: string) => invoke<void>("team_create", { name, me, folder }),
  teamInvite: (codeText: string, name: string) => invoke<void>("team_invite", { codeText, name }),
  teamRemove: (publicKey: string) => invoke<void>("team_remove", { public: publicKey }),
  teamPeek: (folder: string) => invoke<{ name: string; admin_safety: string; ticket: string }>("team_peek", { folder }),
  teamJoin: (folder: string, ticket: string) => invoke<void>("team_join", { folder, ticket }),
  teamLeave: () => invoke<void>("team_leave"),
  profileList: () => invoke<Profiles>("profile_list"),
  profileSwitch: (id: string) => invoke<void>("profile_switch", { id }),
  profileCreate: (name: string, password: string) => invoke<void>("profile_create", { name, password }),
  profileRename: (id: string, name: string) => invoke<void>("profile_rename", { id, name }),
  profileDelete: (id: string, password: string) => invoke<void>("profile_delete", { id, password }),
  createVault: (password: string) => invoke<void>("create_vault", { password }),
  unlock: (password: string) => invoke<void>("unlock", { password }),
  lock: () => invoke<void>("lock"),
  touch: () => invoke<void>("touch"),

  listEntries: () => invoke<EntrySummary[]>("list_entries"),
  getEntry: (id: string) => invoke<Entry>("get_entry", { id }),
  addEntry: (input: EntryInput) => invoke<string>("add_entry", { input }),
  updateEntry: (id: string, input: EntryInput) => invoke<void>("update_entry", { id, input }),
  deleteEntry: (id: string) => invoke<void>("delete_entry", { id }),
  markBackupCode: (id: string, index: number, used: boolean) => invoke<void>("mark_backup_code", { id, index, used }),
  /** Copied by Rust; passwords are auto-cleared from the clipboard after 30 s. */
  copyField: (id: string, field: "username" | "password" | "totp") => invoke<void>("copy_field", { id, field }),
  totpCode: (id: string) => invoke<TotpCode>("totp_code", { id }),

  /** Copies arbitrary text via Rust, auto-cleared after 30 s. */
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  envList: () => invoke<EnvSummary[]>("env_list"),
  envGet: (id: string) => invoke<EnvFile>("env_get", { id }),
  envPut: (project: string, name: string, content: string) => invoke<string>("env_put", { project, name, content }),
  envUpdate: (id: string, content: string) => invoke<void>("env_update", { id, content }),
  envSetAi: (id: string, on: boolean) => invoke<void>("env_set_ai", { id, on }),
  envDelete: (id: string) => invoke<void>("env_delete", { id }),
  envRestore: (id: string) => invoke<void>("env_restore", { id }),
  envDiff: (a: string, b: string) => invoke<EnvDiff>("env_diff", { a, b }),
  envExample: (id: string) => invoke<string>("env_example", { id }),
  envPickFile: () => invoke<{ name: string; content: string } | null>("env_pick_file"),
  envExport: (id: string) => invoke<boolean>("env_export", { id }),
  envScan: () => invoke<FoundEnv[] | null>("env_scan"),
  envImportFound: (items: FoundEnv[]) => invoke<number>("env_import_found", { items }),
  cliStatus: () => invoke<{ installed: boolean; new_terminal: boolean }>("cli_status"),
  cliInstall: () => invoke<{ installed: boolean; new_terminal: boolean }>("cli_install"),
  onEnvUsed: (cb: (e: { title: string; op: string }) => void): Promise<UnlistenFn> => listen<{ title: string; op: string }>("env-used", (e) => cb(e.payload)),
  securityReport: () => invoke<HealthReport>("security_report"),

  generatePassword: (options: GeneratorOptions = defaultGenerator) =>
    invoke<{ password: string; entropy_bits: number }>("generate_password", { options }),
  changeMasterPassword: (current: string, next: string) =>
    invoke<void>("change_master_password", { current, new: next }),
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Settings) => invoke<void>("update_settings", { settings }),
  /** Opens a native file picker; resolves null if cancelled. */
  copyHistory: (id: string, index: number) => invoke<void>("copy_history", { id, index }),
  trashList: () => invoke<TrashedEntry[]>("trash_list"),
  trashRestore: (id: string) => invoke<void>("trash_restore", { id }),
  trashPurge: (id: string) => invoke<void>("trash_purge", { id }),
  trashEmpty: () => invoke<void>("trash_empty"),
  smartSuggestions: () => invoke<{ titles: TitleSuggestion[]; duplicates: DuplicateGroup[] }>("smart_suggestions"),
  applyTitles: (titles: { id: string; title: string }[]) => invoke<number>("apply_titles", { titles }),
  mergeDuplicates: (ids: string[]) => invoke<string>("merge_duplicates", { ids }),
  generatePassphrase: (words: number, separator: string, capitalize: boolean, digit: boolean) =>
    invoke<{ password: string; entropy_bits: number }>("generate_passphrase", { words, separator, capitalize, digit }),
  openChangePassword: (id: string) => invoke<void>("open_change_password", { id }),
  otpAddAnswer: (id: number, choice: string | null) => invoke<void>("otp_add_answer", { id, choice }),
  onOtpAdd: (cb: (r: OtpAddRequest) => void): Promise<UnlistenFn> => listen<OtpAddRequest>("otp-add", (e) => cb(e.payload)),
  onOtpAddDone: (cb: (id: number) => void): Promise<UnlistenFn> => listen<number>("otp-add-done", (e) => cb(e.payload)),
  /** The browser extension asked to show an entry (search result in the popup). */
  onOpenEntry: (cb: (id: string) => void): Promise<UnlistenFn> => listen<string>("open-entry", (e) => cb(e.payload)),
  onOtpAdded: (cb: (title: string) => void): Promise<UnlistenFn> => listen<{ title: string }>("otp-added", (e) => cb(e.payload.title)),
  aiStatus: () => invoke<AiStatus>("ai_status"),
  aiSetEnabled: (enabled: boolean) => invoke<void>("ai_set_enabled", { enabled }),
  aiNewCode: (hours: number) => invoke<{ code: string; config: string }>("ai_new_code", { hours }),
  aiRevoke: () => invoke<void>("ai_revoke"),
  onAiUsed: (cb: (e: { client: string; title: string; op: string }) => void): Promise<UnlistenFn> =>
    listen<{ client: string; title: string; op: string }>("ai-used", (e) => cb(e.payload)),
  cloudProviders: () => invoke<CloudProvider[]>("cloud_providers"),
  /** Android: the system folder picker. */
  cloudPickFolder: () => invoke<CloudProvider | null>("cloud_pick_folder"),
  cloudStatus: () => invoke<CloudStatus>("cloud_status"),
  cloudEnable: (folder: string, password: string, cloudPassword?: string) =>
    invoke<{ outcome: "created" | "synced" | "joined"; added: number }>("cloud_enable", { folder, password, cloudPassword }),
  cloudDisable: () => invoke<void>("cloud_disable"),
  onVaultSynced: (cb: (r: SyncReport) => void): Promise<UnlistenFn> =>
    listen<{ report: SyncReport }>("vault-synced", (e) => cb(e.payload.report)),
  importOtp: () => invoke<{ linked: number; added: number; skipped: number; file: string } | null>("import_otp"),
  readBackupCodes: () => invoke<string[] | null>("read_backup_codes"),
  importCsv: () => invoke<{ added: number; skipped: number; file: string } | null>("import_csv"),
  exportBackup: () => invoke<boolean>("export_backup"),
  connectBrowser: () => invoke<{ browsers: string[] }>("connect_browser"),
  disconnectBrowser: () => invoke<void>("disconnect_browser"),
  /** Android: whether Kryptos is the system autofill service. */
  autofillStatus: () => invoke<boolean>("autofill_status"),
  /** Android: opens the system prompt to select Kryptos as autofill service. */
  openAutofillSettings: () => invoke<void>("open_autofill_settings"),

  /** Fingerprint unlock (Android). The prompt calls reject with a code such as
   *  "cancelled", "use_master", "lockout", "invalidated", "master_required". */
  biometricStatus: () => invoke<BiometricStatus>("biometric_status"),
  /** Android: saves a login the user accepted to keep while the vault was locked. */
  applyPendingSave: () => invoke<string | null>("apply_pending_save"),
  biometricEnable: () => invoke<void>("biometric_enable"),
  biometricDisable: () => invoke<void>("biometric_disable"),
  biometricUnlock: () => invoke<void>("biometric_unlock"),

  /** Sync without network: pick a backup / hand over a copy received as QR codes
   *  (base64), then merge it with that copy's master password. */
  syncStageFile: () => invoke<StagedCopy | null>("sync_stage_file"),
  syncStageBytes: (data: string) => invoke<StagedCopy>("sync_stage_bytes", { data }),
  syncMerge: (password: string) => invoke<MergeReport>("sync_merge", { password }),
  syncCancel: () => invoke<void>("sync_cancel"),
  /** This vault (encrypted) as QR codes to show in a loop, one SVG per frame. */
  syncQrFrames: () => invoke<{ frames: string[]; bytes: number }>("sync_qr_frames"),
  /** Android: encrypted backup to the system share sheet. */
  shareBackup: () => invoke<void>("share_backup"),

  /** Browser fill confirmation (desktop). `reason` is shown in the Touch ID prompt.
   *  Rejects with "cancelled", "failed", "wrong_password" or "timeout" (request gone). */
  fillConfirmSystem: (id: number, reason: string) => invoke<void>("fill_confirm_system", { id, reason }),
  fillConfirmMaster: (id: number, password: string) => invoke<void>("fill_confirm_master", { id, password }),
  fillReasonTemplate: (template: string) => invoke<void>("fill_reason_template", { template }),
  fillConfirmCancel: (id: number) => invoke<void>("fill_confirm_cancel", { id }),
  onFillConfirm: (cb: (r: FillRequest) => void): Promise<UnlistenFn> =>
    listen<FillRequest>("fill-confirm", (e) => cb(e.payload)),
  /** The request was answered or expired: close its dialog. */
  onFillConfirmDone: (cb: (id: number) => void): Promise<UnlistenFn> =>
    listen<number>("fill-confirm-done", (e) => cb(e.payload)),

  onLocked: (cb: () => void): Promise<UnlistenFn> => listen("vault-locked", cb),
  /** Unlocked outside the UI (fingerprint from an autofill suggestion). */
  onUnlocked: (cb: () => void): Promise<UnlistenFn> => listen("vault-unlocked", cb),
  /** A login typed elsewhere was saved: Android's save sheet, or the extension's "Save login" button. */
  onLoginSaved: (cb: (e: { title: string; outcome: "added" | "updated" | "unchanged" }) => void): Promise<UnlistenFn> =>
    listen<{ title: string; outcome: "added" | "updated" | "unchanged" }>("login-saved", (e) => cb(e.payload)),
  onBrowserFill: (cb: (e: { title: string; host: string }) => void): Promise<UnlistenFn> =>
    listen<{ title: string; host: string }>("browser-fill", (e) => cb(e.payload)),
};
