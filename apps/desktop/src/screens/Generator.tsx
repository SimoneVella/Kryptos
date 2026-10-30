import { useEffect, useState } from "react";
import { Icon } from "../components/Icon";
import { api, defaultGenerator, type GeneratorOptions } from "../lib/api";
import { Switch, useToast } from "../components/ui";
import { localStorageGet, localStorageSet, strengthOf } from "../lib/utils";
import { useT, type TKey } from "../i18n";

const TOGGLES: { key: keyof GeneratorOptions; label: TKey; hint: string }[] = [
  { key: "uppercase", label: "generator.toggleUppercase", hint: "A–Z" },
  { key: "lowercase", label: "generator.toggleLowercase", hint: "a–z" },
  { key: "digits", label: "generator.toggleDigits", hint: "0–9" },
  { key: "symbols", label: "generator.toggleSymbols", hint: "!@#$%" },
  { key: "exclude_ambiguous", label: "generator.toggleExcludeAmbiguous", hint: "I l 1 O 0" },
];

function loadOpts(): GeneratorOptions {
  try {
    return { ...defaultGenerator, ...JSON.parse(localStorageGet("generator") ?? "{}") };
  } catch {
    return defaultGenerator;
  }
}

type Phrase = { words: number; separator: string; capitalize: boolean; digit: boolean };
const PHRASE_DEFAULT: Phrase = { words: 5, separator: "-", capitalize: false, digit: true };

function loadPhrase(): Phrase {
  try {
    return { ...PHRASE_DEFAULT, ...JSON.parse(localStorageGet("passphrase") ?? "{}") };
  } catch {
    return PHRASE_DEFAULT;
  }
}

export default function Generator({ onSave, clipboardClearSecs }: { onSave: (pw: string) => void; clipboardClearSecs: number }) {
  const { t } = useT();
  const [opts, setOpts] = useState<GeneratorOptions>(loadOpts);
  // Random characters, or random words (easier to type and remember: master passwords).
  const [mode, setMode] = useState<"password" | "phrase">(() => (localStorageGet("generatorMode") === "phrase" ? "phrase" : "password"));
  const [phrase, setPhrase] = useState<Phrase>(loadPhrase);
  const [pw, setPw] = useState("");
  const [bits, setBits] = useState(0);
  const toast = useToast();

  const regen = (o = opts) =>
    (mode === "phrase"
      ? api.generatePassphrase(phrase.words, phrase.separator, phrase.capitalize, phrase.digit)
      : api.generatePassword(o)
    ).then((r) => {
      setPw(r.password);
      setBits(r.entropy_bits);
    });

  useEffect(() => {
    localStorageSet("generator", JSON.stringify(opts));
    localStorageSet("passphrase", JSON.stringify(phrase));
    localStorageSet("generatorMode", mode);
    regen(opts).catch(() => {});
  }, [opts, phrase, mode]);

  const update = (patch: Partial<GeneratorOptions>) => {
    const next = { ...opts, ...patch };
    // Keep at least one character set on.
    if (!next.lowercase && !next.uppercase && !next.digits && !next.symbols) return;
    setOpts(next);
  };

  const s = strengthOf(bits);

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>{t("generator.title")}</h1>
          <p className="muted" style={{ marginTop: 6 }}>{t("generator.subtitle")}</p>
        </div>
      </div>

      <div className="segmented" style={{ alignSelf: "flex-start" }}>
        <button className={mode === "password" ? "active" : ""} onClick={() => setMode("password")}>{t("generator.modePassword")}</button>
        <button className={mode === "phrase" ? "active" : ""} onClick={() => setMode("phrase")}>{t("generator.modePhrase")}</button>
      </div>

      <section className="card" style={{ padding: 0 }}>
        <div className="gen-display">
          {[...pw].map((c, i) => (
            <span key={i} className={/\d/.test(c) ? "d" : /[A-Za-z]/.test(c) ? "" : "s"}>{c}</span>
          ))}
        </div>
        <div className="strength">
          <div className="strength-bar">
            <div style={{ width: `${s.pct}%`, background: s.color }} />
          </div>
          <div className="strength-meta">
            <span style={{ color: s.color }}>{t(`strength.${s.key}`)}</span>
            <span className="muted">{t("generator.entropyBits", { n: Math.round(bits) })}</span>
          </div>
        </div>
        <div style={{ display: "flex", gap: 10, padding: "0 24px 24px", justifyContent: "center" }}>
          <button className="btn btn-secondary" onClick={() => regen()}>
            <Icon name="refresh" size={18} /> {t("generator.regenerate")}
          </button>
          <button
            className="btn btn-primary"
            onClick={async () => {
              await api.copyText(pw);
              toast(t("common.copiedClearing", { secs: clipboardClearSecs }));
            }}
          >
            <Icon name="content_copy" size={18} /> {t("common.copy")}
          </button>
          <button className="btn btn-secondary" onClick={() => onSave(pw)}>
            <Icon name="add" size={18} /> {t("common.save")}
          </button>
        </div>
      </section>

      {mode === "phrase" ? (
        <section className="card card-pad">
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
            <h3>{t("generator.words")}</h3>
            <span style={{ fontSize: 22, fontWeight: 700, letterSpacing: "-0.02em" }}>{phrase.words}</span>
          </div>
          <input className="range" type="range" min={4} max={10} value={phrase.words} onChange={(e) => setPhrase({ ...phrase, words: Number(e.target.value) })} />
          <div className="toggle-row">
            <div>{t("generator.separator")}</div>
            <div className="segmented">
              {["-", " ", ".", "_"].map((sep) => (
                <button key={sep} className={phrase.separator === sep ? "active" : ""} onClick={() => setPhrase({ ...phrase, separator: sep })}>
                  <span className="mono">{sep === " " ? "␣" : sep}</span>
                </button>
              ))}
            </div>
          </div>
          <div className="toggle-row">
            <div>{t("generator.capitalize")}</div>
            <Switch checked={phrase.capitalize} onChange={(b) => setPhrase({ ...phrase, capitalize: b })} />
          </div>
          <div className="toggle-row">
            <div>{t("generator.addDigit")}</div>
            <Switch checked={phrase.digit} onChange={(b) => setPhrase({ ...phrase, digit: b })} />
          </div>
          <p className="hint">{t("generator.phraseHint")}</p>
        </section>
      ) : (
      <section className="card card-pad">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
          <h3>{t("generator.length")}</h3>
          <span style={{ fontSize: 22, fontWeight: 700, letterSpacing: "-0.02em" }}>{opts.length}</span>
        </div>
        <input
          className="range"
          type="range"
          min={8}
          max={64}
          value={opts.length}
          onChange={(e) => update({ length: Number(e.target.value) })}
        />
        <div style={{ marginTop: 8 }}>
          {TOGGLES.map((tg) => (
            <div className="toggle-row" key={tg.key}>
              <div>
                <div>{t(tg.label)}</div>
                <div className="muted mono" style={{ fontSize: 12, marginTop: 2 }}>{tg.hint}</div>
              </div>
              <Switch checked={opts[tg.key] as boolean} onChange={(b) => update({ [tg.key]: b })} />
            </div>
          ))}
        </div>
      </section>
      )}
    </div>
  );
}
