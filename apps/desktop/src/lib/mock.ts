// Browser-only design preview: fakes the Rust backend with in-memory sample data.
// Loaded only by `npm run dev` outside Tauri (see main.tsx); never in production builds.
type Any = Record<string, any>;
const reg: Any = { active: "default", profiles: [{ id: "default", name: "" }, { id: "p1", name: "Team Oikos" }] };
let cliOn = false;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let team: any = null;
const envs: Any[] = [
  { id: "env1", project: "Oikos", name: "development", content: "# database\nDATABASE_URL=postgres://localhost/oikos\nDEBUG=true\n", ai_access: false, history: [] },
  { id: "env2", project: "Oikos", name: "production", content: "DATABASE_URL=postgres://db/oikos\nSTRIPE_KEY=sk_live_demo\n", ai_access: false, history: [{ content: "DATABASE_URL=old\n", replaced_at: 1790000000 }] },
];

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
].map((e, i) => ({ history: e.title === "GitHub" ? [{ password: "oldGitHub#2024", replaced_at: now - 90 * day }] : [] as Any[], ai_access: e.title === "GitHub", totp: e.title === "GitHub" ? "JBSWY3DPEHPK3PXP" : "", backup_codes: e.title === "GitHub" ? ["a1b2c-3d4e5", "f6g7h-8i9j0", "k1l2m-3n4o5", "p6q7r-8s9t0"].map((code, n) => ({ code, used: n === 0 })) : [] as Any[], ...e, id: `id-${i}`, notes: "", created_at: now - e.age * day, updated_at: now - e.age * day }));

const usage: Record<string, number> = { "id-3": now - 60, "id-0": now - 3600 };
const trash: Any[] = [{ id: "id-90", title: "Old forum", username: "mario", urls: ["forum.test"], favorite: false, updated_at: now - 400 * day, has_totp: false, ai_access: false, last_used: 0, deleted_at: now - 2 * day }];
const summary = (e: Any) => ({ id: e.id, title: e.title, username: e.username, urls: e.urls, favorite: e.favorite, updated_at: e.updated_at, has_totp: !!e.totp, ai_access: !!e.ai_access, last_used: usage[e.id] ?? 0 });

