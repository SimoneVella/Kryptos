import { useEffect, useState } from "react";
import { Icon } from "../components/Icon";
import { api, type Profile, type Profiles } from "../lib/api";
import { PasswordInput, Sheet, StrengthBar, useToast } from "../components/ui";
import { estimateBits } from "../lib/utils";
import { useT } from "../i18n";

/** The name to show: the first profile has none until the user gives it one. */
export function useProfileName() {
  const { t } = useT();
  return (p?: Profile) => p?.name || t("profiles.default");
}

/** Settings: the vaults on this device. Only the active one is ever open. */
export default function ProfilesSheet({ onClose, onSwitched }: { onClose: () => void; onSwitched: () => void }) {
  const { t, errorMessage } = useT();
  const toast = useToast();
  const label = useProfileName();
  const [reg, setReg] = useState<Profiles | null>(null);
  const [adding, setAdding] = useState(false);
  const [renaming, setRenaming] = useState<Profile | null>(null);
  const [deleting, setDeleting] = useState<Profile | null>(null);
  const load = () => api.profileList().then(setReg);
  useEffect(() => {
    load();
  }, []);

  const attempt = async (fn: () => Promise<unknown>) => {
    try {
      await fn();
    } catch (e) {
      toast(errorMessage(e));
    }
  };
  if (!reg) return null;
  return (
    <Sheet onClose={onClose} title={t("profiles.title")}>
      <p className="muted" style={{ margin: "6px 0 14px" }}>{t("profiles.intro")}</p>
      <div className="detail-list">
        {reg.profiles.map((p) => (
          <div key={p.id} className="detail-item">
            <span className="row-main">
              <span className="row-title">{label(p)}</span>
              <span className="row-sub">{p.id === reg.active ? t("profiles.active") : t("profiles.locked")}</span>
            </span>
            <button className="icon-btn" title={t("common.edit")} onClick={() => setRenaming(p)}><Icon name="edit" size={18} /></button>
            {p.id !== reg.active && (
              <>
                <button className="btn btn-secondary btn-sm" onClick={() => attempt(() => api.profileSwitch(p.id).then(onSwitched))}>{t("profiles.switch")}</button>
                {p.id !== "default" && (
                  <button className="icon-btn" title={t("common.delete")} onClick={() => setDeleting(p)}><Icon name="delete" size={18} /></button>
                )}
              </>
            )}
          </div>
        ))}
      </div>
      <button className="btn btn-primary btn-block" style={{ marginTop: 14 }} onClick={() => setAdding(true)}>
        <Icon name="add" size={18} /> {t("profiles.add")}
      </button>
      {adding && <AddSheet onClose={() => setAdding(false)} onDone={onSwitched} />}
      {renaming && <RenameSheet profile={renaming} onClose={() => setRenaming(null)} onDone={() => (setRenaming(null), load())} />}
      {deleting && <DeleteSheet profile={deleting} onClose={() => setDeleting(null)} onDone={() => (setDeleting(null), load())} />}
    </Sheet>
  );
}

function AddSheet({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const { t, errorMessage } = useT();
  const [name, setName] = useState("");
  const [pw, setPw] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const create = async () => {
    if (pw.length < 8) return setError(t("common.errorTooShort"));
    if (pw !== confirm) return setError(t("setup.errorMismatch"));
    setBusy(true);
    try {
      await api.profileCreate(name, pw);
      onDone();
    } catch (e) {
      setError(errorMessage(e));
      setBusy(false);
    }
  };
  return (
    <Sheet onClose={onClose} title={t("profiles.addTitle")}>
      <div className="form">
        <p className="muted">{t("profiles.addIntro")}</p>
        <input className="input" placeholder={t("profiles.namePlaceholder")} value={name} onChange={(e) => setName(e.target.value)} autoFocus />
        <PasswordInput value={pw} onChange={setPw} placeholder={t("setup.placeholderMaster")} />
        {pw && <StrengthBar bits={estimateBits(pw)} />}
        <PasswordInput value={confirm} onChange={setConfirm} placeholder={t("setup.placeholderConfirm")} />
        {error && <p className="error">{error}</p>}
        <div className="form-actions">
          <button className="btn btn-secondary" onClick={onClose}>{t("common.cancel")}</button>
          <button className="btn btn-primary" disabled={busy || !name.trim() || !pw || !confirm} onClick={create}>{busy ? t("setup.submitBusy") : t("profiles.create")}</button>
        </div>
      </div>
    </Sheet>
  );
}

function RenameSheet({ profile, onClose, onDone }: { profile: Profile; onClose: () => void; onDone: () => void }) {
  const { t, errorMessage } = useT();
  const label = useProfileName();
  const [name, setName] = useState(label(profile));
  const [error, setError] = useState("");
  return (
    <Sheet onClose={onClose} title={t("profiles.renameTitle")}>
      <div className="form">
        <input className="input" value={name} onChange={(e) => setName(e.target.value)} autoFocus />
        {error && <p className="error">{error}</p>}
        <div className="form-actions">
          <button className="btn btn-secondary" onClick={onClose}>{t("common.cancel")}</button>
          <button className="btn btn-primary" disabled={!name.trim()} onClick={() => api.profileRename(profile.id, name).then(onDone, (e) => setError(errorMessage(e)))}>{t("common.save")}</button>
        </div>
      </div>
    </Sheet>
  );
}

function DeleteSheet({ profile, onClose, onDone }: { profile: Profile; onClose: () => void; onDone: () => void }) {
  const { t, errorMessage } = useT();
  const label = useProfileName();
  const [pw, setPw] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const remove = async () => {
    setBusy(true);
    try {
      await api.profileDelete(profile.id, pw);
      onDone();
    } catch (e) {
      setError(e === "wrong_password" ? t("unlock.wrongPassword") : errorMessage(e));
      setBusy(false);
    }
  };
  return (
    <Sheet onClose={onClose} title={t("profiles.deleteTitle", { name: label(profile) })}>
      <div className="form">
        <p className="muted">{t("profiles.deleteWarn")}</p>
        <PasswordInput value={pw} onChange={setPw} placeholder={t("profiles.deletePassword")} autoFocus />
        {error && <p className="error">{error}</p>}
        <div className="form-actions">
          <button className="btn btn-secondary" onClick={onClose}>{t("common.cancel")}</button>
          <button className="btn btn-danger" disabled={busy || !pw} onClick={remove}>{t("common.delete")}</button>
        </div>
      </div>
    </Sheet>
  );
}
