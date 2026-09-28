// Offline brand logos for vault entries. The icon set (simple-icons, CC0) is
// bundled with the app and loaded lazily as a separate chunk: no network request
// ever reveals which sites the user has accounts on.
import { useEffect, useState } from "react";

export type BrandIcon = { hex: string; path: string; glyph: string; ring: boolean };
type IconTable = Record<string, [string, string]>;

/** Domains whose name differs from the simple-icons slug. */
const ALIASES: Record<string, string> = {
  "youtu.be": "youtube",
  "twitter.com": "x",
  "t.co": "x",
  "fb.com": "facebook",
  "wa.me": "whatsapp",
  "t.me": "telegram",
  "paypal.me": "paypal",
  "discord.gg": "discord",
  "steampowered.com": "steam",
  "steamcommunity.com": "steam",
  "booking.com": "bookingdotcom",
  "proton.me": "proton",
  "protonmail.com": "proton",
  "npmjs.com": "npm",
  "chatgpt.com": "openai",
  "claude.ai": "claude",
  "icloud.com": "icloud",
  "mail.ru": "maildotru",
  "hbomax.com": "hbomax",
  "max.com": "hbomax",
  "disneyplus.com": "disneyplus",
  "primevideo.com": "primevideo",
};

/** Second-level registries where the brand is the third label from the end. */
const SECOND_LEVEL = new Set([
  "co.uk", "org.uk", "ac.uk", "gov.uk", "com.au", "net.au", "co.jp", "ne.jp", "com.br", "com.mx", "co.in",
  "co.nz", "co.za", "com.tr", "com.ar", "com.cn", "co.kr", "com.sg", "com.hk", "com.tw", "co.il", "com.pl",
]);

function hostOf(url: string): string | null {
  try {
    const host = new URL(url.includes("://") ? url : `https://${url}`).hostname.toLowerCase();
    return /^[\d.]+$|^\[|^localhost$/.test(host) || !host.includes(".") ? null : host.replace(/^www\./, "");
  } catch {
    return null;
  }
}

/** "accounts.google.co.uk" -> { domain: "google.co.uk", label: "google" } */
function registrable(host: string): { domain: string; label: string } {
  const parts = host.split(".");
  const n = SECOND_LEVEL.has(parts.slice(-2).join(".")) ? 3 : 2;
  const domain = parts.slice(-n).join(".");
  return { domain, label: parts[parts.length - n] ?? "" };
}

const norm = (s: string) => s.toLowerCase().replace(/[^a-z0-9]/g, "");

/** Candidate slugs, most specific first: alias of host/domain, domain label, then the title. */
export function brandCandidates(title: string, urls: string[]): string[] {
  const out: string[] = [];
  for (const u of urls) {
    const host = hostOf(u);
    if (!host) continue;
    const { domain, label } = registrable(host);
    for (const c of [ALIASES[host], ALIASES[domain], norm(label)]) if (c) out.push(c);
  }
  const t = norm(title);
  if (t.length >= 3) out.push(t);
  return out;
}

let table: IconTable | null = null;
let loading: Promise<IconTable> | null = null;

function load(): Promise<IconTable> {
  loading ??= import("../assets/brand-icons.js").then((m) => (table = m.default));
  return loading;
}

function luminance(hex: string): number {
  const [r, g, b] = [0, 2, 4].map((i) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function resolve(t: IconTable, candidates: string[]): BrandIcon | null {
  for (const slug of candidates) {
    const hit = t[slug];
    if (!hit) continue;
    const [hex, path] = hit;
    if (!/^[0-9A-Fa-f]{6}$/.test(hex)) continue;
    const lum = luminance(hex);
    return { hex: `#${hex}`, path, glyph: lum > 0.55 ? "#111111" : "#ffffff", ring: lum < 0.03 || lum > 0.85 };
  }
  return null;
}

/** The brand logo for an entry, or null (caller shows the initial-letter avatar). */
export function useBrandIcon(title: string, urls: string[] = []): BrandIcon | null {
  const key = brandCandidates(title, urls).join("|");
  const [icon, setIcon] = useState<BrandIcon | null>(() => (table ? resolve(table, key.split("|")) : null));
  useEffect(() => {
    if (!key) return setIcon(null);
    let alive = true;
    load().then((t) => alive && setIcon(resolve(t, key.split("|"))));
    return () => {
      alive = false;
    };
  }, [key]);
  return icon;
}
