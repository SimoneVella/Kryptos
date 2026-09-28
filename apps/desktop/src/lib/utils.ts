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

export function strengthOf(bits: number): { label: string; color: string; pct: number } {
  if (bits < 40) return { label: "Debole", color: "var(--danger)", pct: 20 };
  if (bits < 64) return { label: "Discreta", color: "var(--warning)", pct: 45 };
  if (bits < 90) return { label: "Forte", color: "var(--success)", pct: 75 };
  return { label: "Fortissima", color: "var(--success)", pct: 100 };
}


export const emptyInput: EntryInput = { title: "", username: "", password: "", urls: [], notes: "", favorite: false };

export function hostOf(url?: string): string {
  if (!url) return "";
  try {
    return new URL(url.includes("://") ? url : `https://${url}`).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
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
