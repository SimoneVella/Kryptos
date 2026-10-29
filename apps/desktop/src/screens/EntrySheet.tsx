import type { ReactElement } from "react";
import { useEffect, useState } from "react";
import { Check, Copy, Eye, EyeOff, Globe, KeyRound, ListChecks, Pencil, ShieldCheck, Star, StickyNote, Trash2, User } from "lucide-react";
import { api, type Entry, type EntryInput, type TotpCode } from "../lib/api";
import { Action, Avatar, Sheet, useToast } from "../components/ui";
import { hostOf } from "../lib/utils";
import { useT } from "../i18n";

export default function EntrySheet(props: {
  id: string;
  clipboardClearSecs: number;
  onClose: () => void;
  onEdit: (id: string, input: EntryInput) => void;
  onDeleted: () => void;
}) {
  const { t, formatDate } = useT();
  const [entry, setEntry] = useState<Entry | null>(null);
  const [reveal, setReveal] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const toast = useToast();

  useEffect(() => {
    api.getEntry(props.id).then(setEntry).catch(props.onClose);
  }, [props.id]);

  if (!entry) return null;

  const copy = async (field: "username" | "password" | "totp") => {
    await api.copyField(entry.id, field);
    toast(
      field === "password"
        ? t("common.copiedClearing", { secs: props.clipboardClearSecs })
        : field === "totp"
          ? t("entry.copyTotpToast")
          : t("entry.copyUsernameToast"),
    );
  };
  const edit = () =>
    props.onEdit(entry.id, {
      title: entry.title,
      username: entry.username,
      password: entry.password,
      urls: entry.urls,
      notes: entry.notes,
      favorite: entry.favorite,
      totp: entry.totp,
      backup_codes: entry.backup_codes,
    });

  return (
    <Sheet onClose={props.onClose}>
      <div className="sheet-hero">
        <Avatar name={entry.title} urls={entry.urls} size="lg" />
        <div>
          <h2 style={{ display: "flex", alignItems: "center", gap: 8, justifyContent: "center" }}>
            {entry.title}
            {entry.favorite && <Star size={18} className="star" fill="currentColor" />}
          </h2>
          {entry.urls[0] && <p className="muted" style={{ marginTop: 4 }}>{hostOf(entry.urls[0])}</p>}
        </div>
      </div>

      <div className="sheet-actions">
        <Action icon={<User size={20} />} label={t("entry.actionUser")} onClick={() => copy("username")} />
        <Action icon={<KeyRound size={20} />} label={t("entry.actionPassword")} onClick={() => copy("password")} />
        <Action icon={<Pencil size={19} />} label={t("common.edit")} onClick={edit} />
      </div>

      <div className="detail-list">
        {entry.username && (
          <Item icon={<User size={18} />} label={t("entry.usernameLabel")} value={entry.username} onCopy={() => copy("username")} />
        )}
        <div className="detail-item">
          <KeyRound size={18} className="muted" />
          <div className="row-main">
            <span className="detail-label">{t("entry.passwordLabel")}</span>
            <span className="detail-value mono">{reveal ? entry.password : "•".repeat(Math.min(entry.password.length, 16)) || "—"}</span>
          </div>
          <button className="icon-btn" onClick={() => setReveal(!reveal)} aria-label={reveal ? t("common.hide") : t("common.show")}>
            {reveal ? <EyeOff size={18} /> : <Eye size={18} />}
          </button>
          <button className="icon-btn" onClick={() => copy("password")} aria-label={t("entry.copyPasswordAria")}>
            <Copy size={18} />
          </button>
        </div>
        {entry.totp && <TotpItem id={entry.id} onCopy={() => copy("totp")} />}
        {entry.backup_codes.length > 0 && (
          <BackupCodes
            entry={entry}
            onCopy={async (code) => {
              await api.copyText(code);
              toast(t("common.copiedClearing", { secs: props.clipboardClearSecs }));
            }}
            onMarked={(index, used) =>
              setEntry({ ...entry, backup_codes: entry.backup_codes.map((c, i) => (i === index ? { ...c, used } : c)) })
            }
          />
        )}
        {entry.urls.map((u) => (
          <Item key={u} icon={<Globe size={18} />} label={t("entry.websiteLabel")} value={u} />
        ))}
        {entry.notes && <Item icon={<StickyNote size={18} />} label={t("entry.notesLabel")} value={entry.notes} />}
      </div>

      <p className="muted" style={{ fontSize: 12, textAlign: "center", margin: "16px 0" }}>
        {t("entry.modifiedOn", { date: formatDate(entry.updated_at) })}
      </p>

      {confirmDelete ? (
        <div className="form-actions">
          <button className="btn btn-secondary" onClick={() => setConfirmDelete(false)}>{t("common.cancel")}</button>
          <button
            className="btn btn-danger"
            onClick={async () => {
              await api.deleteEntry(entry.id);
              toast(t("entry.deletedToast"));
              props.onDeleted();
            }}
          >
            {t("entry.deletePermanently")}
          </button>
        </div>
      ) : (
        <button className="btn btn-danger btn-block" onClick={() => setConfirmDelete(true)}>
          <Trash2 size={18} /> {t("common.delete")}
        </button>
      )}
    </Sheet>
  );
}

