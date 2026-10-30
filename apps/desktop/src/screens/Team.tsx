// icons: groups person_add content_copy check_circle folder_open delete logout verified_user warning
import { useEffect, useState } from "react";
import { Icon } from "../components/Icon";
import { api, type TeamMember, type TeamStatus } from "../lib/api";
import { Sheet, useToast } from "../components/ui";
import { useT } from "../i18n";

/** Sharing a vault with invitations: each person joins with their own key. */
export default function TeamSheet({ onClose }: { onClose: () => void }) {
  const { t, errorMessage } = useT();
  const toast = useToast();
  const [st, setSt] = useState<TeamStatus | null>(null);
  const [mode, setMode] = useState<"create" | "join" | "invite" | null>(null);
  const [removing, setRemoving] = useState<TeamMember | null>(null);
  const load = () => api.teamStatus().then(setSt, (e) => toast(errorMessage(e)));
  useEffect(() => {
    load();
  }, []);
  const done = () => {
    setMode(null);
    setRemoving(null);
    load();
  };
  if (!st) return null;
  const team = st.team;
  return (
    <Sheet onClose={onClose} title={t("team.title")}>
      {team ? (
        <>
          <div className="cli-card done">
            <span className="guide-hero-icon"><Icon name="groups" size={24} /></span>
            <span className="row-main">
              <span className="row-title">{team.name}</span>
              <span className="row-sub">{t(team.is_admin ? "team.youAdmin" : "team.youMember")}</span>
            </span>
          </div>
          <div className="detail-list" style={{ marginTop: 14 }}>
            {team.members.map((m) => (
              <div key={m.public} className="detail-item">
                <span className="row-main">
                  <span className="row-title">{m.name || "—"}{m.me ? ` · ${t("team.you")}` : ""}{m.admin ? ` · ${t("team.admin")}` : ""}</span>
                  <span className="row-sub mono">{m.safety}</span>
                </span>
                {team.is_admin && !m.me && (
                  <button className="icon-btn" title={t("team.remove")} onClick={() => setRemoving(m)}><Icon name="delete" size={18} /></button>
                )}
              </div>
            ))}
          </div>
          {team.is_admin ? (
            <button className="btn btn-primary btn-block" style={{ marginTop: 14 }} onClick={() => setMode("invite")}>
              <Icon name="person_add" size={18} /> {t("team.invite")}
            </button>
          ) : (
            <button className="btn btn-danger btn-block" style={{ marginTop: 14 }} onClick={() => api.teamLeave().then(done, (e) => toast(errorMessage(e)))}>
              <Icon name="logout" size={18} /> {t("team.leave")}
            </button>
          )}
        </>
      ) : (
        <>
          <p className="muted" style={{ margin: "6px 0 14px" }}>{t("team.intro")}</p>
          <div className="team-code">
            <span className="label">{t("team.yourCode")}</span>
            <div className="guide-code">
              <code>{st.code}</code>
              <button className="icon-btn" title={t("common.copy")} onClick={() => api.copyText(st.code).then(() => toast(t("team.codeCopied")))}>
                <Icon name="content_copy" size={16} />
              </button>
            </div>
            <span className="label" style={{ marginTop: 12 }}>{t("team.yourSafety")}</span>
            <b className="team-safety mono">{st.safety}</b>
            <p className="muted">{t("team.yourSafetySub")}</p>
          </div>
          <div className="form-actions" style={{ marginTop: 14 }}>
            <button className="btn btn-secondary" onClick={() => setMode("join")}>{t("team.join")}</button>
            <button className="btn btn-primary" onClick={() => setMode("create")}>{t("team.create")}</button>
          </div>
        </>
      )}
      {mode === "create" && <CreateSheet onClose={() => setMode(null)} onDone={done} />}
      {mode === "join" && <JoinSheet onClose={() => setMode(null)} onDone={done} />}
      {mode === "invite" && <InviteSheet onClose={() => setMode(null)} onDone={done} />}
      {removing && <RemoveSheet member={removing} onClose={() => setRemoving(null)} onDone={done} />}
    </Sheet>
  );
}

function usePickFolder() {
  const [folder, setFolder] = useState<{ name: string; path: string } | null>(null);
  return { folder, pick: () => api.teamPickFolder().then((f) => f && setFolder(f)) };
}

