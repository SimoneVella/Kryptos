import { useEffect, useState } from "react";
import { Copy, Plus, RefreshCw } from "lucide-react";
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

export default function Generator({ onSave, clipboardClearSecs }: { onSave: (pw: string) => void; clipboardClearSecs: number }) {
  const { t } = useT();
  const [opts, setOpts] = useState<GeneratorOptions>(loadOpts);
  const [pw, setPw] = useState("");
  const [bits, setBits] = useState(0);
  const toast = useToast();

  const regen = (o = opts) =>
    api.generatePassword(o).then((r) => {
      setPw(r.password);
      setBits(r.entropy_bits);
    });

  useEffect(() => {
    localStorageSet("generator", JSON.stringify(opts));
    regen(opts).catch(() => {});
  }, [opts]);

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
            <RefreshCw size={18} /> {t("generator.regenerate")}
          </button>
          <button
            className="btn btn-primary"
            onClick={async () => {
              await api.copyText(pw);
              toast(t("common.copiedClearing", { secs: clipboardClearSecs }));
            }}
          >
            <Copy size={18} /> {t("common.copy")}
          </button>
          <button className="btn btn-secondary" onClick={() => onSave(pw)}>
            <Plus size={18} /> {t("common.save")}
          </button>
        </div>
      </section>

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
    </div>
  );
}
