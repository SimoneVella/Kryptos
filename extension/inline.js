// The list under a login field. Runs as an extension page: the site can't read
// it. Only text nodes are written, and clicks count only after the list has been
// on screen for a moment, so a page can't slip it under a click meant for
// something else. Choosing a login still asks the user to confirm.

const ext = globalThis.chrome ?? globalThis.browser;
const token = location.hash.slice(1);
const ARMED_AFTER_MS = 400;
const shownAt = performance.now();
const $list = document.getElementById("list");

ext.runtime.sendMessage({ type: "inline_list", token }, (res) => {
  void ext.runtime.lastError;
  if (!res?.logins) return;
  document.getElementById("site").textContent = res.host;
  for (const login of res.logins) {
    const row = document.createElement("button");
    const letter = document.createElement("span");
    letter.className = "letter";
    letter.textContent = (login.title.trim()[0] ?? "?").toUpperCase();
    const text = document.createElement("span");
    text.className = "text";
    const title = document.createElement("b");
    title.textContent = login.title;
    const user = document.createElement("span");
    user.textContent = login.username;
    text.append(title, user);
    row.append(letter, text);
    row.addEventListener("click", (e) => {
      if (!e.isTrusted || performance.now() - shownAt < ARMED_AFTER_MS || document.visibilityState !== "visible") return;
      ext.runtime.sendMessage({ type: "inline_fill", token, id: login.id });
    });
    $list.append(row);
  }
});
