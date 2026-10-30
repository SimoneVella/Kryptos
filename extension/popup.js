// Kryptos popup. Talks to the desktop app only through Native Messaging
// (browser -> kryptos-native-host over stdio -> app over a Unix socket).
// Fills, saves and generated passwords inject one-shot functions only when the
// user asks, using the activeTab permission, and the visible tab is captured only
// to read a 2FA QR code when the user presses that button. The one script on pages
// is content.js, which offers logins under a login field and can be turned off here.

const HOST = "com.kryptos.bridge";
// Prefer the callback-style `chrome` namespace (also present in Firefox).
const ext = globalThis.chrome ?? globalThis.browser;
const m = (key, subs) => ext.i18n.getMessage(key, subs);
const $ = (id) => document.getElementById(id);
const $content = $("content");
const $actions = $("actions");

/** A Material Symbols icon (the font is a subset made by apps/desktop/scripts/gen-material-icons.mjs). */
const I = (name, fill) => `<span class="ms${fill ? " f" : ""}">${name}</span>`;

document.documentElement.lang = ext.i18n.getUILanguage();
for (const node of document.querySelectorAll("[data-i18n]")) node.textContent = m(node.dataset.i18n);
for (const node of document.querySelectorAll("[data-i18n-title]")) node.title = m(node.dataset.i18nTitle);
// Static, trusted markup only below.
$("open-app").innerHTML = I("open_in_new");
const TAB_ICONS = { site: I("key"), search: I("search"), gen: I("casino") };
for (const b of document.querySelectorAll("#tabs button")) b.querySelector(".ms").outerHTML = TAB_ICONS[b.dataset.tab];
$("save-btn").querySelector(".ms").outerHTML = I("add");
$("qr-btn").querySelector(".ms").outerHTML = I("qr_code_scanner");

const $inlineOn = $("inline-on");
ext.storage.local.get("inlineOff", (v) => ($inlineOn.checked = !v.inlineOff));
$inlineOn.addEventListener("change", () => ext.storage.local.set({ inlineOff: !$inlineOn.checked }));

function send(msg) {
  return new Promise((resolve) => {
    try {
      ext.runtime.sendNativeMessage(HOST, msg, (reply) => {
        if (ext.runtime.lastError || !reply) resolve({ ok: false, error: "host_unavailable" });
        else resolve(reply);
      });
    } catch {
      resolve({ ok: false, error: "host_unavailable" });
    }
  });
}

/** Requests that wait for the user in the app window go through background.js:
 *  the popup closes as soon as Kryptos takes the focus. */
function viaBackground(msg) {
  return new Promise((resolve) => ext.runtime.sendMessage(msg, (r) => resolve(ext.runtime.lastError ? null : r)));
}

function el(tag, props = {}, ...children) {
  const n = Object.assign(document.createElement(tag), props);
  n.append(...children);
  return n;
}

function state(icon, title, text, action) {
  $actions.hidden = true;
  const box = el("div", { className: "state" });
  const i = el("div", { className: "state-icon" });
  i.innerHTML = I(icon); // static, trusted markup
  box.append(i, el("h2", { textContent: title }));
  if (text) box.append(typeof text === "string" ? el("p", { textContent: text }) : text);
  if (action) box.append(el("button", { className: "btn", textContent: action.label, onclick: action.run }));
  $content.replaceChildren(box);
  box.querySelector("button")?.focus();
}

