// Pure helpers, kept out of component files so Vite Fast Refresh works.
import type { EntryInput } from "./api";

/* ───────── Strength (rough, UI-only; generator uses real entropy) ───────── */
export function estimateBits(pw: string): number {
  let pool = 0;
  if (/[a-z]/.test(pw)) pool += 26;
  if (/[A-Z]/.test(pw)) pool += 26;
  if (/\d/.test(pw)) pool += 10;
  if (/[^A-Za-z0-9]/.test(pw)) pool += 30;
  return pool ? pw.length * Math.log2(pool) : 0;
}

export function strengthOf(bits: number): { key: "weak" | "fair" | "strong" | "excellent"; color: string; pct: number } {
  if (bits < 40) return { key: "weak", color: "var(--danger)", pct: 20 };
  if (bits < 64) return { key: "fair", color: "var(--warning)", pct: 45 };
  if (bits < 90) return { key: "strong", color: "var(--success)", pct: 75 };
  return { key: "excellent", color: "var(--success)", pct: 100 };
}


export const emptyInput: EntryInput = { title: "", username: "", password: "", urls: [], notes: "", favorite: false, totp: "", backup_codes: [], ai_access: false };

export function hostOf(url?: string): string {
  if (!url) return "";
  try {
    return new URL(url.includes("://") ? url : `https://${url}`).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}

const TWO_PART_SUFFIXES = new Set(["co", "com", "org", "net", "edu", "gov", "ac", "ne", "or"]);

/** Normalizes an entry's primary site (registrable domain, or title fallback) for grouping same-site accounts. */
export function siteKey(title: string, urls: string[]): string {
  const host = urls.map(hostOf).find(Boolean)?.toLowerCase().split(":")[0];
  if (!host) return title.trim().toLowerCase();
  if (/^\d+(\.\d+)*$/.test(host)) return host;
  const parts = host.split(".").filter(Boolean);
  if (parts.length <= 2) return host;
  const tld = parts[parts.length - 1];
  const sld = parts[parts.length - 2];
  if (parts.length >= 3 && tld.length === 2 && TWO_PART_SUFFIXES.has(sld)) {
    return parts.slice(-3).join(".");
  }
  return parts.slice(-2).join(".");
}

// Per-viewer UI conveniences only (e.g. generator options); never secrets or security settings.
export function localStorageGet(k: string): string | null {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
}
export function localStorageSet(k: string, v: string) {
  try {
    localStorage.setItem(k, v);
  } catch {
    /* storage unavailable */
  }
}

let platform = "macos";
/** Set once from the backend status (std::env::consts::OS). */
export function setPlatform(p: string) {
  platform = p;
}
export const isMobile = () => platform === "android" || platform === "ios";
export const isMac = () => platform === "macos";
export const isWindows = () => platform === "windows";