function CreateSheet({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const { t, errorMessage } = useT();
  const [name, setName] = useState("");
  const [me, setMe] = useState("");
  const [error, setError] = useState("");
  const { folder, pick } = usePickFolder();
  return (
    <Sheet onClose={onClose} title={t("team.create")}>
      <div className="form">
        <p className="muted">{t("team.createIntro")}</p>
        <input className="input" placeholder={t("team.teamName")} value={name} onChange={(e) => setName(e.target.value)} autoFocus />
        <input className="input" placeholder={t("team.yourName")} value={me} onChange={(e) => setMe(e.target.value)} />
        <button className="btn btn-secondary" onClick={pick}><Icon name="folder_open" size={18} /> {folder ? folder.name || folder.path : t("team.pickFolder")}</button>
        {error && <p className="error">{error}</p>}
        <div className="form-actions">
          <button className="btn btn-secondary" onClick={onClose}>{t("common.cancel")}</button>
          <button className="btn btn-primary" disabled={!name.trim() || !me.trim() || !folder} onClick={() => api.teamCreate(name, me, folder!.path).then(onDone, (e) => setError(errorMessage(e)))}>{t("profiles.create")}</button>
        </div>
      </div>
    </Sheet>
  );
}

function JoinSheet({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const { t, errorMessage } = useT();
  const [error, setError] = useState("");
  const [invite, setInvite] = useState<{ name: string; admin_safety: string; ticket: string } | null>(null);
  const [same, setSame] = useState(false);
  const { folder, pick } = usePickFolder();
  useEffect(() => {
    setInvite(null);
    setSame(false);
    if (folder) api.teamPeek(folder.path).then(setInvite, (e) => setError(errorMessage(e)));
  }, [folder, errorMessage]);
  return (
    <Sheet onClose={onClose} title={t("team.join")}>
      <div className="form">
        <p className="muted">{t("team.joinIntro")}</p>
        <button className="btn btn-secondary" onClick={() => (setError(""), pick())}><Icon name="folder_open" size={18} /> {folder ? folder.name || folder.path : t("team.pickFolder")}</button>
        {invite && (
          <div className="team-code">
            <span className="label">{t("team.inviteFor", { name: invite.name })}</span>
            <b className="team-safety mono">{invite.admin_safety}</b>
            <label className="check-row team-check">
              <input type="checkbox" checked={same} onChange={(e) => setSame(e.target.checked)} />
              <span>{t("team.adminSame")}</span>
            </label>
          </div>
        )}
        {error && <p className="error">{error}</p>}
        <div className="form-actions">
          <button className="btn btn-secondary" onClick={onClose}>{t("common.cancel")}</button>
          <button className="btn btn-primary" disabled={!invite || !same} onClick={() => api.teamJoin(folder!.path, invite!.ticket).then(onDone, (e) => setError(errorMessage(e)))}>{t("team.join")}</button>
        </div>
      </div>
    </Sheet>
  );
}

function InviteSheet({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const { t, errorMessage } = useT();
  const [code, setCode] = useState("");
  const [name, setName] = useState("");
  const [safety, setSafety] = useState("");
  const [same, setSame] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    setSame(false);
    setSafety("");
    if (code.trim()) api.teamCodeSafety(code).then(setSafety, () => {});
  }, [code]);
  return (
    <Sheet onClose={onClose} title={t("team.invite")}>
      <div className="form">
        <p className="muted">{t("team.inviteIntro")}</p>
        <input className="input mono" placeholder={t("team.pasteCode")} value={code} onChange={(e) => setCode(e.target.value)} spellCheck={false} autoFocus />
        <input className="input" placeholder={t("team.memberName")} value={name} onChange={(e) => setName(e.target.value)} />
        {safety && (
          <div className="team-code">
            <span className="label">{t("team.compare")}</span>
            <b className="team-safety mono">{safety}</b>
            <label className="check-row team-check">
              <input type="checkbox" checked={same} onChange={(e) => setSame(e.target.checked)} />
              <span>{t("team.memberSame")}</span>
            </label>
          </div>
        )}
        {error && <p className="error">{error}</p>}
        <div className="form-actions">
          <button className="btn btn-secondary" onClick={onClose}>{t("common.cancel")}</button>
          <button className="btn btn-primary" disabled={!safety || !same || !name.trim()} onClick={() => api.teamInvite(code, name).then(onDone, (e) => setError(errorMessage(e)))}>{t("team.invite")}</button>
        </div>
      </div>
    </Sheet>
  );
}

function RemoveSheet({ member, onClose, onDone }: { member: TeamMember; onClose: () => void; onDone: () => void }) {
  const { t, errorMessage } = useT();
  const [error, setError] = useState("");
  return (
    <Sheet onClose={onClose} title={t("team.removeTitle", { name: member.name })}>
      <div className="form">
        <aside className="guide-note"><Icon name="warning" size={20} /><p>{t("team.removeWarn")}</p></aside>
        {error && <p className="error">{error}</p>}
        <div className="form-actions">
          <button className="btn btn-secondary" onClick={onClose}>{t("common.cancel")}</button>
          <button className="btn btn-danger" onClick={() => api.teamRemove(member.public).then(onDone, (e) => setError(errorMessage(e)))}>{t("team.remove")}</button>
        </div>
      </div>
    </Sheet>
  );
}
