import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import { Clipboard, Download, Globe, KeyRound, Moon, ShieldCheck, Smartphone, Timer, Upload, WifiOff } from "lucide-react";
import { api, type Settings as SettingsT } from "../lib/api";
import { PasswordInput, Sheet, Switch, useToast } from "../components/ui";
import { isMobile } from "../lib/utils";

const LOCK_OPTIONS: [number, string][] = [[1, "1 min"], [5, "5 min"], [15, "15 min"], [60, "1 ora"]];
const CLIP_OPTIONS: [number, string][] = [[15, "15 s"], [30, "30 s"], [60, "1 min"], [120, "2 min"]];

export default function Settings({ onImported }: { onImported: () => void }) {
  const [s, setS] = useState<SettingsT | null>(null);
  const [browsers, setBrowsers] = useState<string[] | null>(null);
  const [changing, setChanging] = useState(false);
  const [busy, setBusy] = useState(false);
  const toast = useToast();

  useEffect(() => {
    api.getSettings().then(setS);
  }, []);
  if (!s) return null;

  const save = async (patch: Partial<SettingsT>) => {
    const next = { ...s, ...patch };
    setS(next);
    try {
      await api.updateSettings(next);
    } catch (e) {
      setS(s);
      toast(String(e));
    }
  };

  const importCsv = async () => {
    setBusy(true);
    try {
      const r = await api.importCsv();
      if (r) {
        onImported();
        toast(`${r.added} importate${r.skipped ? ` · ${r.skipped} già presenti` : ""}. Ora elimina "${r.file}".`);
      }
    } catch (e) {
      toast(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>Impostazioni</h1>
          <p className="muted" style={{ marginTop: 6 }}>Tutto resta su questo dispositivo.</p>
        </div>
      </div>

      <Group title="Blocco">
        <Row icon={<Timer size={20} />} title="Blocco automatico" sub="Dopo un periodo di inattività">
          <Segmented options={LOCK_OPTIONS} value={s.auto_lock_minutes} onChange={(v) => save({ auto_lock_minutes: v })} />
        </Row>
        <Row
          icon={<Moon size={20} />}
          title="Blocca in stop"
          sub={isMobile() ? "Quando il telefono va in standby" : "Quando il computer va in sospensione o blocchi lo schermo"}
        >
          <Switch checked={s.lock_on_sleep} onChange={(v) => save({ lock_on_sleep: v })} />
        </Row>
        <Row icon={<Clipboard size={20} />} title="Svuota appunti" sub="Cancella la password copiata dopo">
          <Segmented options={CLIP_OPTIONS} value={s.clipboard_clear_secs} onChange={(v) => save({ clipboard_clear_secs: v })} />
        </Row>
        <Row icon={<KeyRound size={20} />} title="Master password" sub="Argon2id · XChaCha20-Poly1305">
          <button className="btn btn-secondary btn-sm" onClick={() => setChanging(true)}>Cambia</button>
        </Row>
      </Group>

      {isMobile() ? (
        <Group title="Compilazione automatica">
          <Row
            icon={<Smartphone size={20} />}
            title="Autofill di sistema"
            sub="Impostazioni Android → Password e account → Servizio di compilazione automatica → Kryptos"
          >
            <span />
          </Row>
        </Group>
      ) : (
        <>
        <Group title="Browser">
          <Row
            icon={<Globe size={20} />}
            title="Estensione browser"
            sub={
              s.browser_integration
                ? browsers?.length
                  ? `Collegata a ${browsers.join(", ")}`
                  : "Collegata"
                : "Compila le password su Chrome, Arc, Brave, Edge e Firefox"
            }
          >
            {s.browser_integration ? (
              <button
                className="btn btn-secondary btn-sm"
                onClick={async () => {
                  await api.disconnectBrowser();
                  setS({ ...s, browser_integration: false });
                  setBrowsers(null);
                }}
              >
                Scollega
              </button>
            ) : (
              <button
                className="btn btn-primary btn-sm"
                onClick={async () => {
                  try {
                    const r = await api.connectBrowser();
                    setBrowsers(r.browsers);
                    setS({ ...s, browser_integration: true });
                    toast(r.browsers.length ? `Collegata a ${r.browsers.join(", ")}` : "Nessun browser trovato");
                  } catch (e) {
                    toast(String(e));
                  }
                }}
              >
                Collega
              </button>
            )}
          </Row>
        </Group>

        <Group title="Dati">
          <Row icon={<Upload size={20} />} title="Importa password" sub="CSV da Google, Chrome, Bitwarden, 1Password, Firefox">
            <button className="btn btn-secondary btn-sm" onClick={importCsv} disabled={busy}>Importa</button>
          </Row>
          <Row icon={<Download size={20} />} title="Backup cifrato" sub="Copia del vault, leggibile solo con la master password">
            <button
              className="btn btn-secondary btn-sm"
              onClick={async () => (await api.exportBackup()) && toast("Backup salvato")}
            >
              Esporta
            </button>
          </Row>
        </Group>
        </>
      )}

      <section className="about">
        <WifiOff size={16} />
        <span>Kryptos 0.1 · 100% offline · nessuna connessione di rete</span>
        <ShieldCheck size={16} />
      </section>

      {changing && (
        <ChangeMaster
          onClose={() => setChanging(false)}
          onDone={() => {
            setChanging(false);
            toast("Master password aggiornata");
          }}
        />
      )}
    </div>
  );
}

function Group({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section>
      <h3 className="group-title">{title}</h3>
      <div className="card settings-card">{children}</div>
    </section>
  );
}

function Row({ icon, title, sub, children }: { icon: ReactNode; title: string; sub: string; children: ReactNode }) {
  return (
    <div className="setting-row">
      <div className="issue-icon" style={{ background: "var(--accent-soft)", color: "var(--accent)" }}>{icon}</div>
      <div className="row-main">
        <span className="row-title">{title}</span>
        <span className="row-sub">{sub}</span>
      </div>
      {children}
    </div>
  );
}

function Segmented({ options, value, onChange }: { options: [number, string][]; value: number; onChange: (v: number) => void }) {
  return (
    <div className="segmented">
      {options.map(([v, label]) => (
        <button key={v} className={value === v ? "active" : ""} onClick={() => onChange(v)}>
          {label}
        </button>
      ))}
    </div>
  );
}

function ChangeMaster({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (next.length < 8) return setError("Usa almeno 8 caratteri.");
    if (next !== confirm) return setError("Le nuove password non coincidono.");
    setBusy(true);
    try {
      await api.changeMasterPassword(current, next);
      onDone();
    } catch (err) {
      setError(err === "wrong_password" ? "La password attuale non è corretta." : String(err));
      setBusy(false);
    }
  };

  return (
    <Sheet onClose={onClose} title="Cambia master password">
      <form className="form" onSubmit={submit} style={{ marginTop: 12 }}>
        <PasswordInput value={current} onChange={setCurrent} placeholder="Password attuale" autoFocus />
        <PasswordInput value={next} onChange={setNext} placeholder="Nuova password" />
        <PasswordInput value={confirm} onChange={setConfirm} placeholder="Ripeti la nuova password" />
        {error && <p className="error">{error}</p>}
        <div className="form-actions">
          <button type="button" className="btn btn-secondary" onClick={onClose}>Annulla</button>
          <button className="btn btn-primary" disabled={busy || !current || !next}>{busy ? "Aggiornamento…" : "Aggiorna"}</button>
        </div>
      </form>
    </Sheet>
  );
}
