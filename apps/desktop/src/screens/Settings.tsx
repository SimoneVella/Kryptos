import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import { CheckCircle2, Clipboard, Download, Fingerprint, Globe, KeyRound, Languages, Moon, ShieldCheck, Smartphone, Timer, Upload, WifiOff } from "lucide-react";
import { api, type BiometricStatus, type Settings as SettingsT } from "../lib/api";
import { PasswordInput, Sheet, Switch, useToast } from "../components/ui";
import { isMobile } from "../lib/utils";
import { useT, LOCALES, type TKey } from "../i18n";

const LOCK_OPTIONS: [number, TKey][] = [
  [1, "settings.lockOption1m"],
  [5, "settings.lockOption5m"],
  [15, "settings.lockOption15m"],
  [60, "settings.lockOption1h"],
  [240, "settings.lockOption4h"],
];
const CLIP_OPTIONS: [number, TKey][] = [
  [15, "settings.clipOption15s"],
  [30, "settings.clipOption30s"],
  [60, "settings.clipOption1m"],
  [120, "settings.clipOption2m"],
];

export default function Settings({ onImported }: { onImported: () => void }) {
  const { t, errorMessage, setLanguagePreference } = useT();
  const [s, setS] = useState<SettingsT | null>(null);
  const [browsers, setBrowsers] = useState<string[] | null>(null);
  const [changing, setChanging] = useState(false);
  const [busy, setBusy] = useState(false);
  const toast = useToast();

  useEffect(() => {
    api.getSettings().then(setS);
  }, []);

  // Re-check when coming back from the system settings.
  const [autofill, setAutofill] = useState<boolean | null>(null);
  useEffect(() => {
    if (!isMobile()) return;
    const check = () => document.visibilityState === "visible" && api.autofillStatus().then(setAutofill);
    check();
    document.addEventListener("visibilitychange", check);
    return () => document.removeEventListener("visibilitychange", check);
  }, []);

  const [bio, setBio] = useState<BiometricStatus | null>(null);
  useEffect(() => {
    if (isMobile()) api.biometricStatus().then(setBio);
  }, []);

  if (!s) return null;

  const toggleBiometric = async (on: boolean) => {
    try {
      if (on) {
        await api.biometricEnable();
        toast(t("settings.biometricOnToast"));
      } else {
        await api.biometricDisable();
      }
    } catch (e) {
      if (e !== "cancelled" && e !== "use_master") toast(errorMessage(e));
    }
    setBio(await api.biometricStatus());
  };

  const save = async (patch: Partial<SettingsT>) => {
    const next = { ...s, ...patch };
    setS(next);
    try {
      await api.updateSettings(next);
    } catch (e) {
      setS(s);
      toast(errorMessage(e));
    }
  };

  const importCsv = async () => {
    setBusy(true);
    try {
      const r = await api.importCsv();
      if (r) {
        onImported();
        toast(
          r.skipped
            ? t("settings.importResultSkipped", { added: r.added, skipped: r.skipped, file: r.file })
            : t("settings.importResult", { added: r.added, file: r.file }),
        );
      }
    } catch (e) {
      toast(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>{t("settings.title")}</h1>
          <p className="muted" style={{ marginTop: 6 }}>{t("settings.subtitle")}</p>
        </div>
      </div>

      <Group title={t("settings.groupLock")}>
        <Row icon={<Languages size={20} />} title={t("settings.languageTitle")} sub={t("settings.languageSub")}>
          <select
            className="select"
            value={s.language}
            onChange={(e) => {
              save({ language: e.target.value });
              setLanguagePreference(e.target.value);
            }}
          >
            <option value="system">{t("settings.languageSystem")}</option>
            {(Object.keys(LOCALES) as (keyof typeof LOCALES)[]).map((l) => (
              <option key={l} value={l}>{LOCALES[l]}</option>
            ))}
          </select>
        </Row>
        <Row icon={<Timer size={20} />} title={t("settings.autoLockTitle")} sub={isMobile() ? t("settings.autoLockSubMobile") : t("settings.autoLockSub")}>
          <Segmented options={LOCK_OPTIONS} value={s.auto_lock_minutes} onChange={(v) => save({ auto_lock_minutes: v })} />
        </Row>
        <Row
          icon={<Moon size={20} />}
          title={t("settings.lockOnSleepTitle")}
          sub={isMobile() ? t("settings.lockOnSleepSubMobile") : t("settings.lockOnSleepSubDesktop")}
        >
          <Switch checked={s.lock_on_sleep} onChange={(v) => save({ lock_on_sleep: v })} />
        </Row>
        <Row icon={<Clipboard size={20} />} title={t("settings.clipboardTitle")} sub={t("settings.clipboardSub")}>
          <Segmented options={CLIP_OPTIONS} value={s.clipboard_clear_secs} onChange={(v) => save({ clipboard_clear_secs: v })} />
        </Row>
        <Row icon={<KeyRound size={20} />} title={t("settings.masterPasswordTitle")} sub={t("settings.masterPasswordSub")}>
          <button className="btn btn-secondary btn-sm" onClick={() => setChanging(true)}>{t("settings.change")}</button>
        </Row>
        {isMobile() && bio && (
          <Row
            icon={<Fingerprint size={20} />}
            title={t("settings.biometricTitle")}
            sub={bio === "unavailable" ? t("settings.biometricUnavailableSub") : t("settings.biometricSub")}
          >
            {bio !== "unavailable" && <Switch checked={bio !== "off"} onChange={toggleBiometric} />}
          </Row>
        )}
      </Group>

      {isMobile() ? (
        <Group title={t("settings.groupAutofill")}>
          <Row
            icon={<Smartphone size={20} />}
            title={t("settings.systemAutofillTitle")}
            sub={autofill ? t("settings.systemAutofillOnSub") : t("settings.systemAutofillOffSub")}
          >
            {autofill === false && (
              <button className="btn btn-primary btn-sm" onClick={() => api.openAutofillSettings()}>{t("settings.turnOn")}</button>
            )}
            {autofill && <CheckCircle2 size={22} style={{ color: "var(--success)", flex: "none" }} />}
          </Row>
        </Group>
      ) : (
        <Group title={t("settings.groupBrowser")}>
          <Row
            icon={<Globe size={20} />}
            title={t("settings.browserExtTitle")}
            sub={
              s.browser_integration
                ? browsers?.length
                  ? t("settings.browserExtConnectedTo", { browsers: browsers.join(", ") })
                  : t("settings.browserExtConnected")
                : t("settings.browserExtDisconnectedSub")
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
                {t("settings.disconnect")}
              </button>
            ) : (
              <button
                className="btn btn-primary btn-sm"
                onClick={async () => {
                  try {
                    const r = await api.connectBrowser();
                    setBrowsers(r.browsers);
                    setS({ ...s, browser_integration: true });
                    toast(r.browsers.length ? t("settings.browserExtConnectedTo", { browsers: r.browsers.join(", ") }) : t("settings.noBrowserFound"));
                  } catch (e) {
                    toast(errorMessage(e));
                  }
                }}
              >
                {t("settings.connect")}
              </button>
            )}
          </Row>
        </Group>
      )}

      <Group title={t("settings.groupData")}>
        <Row icon={<Upload size={20} />} title={t("settings.importTitle")} sub={t("settings.importSub")}>
          <button className="btn btn-secondary btn-sm" onClick={importCsv} disabled={busy}>{t("settings.import")}</button>
        </Row>
        <Row icon={<Download size={20} />} title={t("settings.backupTitle")} sub={t("settings.backupSub")}>
          <button
            className="btn btn-secondary btn-sm"
            onClick={async () => {
              try {
                if (await api.exportBackup()) toast(t("settings.backupSaved"));
              } catch (e) {
                toast(errorMessage(e));
              }
            }}
          >
            {t("settings.export")}
          </button>
        </Row>
      </Group>

      <section className="about">
        <WifiOff size={16} />
        <span>{t("settings.about")}</span>
        <ShieldCheck size={16} />
      </section>

      {changing && (
        <ChangeMaster
          onClose={() => setChanging(false)}
          onDone={() => {
            setChanging(false);
            toast(t("settings.masterUpdatedToast"));
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

function Segmented({ options, value, onChange }: { options: [number, TKey][]; value: number; onChange: (v: number) => void }) {
  const { t } = useT();
  return (
    <div className="segmented">
      {options.map(([v, label]) => (
        <button key={v} className={value === v ? "active" : ""} onClick={() => onChange(v)}>
          {t(label)}
        </button>
      ))}
    </div>
  );
}

function ChangeMaster({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const { t, errorMessage } = useT();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (next.length < 8) return setError(t("common.errorTooShort"));
    if (next !== confirm) return setError(t("settings.errorMismatchNew"));
    setBusy(true);
    try {
      await api.changeMasterPassword(current, next);
      onDone();
    } catch (err) {
      setError(err === "wrong_password" ? t("settings.currentPasswordWrong") : errorMessage(err));
      setBusy(false);
    }
  };

  return (
    <Sheet onClose={onClose} title={t("settings.changeMasterTitle")}>
      <form className="form" onSubmit={submit} style={{ marginTop: 12 }}>
        <PasswordInput value={current} onChange={setCurrent} placeholder={t("settings.currentPasswordPlaceholder")} autoFocus />
        <PasswordInput value={next} onChange={setNext} placeholder={t("settings.newPasswordPlaceholder")} />
        <PasswordInput value={confirm} onChange={setConfirm} placeholder={t("settings.confirmNewPasswordPlaceholder")} />
        {error && <p className="error">{error}</p>}
        <div className="form-actions">
          <button type="button" className="btn btn-secondary" onClick={onClose}>{t("common.cancel")}</button>
          <button className="btn btn-primary" disabled={busy || !current || !next}>{busy ? t("settings.updateBusy") : t("settings.update")}</button>
        </div>
      </form>
    </Sheet>
  );
}
