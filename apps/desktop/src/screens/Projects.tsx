// icons: shield
import Guide from "./Guide";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Icon } from "../components/Icon";
import { api, type EnvDiff, type EnvFile, type EnvSummary, type FoundEnv } from "../lib/api";
import { Sheet, Switch, useToast } from "../components/ui";
import { isMobile } from "../lib/utils";
import { useT, type TKey } from "../i18n";

/** Variables of a `.env`, in order. Display only: the Rust parser is what `run` uses. */
function variables(text: string): { key: string; value: string }[] {
  const out: { key: string; value: string }[] = [];
  for (const line of text.split("\n")) {
    const m = line.match(/^\s*(?:export\s+)?([A-Za-z_][\w.-]*)\s*=\s*(.*)$/);
    if (m) out.push({ key: m[1], value: m[2].replace(/^(['"])(.*)\1$/, "$2") });
  }
  return out;
}

/** `.env` files per project and environment, encrypted in the vault. */
export default function Projects() {
  const { t, plural, errorMessage } = useT();
  const toast = useToast();
  const [envs, setEnvs] = useState<EnvSummary[]>([]);
  const [open, setOpen] = useState<string | null>(null);
  const [adding, setAdding] = useState<{ project: string } | null>(null);
  const [found, setFound] = useState<FoundEnv[] | null>(null);
  const [cli, setCli] = useState(false);
  const [guide, setGuide] = useState(false);

  const load = useCallback(() => api.envList().then(setEnvs), []);
  useEffect(() => {
    load();
  }, [load]);

  const live = envs.filter((e) => !e.deleted_at);
  const trashed = envs.filter((e) => e.deleted_at);
  const projects = useMemo(() => {
    const map = new Map<string, EnvSummary[]>();
    for (const e of live) map.set(e.project, [...(map.get(e.project) ?? []), e]);
    return [...map.entries()];
  }, [envs]);

  const scan = async () => {
    try {
      const list = await api.envScan();
      if (list?.length === 0) toast(t("projects.scanNone"));
      else if (list) setFound(list);
    } catch (e) {
      toast(errorMessage(e));
    }
  };

  const restore = async (id: string) => {
    try {
      await api.envRestore(id);
      await load();
    } catch (e) {
      toast(errorMessage(e));
    }
  };

  if (guide) {
    return (
      <>
        <Guide onBack={() => setGuide(false)} onInstallCli={() => setCli(true)} />
        {cli && <CliSheet onClose={() => setCli(false)} />}
      </>
    );
  }

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>{t("projects.title")}</h1>
          <p className="muted">{t("projects.sub")}</p>
        </div>
        <button className="btn btn-primary" onClick={() => setAdding({ project: "" })}>
          <Icon name="add" size={18} /> {t("projects.add")}
        </button>
      </div>

      <button className="guide-entry" onClick={() => setGuide(true)}>
        <span className="guide-hero-icon"><Icon name="menu_book" size={22} /></span>
        <span className="row-main">
          <span className="row-title">{t("guide.title")}</span>
          <span className="row-sub">{t("guide.sub")}</span>
        </span>
        <Icon name="chevron_right" size={20} />
      </button>

      {!isMobile() && (
        <div className="form-actions">
          <button className="btn btn-secondary" onClick={scan}>
            <Icon name="folder_open" size={18} /> {t("projects.scan")}
          </button>
          <button className="btn btn-secondary" onClick={() => setCli(true)}>
            <Icon name="terminal" size={18} /> {t("projects.cli")}
          </button>
        </div>
      )}

      {projects.length === 0 && (
        <div className="empty">
          <div className="empty-icon"><Icon name="folder" size={28} /></div>
          <p className="muted">{t("projects.empty")}</p>
        </div>
      )}

      {projects.map(([name, list]) => (
        <section key={name} className="card">
          <div className="card-head">
            <h3>{name}</h3>
            <button className="icon-btn" title={t("projects.addEnv")} onClick={() => setAdding({ project: name })}>
              <Icon name="add" size={18} />
            </button>
          </div>
          {list.map((e) => (
            <button key={e.id} className="row env-row" onClick={() => setOpen(e.id)}>
              <Icon name={e.production ? "shield" : "description"} size={20} />
              <span className="row-main">
                <span className="row-title">{e.name}</span>
                <span className="row-sub">{plural("projects.vars", e.vars)}</span>
              </span>
              {e.production && <span className="env-tag">{t("projects.production")}</span>}
              {e.ai_access && <Icon name="smart_toy" size={18} />}
            </button>
          ))}
        </section>
      ))}

      {trashed.length > 0 && (
        <section className="card">
          <div className="card-head"><h3>{t("projects.trash")}</h3></div>
          {trashed.map((e) => (
            <div key={e.id} className="detail-item">
              <span className="row-main">
                <span className="row-title">{e.project} · {e.name}</span>
                <span className="row-sub">{t("projects.trashSub")}</span>
              </span>
              <button className="btn btn-secondary btn-sm" onClick={() => restore(e.id)}>{t("trash.restore")}</button>
            </div>
          ))}
        </section>
      )}

      {open && <EnvSheet id={open} siblings={live} onClose={() => setOpen(null)} onChanged={load} />}
      {adding && (
        <AddSheet
          project={adding.project}
          existing={live}
          onClose={() => setAdding(null)}
          onDone={(id) => {
            setAdding(null);
            load();
            setOpen(id);
          }}
        />
      )}
      {found && <ScanSheet found={found} onClose={() => setFound(null)} onDone={() => (setFound(null), load())} />}
      {cli && <CliSheet onClose={() => setCli(false)} />}
    </div>
  );
}

function AddSheet({ project, existing, onClose, onDone }: { project: string; existing: EnvSummary[]; onClose: () => void; onDone: (id: string) => void }) {
  const { t, errorMessage } = useT();
  const toast = useToast();
  const [v, setV] = useState({ project, name: "development", content: "" });
  const pick = async () => {
    try {
      const f = await api.envPickFile();
      if (f) setV((o) => ({ ...o, name: f.name, content: f.content }));
    } catch (e) {
      toast(errorMessage(e));
    }
  };
  const same = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();
  const save = async () => {
    try {
      // Saving over an environment is what editing it is for, not this form.
      if (existing.some((e) => same(e.project, v.project) && same(e.name, v.name))) throw "env_exists";
      onDone(await api.envPut(v.project, v.name, v.content));
    } catch (e) {
      toast(errorMessage(e));
    }
  };
  return (
    <Sheet onClose={onClose} title={t("projects.addTitle")}>
      <div className="form">
        <label>
          <span className="label">{t("projects.project")}</span>
          <input className="input" list="env-projects" value={v.project} onChange={(e) => setV({ ...v, project: e.target.value })} spellCheck={false} autoFocus={!project} />
          <datalist id="env-projects">{[...new Set(existing.map((e) => e.project))].map((n) => <option key={n} value={n} />)}</datalist>
        </label>
        <label>
          <span className="label">{t("projects.environment")}</span>
          <input className="input" list="env-names" value={v.name} onChange={(e) => setV({ ...v, name: e.target.value })} spellCheck={false} />
          <datalist id="env-names">{["development", "staging", "production", "local", "test"].map((n) => <option key={n} value={n} />)}</datalist>
        </label>
        <label>
          <div className="label-row">
            <span className="label">{t("projects.content")}</span>
            <button type="button" className="btn btn-secondary btn-sm" onClick={pick}>
              <Icon name="upload_file" size={16} /> {t("projects.fromFile")}
            </button>
          </div>
          <textarea className="input mono env-text" value={v.content} onChange={(e) => setV({ ...v, content: e.target.value })} placeholder={"DATABASE_URL=…\nAPI_KEY=…"} spellCheck={false} />
        </label>
        <div className="form-actions">
          <button className="btn btn-secondary" onClick={onClose}>{t("common.cancel")}</button>
          <button className="btn btn-primary" disabled={!v.project.trim() || !v.name.trim()} onClick={save}>{t("common.save")}</button>
        </div>
      </div>
    </Sheet>
  );
}

function EnvSheet({ id, siblings, onClose, onChanged }: { id: string; siblings: EnvSummary[]; onClose: () => void; onChanged: () => void }) {
  const { t, formatDate, errorMessage } = useT();
  const toast = useToast();
  const [env, setEnv] = useState<EnvFile | null>(null);
  const [edit, setEdit] = useState<string | null>(null);
  const [shown, setShown] = useState<Set<string>>(new Set());
  const [compare, setCompare] = useState<{ name: string; diff: EnvDiff } | null>(null);
  const [history, setHistory] = useState(false);

  const load = useCallback(() => api.envGet(id).then(setEnv), [id]);
  useEffect(() => {
    load();
  }, [load]);

  const attempt = async (fn: () => Promise<unknown>, msg?: string) => {
    try {
      await fn();
      if (msg) toast(msg);
      await load();
      onChanged();
    } catch (e) {
      toast(errorMessage(e));
    }
  };
  if (!env) return null;
  const production = siblings.find((s) => s.id === id)?.production ?? false;
  const others = siblings.filter((s) => s.project === env.project && s.id !== id);

  if (edit !== null) {
    return (
      <Sheet onClose={() => setEdit(null)} title={`${env.project} · ${env.name}`}>
        <div className="form">
          <textarea className="input mono env-text env-text-lg" value={edit} onChange={(e) => setEdit(e.target.value)} spellCheck={false} autoFocus />
          <div className="form-actions">
            <button className="btn btn-secondary" onClick={() => setEdit(null)}>{t("common.cancel")}</button>
            <button className="btn btn-primary" onClick={() => attempt(() => api.envUpdate(id, edit)).then(() => setEdit(null))}>{t("common.save")}</button>
          </div>
        </div>
      </Sheet>
    );
  }

  return (
    <Sheet onClose={onClose} title={`${env.project} · ${env.name}`}>
      <div className="detail-list">
        {variables(env.content).map(({ key, value }) => (
          <div key={key} className="detail-item">
            <span className="row-main">
              <span className="row-sub mono">{key}</span>
              <span className="row-title mono env-value">{shown.has(key) ? value : "••••••••"}</span>
            </span>
            <button
              className="icon-btn"
              title={t(shown.has(key) ? "common.hide" : "common.show")}
              onClick={() => {
                const next = new Set(shown);
                if (!next.delete(key)) next.add(key);
                setShown(next);
              }}
            >
              <Icon name={shown.has(key) ? "visibility_off" : "visibility"} size={18} />
            </button>
            <button className="icon-btn" title={t("common.copy")} onClick={() => api.copyText(value).then(() => toast(t("projects.copied", { key })))}>
              <Icon name="content_copy" size={18} />
            </button>
          </div>
        ))}
      </div>

      <div className="env-actions">
        <button className="btn btn-secondary btn-sm" onClick={() => setEdit(env.content)}><Icon name="edit" size={16} /> {t("common.edit")}</button>
        <button className="btn btn-secondary btn-sm" onClick={() => api.copyText(env.content).then(() => toast(t("projects.copiedAll")))}><Icon name="content_copy" size={16} /> {t("projects.copyAll")}</button>
        <button className="btn btn-secondary btn-sm" onClick={() => attempt(async () => (await api.envExport(id)) && toast(t("projects.exported")))}><Icon name="download" size={16} /> {t("projects.export")}</button>
        <button className="btn btn-secondary btn-sm" onClick={() => attempt(async () => { await api.copyText(await api.envExample(id)); toast(t("projects.exampleCopied")); })}><Icon name="draft" size={16} /> {t("projects.example")}</button>
        {env.history.length > 0 && (
          <button className="btn btn-secondary btn-sm" onClick={() => setHistory(!history)}><Icon name="history" size={16} /> {t("projects.versions", { n: env.history.length })}</button>
        )}
      </div>

      {history && (
        <div className="detail-list" style={{ marginTop: 12 }}>
          {env.history.map((h) => (
            <div key={h.replaced_at} className="detail-item">
              <span className="row-main"><span className="row-title">{formatDate(h.replaced_at)}</span></span>
              <button className="btn btn-secondary btn-sm" onClick={() => attempt(() => api.envUpdate(id, h.content), t("projects.versionRestored"))}>{t("trash.restore")}</button>
            </div>
          ))}
        </div>
      )}

      {others.length > 0 && (
        <div style={{ marginTop: 16 }}>
          <span className="label">{t("projects.compareWith")}</span>
          <div className="env-actions">
            {others.map((o) => (
              <button key={o.id} className="btn btn-secondary btn-sm" onClick={() => api.envDiff(id, o.id).then((diff) => setCompare({ name: o.name, diff }))}>{o.name}</button>
            ))}
          </div>
          {compare && (
            <div className="env-diff">
              {!compare.diff.only_a.length && !compare.diff.only_b.length && !compare.diff.different.length && <p className="muted">{t("projects.diffSame")}</p>}
              {compare.diff.only_a.length > 0 && <p>{t("projects.diffOnly", { name: env.name })} <span className="mono">{compare.diff.only_a.join(", ")}</span></p>}
              {compare.diff.only_b.length > 0 && <p>{t("projects.diffOnly", { name: compare.name })} <span className="mono">{compare.diff.only_b.join(", ")}</span></p>}
              {compare.diff.different.length > 0 && <p>{t("projects.diffDifferent")} <span className="mono">{compare.diff.different.join(", ")}</span></p>}
            </div>
          )}
        </div>
      )}

      {!isMobile() && (
        <div className="detail-item check-row env-ai">
          <span className="row-main">
            <span className="row-title">{t("projects.aiTitle")}</span>
            <span className="row-sub">{production ? t("projects.aiProduction") : t("projects.aiSub")}</span>
          </span>
          {!production && <Switch checked={env.ai_access} onChange={(on) => attempt(() => api.envSetAi(id, on))} />}
        </div>
      )}

      <button
        className="btn btn-danger btn-block"
        style={{ marginTop: 14 }}
        onClick={async () => {
          try {
            await api.envDelete(id);
            toast(t("projects.deletedToast"));
            onChanged();
            onClose();
          } catch (e) {
            toast(errorMessage(e));
          }
        }}
      >
        <Icon name="delete" size={18} /> {t("entry.deletePermanently")}
      </button>
    </Sheet>
  );
}

function ScanSheet({ found, onClose, onDone }: { found: FoundEnv[]; onClose: () => void; onDone: () => void }) {
  const { t, plural, errorMessage } = useT();
  const toast = useToast();
  const [picked, setPicked] = useState(() => new Set(found.map((f) => f.path)));
  const run = async () => {
    try {
      const n = await api.envImportFound(found.filter((f) => picked.has(f.path)));
      toast(t("projects.imported", { n }));
      onDone();
    } catch (e) {
      toast(errorMessage(e));
    }
  };
  return (
    <Sheet onClose={onClose} title={t("projects.scanTitle")}>
      <p className="muted" style={{ margin: "6px 0 14px" }}>{t("projects.scanIntro")}</p>
      <div className="detail-list">
        {found.map((f) => (
          <label key={f.path} className="detail-item check-row">
            <input
              type="checkbox"
              checked={picked.has(f.path)}
              onChange={(e) => {
                const next = new Set(picked);
                if (e.target.checked) next.add(f.path);
                else next.delete(f.path);
                setPicked(next);
              }}
            />
            <span className="row-main">
              <span className="row-title">{f.project} · {f.name}</span>
              <span className="row-sub">{plural("projects.vars", f.vars)}</span>
            </span>
          </label>
        ))}
      </div>
      <div className="form-actions" style={{ marginTop: 16 }}>
        <button className="btn btn-secondary" onClick={onClose}>{t("common.cancel")}</button>
        <button className="btn btn-primary" disabled={!picked.size} onClick={run}>{t("projects.importN", { n: picked.size })}</button>
      </div>
    </Sheet>
  );
}

function CliSheet({ onClose }: { onClose: () => void }) {
  const { t, errorMessage } = useT();
  const toast = useToast();
  const [state, setState] = useState<{ installed: boolean; new_terminal: boolean } | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    api.cliStatus().then(setState);
  }, []);
  const install = async () => {
    setBusy(true);
    try {
      setState(await api.cliInstall());
    } catch (e) {
      toast(errorMessage(e));
    }
    setBusy(false);
  };
  const commands: { code: string; text: TKey }[] = [
    { code: "kryptos env run oikos development -- npm run dev", text: "guide.cRun" },
    { code: "kryptos env list", text: "guide.cList" },
    { code: "kryptos env pull oikos development", text: "guide.cPull" },
    { code: "kryptos env push oikos development", text: "guide.cPush" },
  ];
  return (
    <Sheet onClose={onClose} title={t("projects.cliTitle")}>
      <div className={`cli-card ${state?.installed ? "done" : ""}`}>
        <span className="guide-hero-icon"><Icon name={state?.installed ? "check_circle" : "terminal"} size={24} /></span>
        <span className="row-main">
          <span className="row-title">{t(state?.installed ? "projects.cliReady" : "projects.cliNotReady")}</span>
          <span className="row-sub">{t(state?.new_terminal ? "projects.cliNewTerminal" : state?.installed ? "projects.cliReadySub" : "projects.cliNotReadySub")}</span>
        </span>
        {state && !state.installed && (
          <button className="btn btn-primary btn-sm" disabled={busy} onClick={install}>{t("projects.cliInstallButton")}</button>
        )}
      </div>
      <div className="guide-commands" style={{ marginTop: 18 }}>
        {commands.map((c) => (
          <div key={c.code}>
            <div className="guide-code">
              <code>{c.code}</code>
              <button className="icon-btn" title={t("common.copy")} onClick={() => api.copyText(c.code).then(() => toast(t("projects.cliCopied")))}>
                <Icon name="content_copy" size={16} />
              </button>
            </div>
            <p>{t(c.text)}</p>
          </div>
        ))}
      </div>
    </Sheet>
  );
}
