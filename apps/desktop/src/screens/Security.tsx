import type { ReactElement } from "react";
import { useEffect, useState } from "react";
import { Icon } from "../components/Icon";
import { api, type DuplicateGroup, type EntrySummary, type HealthReport, type TitleSuggestion } from "../lib/api";
import { DuplicatesSheet, TitlesSheet } from "./Cleanup";
import { Avatar } from "../components/ui";
import { useT } from "../i18n";

export default function Security({
  entries,
  report,
  onOpen,
  onChanged,
}: {
  entries: EntrySummary[];
  report: HealthReport | null;
  onOpen: (id: string) => void;
  onChanged: () => void;
}) {
  const { t } = useT();
  const [smart, setSmart] = useState<{ titles: TitleSuggestion[]; duplicates: DuplicateGroup[] } | null>(null);
  const [sheet, setSheet] = useState<"titles" | "dups" | null>(null);
  const loadSmart = () => api.smartSuggestions().then(setSmart, () => {});
  useEffect(() => {
    loadSmart();
  }, [entries]);
  if (!report) return null;
  const byId = new Map(entries.map((e) => [e.id, e]));
  const score = report.score;
  const color = score >= 80 ? "var(--success)" : score >= 50 ? "var(--warning)" : "var(--danger)";
  const C = 2 * Math.PI * 56;

  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>{t("security.title")}</h1>
          <p className="muted" style={{ marginTop: 6 }}>{t("security.subtitle")}</p>
        </div>
      </div>

      <section className="card card-pad score">
        <div className="score-ring">
          <svg width="132" height="132" viewBox="0 0 132 132">
            <circle cx="66" cy="66" r="56" fill="none" stroke="var(--surface-3)" strokeWidth="12" />
            <circle
              cx="66" cy="66" r="56" fill="none" stroke={color} strokeWidth="12" strokeLinecap="round"
              strokeDasharray={C} strokeDashoffset={C * (1 - score / 100)} style={{ transition: "stroke-dashoffset 0.6s" }}
            />
          </svg>
          <b>{score}</b>
        </div>
        <div>
          <h2>{score >= 80 ? t("security.scoreGreat") : score >= 50 ? t("security.scoreOk") : t("security.scoreBad")}</h2>
          <p className="muted" style={{ marginTop: 6, lineHeight: 1.5 }}>
            {report.total === 0
              ? t("security.scoreEmptyHint")
              : t("security.scoreSummary", {
                  ok: report.total - new Set([...report.weak, ...report.reused, ...report.old]).size,
                  total: report.total,
                })}
          </p>
        </div>
      </section>

      <section className="card">
        <Issue
          icon={<Icon name="warning" size={20} />} tint="var(--danger)" title={t("security.weakTitle")}
          sub={t("security.weakSub")} ids={report.weak} byId={byId} onOpen={onOpen}
        />
        <Issue
          icon={<Icon name="content_copy" size={19} />} tint="var(--warning)" title={t("security.reusedTitle")}
          sub={t("security.reusedSub")} ids={report.reused} byId={byId} onOpen={onOpen}
        />
        <Issue
          icon={<Icon name="schedule" size={19} />} tint="var(--accent)" title={t("security.oldTitle")}
          sub={t("security.oldSub")} ids={report.old} byId={byId} onOpen={onOpen}
        />
        <Issue
          icon={<Icon name="verified_user" size={19} />} tint="var(--cool)" title={t("security.no2faTitle")}
          sub={t("security.no2faSub")} ids={report.no_2fa} byId={byId} onOpen={onOpen}
        />
      </section>

      {smart && (smart.titles.length > 0 || smart.duplicates.length > 0) && (
        <section>
          <h3 className="group-title">{t("cleanup.title")}</h3>
          <div className="card">
            {smart.titles.length > 0 && (
              <button className="row" onClick={() => setSheet("titles")}>
                <div className="issue-icon" style={{ background: "var(--accent-soft)", color: "var(--accent)" }}><Icon name="edit" size={19} /></div>
                <div className="row-main">
                  <span className="row-title">{t("cleanup.titlesRow", { n: smart.titles.length })}</span>
                  <span className="row-sub">{t("cleanup.titlesRowSub", { example: `${smart.titles[0].title} → ${smart.titles[0].suggested}` })}</span>
                </div>
                <Icon name="chevron_right" size={18} className="muted" />
              </button>
            )}
            {smart.duplicates.length > 0 && (
              <button className="row" onClick={() => setSheet("dups")}>
                <div className="issue-icon" style={{ background: "var(--accent-soft)", color: "var(--accent)" }}><Icon name="content_copy" size={19} /></div>
                <div className="row-main">
                  <span className="row-title">{t("cleanup.dupRow", { n: smart.duplicates.length })}</span>
                  <span className="row-sub">{t("cleanup.dupRowSub")}</span>
                </div>
                <Icon name="chevron_right" size={18} className="muted" />
              </button>
            )}
          </div>
        </section>
      )}
      {sheet === "titles" && smart && (
        <TitlesSheet items={smart.titles} onClose={() => setSheet(null)} onDone={() => (setSheet(null), onChanged(), loadSmart())} />
      )}
      {sheet === "dups" && smart && (
        <DuplicatesSheet groups={smart.duplicates} byId={byId} onClose={() => setSheet(null)} onDone={() => (onChanged(), loadSmart())} />
      )}

    </div>
  );
}

function Issue(props: {
  icon: ReactElement; tint: string; title: string; sub: string; ids: string[];
  byId: Map<string, EntrySummary>; onOpen: (id: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const n = props.ids.length;
  return (
    <>
      <button className="row" onClick={() => n && setExpanded(!expanded)} style={{ cursor: n ? "pointer" : "default" }}>
        <div className="issue-icon" style={{ background: `color-mix(in srgb, ${props.tint} 14%, transparent)`, color: props.tint }}>{props.icon}</div>
        <div className="row-main">
          <span className="row-title">{props.title}</span>
          <span className="row-sub">{props.sub}</span>
        </div>
        <div className="row-end">
          <span className="badge" style={n ? { background: props.tint, color: "var(--bg)" } : { background: "var(--surface-2)" }}>{n}</span>
          {n > 0 && (expanded ? <Icon name="expand_more" size={18} /> : <Icon name="chevron_right" size={18} />)}
        </div>
      </button>
      {expanded &&
        props.ids.map((id) => {
          const e = props.byId.get(id);
          if (!e) return null;
          return (
            <button key={id} className="row" style={{ paddingLeft: 32 }} onClick={() => props.onOpen(id)}>
              <Avatar name={e.title} urls={e.urls} />
              <div className="row-main">
                <span className="row-title">{e.title}</span>
                <span className="row-sub">{e.username}</span>
              </div>
              <Icon name="chevron_right" size={18} className="muted" />
            </button>
          );
        })}
    </>
  );
}
