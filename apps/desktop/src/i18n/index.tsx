import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { api } from "../lib/api";
import en, { type Dict } from "./en";
import it from "./it";
import es from "./es";
import fr from "./fr";
import de from "./de";
import pt from "./pt";

export type Locale = "en" | "it" | "es" | "fr" | "de" | "pt";

/** Each language's name, written in itself. */
export const LOCALES: Record<Locale, string> = {
  en: "English",
  it: "Italiano",
  es: "Español",
  fr: "Français",
  de: "Deutsch",
  pt: "Português",
};

const dicts: Record<Locale, Dict> = { en, it, es, fr, de, pt };

/** First `navigator.languages` entry whose language prefix we support, else `en`. */
export function detectLocale(): Locale {
  const supported = Object.keys(LOCALES) as Locale[];
  for (const tag of navigator.languages ?? [navigator.language]) {
    const prefix = tag.split("-")[0].toLowerCase();
    const match = supported.find((l) => l === prefix);
    if (match) return match;
  }
  return "en";
}

type Vars = Record<string, string | number>;

function resolve(dict: unknown, path: string): unknown {
  return path.split(".").reduce<unknown>((o, k) => (o && typeof o === "object" ? (o as Record<string, unknown>)[k] : undefined), dict);
}

function interpolate(template: string, vars?: Vars): string {
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m));
}

/** Every dotted path in `Dict` that resolves to a plain string. */
type LeafPath<T> = {
  [K in keyof T & string]: T[K] extends string ? K : T[K] extends PluralEntry ? never : `${K}.${LeafPath<T[K]>}`;
}[keyof T & string];

type PluralEntry = { one: string; other: string };

/** Every dotted path in `Dict` that resolves to a `{ one, other }` pair. */
type PluralPath<T> = {
  [K in keyof T & string]: T[K] extends string ? never : T[K] extends PluralEntry ? K : `${K}.${PluralPath<T[K]>}`;
}[keyof T & string];

export type TKey = LeafPath<Dict>;
export type TPluralKey = PluralPath<Dict>;

/** Known backend error codes, translated via `errors.<code>`. */
const ERROR_CODES = new Set(Object.keys(en.errors));

type Ctx = {
  locale: Locale;
  /** `"system"` re-detects the browser/OS language; anything else pins the given locale. */
  setLanguagePreference: (pref: string) => void;
  t: (key: TKey, vars?: Vars) => string;
  plural: (key: TPluralKey, n: number, vars?: Vars) => string;
  formatDate: (ts: number) => string;
  errorMessage: (err: unknown) => string;
};

const I18nContext = createContext<Ctx | null>(null);

function isLocale(v: string): v is Locale {
  return v in LOCALES;
}

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocale] = useState<Locale>(detectLocale);

  const setLanguagePreference = useCallback((pref: string) => {
    setLocale(isLocale(pref) ? pref : detectLocale());
  }, []);

  // Settings are readable even while the vault is locked, so this also covers Unlock/Setup.
  useEffect(() => {
    api.getSettings().then((s) => setLanguagePreference(s.language));
  }, [setLanguagePreference]);

  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  const t = useCallback(
    (key: TKey, vars?: Vars) => {
      const template = (resolve(dicts[locale], key) ?? resolve(dicts.en, key)) as string;
      return interpolate(template, vars);
    },
    [locale],
  );

  const plural = useCallback(
    (key: TPluralKey, n: number, vars?: Vars) => {
      const node = (resolve(dicts[locale], key) ?? resolve(dicts.en, key)) as PluralEntry;
      const category = new Intl.PluralRules(locale).select(n) as "one" | "other";
      const template = node[category] ?? node.other;
      return interpolate(template, { n, ...vars });
    },
    [locale],
  );

  const formatDate = useCallback(
    (ts: number) => new Date(ts * 1000).toLocaleDateString(locale, { day: "numeric", month: "long", year: "numeric" }),
    [locale],
  );

  const errorMessage = useCallback(
    (err: unknown) => (typeof err === "string" && ERROR_CODES.has(err) ? t(`errors.${err}` as TKey) : t("errors.unknown", { message: String(err) })),
    [t],
  );

  const value = useMemo<Ctx>(
    () => ({ locale, setLanguagePreference, t, plural, formatDate, errorMessage }),
    [locale, setLanguagePreference, t, plural, formatDate, errorMessage],
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useT(): Ctx {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error("useT() must be used inside <I18nProvider>");
  return ctx;
}
