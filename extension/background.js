// Kryptos background script: does the actual fill for the popup.
//
// The app asks the user to confirm every fill (Touch ID, Mac password or master
// password) in its own window. Bringing that window forward closes the popup,
// so the popup hands the fill over to this script and the wait happens here.
// The native port is kept open for the whole wait, which also keeps the
// service worker alive (a pending request alone would not).

const HOST = "com.kryptos.bridge";
const ext = globalThis.chrome ?? globalThis.browser;

ext.runtime.onMessage.addListener((msg, sender, reply) => {
  if (sender.id !== ext.runtime.id) return;
  if (msg?.type?.startsWith("inline_")) {
    inline(msg, sender).then(reply, () => reply({}));
    return true;
  }
  // Only our own popup (extension pages have no tab).
  if (sender.id !== ext.runtime.id || sender.tab) return;
  if (msg?.type === "fill") fill(msg.tabId, msg.url, msg.id).then(reply);
  // A 2FA QR read by the popup: the app asks the user which entry gets it.
  else if (msg?.type === "add_totp") ask({ type: "add_totp", url: msg.url, uri: msg.uri }).then(reply);
  else return;
  return true; // reply asynchronously
});

// ───────── Logins under the field (content.js, inline.html) ─────────
// What the page can influence stops at "a login field was clicked". The site
// comes from the browser (`sender`), the list goes only to our own page, which
// gets it with a random token, and every fill needs the user's confirmation.

/** Our list page. Its address has a random id (use_dynamic_url), so the path and
 * scheme are what's compared; `sender.id` was checked to be this extension. */
function isInlinePage(url) {
  const u = URL.canParse(url ?? "") ? new URL(url) : null;
  return u?.protocol === new URL(ext.runtime.getURL("")).protocol && u.pathname === "/inline.html";
}

/** The request a token stands for, kept where content scripts can't read it. */
async function inlineRequest(token) {
  const key = `inline:${token}`;
  return (await ext.storage.session.get(key))[key];
}

async function inline(msg, sender) {
  if (msg.type === "inline_open") {
    // Only the content script in a tab's top frame.
    if (!sender.tab || sender.frameId !== 0 || !sender.url || sender.url.startsWith(ext.runtime.getURL(""))) return {};
    if ((await ext.storage.local.get("inlineOff")).inlineOff) return {};
    const url = new URL(sender.url);
    if (!/^https?:$/.test(url.protocol)) return {};
    const [res, look] = await Promise.all([ask({ type: "logins", url: url.href }), ask({ type: "lookalike", url: url.href })]);
    // Nothing on a page that imitates a saved site.
    if (!res.ok || !res.logins?.length || !look.ok || look.similar) return {};
    const token = [...crypto.getRandomValues(new Uint8Array(16))].map((b) => b.toString(16).padStart(2, "0")).join("");
    await ext.storage.session.set({ [`inline:${token}`]: { url: url.href, tabId: sender.tab.id } });
    return { token, count: res.logins.length };
  }

  // The rest comes only from our own list page.
  if (!isInlinePage(sender.url) || !sender.tab) return {};
  const request = await inlineRequest(msg.token);
  if (!request || request.tabId !== sender.tab.id) return {};
  if (msg.type === "inline_list") {
    const res = await ask({ type: "logins", url: request.url });
    if (!res.ok) {
      ext.tabs.sendMessage(request.tabId, { type: "inline_close" }, { frameId: 0 }, () => void ext.runtime.lastError);
      return {};
    }
    const logins = res.logins.map(({ id, title, username }) => ({ id, title, username }));
    return { host: new URL(request.url).hostname, logins };
  }
  if (msg.type === "inline_fill") {
    await ext.storage.session.remove(`inline:${msg.token}`);
    // Only if the tab is still on the page the list was made for.
    const tab = await ext.tabs.get(request.tabId);
    const same = tab.url && new URL(tab.url).origin === new URL(request.url).origin;
    ext.tabs.sendMessage(request.tabId, { type: "inline_close" }, { frameId: 0 }, () => void ext.runtime.lastError);
    if (same) await fill(request.tabId, request.url, msg.id);
  }
  return {};
}

// ───────── Keyboard shortcut and right-click menu ─────────
// Both act on the tab the user is looking at (activeTab is granted by the
// shortcut or the menu click itself) and fill only after the usual
// confirmation in the app.

ext.runtime.onInstalled.addListener(() => {
  ext.contextMenus?.removeAll(() => {
    ext.contextMenus.create({ id: "kryptos-fill", title: ext.i18n.getMessage("menuFill"), contexts: ["editable"] });
    // No "generate here" item: a password put in a form without being saved
    // could be lost. The popup's "Put in the form and save" is the safe path.
  });
});

ext.commands?.onCommand.addListener(async (command, tab) => {
  if (command !== "fill-login") return;
  tab ??= (await ext.tabs.query({ active: true, currentWindow: true }))[0];
  if (tab) fillBest(tab);
});

ext.contextMenus?.onClicked.addListener((info, tab) => {
  if (!tab) return;
  if (info.menuItemId === "kryptos-fill") fillBest(tab);
  if (info.menuItemId === "kryptos-generate") {
    ext.scripting
      .executeScript({ target: { tabId: tab.id, frameIds: [info.frameId ?? 0] }, func: fillFocusedNewPassword, args: [generatePassword()] })
      .catch(() => {});
  }
});

