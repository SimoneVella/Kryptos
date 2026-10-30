// Downloads a Material Symbols Rounded subset with only the icons the UI uses,
// into src/assets/material-symbols.woff2 (bundled: the app never fetches fonts).
// Run after adding an icon: `npm run material-icons`. Names are collected from
// `<Icon name="...">` in src/, `icons: a b c` comments for computed names, and
// `I("...")` in the browser extension, which gets a copy of the same file.
import { copyFileSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const src = fileURLToPath(new URL("../src/", import.meta.url));
const names = new Set();
(function walk(dir) {
  for (const f of readdirSync(dir)) {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) walk(p);
    else if (/\.tsx?$/.test(f)) {
      const s = readFileSync(p, "utf8");
      for (const m of s.matchAll(/<Icon\b[^>]*?\bname="([a-z0-9_]+)"/g)) names.add(m[1]);
      for (const m of s.matchAll(/icons:((?: [a-z0-9_]+)+)/g)) m[1].trim().split(" ").forEach((n) => names.add(n));
    }
  }
})(src);
const extension = fileURLToPath(new URL("../../../extension/", import.meta.url));
for (const f of readdirSync(extension).filter((f) => f.endsWith(".js"))) {
  for (const m of readFileSync(join(extension, f), "utf8").matchAll(/\bI\("([a-z0-9_]+)"/g)) names.add(m[1]);
}

const list = [...names].sort();
const ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120 Safari/537.36";
const css = await (await fetch(
  `https://fonts.googleapis.com/css2?family=Material+Symbols+Rounded:opsz,wght,FILL,GRAD@20..48,300..600,0..1,-25..0&icon_names=${list.join(",")}&display=block`,
  { headers: { "User-Agent": ua } },
)).text();
const url = css.match(/url\((https:[^)]+)\)/)?.[1];
if (!url) throw new Error("no font URL in:\n" + css);
const font = Buffer.from(await (await fetch(url)).arrayBuffer());
writeFileSync(new URL("../src/assets/material-symbols.woff2", import.meta.url), font);
copyFileSync(new URL("../src/assets/material-symbols.woff2", import.meta.url), new URL("../../../extension/material-symbols.woff2", import.meta.url));
console.log(`${list.length} icons, ${(font.length / 1024).toFixed(1)} KiB: ${list.join(" ")}`);