const HUES = [217, 262, 330, 12, 38, 150, 190, 290];
// Offline brand logos (simple-icons, CC0), generated into brand-icons.js at build
// time. If the file is missing the popup falls back to initial-letter avatars.
const BRAND_ALIASES = { "youtu.be": "youtube", "twitter.com": "x", "fb.com": "facebook", "steampowered.com": "steam", "booking.com": "bookingdotcom", "proton.me": "proton", "npmjs.com": "npm", "chatgpt.com": "openai", "claude.ai": "claude" };
const SECOND_LEVEL = new Set(["co.uk", "org.uk", "com.au", "co.jp", "com.br", "com.mx", "co.in", "co.nz", "co.za", "com.tr", "com.ar", "co.kr", "com.sg"]);
const norm = (s) => s.toLowerCase().replace(/[^a-z0-9]/g, "");
const hostOf = (u) => {
  try {
    return new URL(/^[a-z]+:\/\//i.test(u) ? u : `https://${u}`).hostname.replace(/^www\./, "");
  } catch {
    return "";
  }
};

function brandIcon(title, host) {
  const table = globalThis.KRYPTOS_BRAND_ICONS;
  if (!table || !host) return null;
  const parts = host.split(".");
  const n = SECOND_LEVEL.has(parts.slice(-2).join(".")) ? 3 : 2;
  const domain = parts.slice(-n).join(".");
  for (const slug of [BRAND_ALIASES[host], BRAND_ALIASES[domain], norm(parts[parts.length - n] ?? ""), norm(title)]) {
    const hit = slug && table[slug];
    if (hit && /^[0-9A-Fa-f]{6}$/.test(hit[0])) return hit;
  }
  return null;
}

function avatar(name, host) {
  const brand = brandIcon(name, host);
  if (brand) {
    const [hex, path] = brand;
    const [r, g, b] = [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16));
    const lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    const p = document.createElementNS("http://www.w3.org/2000/svg", "path");
    p.setAttribute("d", path);
    p.setAttribute("fill", lum > 150 ? "#111" : "#fff");
    svg.append(p);
    const div = el("div", { className: "avatar", style: `background: ${lum < 30 ? "#1d1f24" : `#${hex}`}` });
    div.append(svg);
    return div;
  }
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return el("div", { className: "avatar", textContent: (name.trim()[0] ?? "?").toUpperCase(), style: `background: hsl(${HUES[h % HUES.length]} 38% 42%)` });
}

const openApp = { label: m("openApp"), run: () => send({ type: "focus" }).then(() => window.close()) };
$("open-app").onclick = openApp.run;

function showError(error) {
  switch (error) {
    case "locked":
      return state("lock", m("vaultLockedTitle"), m("vaultLockedText"), openApp);
    case "app_not_running":
      return state("key", m("appClosedTitle"), m("appClosedText"));
    case "host_unavailable": {
      const p = el("p", {}, m("connectAppPre"), el("b", { textContent: m("connectAppSettingsPath") }), m("connectAppMid"), el("b", { textContent: m("connectAppButton") }), m("connectAppPost"));
      return state("lock", m("connectAppTitle"), p);
    }
    case "untrusted_caller":
    case "untrusted_peer":
      return state("lock", m("untrustedTitle"), m("untrustedText"));
    default:
      return state("warning", m("genericErrorTitle"), error);
  }
}

// ───────── Tabs ─────────

let tab = null;
let url = null;
const TABS = { site: showSite, search: showSearch, gen: showGenerator };
for (const b of document.querySelectorAll("#tabs button")) {
  b.onclick = () => {
    for (const x of document.querySelectorAll("#tabs button")) x.classList.toggle("on", x === b);
    TABS[b.dataset.tab]();
  };
}

async function main() {
  [tab] = await ext.tabs.query({ active: true, currentWindow: true });
  try {
    url = new URL(tab.url);
  } catch {}
  if (!url || !/^https?:$/.test(url.protocol)) url = null;
  $("site").textContent = url ? url.hostname.replace(/^www\./, "") : m("noSite");
  if (url) showSite();
  else {
    for (const x of document.querySelectorAll("#tabs button")) x.classList.toggle("on", x.dataset.tab === "search");
    showSearch();
  }
}

// ───────── This site ─────────

async function showSite() {
  if (!url) return state("language", m("unsupportedPageTitle"), m("unsupportedPageText"));
  const host = url.hostname.replace(/^www\./, "");
  const [res, look] = await Promise.all([send({ type: "logins", url: url.href }), send({ type: "lookalike", url: url.href })]);
  if (!res.ok) return showError(res.error);

  const items = [];
  // A page imitating a saved site: say it loudly, before anything else.
  if (look.ok && look.similar) {
    const w = el("div", { className: "warn" });
    w.innerHTML = I("warning", true); // static, trusted markup
    w.append(el("div", {}, el("b", { textContent: m("lookalikeTitle") }), m("lookalikeText", [look.similar.title, look.similar.domain])));
    items.push(w);
  }
  if (res.logins.length === 0) {
    $content.replaceChildren(...items);
    const s = el("div", { className: "state" });
    const i = el("div", { className: "state-icon" });
    i.innerHTML = I("key");
    s.append(i, el("h2", { textContent: m("noLoginsTitle") }), el("p", { textContent: m("noLoginsText", [host]) }));
    $content.append(s);
  } else {
    for (const l of res.logins) {
      const tags = el("div", { className: "tags" });
      if (l.reused) tags.append(el("span", { className: "tag", textContent: m("tagReused") }));
      else if (l.weak) tags.append(el("span", { className: "tag", textContent: m("tagWeak") }));
      if (l.has_totp) tags.append(el("span", { className: "tag cool", textContent: m("tag2fa") }));
      const main = el("div", { className: "row-main" }, el("strong", { textContent: l.title }), el("span", { textContent: l.username || "—" }));
      if (tags.childElementCount) main.append(tags);
      items.push(el("button", { className: "row", onclick: () => fill(l.id) }, avatar(l.title, host), main, el("span", { className: "go", textContent: m("fillLabel") })));
    }
    $content.replaceChildren(...items);
    $content.querySelector(".row")?.focus();
  }
  $actions.hidden = false;
  $("save-btn").onclick = () => save();
  $("qr-btn").onclick = () => scanQr();
}

// The fill itself runs in background.js: confirming it in Kryptos takes the
// focus, which closes this popup.
async function fill(id) {
  state("lock", m("confirmInAppTitle"), m("confirmInAppText"));
  const res = await viaBackground({ type: "fill", tabId: tab.id, url: url.href, id });
  if (res?.ok) return window.close();
  const known = { rate_limited: "rateLimitedText", denied: "fillDeniedText", timeout: "fillTimeoutText", busy: "fillBusyText" };
  state("warning", m("fillFailedTitle"), known[res?.error] ? m(known[res.error]) : res?.error);
}

// "Save login from this page": reads the form only now, when the user asks,
// shows what will be saved and waits for an OK.
async function save(password) {
  let typed = null;
  try {
    const results = await ext.scripting.executeScript({ target: { tabId: tab.id, allFrames: true }, func: readCredentials, args: [url.origin] });
    typed = results.map((r) => r.result).find(Boolean) ?? null;
  } catch {}
  if (password) typed = { username: typed?.username ?? "", password };
  if (!typed) return state("key", m("saveNothingTitle"), m("saveNothingText"));

  const box = el(
    "div",
    { className: "row", style: "background: var(--s1); box-shadow: 0 0 0 1px var(--line); max-width: 280px" },
    el("div", { className: "row-main" }, el("strong", { textContent: typed.username || m("saveNoUsername") }), el("span", { textContent: "•".repeat(Math.min(typed.password.length, 14)) })),
  );
  state("add", m("saveTitle", [$("site").textContent]), box, {
    label: m("saveConfirm"),
    run: async () => {
      const res = await send({ type: "save", url: url.href, username: typed.username, password: typed.password });
      typed = null;
      if (!res.ok) {
        const known = { locked: "vaultLockedText", rate_limited: "rateLimitedText" };
        return state("warning", m("saveFailedTitle"), known[res.error] ? m(known[res.error]) : res.error);
      }
      const text = { added: "savedText", updated: "updatedText", unchanged: "unchangedText" }[res.outcome] ?? "savedText";
      state("check", m("savedTitle"), m(text));
      setTimeout(() => window.close(), 1500);
    },
  });
}

// "Add 2FA code from QR": captures the visible tab (allowed only after this
// click), finds the otpauth:// QR in it, and lets the app ask where it goes.
// The screenshot stays in this popup's memory.
async function scanQr() {
  state("qr_code_scanner", m("qrReadingTitle"), m("qrReadingText"));
  let shot;
  try {
    shot = await ext.tabs.captureVisibleTab(tab.windowId, { format: "png" });
  } catch {
    return state("warning", m("qrNoneTitle"), m("qrCaptureFailed"));
  }
  const uri = await decodeQr(shot).catch(() => null);
  shot = null;
  if (!uri) return state("qr_code_scanner", m("qrNoneTitle"), m("qrNoneText"));
  state("lock", m("confirmInAppTitle"), m("qrConfirmText"));
  const res = await viaBackground({ type: "add_totp", url: url.href, uri });
  if (res?.ok) {
    state("check", m("qrAddedTitle"), m("qrAddedText", [res.title]));
    return setTimeout(() => window.close(), 1600);
  }
  const known = { denied: "qrDeniedText", timeout: "fillTimeoutText", busy: "fillBusyText", invalid_totp: "qrInvalidText" };
  state("warning", m("qrFailedTitle"), known[res?.error] ? m(known[res.error]) : res?.error);
}

/** The first otpauth:// QR code in a screenshot (data: URL), or null. Read as
 * bytes, since this page's CSP rightly refuses `data:` images. */
async function decodeQr(dataUrl) {
  const bytes = Uint8Array.from(atob(dataUrl.split(",")[1]), (c) => c.charCodeAt(0));
  const shot = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
  const read = (sx, sy, sw, sh, max) => {
    const scale = Math.min(1, max / sw);
    const canvas = el("canvas", { width: Math.round(sw * scale), height: Math.round(sh * scale) });
    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    ctx.drawImage(shot, sx, sy, sw, sh, 0, 0, canvas.width, canvas.height);
    const data = ctx.getImageData(0, 0, canvas.width, canvas.height);
    const code = globalThis.jsQR?.(data.data, data.width, data.height, { inversionAttempts: "attemptBoth" });
    return code?.data?.toLowerCase().startsWith("otpauth://") ? code.data : null;
  };
  // The whole page, smaller then full size.
  let found = read(0, 0, shot.width, shot.height, 1600) ?? read(0, 0, shot.width, shot.height, Infinity);
  // A busy page can hide a small code: overlapping pieces, in a 3 by 3 grid.
  const [w, h] = [shot.width / 2, shot.height / 2];
  for (let y = 0; !found && y < 3; y++) {
    for (let x = 0; !found && x < 3; x++) found = read((x * w) / 2, (y * h) / 2, w, h, Infinity);
  }
  shot.close();
  return found;
}

// ───────── Search the whole vault ─────────

function showSearch() {
  $actions.hidden = true;
  const input = el("input", { placeholder: m("searchAllPlaceholder"), autocomplete: "off", spellcheck: false });
  const find = el("label", { className: "find" });
  find.innerHTML = I("search");
  find.append(input);
  const list = el("div");
  const hint = el("p", { className: "hint", textContent: m("searchAllHint") });
  $content.replaceChildren(find, hint, list);
  input.focus();
  let timer = 0;
  let seq = 0;
  input.oninput = () => {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const mine = ++seq;
      const q = input.value.trim();
      if (q.length < 2) return list.replaceChildren();
      const res = await send({ type: "search", query: q });
      if (mine !== seq) return;
      if (!res.ok) return showError(res.error);
      hint.hidden = true;
      if (!res.results.length) return list.replaceChildren(el("p", { className: "hint", textContent: m("searchNone") }));
      list.replaceChildren(
        ...res.results.map((r) => {
          const host = hostOf(r.url ?? "");
          return el(
            "button",
            { className: "row", onclick: () => send({ type: "open", id: r.id }).then(() => window.close()) },
            avatar(r.title, host),
            el("div", { className: "row-main" }, el("strong", { textContent: r.title }), el("span", { textContent: [r.username, host].filter(Boolean).join(" · ") || "—" })),
            el("span", { className: "go", textContent: m("openShort") }),
          );
        }),
      );
    }, 150);
  };
}

