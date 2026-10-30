import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import ProfilesSheet from "./Profiles";
import TeamSheet from "./Team";
import { Icon } from "../components/Icon";
import { api, type AiStatus, type BiometricStatus, type CloudProvider, type CloudStatus, type Settings as SettingsT, type StagedCopy } from "../lib/api";
import { MergeSheet, ScanQrSheet, SendQrSheet } from "./Sync";
import { TrashSheet } from "./Cleanup";
import { PasswordInput, Sheet, StrengthBar, Switch, useToast } from "../components/ui";
import { estimateBits, isMac, isMobile, strengthOf } from "../lib/utils";
import { useT, LOCALES, type TKey } from "../i18n";

const LOCK_OPTIONS: [number, TKey][] = [
  [1, "settings.lockOption1m"],
  [5, "settings.lockOption5m"],
  [15, "settings.lockOption15m"],
  [60, "settings.lockOption1h"],
  [240, "settings.lockOption4h"],
];
const FILL_CONFIRM_OPTIONS: [SettingsT["fill_confirm"], TKey][] = [
  ["system", "settings.fillConfirmSystem"],
  ["master", "settings.fillConfirmMaster"],
];
const CLIP_OPTIONS: [number, TKey][] = [
  [15, "settings.clipOption15s"],
  [30, "settings.clipOption30s"],
  [60, "settings.clipOption1m"],
  [120, "settings.clipOption2m"],
];

