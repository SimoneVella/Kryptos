// Browser-only design preview: fakes the Rust backend with in-memory sample data.
// Loaded only by `npm run dev` outside Tauri (see main.tsx); never in production builds.
type Any = Record<string, any>;

const day = 86400;
const now = Math.floor(Date.now() / 1000);
let unlocked = false;
let exists = true;
let seq = 100;
let autofill = false;
let biometric = "off";
const settings = { auto_lock_minutes: 5, lock_on_sleep: true, clipboard_clear_secs: 30, browser_integration: false, language: "system", fill_confirm: "system", cloud_folder: "" };
const entries: Any[] = [
  { title: "Google", username: "mario.rossi@example.com", password: "Xk9#mP2$vL8@qR4!wT", urls: ["google.com"], favorite: true, age: 20 },
  { title: "Netflix", username: "mario.rossi@example.com", password: "netflix2021", urls: ["netflix.com"], favorite: false, age: 400 },
  { title: "Intesa Sanpaolo", username: "88123456", password: "Tq7!vB3#nM9$wZ5%", urls: ["intesasanpaolo.com"], favorite: true, age: 60 },
  { title: "GitHub", username: "mrossi", password: "hN4&kP8*rS2^fD6!", urls: ["github.com"], favorite: false, age: 5 },
  { title: "Amazon", username: "mario.rossi@example.com", password: "netflix2021", urls: ["amazon.it"], favorite: false, age: 90 },
  { title: "Spotify", username: "mario.rossi", password: "cV5@jL9#xB2$", urls: ["spotify.com"], favorite: false, age: 200 },
  { title: "Instagram", username: "@mario.rossi", password: "Wm3!zQ7&pY1*eR8^", urls: ["instagram.com"], favorite: false, age: 30 },
].map((e, i) => ({ totp: e.title === "GitHub" ? "JBSWY3DPEHPK3PXP" : "", backup_codes: e.title === "GitHub" ? ["a1b2c-3d4e5", "f6g7h-8i9j0", "k1l2m-3n4o5", "p6q7r-8s9t0"].map((code, n) => ({ code, used: n === 0 })) : [] as Any[], ...e, id: `id-${i}`, notes: "", created_at: now - e.age * day, updated_at: now - e.age * day }));

const summary = (e: Any) => ({ id: e.id, title: e.title, username: e.username, urls: e.urls, favorite: e.favorite, updated_at: e.updated_at });

function report() {
  const weak = entries.filter((e) => e.password.length < 10 || (e.password.length < 16 && !/[^A-Za-z0-9]/.test(e.password))).map((e) => e.id);
  const counts = new Map<string, number>();
  entries.forEach((e) => counts.set(e.password, (counts.get(e.password) ?? 0) + 1));
  const reused = entries.filter((e) => counts.get(e.password)! > 1).map((e) => e.id);
  const old = entries.filter((e) => e.updated_at < now - 365 * day).map((e) => e.id);
  const flagged = new Set([...weak, ...reused, ...old]).size;
  return { total: entries.length, weak, reused, old, score: entries.length ? Math.floor((100 * (entries.length - flagged)) / entries.length) : 100 };
}