function report() {
  const weak = entries.filter((e) => e.password.length < 10 || (e.password.length < 16 && !/[^A-Za-z0-9]/.test(e.password))).map((e) => e.id);
  const counts = new Map<string, number>();
  entries.forEach((e) => counts.set(e.password, (counts.get(e.password) ?? 0) + 1));
  const reused = entries.filter((e) => counts.get(e.password)! > 1).map((e) => e.id);
  const old = entries.filter((e) => e.updated_at < now - 365 * day).map((e) => e.id);
  const flagged = new Set([...weak, ...reused, ...old]).size;
  const no_2fa = entries.filter((e) => !e.totp && /google|amazon|github|instagram/.test(e.urls.join())).map((e) => e.id);
  return { total: entries.length, weak, reused, old, no_2fa, score: entries.length ? Math.floor((100 * (entries.length - flagged)) / entries.length) : 100 };
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
    case "status": return { exists, unlocked, platform: /Android|iPhone/.test(navigator.userAgent) ? "android" : "macos", profile: reg.active };
    case "team_status": return { code: "kryptos_member_" + "a1b2".repeat(16), safety: "4821 0937 5512 7706", team };
    case "team_code_safety": if (!a.codeText.startsWith("kryptos_member_")) throw "team_bad_code"; return "1234 5678 9012 3456";
    case "team_pick_folder": return { name: "Team Oikos", path: "/Users/me/Drive/Team Oikos", has_vault: true };
    case "team_create": team = { name: a.name, is_admin: true, admin_safety: "4821 0937 5512 7706", members: [{ public: "me", name: a.me, safety: "4821 0937 5512 7706", added_at: 0, me: true, admin: true }] }; return;
    case "team_invite": team.members.push({ public: `m${team.members.length}`, name: a.name, safety: "1234 5678 9012 3456", added_at: 0, me: false, admin: false }); return;
    case "team_remove": team.members = team.members.filter((m: Any) => m.public !== a.public); return;
    case "team_peek": return { name: "Oikos", admin_safety: "7310 2298 4471 0056", ticket: "t" };
    case "team_join": team = { name: "Oikos", is_admin: false, admin_safety: "7310 2298 4471 0056", members: [{ public: "a", name: "Simone", safety: "7310 2298 4471 0056", added_at: 0, me: false, admin: true }, { public: "me", name: "Marco", safety: "4821 0937 5512 7706", added_at: 0, me: true, admin: false }] }; return;
    case "team_leave": team = null; return;
    case "profile_list": return reg;
    case "profile_switch": reg.active = a.id; unlocked = false; return;
    case "profile_create": await wait(600); { const id = `p${reg.profiles.length}`; reg.profiles.push({ id, name: a.name }); reg.active = id; } return;
    case "profile_rename": reg.profiles.find((p: Any) => p.id === a.id)!.name = a.name; return;
    case "profile_delete": if (a.password !== "password") throw "wrong_password"; reg.profiles = reg.profiles.filter((p: Any) => p.id !== a.id); return;
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
    case "otp_add_answer": emit("otp-add-done", a.id); if (a.choice) emit("otp-added", { title: "GitHub" }); return;
    case "copy_history": return;
    case "env_list": return envs.map((e) => ({ id: e.id, project: e.project, name: e.name, vars: e.content.split("\n").filter((l: string) => l.includes("=")).length, versions: e.history.length, ai_access: e.ai_access, production: e.name.startsWith("prod"), updated_at: now, deleted_at: e.deleted_at ?? 0 }));
    case "env_get": return envs.find((e) => e.id === a.id);
    case "env_put": { const id = `env${envs.length + 1}`; envs.push({ id, project: a.project, name: a.name, content: a.content, ai_access: false, history: [] }); return id; }
    case "env_update": { const e = envs.find((x) => x.id === a.id)!; e.history.unshift({ content: e.content, replaced_at: now }); e.content = a.content; return; }
    case "env_set_ai": envs.find((e) => e.id === a.id)!.ai_access = a.on; return;
    case "env_delete": envs.find((e) => e.id === a.id)!.deleted_at = now; return;
    case "env_restore": envs.find((e) => e.id === a.id)!.deleted_at = 0; return;
    case "env_diff": return { only_a: ["DEBUG"], only_b: ["STRIPE_KEY"], different: ["DATABASE_URL"] };
    case "env_example": return "DATABASE_URL=\nDEBUG=\n";
    case "env_pick_file": return { name: "staging", content: "A=1\n" };
    case "env_export": return true;
    case "env_scan": return [{ path: "/Users/me/progetti/oikos/.env", project: "oikos", name: "development", vars: 8 }, { path: "/Users/me/progetti/sito/.env.local", project: "sito", name: "local", vars: 3 }];
    case "env_import_found": return a.items.length;
    case "cli_status": return { installed: cliOn, new_terminal: false };
    case "cli_install": await wait(400); cliOn = true; return { installed: true, new_terminal: true };
    case "ai_status": return { enabled: aiOn, has_code: aiCode, expires: aiCode ? now + 86400 : 0, binary: "/Applications/Kryptos.app/Contents/MacOS/kryptos-mcp", shared: 1, log: aiOn ? [{ at: now - 120, client: "Claude", title: "GitHub", op: "credential", outcome: "given" }, { at: now - 900, client: "Claude", title: "GitHub", op: "totp", outcome: "denied" }] : [] };
    case "ai_set_enabled": aiOn = a.enabled; return;
    case "ai_new_code": aiCode = true; return { code: "kryptos_ai_demo", config: JSON.stringify({ mcpServers: { kryptos: { command: "/Applications/Kryptos.app/Contents/MacOS/kryptos-mcp", env: { KRYPTOS_MCP_TOKEN: "kryptos_ai_demo" } } } }, null, 2) };
    case "ai_revoke": aiCode = false; return;
    case "trash_list": return trash;
    case "trash_restore": case "trash_purge": trash.splice(0); return;
    case "trash_empty": trash.splice(0); return;
    case "smart_suggestions": return { titles: [{ id: "id-1", title: "netflix.com", suggested: "Netflix" }], duplicates: [{ ids: ["id-0", "id-4"], title: "Google", username: "mario.rossi@example.com", same_password: false }] };
    case "apply_titles": return a.titles.length;
    case "merge_duplicates": return a.ids[0];
    case "generate_passphrase": return { password: Array.from({ length: a.words }, (_, i) => ["tavolo", "nuvola", "ferro", "salto", "viola", "carta", "lume", "onda", "picco", "rame"][i]).join(a.separator) + (a.digit ? "7" : ""), entropy_bits: a.words * 12.9 };
    case "open_change_password": return;
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
    case "fill_reason_template": return;
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
let aiOn = false;
let aiCode = false;
/** Preview an AI assistant's request: run `__mockAi()` in the console. */
(window as Any).__mockAi = () =>
  emit("fill-confirm", { id: ++fillSeq, title: "GitHub", username: "mrossi", host: "github.com", mode: settings.fill_confirm, source: "ai", client: "Claude" });
/** Preview the 2FA-from-QR dialog: run `__mockOtpAdd()` in the console. */
(window as Any).__mockOtpAdd = () =>
  emit("otp-add", { id: ++fillSeq, host: "github.com", issuer: "GitHub", account: "mrossi", candidates: [{ id: "id-3", title: "GitHub", username: "mrossi" }] });
/** Preview the browser-fill confirmation: run `__mockFill()` in the console. */
(window as Any).__mockFill = () =>
  emit("fill-confirm", { id: ++fillSeq, title: "GitHub", username: "mrossi", host: "github.com", mode: settings.fill_confirm, source: "browser", client: "" });
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