export default function Settings({ onImported, onProfileChanged }: { onImported: () => void; onProfileChanged: () => void }) {
  const { t, plural, locale, errorMessage, setLanguagePreference } = useT();
  const [s, setS] = useState<SettingsT | null>(null);
  const [browsers, setBrowsers] = useState<string[] | null>(null);
  const [changing, setChanging] = useState(false);
  const [staged, setStaged] = useState<StagedCopy | null>(null);
  const [qr, setQr] = useState<"send" | "scan" | null>(null);
  const [trash, setTrash] = useState(false);
  const [profiles, setProfiles] = useState(false);
  const [team, setTeam] = useState(false);
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

  /** Runs an action and shows its error, if any, as a toast. */
  const attempt = async (f: () => Promise<unknown>) => {
    try {
      await f();
    } catch (e) {
      toast(errorMessage(e));
    }
  };

  const received = async (base64: string) => {
    setQr(null);
    await attempt(async () => setStaged(await api.syncStageBytes(base64)));
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

  const importOtp = async () => {
    setBusy(true);
    try {
      const r = await api.importOtp();
      if (r) {
        onImported();
        toast(t("settings.importOtpResult", { linked: r.linked, added: r.added, skipped: r.skipped, file: r.file }));
      }
    } catch (e) {
      toast(errorMessage(e));
    } finally {
      setBusy(false);
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
        <Row icon={<Icon name="translate" size={20} />} title={t("settings.languageTitle")} sub={t("settings.languageSub")}>
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
        <Row icon={<Icon name="timer" size={20} />} title={t("settings.autoLockTitle")} sub={isMobile() ? t("settings.autoLockSubMobile") : t("settings.autoLockSub")}>
          <Segmented options={LOCK_OPTIONS} value={s.auto_lock_minutes} onChange={(v) => save({ auto_lock_minutes: v })} />
        </Row>
        <Row
          icon={<Icon name="dark_mode" size={20} />}
          title={t("settings.lockOnSleepTitle")}
          sub={isMobile() ? t("settings.lockOnSleepSubMobile") : t("settings.lockOnSleepSubDesktop")}
        >
          <Switch checked={s.lock_on_sleep} onChange={(v) => save({ lock_on_sleep: v })} />
        </Row>
        <Row icon={<Icon name="content_paste" size={20} />} title={t("settings.clipboardTitle")} sub={t("settings.clipboardSub")}>
          <Segmented options={CLIP_OPTIONS} value={s.clipboard_clear_secs} onChange={(v) => save({ clipboard_clear_secs: v })} />
        </Row>
        <Row icon={<Icon name="key" size={20} />} title={t("settings.masterPasswordTitle")} sub={t("settings.masterPasswordSub")}>
          <button className="btn btn-secondary btn-sm" onClick={() => setChanging(true)}>{t("settings.change")}</button>
        </Row>
        {isMobile() && bio && (
          <Row
            icon={<Icon name="fingerprint" size={20} />}
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
            icon={<Icon name="smartphone" size={20} />}
            title={t("settings.systemAutofillTitle")}
            sub={autofill ? t("settings.systemAutofillOnSub") : t("settings.systemAutofillOffSub")}
          >
            {autofill === false && (
              <button className="btn btn-primary btn-sm" onClick={() => api.openAutofillSettings()}>{t("settings.turnOn")}</button>
            )}
            {autofill && <Icon name="check_circle" size={22} style={{ color: "var(--success)", flex: "none" }} />}
          </Row>
        </Group>
      ) : (
        <Group title={t("settings.groupBrowser")}>
          <Row
            icon={<Icon name="language" size={20} />}
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
          <Row
            icon={<Icon name="password" size={20} />}
            title={t("settings.fillConfirmTitle")}
            sub={isMac() ? t("settings.fillConfirmSubMac") : t("settings.fillConfirmSubOther")}
          >
            {/* Touch ID / Mac password only exist on macOS: elsewhere it's always the master password. */}
            {isMac() && <Segmented options={FILL_CONFIRM_OPTIONS} value={s.fill_confirm} onChange={(v) => save({ fill_confirm: v })} />}
          </Row>
        </Group>
      )}

      <Group title={t("profiles.title")}>
        <Row icon={<Icon name="person" size={20} />} title={t("profiles.rowTitle")} sub={t("profiles.rowSub")}>
          <button className="btn btn-secondary btn-sm" onClick={() => setProfiles(true)}>{t("common.edit")}</button>
        </Row>
        <Row icon={<Icon name="groups" size={20} />} title={t("team.rowTitle")} sub={t("team.rowSub")}>
          <button className="btn btn-secondary btn-sm" onClick={() => setTeam(true)}>{t("common.show")}</button>
        </Row>
      </Group>

      <Group title={t("settings.groupData")}>
        <Row icon={<Icon name="upload" size={20} />} title={t("settings.importTitle")} sub={t("settings.importSub")}>
          <button className="btn btn-secondary btn-sm" onClick={importCsv} disabled={busy}>{t("settings.import")}</button>
        </Row>
        <Row icon={<Icon name="delete" size={20} />} title={t("trash.title")} sub={t("trash.rowSub")}>
          <button className="btn btn-secondary btn-sm" onClick={() => setTrash(true)}>{t("trash.open")}</button>
        </Row>
        <Row icon={<Icon name="add_moderator" size={20} />} title={t("settings.importOtpTitle")} sub={t("settings.importOtpSub")}>
          <button className="btn btn-secondary btn-sm" onClick={importOtp} disabled={busy}>{t("settings.import")}</button>
        </Row>
        <Row icon={<Icon name="download" size={20} />} title={t("settings.backupTitle")} sub={t("settings.backupSub")}>
          <div className="row-buttons">
            <button className="btn btn-secondary btn-sm" onClick={() => attempt(async () => (await api.exportBackup()) && toast(t("settings.backupSaved")))}>
              {t("settings.export")}
            </button>
            {isMobile() && (
              <button className="btn btn-secondary btn-sm" onClick={() => attempt(api.shareBackup)}>{t("settings.share")}</button>
            )}
          </div>
        </Row>
      </Group>

      <Group title={t("settings.groupSync")}>
        <CloudSync onChangeMaster={() => setChanging(true)} onSynced={onImported} />
        <Row icon={<Icon name="file_open" size={20} />} title={t("settings.syncMergeTitle")} sub={t("settings.syncMergeSub")}>
          <button className="btn btn-secondary btn-sm" onClick={() => attempt(async () => setStaged(await api.syncStageFile()))}>
            {t("settings.syncChooseFile")}
          </button>
        </Row>
        <Row icon={<Icon name="qr_code_scanner" size={20} />} title={t("settings.syncScanTitle")} sub={t("settings.syncScanSub")}>
          <button className="btn btn-secondary btn-sm" onClick={() => setQr("scan")}>{t("settings.syncScan")}</button>
        </Row>
        <Row icon={<Icon name="qr_code_2" size={20} />} title={t("settings.syncSendTitle")} sub={t("settings.syncSendSub")}>
          <button className="btn btn-secondary btn-sm" onClick={() => setQr("send")}>{t("settings.syncShow")}</button>
        </Row>
      </Group>

      {!isMobile() && <AiAccess />}

      <section className="about">
        <Icon name="wifi_off" size={16} />
        <span>{t("settings.about")}</span>
        <Icon name="verified_user" size={16} />
      </section>
      <p className="version">{t("settings.version", { version: __APP_VERSION__, build: __BUILD_ID__ })}</p>

      {team && <TeamSheet onClose={() => setTeam(false)} />}
      {profiles && <ProfilesSheet onClose={() => setProfiles(false)} onSwitched={onProfileChanged} />}
      {trash && <TrashSheet onClose={() => setTrash(false)} onChanged={onImported} />}
      {qr === "send" && <SendQrSheet onClose={() => setQr(null)} />}
      {qr === "scan" && <ScanQrSheet onClose={() => setQr(null)} onReceived={received} />}
      {staged && (
        <MergeSheet
          copy={staged}
          onClose={() => setStaged(null)}
          onDone={(r) => {
            setStaged(null);
            onImported();
            const details = ([["sync.mergedAdded", r.added], ["sync.mergedUpdated", r.updated], ["sync.mergedDeleted", r.deleted]] as const)
              .filter(([, n]) => n > 0)
              .map(([key, n]) => plural(key, n))
              .join(locale === "zh" ? "，" : ", ");
            toast(details ? t("settings.syncMergedToast", { details }) : t("settings.syncNothingToast"));
          }}
        />
      )}
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

/**
 * Sync through a cloud folder (cloud.rs): the row with its state, and the sheet
 * that turns it on. The master password must be very strong first, since a copy
 * of the vault then sits with the cloud provider.
 */
function CloudSync({ onChangeMaster, onSynced }: { onChangeMaster: () => void; onSynced: () => void }) {
  const { t, locale, errorMessage } = useT();
  const toast = useToast();
  const [status, setStatus] = useState<CloudStatus | null>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const load = () => api.cloudStatus().then(setStatus, () => {});
    load();
    const timer = setInterval(load, 5000);
    return () => clearInterval(timer);
  }, []);

  if (!status) return null;
  const on = status.folder !== "";
  const name = status.name || providerName(status.folder);
  const time = status.status.last_sync
    ? new Date(status.status.last_sync * 1000).toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" })
    : "";
  const sub = !on
    ? t("cloud.subOff")
    : status.status.state === "other_vault"
      ? t("cloud.subOtherVault")
      : status.status.state === "team_removed"
        ? t("cloud.subTeamRemoved")
      : status.status.state === "error"
        ? t("cloud.subError", { name })
        : status.status.state === "waiting" || !time
          ? t("cloud.subWaiting", { name })
          : t("cloud.subOn", { name, time });

  return (
    <>
      <Row icon={<Icon name="cloud" size={20} />} title={t("cloud.title")} sub={sub}>
        {on && status.status.state === "other_vault" ? (
          <button className="btn btn-primary btn-sm" onClick={() => setOpen(true)}>{t("cloud.join")}</button>
        ) : on ? (
          <button
            className="btn btn-secondary btn-sm"
            onClick={async () => {
              try {
                await api.cloudDisable();
                setStatus(await api.cloudStatus());
                toast(t("cloud.disabledToast"));
              } catch (e) {
                toast(errorMessage(e));
              }
            }}
          >
            {t("cloud.disable")}
          </button>
        ) : (
          <button className="btn btn-secondary btn-sm" onClick={() => setOpen(true)}>{t("cloud.enable")}</button>
        )}
      </Row>
      {open && (
        <CloudSheet
          current={on ? { name, path: status.folder, has_vault: true } : undefined}
          onClose={() => setOpen(false)}
          onChangeMaster={() => (setOpen(false), onChangeMaster())}
          onDone={async () => {
            setOpen(false);
            onSynced();
            setStatus(await api.cloudStatus());
          }}
        />
      )}
    </>
  );
}

/** "Google Drive" for ~/Library/CloudStorage/GoogleDrive-…/My Drive, and so on. */
function providerName(folder: string) {
  if (/GoogleDrive/i.test(folder)) return "Google Drive";
  if (/CloudDocs/i.test(folder)) return "iCloud Drive";
  if (/Dropbox/i.test(folder)) return "Dropbox";
  if (/OneDrive/i.test(folder)) return "OneDrive";
  return folder.split("/").filter(Boolean).pop() ?? folder;
}

/** `current`: the folder already in use, when joining the different vault found there. */
function CloudSheet({
  current,
  onClose,
  onChangeMaster,
  onDone,
}: {
  current?: CloudProvider;
  onClose: () => void;
  onChangeMaster: () => void;
  onDone: () => void;
}) {
  const { t, errorMessage } = useT();
  const toast = useToast();
  const [providers, setProviders] = useState<CloudProvider[] | null>(null);
  const [folder, setFolder] = useState("");
  const [password, setPassword] = useState("");
  const [cloudPassword, setCloudPassword] = useState("");
  const [needsCloudPassword, setNeedsCloudPassword] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  // The Mac finds its cloud folders; Android has the user pick one (see pick below).
  useEffect(() => {
    if (current) {
      setProviders([current]);
      return setFolder(current.path);
    }
    if (isMobile()) return setProviders([]);
    api.cloudProviders().then((p) => {
      setProviders(p);
      // A folder that already has Kryptos (set up on another device) is the one to pick.
      setFolder((p.find((x) => x.has_vault) ?? p[0])?.path ?? "");
    });
  }, []);
  const pick = async () => {
    const p = await api.cloudPickFolder();
    if (p) {
      setProviders([p]);
      setFolder(p.path);
    }
  };

  const strong = strengthOf(estimateBits(password)).key === "excellent";
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!strong) return setError(t("cloud.weakPassword"));
    setBusy(true);
    setError("");
    try {
      const r = await api.cloudEnable(folder, password, needsCloudPassword ? cloudPassword : undefined);
      const name = providers?.find((p) => p.path === folder)?.name || providerName(folder);
      toast(
        r.outcome === "joined"
          ? t(isMobile() ? "cloud.joinedToastMobile" : "cloud.joinedToast", { name, added: r.added })
          : r.outcome === "synced"
            ? t("cloud.syncedToast", { name, added: r.added })
            : t("cloud.createdToast", { name }),
      );
      onDone();
    } catch (err) {
      if (err === "cloud_other_password" && !needsCloudPassword) setNeedsCloudPassword(true);
      else setError(err === "wrong_password" ? t("settings.currentPasswordWrong") : errorMessage(err));
      setBusy(false);
    }
  };

  return (
    <Sheet onClose={onClose} title={t("cloud.title")}>
      <p className="muted" style={{ margin: "8px 0 14px" }}>{t("cloud.intro")}</p>
      {providers?.length === 0 && !isMobile() ? (
        <p className="error">{t("cloud.noProviders")}</p>
      ) : (
        <form className="form" onSubmit={submit}>
          {isMobile() && !current && (
            <button type="button" className="btn btn-secondary btn-block" onClick={pick}>
              <Icon name="cloud" size={18} /> {folder ? t("cloud.pickOther") : t("cloud.pick")}
            </button>
          )}
          <div className="choice-list">
            {providers?.map((p) => (
              <label key={p.path} className={`choice ${folder === p.path ? "on" : ""}`}>
                <input type="radio" name="cloud" checked={folder === p.path} onChange={() => setFolder(p.path)} />
                <Icon name="cloud" size={18} />
                <span className="row-main">
                  <span className="row-title">{p.name}</span>
                  <span className="row-sub">{p.has_vault ? t("cloud.hasVault") : t("cloud.newFolder")}</span>
                </span>
              </label>
            ))}
          </div>
          <div>
            <label className="label">{t("cloud.masterLabel")}</label>
            <PasswordInput value={password} onChange={(v) => (setPassword(v), setError(""))} placeholder={t("settings.currentPasswordPlaceholder")} autoFocus />
            {password && !strong && (
              <p className="hint">
                {t("cloud.weakPassword")}{" "}
                <button type="button" className="link-btn" onClick={onChangeMaster}>{t("cloud.changeMaster")}</button>
              </p>
            )}
          </div>
          {needsCloudPassword && (
            <div>
              <label className="label">{t("cloud.otherLabel")}</label>
              <PasswordInput value={cloudPassword} onChange={setCloudPassword} autoFocus />
              <p className="hint">{t("cloud.otherHint")}</p>
            </div>
          )}
          {error && <p className="error">{error}</p>}
          <div className="form-actions">
            <button type="button" className="btn btn-secondary" onClick={onClose}>{t("common.cancel")}</button>
            <button className="btn btn-primary" disabled={busy || !folder || !password || (needsCloudPassword && !cloudPassword)}>
              {busy ? t("cloud.busy") : t("cloud.enable")}
            </button>
          </div>
        </form>
      )}
    </Sheet>
  );
}