function gen(o: Any) {
  let set = "";
  if (o.lowercase) set += "abcdefghijkmnpqrstuvwxyz";
  if (o.uppercase) set += "ABCDEFGHJKLMNPQRSTUVWXYZ";
  if (o.digits) set += "23456789";
  if (o.symbols) set += "!@#$%^&*()-_=+[]{};:,.<>/?~";
  const a = crypto.getRandomValues(new Uint32Array(o.length));
  const pool = set.length;
  return { password: [...a].map((n) => set[n % pool]).join(""), entropy_bits: o.length * Math.log2(pool) };
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function invoke(cmd: string, a: Any = {}): Promise<unknown> {
  switch (cmd) {
    case "status": return { exists, unlocked, platform: /Android|iPhone/.test(navigator.userAgent) ? "android" : "macos" };
    case "create_vault": await wait(600); exists = unlocked = true; return;
    case "unlock": await wait(500); if (a.password !== "password") throw "wrong_password"; unlocked = true; return;
    case "lock": unlocked = false; return;
    case "touch": case "copy_field": case "copy_text": return;
    case "get_settings": return { ...settings };
    case "update_settings": Object.assign(settings, a.settings); return;
    case "import_csv": await wait(400); return { added: 12, skipped: 2, file: "Password di Chrome.csv" };
    case "export_backup": return true;
    case "connect_browser": settings.browser_integration = true; return { browsers: ["Chrome", "Arc"] };
    case "disconnect_browser": settings.browser_integration = false; return;
    case "autofill_status": return autofill;
    case "open_autofill_settings": autofill = true; return;
    case "biometric_status": return biometric;
    case "cloud_providers": return [{ name: "Google Drive", path: "/Users/me/Library/CloudStorage/GoogleDrive-me@gmail.com/Il mio Drive", has_vault: true }, { name: "iCloud Drive", path: "/Users/me/Library/Mobile Documents/com~apple~CloudDocs", has_vault: false }];
    case "cloud_pick_folder": await wait(300); return { name: "Il mio Drive", path: "content://com.google.android.apps.docs.storage/tree/abc", has_vault: true };
    case "cloud_status": return { folder: settings.cloud_folder, name: "", status: { state: settings.cloud_folder ? "ok" : "off", last_sync: settings.cloud_folder ? Math.floor(Date.now() / 1000) : 0 } };
    case "cloud_enable": await wait(700); if (a.password !== "password" && a.password !== "tavolo-nuvola-ferro-salto-viola") throw "wrong_password"; if (!a.cloudPassword) throw "cloud_other_password"; settings.cloud_folder = a.folder; return { outcome: "joined", added: 3 };
    case "cloud_disable": settings.cloud_folder = ""; return;
    case "import_otp": await wait(400); return { linked: 3, added: 2, skipped: 1, file: "aegis-export.json" };
    case "read_backup_codes": return ["1234 5678", "2345 6789", "3456 7890"];
    case "mark_backup_code": entries.find((e) => e.id === a.id)!.backup_codes[a.index].used = a.used; return;
    case "totp_code": { const t = Math.floor(Date.now() / 1000); return { code: String(100000 + (Math.floor(t / 30) * 7919) % 900000), remaining: 30 - (t % 30), period: 30 }; }
    case "apply_pending_save": return null;
    case "sync_stage_file": return { file: "Kryptos-backup-2026-09-29.kryptos", same_vault: true };
    case "sync_stage_bytes": return { file: "", same_vault: true };
    case "sync_merge": await wait(600); if (a.password !== "password") throw "wrong_password"; return { added: 2, updated: 1, deleted: 0 };
    case "sync_cancel": case "share_backup": return;
    case "sync_qr_frames": return { frames: [], bytes: 0 };
    case "biometric_enable": await wait(400); biometric = "on"; return;
    case "biometric_disable": biometric = "off"; return;
    case "biometric_unlock": await wait(400); unlocked = true; return;
    case "list_entries": return entries.map(summary).sort((x, y) => Number(y.favorite) - Number(x.favorite) || x.title.localeCompare(y.title));
    case "get_entry": return entries.find((e) => e.id === a.id);
    case "add_entry": { const id = `id-${seq++}`; entries.push({ ...a.input, id, created_at: now, updated_at: now }); return id; }
    case "update_entry": Object.assign(entries.find((e) => e.id === a.id)!, a.input, { updated_at: now }); return;
    case "delete_entry": entries.splice(entries.findIndex((e) => e.id === a.id), 1); return;
    case "security_report": return report();
    case "generate_password": return gen(a.options);
    case "change_master_password": await wait(500); if (a.current !== "password") throw "wrong_password"; return;
    case "fill_confirm_system": await wait(800); emit("fill-confirm-done", a.id); return;
    case "fill_confirm_master": await wait(500); if (a.password !== "password") throw "wrong_password"; emit("fill-confirm-done", a.id); return;
    case "fill_confirm_cancel": emit("fill-confirm-done", a.id); return;
    case "plugin:event|listen": (listeners[a.event] ??= []).push(a.handler); return a.handler;
    case "plugin:event|unlisten": return;
  }
  throw `mock: unknown command ${cmd}`;
}

let cb = 0;
const callbacks = new Map<number, (e: Any) => void>();
const listeners: Record<string, number[]> = {};
function emit(event: string, payload: unknown) {
  for (const h of listeners[event] ?? []) callbacks.get(h)?.({ event, id: 0, payload });
}
let fillSeq = 0;
/** Preview the browser-fill confirmation: run `__mockFill()` in the console. */
(window as Any).__mockFill = () =>
  emit("fill-confirm", { id: ++fillSeq, title: "GitHub", username: "mrossi", host: "github.com", mode: settings.fill_confirm });
(window as Any).__TAURI_INTERNALS__ = {
  invoke,
  transformCallback: (f: (e: Any) => void) => {
    callbacks.set(++cb, f);
    return cb;
  },
  metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
};
(window as Any).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
  unregisterListener: (event: string, id: number) => {
    listeners[event] = (listeners[event] ?? []).filter((h) => h !== id);
  },
};
console.info("[Kryptos] design preview with mock data — master password: \"password\"");
export {};