// ───────── Generator ─────────

const gen = { length: 20, symbols: true, value: "" };

/** Unbiased random password from the Web Crypto CSPRNG. */
function generate() {
  const sets = ["abcdefghijkmnopqrstuvwxyz", "ABCDEFGHJKLMNPQRSTUVWXYZ", "23456789", gen.symbols ? "!@#$%^&*-_=+?" : ""].filter(Boolean);
  const all = sets.join("");
  // Uniform integer in [0, n): rejection sampling, no modulo bias.
  const randInt = (n) => {
    const limit = 2 ** 32 - (2 ** 32 % n);
    const b = new Uint32Array(1);
    do crypto.getRandomValues(b);
    while (b[0] >= limit);
    return b[0] % n;
  };
  const pick = (s) => s[randInt(s.length)];
  // One of each kind, the rest from all of them, then a Fisher–Yates shuffle.
  const out = sets.map(pick);
  while (out.length < gen.length) out.push(pick(all));
  for (let i = out.length - 1; i > 0; i--) {
    const j = randInt(i + 1);
    [out[i], out[j]] = [out[j], out[i]];
  }
  gen.value = out.join("");
}

function showGenerator() {
  $actions.hidden = true;
  if (!gen.value) generate();
  const out = el("div", { className: "gen-out" });
  const paint = () => {
    out.replaceChildren(...[...gen.value].map((c) => (/\d/.test(c) ? el("span", { className: "d", textContent: c }) : /[A-Za-z]/.test(c) ? c : el("span", { className: "s", textContent: c }))));
  };
  paint();
  const n = el("span", { className: "n", textContent: gen.length });
  const range = el("input", { type: "range", min: 12, max: 48, value: gen.length });
  range.oninput = () => {
    gen.length = Number(range.value);
    n.textContent = gen.length;
    generate();
    paint();
  };
  const sym = el("input", { type: "checkbox", checked: gen.symbols });
  sym.onchange = () => {
    gen.symbols = sym.checked;
    generate();
    paint();
  };
  const btn = (icon, label, run, cls = "btn alt") => {
    const b = el("button", { className: cls, onclick: run });
    b.innerHTML = I(icon); // static, trusted markup
    b.append(label);
    return b;
  };
  const buttons = el(
    "div",
    { className: "gen-buttons" },
    btn("refresh", m("genAgain"), () => (generate(), paint())),
    btn("content_copy", m("genCopy"), async (e) => {
      await navigator.clipboard.writeText(gen.value);
      e.currentTarget.lastChild.textContent = m("genCopied");
    }),
    url ? btn("edit", m("genInsert"), () => insert(false), "btn alt wide") : "",
    url ? btn("add", m("genInsertSave"), () => insert(true), "btn wide") : "",
  );
  const wrap = el(
    "div",
    { className: "gen" },
    out,
    el("div", { className: "gen-row" }, el("span", { textContent: m("genLength") }), range, n),
    el("div", { className: "gen-row" }, el("label", {}, sym, m("genSymbols"))),
    buttons,
  );
  $content.replaceChildren(wrap);
}