/**
 * AI assistants (MCP server): off by default. On, with an expiring access code
 * shown once, assistants may request only the entries shared with AI, each
 * time confirmed in the app. Recent requests are listed (kept in memory only).
 */
function AiAccess() {
  const { t, locale, errorMessage } = useT();
  const toast = useToast();
  const [st, setSt] = useState<AiStatus | null>(null);
  const [hours, setHours] = useState(24);
  const [made, setMade] = useState<{ code: string; config: string } | null>(null);
  const load = () => api.aiStatus().then(setSt, () => {});
  useEffect(() => {
    load();
    const timer = setInterval(load, 5000);
    return () => clearInterval(timer);
  }, []);
  if (!st) return null;
  const act = async (fn: () => Promise<unknown>) => {
    try {
      await fn();
      await load();
    } catch (e) {
      toast(errorMessage(e));
    }
  };
  const until = st.expires ? new Date(st.expires * 1000).toLocaleString(locale, { dateStyle: "short", timeStyle: "short" }) : "";
  return (
    <Group title={t("ai.group")}>
      <Row icon={<Icon name="smart_toy" size={20} />} title={t("ai.title")} sub={t("ai.sub", { n: st.shared })}>
        <Switch checked={st.enabled} onChange={(b) => act(() => api.aiSetEnabled(b))} />
      </Row>
      {st.enabled && (
        <>
          <Row icon={<Icon name="password" size={20} />} title={t("ai.codeTitle")} sub={st.has_code ? t("ai.codeActive", { until }) : t("ai.codeNone")}>
            <div className="row-buttons">
              <select className="select" value={hours} onChange={(e) => setHours(Number(e.target.value))}>
                {[1, 24, 168, 720].map((h) => (
                  <option key={h} value={h}>{t(`ai.valid${h}` as TKey)}</option>
                ))}
              </select>
              <button className="btn btn-secondary btn-sm" onClick={() => act(async () => setMade(await api.aiNewCode(hours)))}>
                {st.has_code ? t("ai.newCode") : t("ai.makeCode")}
              </button>
              {st.has_code && (
                <button className="btn btn-danger btn-sm" onClick={() => act(api.aiRevoke)}>{t("ai.revoke")}</button>
              )}
            </div>
          </Row>
          {st.log.length > 0 && (
            <div className="ai-log">
              {st.log.slice(0, 6).map((l, i) => (
                <div key={i} className="ai-log-row">
                  <Icon name={l.outcome === "given" ? "check_circle" : "close"} size={15} className={l.outcome === "given" ? "ok" : "no"} />
                  <span>{t(l.op === "totp" ? "ai.logTotp" : l.op === "env" ? "ai.logEnv" : "ai.logCredential", { client: l.client, title: l.title })}</span>
                  <span className="muted">{new Date(l.at * 1000).toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" })}</span>
                </div>
              ))}
            </div>
          )}
        </>
      )}
      {made && (
        <Sheet onClose={() => setMade(null)} title={t("ai.codeSheetTitle")}>
          <p className="muted" style={{ margin: "6px 0 12px" }}>{t("ai.codeSheetIntro")}</p>
          <pre className="ai-config mono">{made.config}</pre>
          <p className="hint">{t("ai.codeSheetWarn")}</p>
          <div className="form-actions" style={{ marginTop: 14 }}>
            <button className="btn btn-secondary" onClick={() => setMade(null)}>{t("common.close")}</button>
            <button
              className="btn btn-primary"
              onClick={async () => {
                await api.copyText(made.config);
                toast(t("ai.copied"));
              }}
            >
              <Icon name="content_copy" size={18} /> {t("ai.copyConfig")}
            </button>
          </div>
        </Sheet>
      )}
    </Group>
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

function Segmented<T extends string | number>({ options, value, onChange }: { options: [T, TKey][]; value: T; onChange: (v: T) => void }) {
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
        {next && <StrengthBar bits={estimateBits(next)} />}
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
