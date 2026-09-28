import { useEffect, useState } from "react";
import { Copy, Plus, RefreshCw } from "lucide-react";
import { api, defaultGenerator, type GeneratorOptions } from "../lib/api";
import { Switch, useToast } from "../components/ui";
import { localStorageGet, localStorageSet, strengthOf } from "../lib/utils";

const TOGGLES: { key: keyof GeneratorOptions; label: string; hint: string }[] = [
  { key: "uppercase", label: "Maiuscole", hint: "A–Z" },
  { key: "lowercase", label: "Minuscole", hint: "a–z" },
  { key: "digits", label: "Numeri", hint: "0–9" },
  { key: "symbols", label: "Simboli", hint: "!@#$%" },
  { key: "exclude_ambiguous", label: "Evita caratteri ambigui", hint: "I l 1 O 0" },
];

function loadOpts(): GeneratorOptions {
  try {
    return { ...defaultGenerator, ...JSON.parse(localStorageGet("generator") ?? "{}") };
  } catch {
    return defaultGenerator;
  }
}

export default function Generator({ onSave }: { onSave: (pw: string) => void }) {
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
          <h1>Generatore</h1>
          <p className="muted" style={{ marginTop: 6 }}>Password casuali create sul dispositivo con il generatore crittografico del sistema.</p>
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
            <span style={{ color: s.color }}>{s.label}</span>
            <span className="muted">{Math.round(bits)} bit di entropia</span>
          </div>
        </div>
        <div style={{ display: "flex", gap: 10, padding: "0 24px 24px", justifyContent: "center" }}>
          <button className="btn btn-secondary" onClick={() => regen()}>
            <RefreshCw size={18} /> Rigenera
          </button>
          <button
            className="btn btn-primary"
            onClick={async () => {
              await api.copyText(pw);
              toast("Password copiata · si cancella tra 30 s");
            }}
          >
            <Copy size={18} /> Copia
          </button>
          <button className="btn btn-secondary" onClick={() => onSave(pw)}>
            <Plus size={18} /> Salva
          </button>
        </div>
      </section>

      <section className="card card-pad">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline" }}>
          <h3>Lunghezza</h3>
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
          {TOGGLES.map((t) => (
            <div className="toggle-row" key={t.key}>
              <div>
                <div>{t.label}</div>
                <div className="muted mono" style={{ fontSize: 12, marginTop: 2 }}>{t.hint}</div>
              </div>
              <Switch checked={opts[t.key] as boolean} onChange={(b) => update({ [t.key]: b })} />
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
