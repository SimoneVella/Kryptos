import { useMemo, useState, type ReactNode } from "react";
import { Icon } from "../components/Icon";
import type { EntrySummary, HealthReport } from "../lib/api";
import { Action, Avatar } from "../components/ui";
import type { Tab } from "./Shell";
import { hostOf } from "../lib/utils";
import { useT } from "../i18n";

type Filter = "all" | "favorites" | "recent";

export default function Home(props: {
  entries: EntrySummary[];
  report: HealthReport | null;
  onOpen: (id: string) => void;
  onNew: () => void;
  onGo: (t: Tab) => void;
  onLock: () => void;
  /** Wide windows: the list sits next to `detail`, the selected entry. */
  wide?: boolean;
  /** Quick copy from the list (buttons shown on hover). */
  onCopy?: (id: string, field: "username" | "password" | "totp") => void;
  selected?: string | null;
  detail?: ReactNode;
}) {
  const { t, plural } = useT();
  const { entries, report } = props;
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = entries.filter(
      (e) =>
        (filter === "all" || (filter === "favorites" && e.favorite) || (filter === "recent" && e.last_used > 0)) &&
        (!q || e.title.toLowerCase().includes(q) || e.username.toLowerCase().includes(q) || e.urls.some((u) => u.toLowerCase().includes(q))),
    );
    // Recent: the last 20 used, most recent first.
    return filter === "recent" ? list.sort((a, b) => b.last_used - a.last_used).slice(0, 20) : list;
  }, [entries, query, filter]);

  const issues = report ? new Set([...report.weak, ...report.reused, ...report.old]).size : 0;

  if (props.wide) {
    // Alphabetical, with a sticky letter above each group.
    const sorted = filter === "recent" ? visible : [...visible].sort((a, b) => a.title.localeCompare(b.title));
    let last = "";
    return (
      <div className="split">
        <section className="list-pane">
          <div className="list-head">
            <h1>
              {t("shell.navPasswords")} <span className="muted">{entries.length}</span>
            </h1>
            <button className="btn btn-primary btn-sq" onClick={props.onNew} title={t("home.actionAdd")}>
              <Icon name="add" size={20} />
            </button>
          </div>
          <label className="search">
            <Icon name="search" size={18} />
            <input placeholder={t("home.searchPlaceholder")} value={query} onChange={(e) => setQuery(e.target.value)} />
            <kbd className="kbd">⌘K</kbd>
          </label>
          <div className="chips">
            <button className={`chip ${filter === "all" ? "active" : ""}`} onClick={() => setFilter("all")}>
              {t("home.filterAll")} · {entries.length}
            </button>
            <button className={`chip ${filter === "favorites" ? "active" : ""}`} onClick={() => setFilter("favorites")}>
              <Icon name="star" size={14} /> {t("home.filterFavorites")}
            </button>
            <button className={`chip ${filter === "recent" ? "active" : ""}`} onClick={() => setFilter("recent")}>
              <Icon name="history" size={14} /> {t("home.filterRecent")}
            </button>
          </div>
          <div className="list-rows">
            {entries.length === 0 ? (
              <div className="empty">
                <div className="empty-icon"><Icon name="key" size={28} /></div>
                <h2>{t("home.emptyTitle")}</h2>
                <p className="muted">{t("home.emptySubtitle")}</p>
              </div>
            ) : sorted.length === 0 ? (
              <p className="muted" style={{ padding: 16 }}>{t("home.noResults")}</p>
            ) : (
              sorted.map((e) => {
                const letter = e.title.trim()[0]?.toUpperCase() ?? "#";
                const head = filter !== "recent" && letter !== last ? ((last = letter), <div key={`l-${letter}`} className="letter">{letter}</div>) : null;
                return [
                  head,
                  // A div, not a button: it holds the quick-copy buttons.
                  <div
                    key={e.id}
                    role="button"
                    tabIndex={0}
                    className={`row ${props.selected === e.id ? "on" : ""}`}
                    onClick={() => props.onOpen(e.id)}
                    onKeyDown={(ev) => (ev.key === "Enter" || ev.key === " ") && (ev.preventDefault(), props.onOpen(e.id))}
                  >
                    <Avatar name={e.title} urls={e.urls} />
                    <div className="row-main">
                      <span className="row-title">{e.title}</span>
                      <span className="row-sub">{e.username || hostOf(e.urls[0]) || "—"}</span>
                    </div>
                    <span className="row-badges">
                      {e.has_totp && <Icon name="schedule" size={15} />}
                      {e.favorite && <Icon name="star" size={16} className="star" fill />}
                    </span>
                    <span className="row-quick" onClick={(ev) => ev.stopPropagation()}>
                      {e.username && (
                        <button className="icon-btn" title={t("entry.copyUsernameToast")} onClick={() => props.onCopy?.(e.id, "username")}>
                          <Icon name="person" size={17} />
                        </button>
                      )}
                      <button className="icon-btn" title={t("entry.copyPasswordAria")} onClick={() => props.onCopy?.(e.id, "password")}>
                        <Icon name="key" size={17} />
                      </button>
                      {e.has_totp && (
                        <button className="icon-btn" title={t("entry.copyTotpAria")} onClick={() => props.onCopy?.(e.id, "totp")}>
                          <Icon name="schedule" size={17} />
                        </button>
                      )}
                    </span>
                  </div>,
                ];
              })
            )}
          </div>
        </section>
        {props.detail ?? (
          <section className="pane-detail pane-empty">
            <div className="empty-icon"><Icon name="key" size={28} /></div>
            <p className="muted">{t("home.pickEntry")}</p>
          </section>
        )}
      </div>
    );
  }

  return (
    <div className="page">
      <section className="hero">
        <div>
          <div className="hero-eyebrow">
            <Icon name="lock" size={14} /> {t("home.eyebrow")}
          </div>
          <div className="hero-value">
            {entries.length}
            <small>{plural("home.countNoun", entries.length)}</small>
          </div>
          {report && (
            <button className="hero-chip" onClick={() => props.onGo("security")}>
              <Icon name="verified_user" size={16} />
              {issues === 0 ? t("home.allGood") : t("home.toFix", { score: report.score, n: issues })}
              <Icon name="chevron_right" size={14} />
            </button>
          )}
        </div>
        <div className="actions">
          <Action icon={<Icon name="add" size={22} />} label={t("home.actionAdd")} onClick={props.onNew} />
          <Action icon={<Icon name="key" size={20} />} label={t("home.actionGenerate")} onClick={() => props.onGo("generator")} />
          <Action icon={<Icon name="verified_user" size={20} />} label={t("home.actionCheck")} onClick={() => props.onGo("security")} />
          <Action icon={<Icon name="lock" size={20} />} label={t("home.actionLock")} onClick={props.onLock} />
        </div>
      </section>

      <div className="search-row" style={{ display: "flex", gap: 12, alignItems: "center" }}>
        <label className="search" style={{ flex: 1 }}>
          <Icon name="search" size={18} />
          <input placeholder={t("home.searchPlaceholder")} value={query} onChange={(e) => setQuery(e.target.value)} />
        </label>
        <div className="chips">
          <button className={`chip ${filter === "all" ? "active" : ""}`} onClick={() => setFilter("all")}>
            {t("home.filterAll")}
          </button>
          <button className={`chip ${filter === "favorites" ? "active" : ""}`} onClick={() => setFilter("favorites")}>
            <Icon name="star" size={14} /> {t("home.filterFavorites")}
          </button>
          <button className={`chip ${filter === "recent" ? "active" : ""}`} onClick={() => setFilter("recent")}>
            <Icon name="history" size={14} /> {t("home.filterRecent")}
          </button>
        </div>
      </div>

      <section className="card">
        {entries.length === 0 ? (
          <div className="empty">
            <div className="empty-icon">
              <Icon name="key" size={28} />
            </div>
            <h2>{t("home.emptyTitle")}</h2>
            <p className="muted">{t("home.emptySubtitle")}</p>
            <button className="btn btn-primary" onClick={props.onNew} style={{ marginTop: 10 }}>
              <Icon name="add" size={18} /> {t("home.emptyAdd")}
            </button>
          </div>
        ) : visible.length === 0 ? (
          <div className="empty">
            <p className="muted">{t("home.noResults")}</p>
          </div>
        ) : (
          <>
            <div className="card-head">
              <h3>{filter === "favorites" ? t("home.favorites") : t("home.allPasswords")}</h3>
              <span className="muted">{visible.length}</span>
            </div>
            {visible.map((e) => (
              <button key={e.id} className="row" onClick={() => props.onOpen(e.id)}>
                <Avatar name={e.title} urls={e.urls} />
                <div className="row-main">
                  <span className="row-title">{e.title}</span>
                  <span className="row-sub">{e.username || hostOf(e.urls[0]) || "—"}</span>
                </div>
                <div className="row-end">
                  {e.favorite && <Icon name="star" size={16} className="star" fill />}
                  <Icon name="chevron_right" size={18} />
                </div>
              </button>
            ))}
          </>
        )}
      </section>
    </div>
  );
}