/** Puts the generated password in the page's new-password fields, then (if
 *  asked) saves it as this site's login. */
async function insert(andSave) {
  let filled = 0;
  try {
    const results = await ext.scripting.executeScript({ target: { tabId: tab.id, allFrames: true }, func: injectNewPassword, args: [url.origin, gen.value] });
    filled = results.reduce((n, r) => n + (r.result || 0), 0);
  } catch {}
  if (!filled) return state("warning", m("genNoFieldTitle"), m("genNoFieldText"));
  if (andSave) return save(gen.value);
  state("check", m("genInsertedTitle"), m("genInsertedText"));
  setTimeout(() => window.close(), 1300);
}

// ───────── Injected functions (run in the page's isolated world, self-contained) ─────────

// Returns what the user typed into the login form, or null.
function readCredentials(expectedOrigin) {
  if (location.origin !== expectedOrigin) return null;
  const visible = (el) => {
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== "hidden";
  };
  const pw = [...document.querySelectorAll('input[type="password"]')].find((el) => visible(el) && el.value);
  if (!pw) return null;
  const scope = pw.form ?? document;
  const candidates = [
    'input[autocomplete~="username"]',
    'input[autocomplete~="email"]',
    'input[type="email"]',
    'input[name*="user" i], input[id*="user" i]',
    'input[name*="login" i], input[id*="login" i]',
    'input[name*="email" i], input[id*="email" i]',
    'input[type="text"], input[type="tel"], input:not([type])',
  ];
  let user = null;
  for (const sel of candidates) {
    user = [...scope.querySelectorAll(sel)].find((el) => visible(el) && el !== pw && el.value);
    if (user) break;
  }
  return { username: user?.value.trim() ?? "", password: pw.value };
}

// Fills the visible new-password fields (password and its confirmation), never
// a "current password" field. Returns how many were filled.
function injectNewPassword(expectedOrigin, password) {
  if (location.origin !== expectedOrigin) return 0;
  const visible = (el) => {
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && !el.disabled && !el.readOnly && getComputedStyle(el).visibility !== "hidden";
  };
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
  const fields = [...document.querySelectorAll('input[type="password"]')].filter(
    (el) => visible(el) && el.autocomplete !== "current-password" && !/current|old|vecchia|attuale/i.test(`${el.name} ${el.id}`),
  );
  for (const el of fields) {
    el.focus();
    setter.call(el, password);
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  }
  return fields.length;
}

main();
