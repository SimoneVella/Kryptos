// Browser-only design preview: fakes the Rust backend with in-memory sample data.
// Loaded only by `npm run dev` outside Tauri (see main.tsx); never in production builds.
type Any = Record<string, any>;

const day = 86400;
const now = Math.floor(Date.now() / 1000);
let unlocked = false;
let exists = true;
let seq = 100;
const settings = { auto_lock_minutes: 5, lock_on_sleep: true, clipboard_clear_secs: 30, browser_integration: false };
const entries: Any[] = [
  { title: "Google", username: "simone@gmail.com", password: "Xk9#mP2$vL8@qR4!wT", urls: ["google.com"], favorite: true, age: 20 },
  { title: "Netflix", username: "simone@gmail.com", password: "netflix2021", urls: ["netflix.com"], favorite: false, age: 400 },
  { title: "Intesa Sanpaolo", username: "12345678", password: "Tq7!vB3#nM9$wZ5%", urls: ["intesasanpaolo.com"], favorite: true, age: 60 },
  { title: "GitHub", username: "simonevella", password: "hN4&kP8*rS2^fD6!", urls: ["github.com"], favorite: false, age: 5 },
  { title: "Amazon", username: "simone@gmail.com", password: "netflix2021", urls: ["amazon.it"], favorite: false, age: 90 },
  { title: "Spotify", username: "simone", password: "cV5@jL9#xB2$", urls: ["spotify.com"], favorite: false, age: 200 },
  { title: "Instagram", username: "@simone", password: "Wm3!zQ7&pY1*eR8^", urls: ["instagram.com"], favorite: false, age: 30 },
].map((e, i) => ({ ...e, id: `id-${i}`, notes: "", created_at: now - e.age * day, updated_at: now - e.age * day }));

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
    case "list_entries": return entries.map(summary).sort((x, y) => Number(y.favorite) - Number(x.favorite) || x.title.localeCompare(y.title));
    case "get_entry": return entries.find((e) => e.id === a.id);
    case "add_entry": { const id = `id-${seq++}`; entries.push({ ...a.input, id, created_at: now, updated_at: now }); return id; }
    case "update_entry": Object.assign(entries.find((e) => e.id === a.id)!, a.input, { updated_at: now }); return;
    case "delete_entry": entries.splice(entries.findIndex((e) => e.id === a.id), 1); return;
    case "security_report": return report();
    case "generate_password": return gen(a.options);
    case "change_master_password": await wait(500); if (a.current !== "password") throw "wrong_password"; return;
    case "plugin:event|listen": return 1;
    case "plugin:event|unlisten": return;
  }
  throw `mock: unknown command ${cmd}`;
}

let cb = 0;
(window as Any).__TAURI_INTERNALS__ = {
  invoke,
  transformCallback: () => ++cb,
  metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
};
console.info("[Kryptos] design preview with mock data — master password: \"password\"");
export {};
