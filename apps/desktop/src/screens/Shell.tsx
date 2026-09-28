import type { ReactElement } from "react";
import { useCallback, useEffect, useState } from "react";
import { Home as HomeIcon, KeyRound, Lock, Settings as SettingsIcon, ShieldCheck } from "lucide-react";
import { api, type EntryInput, type EntrySummary, type HealthReport } from "../lib/api";
import { Logo, useToast } from "../components/ui";
import { emptyInput } from "../lib/utils";
import Home from "./Home";
import Generator from "./Generator";
import Security from "./Security";
import Settings from "./Settings";
import EntrySheet from "./EntrySheet";
import EditorSheet from "./EditorSheet";

export type Tab = "home" | "generator" | "security" | "settings";
export type Editing = { id: string | null; input: EntryInput };



export default function Shell({ onLock }: { onLock: () => void }) {
  const [tab, setTab] = useState<Tab>("home");
  const [entries, setEntries] = useState<EntrySummary[]>([]);
  const [report, setReport] = useState<HealthReport | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [editing, setEditing] = useState<Editing | null>(null);

  const reload = useCallback(async () => {
    try {
      const [list, rep] = await Promise.all([api.listEntries(), api.securityReport()]);
      setEntries(list);
      setReport(rep);
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
    const un = api.onBrowserFill(({ title, host }) => toast(`${title} compilato su ${host}`));
    return () => void un.then((f) => f());
  }, [toast]);

  const startNew = (password = "") => setEditing({ id: null, input: { ...emptyInput, password } });

  const nav: { id: Tab; label: string; icon: ReactElement }[] = [
    { id: "home", label: "Password", icon: <HomeIcon size={20} /> },
    { id: "generator", label: "Generatore", icon: <KeyRound size={20} /> },
    { id: "security", label: "Sicurezza", icon: <ShieldCheck size={20} /> },
    { id: "settings", label: "Impostazioni", icon: <SettingsIcon size={20} /> },
  ];

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
        <div className="nav-spacer" />
        <button className="nav-item" onClick={onLock} title="Blocca">
          <Lock size={20} />
          <span>Blocca</span>
        </button>
      </nav>

      <main className="main" key={tab}>
        {tab === "home" && (
          <Home entries={entries} report={report} onOpen={setOpen} onNew={() => startNew()} onGo={setTab} onLock={onLock} />
        )}
        {tab === "generator" && <Generator onSave={(pw) => startNew(pw)} />}
        {tab === "security" && <Security entries={entries} report={report} onOpen={setOpen} />}
        {tab === "settings" && <Settings onImported={reload} />}
      </main>

      {open && !editing && (
        <EntrySheet
          id={open}
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
    </div>
  );
}

