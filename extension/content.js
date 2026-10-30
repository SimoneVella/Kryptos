// Kryptos: offers saved logins when the user clicks a login field.
//
// This script sees only the page, never a username or a password. The list lives
// in inline.html, an extension page in a frame inside a closed shadow root, so the
// site can't read it. It opens only on a real click, in the top frame, and only
// learns that there's a login here. Choosing one still needs the user's confirmation,
// and the fill goes through background.js, which never trusts this script about
// which site it's on.

const ext = globalThis.chrome ?? globalThis.browser;
const WIDTH = 300;
const ROW = 52;
const CHROME = 42;
const VISIBLE = 5;

const USERNAME = /user|login|e-?mail|account|utente|identifier/i;

function isLoginField(el) {
  if (!(el instanceof HTMLInputElement) || el.disabled || el.readOnly) return false;
  if (el.type === "password") return el.autocomplete !== "new-password";
  if (!/^(text|email|tel)$/.test(el.type)) return false;
  const scope = el.form ?? document;
  if (!scope.querySelector('input[type="password"]') && !/\busername\b/.test(el.autocomplete)) return false;
  return /\b(username|email)\b/.test(el.autocomplete) || USERNAME.test(`${el.name} ${el.id} ${el.placeholder}`) || el.type === "email";
}

let host = null;
let field = null;

function close() {
  host?.remove();
  host = null;
  field = null;
}

function place() {
  if (!host || !field) return;
  const r = field.getBoundingClientRect();
  host.style.left = `${Math.min(Math.max(8, r.left), innerWidth - WIDTH - 8)}px`;
  host.style.top = `${r.bottom + 6}px`;
}

function open(target, token, count) {
  close();
  field = target;
  host = document.createElement("div");
  // Nothing from the page may restyle it into something else.
  host.style.cssText = `all: initial; position: fixed; z-index: 2147483647; width: ${WIDTH}px; height: ${CHROME + ROW * Math.min(count, VISIBLE)}px;`;
  const shadow = host.attachShadow({ mode: "closed" });
  const frame = document.createElement("iframe");
  frame.src = `${ext.runtime.getURL("inline.html")}#${token}`;
  frame.style.cssText = "all: initial; display: block; width: 100%; height: 100%; border: 0; border-radius: 12px; color-scheme: normal;";
  frame.setAttribute("allowtransparency", "true");
  shadow.append(frame);
  place();
  document.documentElement.append(host);
}

document.addEventListener(
  "click",
  (e) => {
    if (!e.isTrusted) return;
    if (host && e.target === host) return;
    const target = e.composedPath()[0];
    if (!isLoginField(target)) return close();
    if (target === field) return;
    ext.runtime.sendMessage({ type: "inline_open" }, (res) => {
      void ext.runtime.lastError;
      if (res?.token && res.count > 0 && document.activeElement === target) open(target, res.token, res.count);
    });
  },
  true,
);

document.addEventListener("keydown", (e) => e.key === "Escape" && close(), true);
addEventListener("scroll", place, true);
addEventListener("resize", place);
addEventListener("pagehide", close);

// Sent by background.js when the list has been used.
ext.runtime.onMessage.addListener((msg, sender) => {
  if (sender.id === ext.runtime.id && msg?.type === "inline_close") close();
});
