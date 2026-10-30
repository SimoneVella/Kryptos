(() => {
  "use strict";
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => [...r.querySelectorAll(s)];
  const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const store = {
    get: (k) => { try { return localStorage.getItem(k); } catch { return null; } },
    set: (k, v) => { try { localStorage.setItem(k, v); } catch {} },
  };

  /* ───────── i18n: Italian is the markup; other languages live in lang/<code>.js ───────── */
  const LANGS = [
    ["it", "Italiano"], ["en", "English"], ["es", "Español"], ["fr", "Français"], ["de", "Deutsch"],
    ["pt", "Português"], ["ru", "Русский"], ["uk", "Українська"], ["zh", "中文"],
  ];
  const KL = (window.KL = window.KL || {});
  const els = $$("[data-i]");
  const IT = Object.fromEntries(els.map((e) => [e.dataset.i, e.innerHTML]));
  IT["vd.open"] = "Vault sbloccato"; IT["vd.lockit"] = "Blocca";
  IT._title = document.title;
  IT._desc = document.querySelector('meta[name="description"]').content;
  KL.it = IT;
  let lang = "it";
  const tr = (k) => (KL[lang] && KL[lang][k]) ?? (KL.en && KL.en[k]) ?? IT[k];
  const loadLang = (l) => new Promise((res) => {
    if (KL[l]) return res();
    const s = document.createElement("script");
    s.src = `lang/${l}.js`; s.onload = res; s.onerror = res; document.head.appendChild(s);
  });
  async function setLang(l, persist = true) {
    await loadLang(l); if (l !== "en") await loadLang("en");
    if (!KL[l]) l = "it";
    lang = l;
    document.documentElement.lang = l;
    els.forEach((e) => { const v = tr(e.dataset.i); if (v != null) e.innerHTML = v; });
    $("#lang-l").textContent = l.toUpperCase();
    document.title = tr("_title");
    document.querySelector('meta[name="description"]').content = tr("_desc");
    $$("#langmenu li").forEach((li) => li.setAttribute("aria-selected", li.dataset.l === l));
    syncDemoLabels();
    if (persist) store.set("k-lang", l);
  }
  const menu = $("#langmenu"), langBtn = $("#lang");
  menu.innerHTML = LANGS.map(([c, n]) => `<li role="option" data-l="${c}">${n}<small>${c.toUpperCase()}</small></li>`).join("");
  const closeMenu = () => { menu.hidden = true; langBtn.setAttribute("aria-expanded", "false"); };
  langBtn.addEventListener("click", (e) => { e.stopPropagation(); menu.hidden = !menu.hidden; langBtn.setAttribute("aria-expanded", String(!menu.hidden)); });
  menu.addEventListener("click", (e) => { const li = e.target.closest("li"); if (li) { setLang(li.dataset.l); closeMenu(); } });
  document.addEventListener("click", closeMenu);
  addEventListener("keydown", (e) => { if (e.key === "Escape") closeMenu(); });

  /* ───────── theme ───────── */
  const root = document.documentElement;
  const setTheme = (t) => {
    root.dataset.theme = t; store.set("k-theme", t);
    $("#theme .ms").textContent = t === "light" ? "dark_mode" : "light_mode";
    document.querySelector('meta[name="theme-color"]').content = t === "light" ? "#f4f0e8" : "#0b0c0e";
  };
  $("#theme").addEventListener("click", () => setTheme(root.dataset.theme === "light" ? "dark" : "light"));
  setTheme(root.dataset.theme === "light" ? "light" : "dark");

  /* ───────── nav ───────── */
  const nav = $("#nav");
  addEventListener("scroll", () => nav.classList.toggle("stuck", scrollY > 8), { passive: true });
  $("#menu").addEventListener("click", () => nav.classList.toggle("open"));
  $$(".links a").forEach((a) => a.addEventListener("click", () => nav.classList.remove("open")));

  /* ───────── reveal on scroll ───────── */
  const rv = $$(".rv");
  if ("IntersectionObserver" in window && !reduce) {
    const io = new IntersectionObserver((es) => es.forEach((e) => { if (e.isIntersecting) { e.target.classList.add("in"); io.unobserve(e.target); } }), { threshold: 0.12, rootMargin: "0px 0px -6% 0px" });
    rv.forEach((e) => io.observe(e));
  } else rv.forEach((e) => e.classList.add("in"));

  /* ───────── hero tilt ───────── */
  const tilt = $("#tilt");
  if (tilt && !reduce && matchMedia("(hover: hover)").matches) {
    const stage = tilt.parentElement;
    stage.addEventListener("pointermove", (e) => {
      const r = stage.getBoundingClientRect();
      const x = (e.clientX - r.left) / r.width - 0.5, y = (e.clientY - r.top) / r.height - 0.5;
      tilt.style.transform = `rotateY(${x * 7}deg) rotateX(${-y * 6}deg)`;
    });
    stage.addEventListener("pointerleave", () => (tilt.style.transform = ""));
  }

  /* ───────── card spotlight ───────── */
  $$(".card").forEach((c) => c.addEventListener("pointermove", (e) => {
    const r = c.getBoundingClientRect();
    c.style.setProperty("--mx", e.clientX - r.left + "px"); c.style.setProperty("--my", e.clientY - r.top + "px");
  }));

  /* ───────── decrypt demo ───────── */
  const vd = $("#vd"), btn = $("#vd-btn"), typed = $("#vd-typed"), meter = $$("#vd-meter i");
  const codes = $$("#vd .entry code");
  const HEX = "0123456789abcdef", GLYPH = "▓▒░#$%&@*+=?";
  const cipher = (n) => Array.from({ length: n }, () => HEX[(Math.random() * 16) | 0]).join("");
  let open = false, timers = [];
  const clearTimers = () => { timers.forEach(clearInterval); timers.forEach(clearTimeout); timers = []; };
  const syncDemoLabels = () => {
    $("#vd-state").textContent = tr(open ? "vd.open" : "vd.locked");
    $("#vd-lbl").textContent = tr(open ? "vd.lockit" : "vd.unlock");
  };
  function scrambleIn(el, plain) {
    el.classList.remove("cipher");
    let i = 0;
    const t = setInterval(() => {
      el.textContent = plain.slice(0, i) + Array.from({ length: Math.max(0, plain.length - i) }, () => GLYPH[(Math.random() * GLYPH.length) | 0]).join("");
      i += 0.7;
      if (i >= plain.length + 1) { clearInterval(t); el.textContent = plain; }
    }, 32);
    timers.push(t);
  }
  function showCipher() {
    codes.forEach((c) => { c.classList.add("cipher"); c.textContent = cipher(Math.max(18, c.dataset.plain.length + 6)); });
  }
  function setMeter(n) { meter.forEach((m, i) => (m.style.background = i < n ? "var(--gold)" : "")); }
  function type(pw, done) {
    let i = 0; typed.textContent = "";
    const t = setInterval(() => { typed.textContent = "•".repeat(++i); setMeter(Math.round((i / pw) * 6)); if (i >= pw) { clearInterval(t); done(); } }, 70);
    timers.push(t);
  }
  function toggle() {
    clearTimers();
    if (!open) {
      btn.disabled = true;
      type(14, () => {
        open = true; vd.classList.add("unlocked"); btn.classList.add("open");
        $("#vd-ic").textContent = "lock"; syncDemoLabels(); btn.disabled = false;
        codes.forEach((c, i) => timers.push(setTimeout(() => scrambleIn(c, c.dataset.plain), i * 180)));
      });
    } else {
      open = false; vd.classList.remove("unlocked"); btn.classList.remove("open");
      $("#vd-ic").textContent = "lock_open"; syncDemoLabels(); typed.textContent = ""; setMeter(0); showCipher();
    }
  }
  btn.addEventListener("click", toggle);
  showCipher();
  const cipherLoop = setInterval(() => { if (!open && !document.hidden) showCipher(); }, 2600);
  if (reduce) clearInterval(cipherLoop);
  // auto-run once when it scrolls into view
  if ("IntersectionObserver" in window && !reduce) {
    const o = new IntersectionObserver((es) => { if (es[0].isIntersecting) { o.disconnect(); setTimeout(() => { if (!open) toggle(); }, 700); } }, { threshold: 0.6 });
    o.observe(vd);
  }

  /* ───────── cost bars + score ring (when visible) ───────── */
  const once = (el, fn) => { if (!el) return; if (!("IntersectionObserver" in window)) return fn(); const o = new IntersectionObserver((es) => { if (es[0].isIntersecting) { o.disconnect(); fn(); } }, { threshold: 0.4 }); o.observe(el); };
  once($("#bars"), () => $$("#bars i").forEach((i) => (i.style.width = i.dataset.w + "%")));
  once($("#ring"), () => {
    const target = 94, circ = 289;
    $("#ring").style.strokeDashoffset = circ * (1 - target / 100);
    let n = 0; const t = setInterval(() => { n += 2; $("#ring-n").textContent = Math.min(n, target); if (n >= target) clearInterval(t); }, 18);
  });

  /* ───────── TOTP-style ticking code ───────── */
  const otp = $("#otp"), ring = $("#otp-r"), sec = $("#otp-s");
  const six = () => { const n = String((Math.random() * 1e6) | 0).padStart(6, "0"); return n.slice(0, 3) + " " + n.slice(3); };
  let left = 22;
  if (!reduce) setInterval(() => {
    left--; if (left <= 0) { left = 30; otp.textContent = six(); }
    sec.textContent = left + "s"; ring.style.strokeDashoffset = (119.4 * (30 - left)) / 30;
  }, 1000);
  ring.style.strokeDashoffset = (119.4 * 8) / 30;

  /* ───────── generator (real CSPRNG) ───────── */
  const POOL = "abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789!@#$%^&*-_=+?";
  const len = $("#gen-len"), out = $("#gen-pw");
  function rnd(n) { const a = new Uint32Array(1), lim = 4294967296 - (4294967296 % n); let v; do { crypto.getRandomValues(a); v = a[0]; } while (v >= lim); return v % n; }
  function gen() {
    const L = +len.value; let s = "";
    for (let i = 0; i < L; i++) s += POOL[rnd(POOL.length)];
    out.textContent = s; $("#gen-n").textContent = L;
    const bits = Math.round(L * Math.log2(POOL.length));
    $("#gen-ent").textContent = bits + " bit";
    $("#gen-bar").style.width = Math.min(100, (bits / 200) * 100) + "%";
  }
  $("#gen-btn").addEventListener("click", gen); len.addEventListener("input", gen); gen();
  out.addEventListener("click", () => { try { navigator.clipboard.writeText(out.textContent); } catch {} });

  /* ───────── fake QR ───────── */
  const qr = $(".qr");
  if (qr) {
    const finder = (r, c) => (r < 3 && c < 3) || (r < 3 && c > 3) || (r > 3 && c < 3);
    let h = "";
    for (let r = 0; r < 7; r++) for (let c = 0; c < 7; c++) h += `<i class="${finder(r, c) || Math.random() > 0.5 ? "" : "o"}"></i>`;
    qr.innerHTML = h;
    if (!reduce) setInterval(() => $$("i", qr).forEach((i, n) => { const r = (n / 7) | 0, c = n % 7; if (!finder(r, c)) i.className = Math.random() > 0.5 ? "" : "o"; }), 900);
  }

  /* ───────── terminal demo ───────── */
  const term = $("#term-out");
  if (term) {
    const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;");
    const script = [
      ["$ ", "p", "kryptos env list", 650],
      ["", "", "<span class=\"d\">oikos</span>   development · staging · <span class=\"d\">production</span>", 0, true],
      ["$ ", "p", "kryptos env run oikos development -- npm run dev", 650],
      ["", "", "<span class=\"d\">▸ Kryptos</span> <span class=\"g\">✓ Touch ID</span> <span class=\"d\">· 14 variables</span>", 0, true],
      ["", "", "<span class=\"g\">ready</span> - started server on http://localhost:3000", 0, true],
      ["", "", "<span class=\"d\">(no .env file written to disk)</span>", 0, true],
    ];
    let tok = 0;
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    async function runTerm() {
      const my = ++tok; let html = "";
      for (const [pre, cls, text, pause, raw] of script) {
        if (my !== tok) return;
        html += pre ? `<span class="p">${pre}</span>` : "";
        if (raw) { html += text + "\n"; term.innerHTML = html + '<span class="c"></span>'; await sleep(520); continue; }
        for (let i = 1; i <= text.length; i++) { if (my !== tok) return; term.innerHTML = html + esc(text.slice(0, i)) + '<span class="c"></span>'; await sleep(reduce ? 0 : 28); }
        html += esc(text) + "\n"; await sleep(pause);
      }
      term.innerHTML = html + '<span class="p">$ </span><span class="c"></span>';
      await sleep(5200); if (my === tok) runTerm();
    }
    if (reduce) { script.forEach(([p, , t, , raw]) => (term.innerHTML += (p ? `<span class="p">${p}</span>` : "") + (raw ? t : esc(t)) + "\n")); }
    else once(term, runTerm);
  }

  /* ───────── init language ───────── */
  const saved = store.get("k-lang");
  const browserLang = (navigator.language || "it").toLowerCase().slice(0, 2);
  const pref = saved || (LANGS.some(([c]) => c === browserLang) ? browserLang : "en");
  setLang(pref, false);
})();
