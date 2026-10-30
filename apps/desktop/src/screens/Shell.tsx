import type { ReactElement } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Icon } from "../components/Icon";
import { api, type EntryInput, type EntrySummary, type HealthReport } from "../lib/api";
import { Logo, useToast } from "../components/ui";
import { emptyInput } from "../lib/utils";
import { useT } from "../i18n";
import { useBackButton } from "../lib/back";
import { useWide } from "../lib/useWide";
import CommandPalette from "./CommandPalette";
import Home from "./Home";
import Projects from "./Projects";
import ProfilesSheet from "./Profiles";
import TeamSheet from "./Team";
import Generator from "./Generator";
import Security from "./Security";
import Settings from "./Settings";
import EntrySheet from "./EntrySheet";
import EditorSheet from "./EditorSheet";

export type Tab = "home" | "projects" | "generator" | "security" | "settings";
export type Editing = { id: string | null; input: EntryInput };



export default function Shell({ onLock, onProfileChanged }: { onLock: () => void; onProfileChanged: () => void }) {
  const { t } = useT();
  const [tab, setTabState] = useState<Tab>("home");
  const [dir, setDir] = useState<"forward" | "back" | undefined>();
  const [entries, setEntries] = useState<EntrySummary[]>([]);
  const [report, setReport] = useState<HealthReport | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [editing, setEditing] = useState<Editing | null>(null);
  const [clipboardClearSecs, setClipboardClearSecs] = useState(30);
  const [palette, setPalette] = useState(false);
  const [panel, setPanel] = useState<"profiles" | "team" | null>(null);
  // Wide windows show the passwords list and the selected entry side by side.
  const wide = useWide();
  const split = wide && tab === "home";

  const reload = useCallback(async () => {
    try {
      const [list, rep, settings] = await Promise.all([api.listEntries(), api.securityReport(), api.getSettings()]);
      setEntries(list);
      setReport(rep);
      setClipboardClearSecs(settings.clipboard_clear_secs);
    } catch {
      onLock();
    }
  }, [onLock]);

  useEffect(() => {
    reload();
  }, [reload]);

  // Every password released to the browser is announced, so a fill you did not
  // trigger is immediately visible.
  const toast = useToast();
  useEffect(() => {
    const un = api.onBrowserFill(({ title, host }) => toast(t("shell.filledToast", { title, host })));
    return () => void un.then((f) => f());
  }, [toast, t]);

  // Logins saved from other apps, sites or the browser extension, including one
  // that waited for this unlock on Android (applied once the listener is in place).
  useEffect(() => {
    const un = api.onLoginSaved(({ title, outcome }) => {
      reload();
      if (outcome !== "unchanged") toast(t(outcome === "added" ? "shell.savedToast" : "shell.updatedToast", { title }));
    });
    un.then(() => api.applyPendingSave());
    return () => void un.then((f) => f());
  }, [reload, toast, t]);

  // Every secret given to an AI assistant is announced, like browser fills.
  useEffect(() => {
    const un = api.onAiUsed(({ client, title, op }) => toast(t(op === "totp" ? "ai.usedTotpToast" : op === "env" ? "ai.usedEnvToast" : "ai.usedToast", { client, title })));
    return () => void un.then((f) => f());
  }, [toast, t]);

  // Every .env read or written from the command line is announced too.
  useEffect(() => {
    const un = api.onEnvUsed(({ title, op }) => toast(t(op === "write" ? "projects.usedWriteToast" : "projects.usedReadToast", { title })));
    return () => void un.then((f) => f());
  }, [toast, t]);

  // "Open in Kryptos" from the browser extension's search.
  useEffect(() => {
    const un = api.onOpenEntry((id) => {
      setTabState("home");
      setOpen(id);
    });
    return () => void un.then((f) => f());
  }, []);

  // A 2FA code the browser extension read from a page and the user placed.
  useEffect(() => {
    const un = api.onOtpAdded((title) => {
      reload();
      toast(t("otpAdd.addedToast", { title }));
    });
    return () => void un.then((f) => f());
  }, [reload, toast, t]);

  // Changes made on another device, brought in through the cloud folder.
  useEffect(() => {
    const un = api.onVaultSynced((r) => {
      reload();
      if (r.password_changed) toast(t("shell.passwordChangedToast"));
      else toast(t("shell.syncedToast", { added: r.added, updated: r.updated, deleted: r.deleted }));
    });
    return () => void un.then((f) => f());
  }, [reload, toast, t]);

  // The detail pane is never empty while there are entries.
  useEffect(() => {
    if (split && !editing && (!open || !entries.some((e) => e.id === open)) && entries.length) {
      setOpen([...entries].sort((a, b) => a.title.localeCompare(b.title))[0].id);
    }
  }, [split, open, entries, editing]);

  // Narrowing the window must not pop the selected entry up as a sheet.
  const wasSplit = useRef(split);
  useEffect(() => {
    if (wasSplit.current && !split) setOpen(null);
    wasSplit.current = split;
  }, [split]);

  // ⌘K / Ctrl+K anywhere: the command palette (desktop).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette((p) => !p);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const startNew = (password = "") => setEditing({ id: null, input: { ...emptyInput, password } });

  const nav: { id: Tab; label: string; icon: ReactElement }[] = [
    { id: "home", label: t("shell.navPasswords"), icon: <Icon name="key" size={20} /> },
    { id: "projects", label: t("shell.navProjects"), icon: <Icon name="folder" size={20} /> },
    { id: "generator", label: t("shell.navGenerator"), icon: <Icon name="casino" size={20} /> },
    { id: "security", label: t("shell.navSecurity"), icon: <Icon name="health_and_safety" size={20} /> },
    { id: "settings", label: t("shell.navSettings"), icon: <Icon name="settings" size={20} /> },
  ];

  const setTab = (next: Tab) => {
    if (next === tab) return;
    const order = nav.map((n) => n.id);
    setDir(order.indexOf(next) > order.indexOf(tab) ? "forward" : "back");
    setTabState(next);
  };
  // Back from another tab returns to the passwords list before leaving the app.
  useBackButton(() => setTab("home"), tab !== "home");

  return (
    <div className="shell">
      <nav className="nav">
        <div className="brand">
          <Logo />
          <span>Kryptos</span>
        </div>
        {nav.map((n) => (
          <button key={n.id} className={`nav-item ${tab === n.id ? "active" : ""}`} onClick={() => setTab(n.id)} title={n.label}>
            {n.icon}
            <span>{n.label}</span>
          </button>
        ))}
        <div className="nav-extra">
          <button className={`nav-item ${panel === "profiles" ? "active" : ""}`} onClick={() => setPanel("profiles")} title={t("profiles.title")}>
            <Icon name="person" size={20} />
            <span>{t("profiles.title")}</span>
          </button>
          <button className={`nav-item ${panel === "team" ? "active" : ""}`} onClick={() => setPanel("team")} title={t("team.title")}>
            <Icon name="groups" size={20} />
            <span>{t("team.title")}</span>
          </button>
        </div>
        <div className="nav-spacer" />
        {report && (
          <button className="vault-card" onClick={() => setTab("security")}>
            <span className="vault-score">
              <svg width="36" height="36" viewBox="0 0 36 36" aria-hidden="true">
                <circle cx="18" cy="18" r="15" fill="none" stroke="var(--surface-3)" strokeWidth="3" />
                <circle
                  cx="18" cy="18" r="15" fill="none" stroke="var(--accent)" strokeWidth="3" strokeLinecap="round"
                  strokeDasharray={2 * Math.PI * 15} strokeDashoffset={2 * Math.PI * 15 * (1 - report.score / 100)}
                />
              </svg>
              <b>{report.score}</b>
            </span>
            <span className="row-main">
              <span className="row-title">{t("shell.vaultHealth")}</span>
              <span className="row-sub">{t("shell.vaultToFix", { n: new Set([...report.weak, ...report.reused, ...report.old]).size })}</span>
            </span>
          </button>
        )}
        <button className="nav-item" onClick={onLock} title={t("shell.navLock")}>
          <Icon name="lock" size={20} />
          <span>{t("shell.navLock")}</span>
        </button>
      </nav>

      <main className={`main ${split ? "main-split" : ""}`} key={tab} data-dir={dir}>
        {tab === "home" && (
          <Home
            entries={entries}
            report={report}
            onOpen={setOpen}
            onNew={() => startNew()}
            onGo={setTab}
            onLock={onLock}
            wide={split}
            selected={open}
            onCopy={async (id, field) => {
              await api.copyField(id, field);
              const e = entries.find((x) => x.id === id);
              const what = field === "password" ? "palette.copiedPassword" : field === "totp" ? "palette.copiedTotp" : "palette.copiedUser";
              toast(t(what, { title: e?.title ?? "", secs: clipboardClearSecs }));
            }}
            detail={
              split && open ? (
                <EntrySheet
                  inline
                  id={open}
                  clipboardClearSecs={clipboardClearSecs}
                  onClose={() => setOpen(null)}
                  onEdit={(id, input) => setEditing({ id, input })}
                  onDeleted={() => {
                    setOpen(null);
                    reload();
                  }}
                />
              ) : null
            }
          />
        )}
        {tab === "projects" && <Projects />}
        {tab === "generator" && <Generator onSave={(pw) => startNew(pw)} clipboardClearSecs={clipboardClearSecs} />}
        {tab === "security" && <Security entries={entries} report={report} onOpen={setOpen} onChanged={reload} />}
        {tab === "settings" && <Settings onImported={reload} onProfileChanged={onProfileChanged} />}
      </main>

      {open && !editing && !split && (
        <EntrySheet
          id={open}
          clipboardClearSecs={clipboardClearSecs}
          onClose={() => setOpen(null)}
          onEdit={(id, input) => setEditing({ id, input })}
          onDeleted={() => {
            setOpen(null);
            reload();
          }}
        />
      )}
      {editing && (
        <EditorSheet
          editing={editing}
          onClose={() => setEditing(null)}
          onSaved={(id) => {
            setEditing(null);
            setOpen(id);
            reload();
          }}
        />
      )}
      {panel === "profiles" && <ProfilesSheet onClose={() => setPanel(null)} onSwitched={onProfileChanged} />}
      {panel === "team" && <TeamSheet onClose={() => setPanel(null)} />}
      {palette && (
        <CommandPalette
          entries={entries}
          onClose={() => setPalette(false)}
          onOpen={(id) => (setTab("home"), setOpen(id))}
          onCopyPassword={async (id) => {
            await api.copyField(id, "password");
            const e = entries.find((x) => x.id === id);
            toast(t("palette.copied", { title: e?.title ?? "", secs: clipboardClearSecs }));
          }}
          onNew={() => startNew()}
          onGo={setTab}
          onLock={onLock}
        />
      )}
    </div>
  );
}