/** The current 2FA code with the seconds it has left; refreshed every second. */
function TotpItem({ id, onCopy }: { id: string; onCopy: () => void }) {
  const { t } = useT();
  const [totp, setTotp] = useState<TotpCode | null>(null);
  const [error, setError] = useState(false);
  useEffect(() => {
    const tick = () => api.totpCode(id).then(setTotp, () => setError(true));
    tick();
    const timer = setInterval(tick, 1000);
    return () => clearInterval(timer);
  }, [id]);

  const code = totp ? `${totp.code.slice(0, totp.code.length / 2)} ${totp.code.slice(totp.code.length / 2)}` : "··· ···";
  const left = totp ? totp.remaining / totp.period : 1;
  return (
    <div className="detail-item">
      <ShieldCheck size={18} className="muted" />
      <div className="row-main">
        <span className="detail-label">{t("entry.totpLabel")}</span>
        <span className={`detail-value mono totp-code ${totp && totp.remaining <= 5 ? "ending" : ""}`}>{error ? "—" : code}</span>
      </div>
      {totp && (
        <svg className="totp-ring" width="22" height="22" viewBox="0 0 22 22" aria-label={t("entry.totpSeconds", { secs: totp.remaining })}>
          <circle cx="11" cy="11" r="9" />
          <circle cx="11" cy="11" r="9" strokeDasharray={`${2 * Math.PI * 9}`} strokeDashoffset={`${2 * Math.PI * 9 * (1 - left)}`} />
        </svg>
      )}
      <button className="icon-btn" onClick={onCopy} disabled={error} aria-label={t("entry.copyTotpAria")}>
        <Copy size={18} />
      </button>
    </div>
  );
}

/** 2FA recovery codes, hidden like the password; each can be copied and marked as spent. */
function BackupCodes({ entry, onCopy, onMarked }: { entry: Entry; onCopy: (code: string) => void; onMarked: (index: number, used: boolean) => void }) {
  const { t, errorMessage } = useT();
  const toast = useToast();
  const [reveal, setReveal] = useState(false);
  const codes = entry.backup_codes;
  const left = codes.filter((c) => !c.used).length;
  const mark = async (index: number, used: boolean) => {
    try {
      await api.markBackupCode(entry.id, index, used);
      onMarked(index, used);
    } catch (err) {
      toast(errorMessage(err));
    }
  };
  return (
    <>
      <div className="detail-item">
        <ListChecks size={18} className="muted" />
        <div className="row-main">
          <span className="detail-label">{t("entry.backupCodesLabel")}</span>
          <span className="detail-value">{t("entry.backupCodesLeft", { left, total: codes.length })}</span>
        </div>
        <button className="icon-btn" onClick={() => setReveal(!reveal)} aria-label={reveal ? t("common.hide") : t("common.show")}>
          {reveal ? <EyeOff size={18} /> : <Eye size={18} />}
        </button>
      </div>
      {reveal &&
        codes.map((c, i) => (
          <div className={`detail-item backup-code ${c.used ? "used" : ""}`} key={c.code}>
            <button
              className={`code-check ${c.used ? "on" : ""}`}
              onClick={() => mark(i, !c.used)}
              aria-pressed={c.used}
              aria-label={c.used ? t("entry.backupCodeUnmark") : t("entry.backupCodeMark")}
              title={c.used ? t("entry.backupCodeUnmark") : t("entry.backupCodeMark")}
            >
              {c.used && <Check size={14} />}
            </button>
            <span className="detail-value mono row-main">{c.code}</span>
            {!c.used && (
              <button className="icon-btn" onClick={() => onCopy(c.code)} aria-label={t("entry.backupCodeCopy")}>
                <Copy size={18} />
              </button>
            )}
          </div>
        ))}
    </>
  );
}

function Item({ icon, label, value, onCopy }: { icon: ReactElement; label: string; value: string; onCopy?: () => void }) {
  const { t } = useT();
  return (
    <div className="detail-item">
      <span className="muted">{icon}</span>
      <div className="row-main">
        <span className="detail-label">{label}</span>
        <span className="detail-value">{value}</span>
      </div>
      {onCopy && (
        <button className="icon-btn" onClick={onCopy} aria-label={t("entry.copyFieldAria", { label })}>
          <Copy size={18} />
        </button>
      )}
    </div>
  );
}
