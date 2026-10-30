// icons: arrow_back menu_book terminal groups upload_file folder history compare_arrows draft delete shield fingerprint play_arrow download upload key lock person_add verified_user schedule check_circle content_copy warning sync link_off
import { useState, type ReactNode } from "react";
import { Icon } from "../components/Icon";
import { api } from "../lib/api";
import { useToast } from "../components/ui";
import { useT, type TKey } from "../i18n";

type Part = "projects" | "cli" | "team";

/** Explains Projects, the command line and team sharing, from the Projects screen. */
export default function Guide({ onBack, onInstallCli }: { onBack: () => void; onInstallCli: () => void }) {
  const { t } = useT();
  const [part, setPart] = useState<Part>("projects");
  const parts: { id: Part; icon: string; label: TKey }[] = [
    { id: "projects", icon: "folder", label: "guide.tabProjects" },
    { id: "cli", icon: "terminal", label: "guide.tabCli" },
    { id: "team", icon: "groups", label: "guide.tabTeam" },
  ];
  return (
    <div className="page guide">
      <button className="guide-back" onClick={onBack}>
        <Icon name="arrow_back" size={18} /> {t("projects.title")}
      </button>

      <header className="guide-hero">
        <div className="guide-hero-icon"><Icon name="menu_book" size={26} /></div>
        <span className="guide-eyebrow">{t("guide.eyebrow")}</span>
        <h1>{t("guide.title")}</h1>
        <p>{t("guide.sub")}</p>
      </header>

      <nav className="guide-tabs">
        {parts.map((p) => (
          <button key={p.id} className={part === p.id ? "on" : ""} onClick={() => setPart(p.id)}>
            <Icon name={p.icon} size={18} /> <span>{t(p.label)}</span>
          </button>
        ))}
      </nav>

      <div key={part} className="guide-body">
        {part === "projects" && <ProjectsPart />}
        {part === "cli" && <CliPart onInstall={onInstallCli} />}
        {part === "team" && <TeamPart />}
      </div>
    </div>
  );
}

function Steps({ items }: { items: { icon: string; title: string; text: string }[] }) {
  return (
    <ol className="guide-steps">
      {items.map((s, i) => (
        <li key={s.title}>
          <span className="guide-step-icon"><Icon name={s.icon} size={20} /><b>{i + 1}</b></span>
          <div>
            <h3>{s.title}</h3>
            <p>{s.text}</p>
          </div>
        </li>
      ))}
    </ol>
  );
}

function Note({ icon, children }: { icon: string; children: ReactNode }) {
  return (
    <aside className="guide-note">
      <Icon name={icon} size={20} />
      <p>{children}</p>
    </aside>
  );
}

function ProjectsPart() {
  const { t } = useT();
  const features = [
    { icon: "history", title: t("guide.fHistory"), text: t("guide.fHistoryText") },
    { icon: "compare_arrows", title: t("guide.fCompare"), text: t("guide.fCompareText") },
    { icon: "draft", title: t("guide.fExample"), text: t("guide.fExampleText") },
    { icon: "delete", title: t("guide.fTrash"), text: t("guide.fTrashText") },
  ];
  return (
    <>
      <h2 className="guide-h">{t("guide.pHow")}</h2>
      <Steps
        items={[
          { icon: "upload_file", title: t("guide.p1"), text: t("guide.p1Text") },
          { icon: "folder", title: t("guide.p2"), text: t("guide.p2Text") },
          { icon: "sync", title: t("guide.p3"), text: t("guide.p3Text") },
        ]}
      />
      <h2 className="guide-h">{t("guide.pMore")}</h2>
      <div className="guide-grid">
        {features.map((f) => (
          <article key={f.title}>
            <Icon name={f.icon} size={20} />
            <h3>{f.title}</h3>
            <p>{f.text}</p>
          </article>
        ))}
      </div>
      <Note icon="shield">{t("guide.pProduction")}</Note>
    </>
  );
}

function CliPart({ onInstall }: { onInstall: () => void }) {
  const { t } = useT();
  const toast = useToast();
  const commands: { code: string; text: string }[] = [
    { code: "kryptos env list", text: t("guide.cList") },
    { code: "kryptos env run oikos development -- npm run dev", text: t("guide.cRun") },
    { code: "kryptos env pull oikos development", text: t("guide.cPull") },
    { code: "kryptos env push oikos development", text: t("guide.cPush") },
    { code: "kryptos env example", text: t("guide.cExample") },
  ];
  return (
    <>
      <section className="guide-spot">
        <span className="guide-eyebrow">{t("guide.runEyebrow")}</span>
        <h2>{t("guide.runTitle")}</h2>
        <p>{t("guide.runText")}</p>
        <div className="guide-flow">
          <span><Icon name="lock" size={20} />Kryptos</span>
          <i />
          <span className="on"><Icon name="fingerprint" size={20} />{t("guide.runConfirm")}</span>
          <i />
          <span><Icon name="play_arrow" size={20} />{t("guide.runCommand")}</span>
        </div>
      </section>

      <h2 className="guide-h">{t("guide.cTitle")}</h2>
      <div className="guide-commands">
        {commands.map((c) => (
          <div key={c.code}>
            <div className="guide-code">
              <code>{c.code}</code>
              <button className="icon-btn" title={t("common.copy")} onClick={() => api.copyText(c.code).then(() => toast(t("projects.cliCopied")))}>
                <Icon name="content_copy" size={16} />
              </button>
            </div>
            <p>{c.text}</p>
          </div>
        ))}
      </div>
      <Note icon="fingerprint">{t("guide.cConfirm")}</Note>
      <button className="btn btn-primary btn-block" onClick={onInstall}>
        <Icon name="terminal" size={18} /> {t("guide.cInstall")}
      </button>
    </>
  );
}

function TeamPart() {
  const { t } = useT();
  return (
    <>
      <div className="guide-compare">
        <article>
          <span className="guide-badge">{t("guide.tNowBadge")}</span>
          <Icon name="key" size={24} />
          <h3>{t("guide.tNow")}</h3>
          <p>{t("guide.tNowText")}</p>
        </article>
        <article className="soon">
          <span className="guide-badge">{t("guide.tSoonBadge")}</span>
          <Icon name="person_add" size={24} />
          <h3>{t("guide.tSoon")}</h3>
          <p>{t("guide.tSoonText")}</p>
        </article>
      </div>

      <h2 className="guide-h">{t("guide.iTitle")}</h2>
      <p className="guide-lead">{t("guide.iLead")}</p>
      <Steps
        items={[
          { icon: "key", title: t("guide.i1"), text: t("guide.i1Text") },
          { icon: "upload", title: t("guide.i2"), text: t("guide.i2Text") },
          { icon: "verified_user", title: t("guide.i3"), text: t("guide.i3Text") },
          { icon: "link_off", title: t("guide.i4"), text: t("guide.i4Text") },
        ]}
      />
      <h2 className="guide-h">{t("guide.tTodayTitle")}</h2>
      <Steps
        items={[
          { icon: "person_add", title: t("guide.t1"), text: t("guide.t1Text") },
          { icon: "sync", title: t("guide.t2"), text: t("guide.t2Text") },
          { icon: "key", title: t("guide.t3"), text: t("guide.t3Text") },
        ]}
      />

      <Note icon="warning">{t("guide.iLimit")}</Note>
    </>
  );
}
