import { useMemo, useState } from "react";
import { ChevronRight, KeyRound, Lock, Plus, Search, ShieldCheck, Star } from "lucide-react";
import type { EntrySummary, HealthReport } from "../lib/api";
import { Action, Avatar } from "../components/ui";
import type { Tab } from "./Shell";
import { hostOf } from "../lib/utils";
import { useT } from "../i18n";

type Filter = "all" | "favorites";

export default function Home(props: {
  entries: EntrySummary[];
  report: HealthReport | null;
  onOpen: (id: string) => void;
  onNew: () => void;
  onGo: (t: Tab) => void;
  onLock: () => void;
}) {
  const { t, plural } = useT();
  const { entries, report } = props;
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return entries.filter(
      (e) =>
        (filter === "all" || e.favorite) &&
        (!q || e.title.toLowerCase().includes(q) || e.username.toLowerCase().includes(q) || e.urls.some((u) => u.toLowerCase().includes(q))),
    );
  }, [entries, query, filter]);

  const issues = report ? new Set([...report.weak, ...report.reused, ...report.old]).size : 0;

  return (
    <div className="page">
      <section className="hero">
        <div>
          <div className="hero-eyebrow">
            <Lock size={14} /> {t("home.eyebrow")}
          </div>
          <div className="hero-value">
            {entries.length}
            <small>{plural("home.countNoun", entries.length)}</small>
          </div>
          {report && (
            <button className="hero-chip" onClick={() => props.onGo("security")}>
              <ShieldCheck size={16} />
              {issues === 0 ? t("home.allGood") : t("home.toFix", { score: report.score, n: issues })}
              <ChevronRight size={14} />
            </button>
          )}
        </div>
        <div className="actions">
          <Action icon={<Plus size={22} />} label={t("home.actionAdd")} onClick={props.onNew} />
          <Action icon={<KeyRound size={20} />} label={t("home.actionGenerate")} onClick={() => props.onGo("generator")} />
          <Action icon={<ShieldCheck size={20} />} label={t("home.actionCheck")} onClick={() => props.onGo("security")} />
          <Action icon={<Lock size={20} />} label={t("home.actionLock")} onClick={props.onLock} />
        </div>
      </section>

      <div className="search-row" style={{ display: "flex", gap: 12, alignItems: "center" }}>
        <label className="search" style={{ flex: 1 }}>
          <Search size={18} />
          <input placeholder={t("home.searchPlaceholder")} value={query} onChange={(e) => setQuery(e.target.value)} />
        </label>
        <div className="chips">
          <button className={`chip ${filter === "all" ? "active" : ""}`} onClick={() => setFilter("all")}>
            {t("home.filterAll")}
          </button>
          <button className={`chip ${filter === "favorites" ? "active" : ""}`} onClick={() => setFilter("favorites")}>
            <Star size={14} /> {t("home.filterFavorites")}
          </button>
        </div>
      </div>

      <section className="card">
        {entries.length === 0 ? (
          <div className="empty">
            <div className="empty-icon">
              <KeyRound size={28} />
            </div>
            <h2>{t("home.emptyTitle")}</h2>
            <p className="muted">{t("home.emptySubtitle")}</p>
            <button className="btn btn-primary" onClick={props.onNew} style={{ marginTop: 10 }}>
              <Plus size={18} /> {t("home.emptyAdd")}
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
                <Avatar name={e.title} />
                <div className="row-main">
                  <span className="row-title">{e.title}</span>
                  <span className="row-sub">{e.username || hostOf(e.urls[0]) || "—"}</span>
                </div>
                <div className="row-end">
                  {e.favorite && <Star size={16} className="star" fill="currentColor" />}
                  <ChevronRight size={18} />
                </div>
              </button>
            ))}
          </>
        )}
      </section>
    </div>
  );
}

