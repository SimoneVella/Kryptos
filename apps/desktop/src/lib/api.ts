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

  onLocked: (cb: () => void): Promise<UnlistenFn> => listen("vault-locked", cb),
  onBrowserFill: (cb: (e: { title: string; host: string }) => void): Promise<UnlistenFn> =>
    listen<{ title: string; host: string }>("browser-fill", (e) => cb(e.payload)),
};