/** Fills this site's most recently used login (after confirming in the app). */
async function fillBest(tab) {
  let url;
  try {
    url = new URL(tab.url);
  } catch {
    return;
  }
  if (!/^https?:$/.test(url.protocol)) return;
  const [res, look] = await Promise.all([ask({ type: "logins", url: url.href }), ask({ type: "lookalike", url: url.href })]);
  // Never fill on a page that imitates a saved site, nor when there's nothing to fill.
  if (look.ok && look.similar) return badge(tab.id, "!", "#e5735f");
  if (!res.ok || !res.logins?.length) return badge(tab.id, "0", "#7d7970");
  const done = await fill(tab.id, url.href, res.logins[0].id);
  if (!done.ok) badge(tab.id, "×", "#7d7970");
}

function badge(tabId, text, color) {
  ext.action.setBadgeBackgroundColor?.({ tabId, color });
  ext.action.setBadgeText({ tabId, text });
  setTimeout(() => ext.action.setBadgeText({ tabId, text: "" }), 2500);
}

/** A 20-character password from the Web Crypto CSPRNG, no modulo bias. */
function generatePassword() {
  const sets = ["abcdefghijkmnopqrstuvwxyz", "ABCDEFGHJKLMNPQRSTUVWXYZ", "23456789", "!@#$%^&*-_=+?"];
  const all = sets.join("");
  const randInt = (n) => {
    const limit = 2 ** 32 - (2 ** 32 % n);
    const b = new Uint32Array(1);
    do crypto.getRandomValues(b);
    while (b[0] >= limit);
    return b[0] % n;
  };
  const out = sets.map((s) => s[randInt(s.length)]);
  while (out.length < 20) out.push(all[randInt(all.length)]);
  for (let i = out.length - 1; i > 0; i--) {
    const j = randInt(i + 1);
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out.join("");
}

// Runs in the right-clicked frame: puts the password in the focused field and
// in the new-password fields of its form (password + confirmation). The user
// then saves it with the popup's "Save login" or the browser's own prompt.
function fillFocusedNewPassword(password) {
  const target = document.activeElement;
  if (!(target instanceof HTMLInputElement)) return 0;
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
  const scope = target.form ?? document;
  const fields = target.type === "password" ? [...scope.querySelectorAll('input[type="password"]')].filter((el) => el.autocomplete !== "current-password") : [target];
  for (const el of fields) {
    setter.call(el, password);
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  }
  return fields.length;
}

async function fill(tabId, href, id) {
  const res = await ask({ type: "credentials", id, url: href });
  if (!res.ok) return res;
  try {
    await ext.scripting.executeScript({
      target: { tabId, allFrames: true },
      func: injectCredentials,
      args: [new URL(href).origin, res.username, res.password, res.totp ?? ""],
    });
  } catch {
    return { ok: false, error: "page_changed" };
  }
  return { ok: true };
}

/** One request over a native port, closed after the reply. */
function ask(msg) {
  return new Promise((resolve) => {
    let port;
    try {
      port = ext.runtime.connectNative(HOST);
    } catch {
      return resolve({ ok: false, error: "host_unavailable" });
    }
    port.onMessage.addListener((reply) => {
      resolve(reply ?? { ok: false, error: "host_unavailable" });
      port.disconnect();
    });
    port.onDisconnect.addListener(() => resolve({ ok: false, error: "host_unavailable" }));
    port.postMessage(msg);
  });
}

// Runs inside each frame of the page (isolated world). Must be self-contained.
function injectCredentials(expectedOrigin, username, password, totp) {
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

  // The 2FA step: autocomplete="one-time-code" or a telling name/id.
  const otpWords = /one.?time|otp|2fa|mfa|two.?factor|verification|authenticator|auth.?code|security.?code|totp|\bcode\b|\bcodice\b/i;
  const notOtp = /postal|zip|promo|coupon|country|captcha/i;
  const otp = [...document.querySelectorAll("input")].find((el) => {
    if (!visible(el)) return false;
    if (el.autocomplete === "one-time-code") return true;
    // Text-like fields only: not "remember this device (mfa)" checkboxes or the password.
    if (!/^(text|tel|number|search)$/.test(el.type)) return false;
    const words = `${el.name} ${el.id} ${el.getAttribute("aria-label") ?? ""} ${el.placeholder}`;
    const digits = el.inputMode === "numeric" && el.maxLength >= 6 && el.maxLength <= 8;
    return (otpWords.test(words) || digits) && !notOtp.test(words);
  });
  const pw = [...document.querySelectorAll('input[type="password"]')].find((el) => visible(el) && el !== otp);
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
  // A page with only the code field is the 2FA step: no username to put anywhere.
  for (const sel of pw || !otp ? candidates : []) {
    user = [...scope.querySelectorAll(sel)].find((el) => visible(el) && el !== pw && el !== otp);
    if (user) break;
  }

  if (user && username) setValue(user, username);
  if (pw) setValue(pw, password);
  if (otp && totp) setValue(otp, totp);
}
