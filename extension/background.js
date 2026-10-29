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
  // Only our own popup (extension pages have no tab).
  if (sender.id !== ext.runtime.id || sender.tab || msg?.type !== "fill") return;
  fill(msg.tabId, msg.url, msg.id).then(reply);
  return true; // reply asynchronously
});

async function fill(tabId, href, id) {
  const res = await ask({ type: "credentials", id, url: href });
  if (!res.ok) return res;
  try {
    await ext.scripting.executeScript({
      target: { tabId, allFrames: true },
      func: injectCredentials,
      args: [new URL(href).origin, res.username, res.password],
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
