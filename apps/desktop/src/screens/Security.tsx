import type { ReactElement } from "react";
import { useState } from "react";
import { AlertTriangle, ChevronDown, ChevronRight, Clock, Copy as CopyIcon } from "lucide-react";
import type { EntrySummary, HealthReport } from "../lib/api";
import { Avatar } from "../components/ui";
import { useT } from "../i18n";

export default function Security({ entries, report, onOpen }: { entries: EntrySummary[]; report: HealthReport | null; onOpen: (id: string) => void }) {
  const { t } = useT();
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
            <circle cx="66" cy="66" r="56" fill="none" stroke="var(--surface-2)" strokeWidth="12" />
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
          icon={<AlertTriangle size={20} />} tint="var(--danger)" title={t("security.weakTitle")}
          sub={t("security.weakSub")} ids={report.weak} byId={byId} onOpen={onOpen}
        />
        <Issue
          icon={<CopyIcon size={19} />} tint="var(--warning)" title={t("security.reusedTitle")}
          sub={t("security.reusedSub")} ids={report.reused} byId={byId} onOpen={onOpen}
        />
        <Issue
          icon={<Clock size={19} />} tint="var(--accent)" title={t("security.oldTitle")}
          sub={t("security.oldSub")} ids={report.old} byId={byId} onOpen={onOpen}
        />
      </section>

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
          <span className="badge" style={n ? { background: props.tint, color: "#fff" } : { background: "var(--surface-2)" }}>{n}</span>
          {n > 0 && (expanded ? <ChevronDown size={18} /> : <ChevronRight size={18} />)}
        </div>
      </button>
      {expanded &&
        props.ids.map((id) => {
          const e = props.byId.get(id);
          if (!e) return null;
          return (
            <button key={id} className="row" style={{ paddingLeft: 32 }} onClick={() => props.onOpen(id)}>
              <Avatar name={e.title} />
              <div className="row-main">
                <span className="row-title">{e.title}</span>
                <span className="row-sub">{e.username}</span>
              </div>
              <ChevronRight size={18} className="muted" />
            </button>
          );
        })}
    </>
  );
}
