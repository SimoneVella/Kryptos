// Typed wrapper around the Rust commands. This is the only file that talks to
// the backend: screens/components can be redesigned freely on top of it.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Status = { exists: boolean; unlocked: boolean; platform: string };

export type EntrySummary = {
  id: string;
  title: string;
  username: string;
  urls: string[];
  favorite: boolean;
  updated_at: number;
};

export type Entry = EntrySummary & {
  password: string;
  notes: string;
  created_at: number;
};

export type EntryInput = {
  title: string;
  username: string;
  password: string;
  urls: string[];
  notes: string;
  favorite: boolean;
};

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
  score: number;
};

export type Settings = {
  auto_lock_minutes: number;
  lock_on_sleep: boolean;
  clipboard_clear_secs: number;
  browser_integration: boolean;
  language: string;
};

export type MergeReport = { added: number; updated: number; deleted: number };
/** A copy of a vault waiting for its master password before being merged. */
export type StagedCopy = { file: string; same_vault: boolean };

export type BiometricStatus = "unavailable" | "off" | "on" | "master_required";

/** Backend error codes worth special-casing in the UI. */
export type ErrorCode = "locked" | "wrong_password" | "not_found" | "vault_exists" | string;

export const api = {
  status: () => invoke<Status>("status"),
  createVault: (password: string) => invoke<void>("create_vault", { password }),
  unlock: (password: string) => invoke<void>("unlock", { password }),
  lock: () => invoke<void>("lock"),
  touch: () => invoke<void>("touch"),

  listEntries: () => invoke<EntrySummary[]>("list_entries"),
  getEntry: (id: string) => invoke<Entry>("get_entry", { id }),
  addEntry: (input: EntryInput) => invoke<string>("add_entry", { input }),
  updateEntry: (id: string, input: EntryInput) => invoke<void>("update_entry", { id, input }),
  deleteEntry: (id: string) => invoke<void>("delete_entry", { id }),
  /** Copied by Rust; passwords are auto-cleared from the clipboard after 30 s. */
  copyField: (id: string, field: "username" | "password") => invoke<void>("copy_field", { id, field }),

  /** Copies arbitrary text via Rust, auto-cleared after 30 s. */
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  securityReport: () => invoke<HealthReport>("security_report"),

  generatePassword: (options: GeneratorOptions = defaultGenerator) =>
    invoke<{ password: string; entropy_bits: number }>("generate_password", { options }),
  changeMasterPassword: (current: string, next: string) =>
    invoke<void>("change_master_password", { current, new: next }),
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Settings) => invoke<void>("update_settings", { settings }),
  /** Opens a native file picker; resolves null if cancelled. */
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

  onLocked: (cb: () => void): Promise<UnlistenFn> => listen("vault-locked", cb),
  /** Unlocked outside the UI (fingerprint from an autofill suggestion). */
  onUnlocked: (cb: () => void): Promise<UnlistenFn> => listen("vault-unlocked", cb),
  onBrowserFill: (cb: (e: { title: string; host: string }) => void): Promise<UnlistenFn> =>
    listen<{ title: string; host: string }>("browser-fill", (e) => cb(e.payload)),
};
