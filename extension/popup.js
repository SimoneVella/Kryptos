// Kryptos popup. Talks to the desktop app only through Native Messaging
// (browser -> kryptos-native-host over stdio -> app over a Unix socket).
// No content script runs on pages: a one-shot fill function is injected only
// when the user picks a login, using the activeTab permission.

const HOST = "com.kryptos.bridge";
// Prefer the callback-style `chrome` namespace (also present in Firefox).
const ext = globalThis.chrome ?? globalThis.browser;

const $content = document.getElementById("content");
const $site = document.getElementById("site");
const $searchWrap = document.getElementById("search-wrap");
const $search = document.getElementById("search");

const ICONS = {
  lock: '<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="11" width="16" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/></svg>',
  key: '<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="7.5" cy="15.5" r="4.5"/><path d="m10.7 12.3 9.3-9.3M17 6l3 3M14 9l2 2"/></svg>',
  plug: '<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 22v-5M9 8V2M15 8V2M18 8v5a6 6 0 0 1-12 0V8z"/></svg>',
  globe: '<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="12" cy="12" r="10"/><path d="M2 12h20M12 2a15 15 0 0 1 0 20M12 2a15 15 0 0 0 0 20"/></svg>',
};

document.documentElement.lang = ext.i18n.getUILanguage();
for (const node of document.querySelectorAll("[data-i18n]")) node.textContent = ext.i18n.getMessage(node.dataset.i18n);
for (const node of document.querySelectorAll("[data-i18n-placeholder]")) node.placeholder = ext.i18n.getMessage(node.dataset.i18nPlaceholder);

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

function el(tag, props = {}, ...children) {
  const n = Object.assign(document.createElement(tag), props);
  n.append(...children);
  return n;
}

function state(icon, title, text, action) {
  $searchWrap.hidden = true;
  const box = el("div", { className: "state" });
  const i = el("div", { className: "state-icon" });
  i.innerHTML = ICONS[icon]; // static, trusted markup
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

function brandIcon(title, host) {
  const table = globalThis.KRYPTOS_BRAND_ICONS;
  if (!table) return null;
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
    const light = 0.2126 * r + 0.7152 * g + 0.0722 * b > 150;
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("width", "52%");
    svg.setAttribute("height", "52%");
    const p = document.createElementNS("http://www.w3.org/2000/svg", "path");
    p.setAttribute("d", path);
    p.setAttribute("fill", light ? "#111" : "#fff");
    svg.append(p);
    const div = el("div", { className: "avatar ring", style: `background: #${hex}` });
    div.append(svg);
    return div;
  }
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  const hue = HUES[h % HUES.length];
  return el("div", {
    className: "avatar",
    textContent: (name.trim()[0] ?? "?").toUpperCase(),
    style: `background: linear-gradient(135deg, hsl(${hue} 85% 62%), hsl(${(hue + 30) % 360} 80% 50%))`,
  });
}

const openApp = { label: ext.i18n.getMessage("openApp"), run: () => send({ type: "focus" }).then(() => window.close()) };

async function main() {
  const [tab] = await ext.tabs.query({ active: true, currentWindow: true });
  let url;
  try {
    url = new URL(tab.url);
  } catch {}
  if (!url || !/^https?:$/.test(url.protocol)) {
    $site.textContent = ext.i18n.getMessage("noSite");
    return state("globe", ext.i18n.getMessage("unsupportedPageTitle"), ext.i18n.getMessage("unsupportedPageText"));
  }
  $site.textContent = url.hostname.replace(/^www\./, "");

  const res = await send({ type: "logins", url: url.href });
  if (!res.ok) {
    switch (res.error) {
      case "locked":
        return state("lock", ext.i18n.getMessage("vaultLockedTitle"), ext.i18n.getMessage("vaultLockedText"), openApp);
      case "app_not_running":
        return state("key", ext.i18n.getMessage("appClosedTitle"), ext.i18n.getMessage("appClosedText"));
      case "host_unavailable": {
        const p = el(
          "p",
          {},
          ext.i18n.getMessage("connectAppPre"),
          el("b", { textContent: ext.i18n.getMessage("connectAppSettingsPath") }),
          ext.i18n.getMessage("connectAppMid"),
          el("b", { textContent: ext.i18n.getMessage("connectAppButton") }),
          ext.i18n.getMessage("connectAppPost"),
        );
        return state("plug", ext.i18n.getMessage("connectAppTitle"), p);
      }
      case "untrusted_caller":
      case "untrusted_peer":
        return state("lock", ext.i18n.getMessage("untrustedTitle"), ext.i18n.getMessage("untrustedText"));
      default:
        return state("key", ext.i18n.getMessage("genericErrorTitle"), res.error);
    }
  }
  if (res.logins.length === 0) {
    return state("key", ext.i18n.getMessage("noLoginsTitle"), ext.i18n.getMessage("noLoginsText", [$site.textContent]), openApp);
  }

  const render = (q = "") => {
    const list = res.logins.filter((l) => !q || l.title.toLowerCase().includes(q) || l.username.toLowerCase().includes(q));
    $content.replaceChildren(
      ...list.map((l) =>
        el(
          "button",
          { className: "login", onclick: () => fill(tab.id, url, l.id) },
          avatar(l.title, url.hostname.replace(/^www\./, "")),
          el("div", { className: "login-main" }, el("strong", { textContent: l.title }), el("span", { textContent: l.username || "—" })),
          el("span", { className: "fill", textContent: ext.i18n.getMessage("fillLabel") }),
        ),
      ),
    );
  };
  render();
  if (res.logins.length > 4) {
    $searchWrap.hidden = false;
    $search.oninput = () => render($search.value.trim().toLowerCase());
    $search.onkeydown = (e) => e.key === "Enter" && $content.querySelector(".login")?.click();
    $search.focus();
  } else {
    $content.querySelector(".login")?.focus();
  }
}

async function fill(tabId, url, id) {
  const res = await send({ type: "credentials", id, url: url.href });
  if (!res.ok)
    return state("key", ext.i18n.getMessage("fillFailedTitle"), res.error === "rate_limited" ? ext.i18n.getMessage("rateLimitedText") : res.error);
  await ext.scripting.executeScript({
    target: { tabId, allFrames: true },
    func: injectCredentials,
    args: [url.origin, res.username, res.password],
  });
  window.close();
}

// Runs inside each frame of the page (isolated world). Must be self-contained.
function injectCredentials(expectedOrigin, username, password) {
  // Never fill a different origin: covers navigation since the popup opened
  // and cross-origin iframes (only same-origin frames are filled).
  if (location.origin !== expectedOrigin) return;

  const visible = (el) => {
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && !el.disabled && !el.readOnly && getComputedStyle(el).visibility !== "hidden";
  };
  const setValue = (el, v) => {
    el.focus();
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
    setter.call(el, v); // works with React/Vue/Angular controlled inputs
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  };

  const pw = [...document.querySelectorAll('input[type="password"]')].find(visible);
  const scope = pw?.form ?? document;
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
    user = [...scope.querySelectorAll(sel)].find((el) => visible(el) && el !== pw);
    if (user) break;
  }

  if (user && username) setValue(user, username);
  if (pw) setValue(pw, password);
}

main();
