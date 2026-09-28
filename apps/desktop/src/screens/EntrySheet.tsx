import type { ReactElement } from "react";
import { useEffect, useState } from "react";
import { Copy, Eye, EyeOff, Globe, KeyRound, Pencil, Star, StickyNote, Trash2, User } from "lucide-react";
import { api, type Entry, type EntryInput } from "../lib/api";
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

  const copy = async (field: "username" | "password") => {
    await api.copyField(entry.id, field);
    toast(field === "password" ? t("common.copiedClearing", { secs: props.clipboardClearSecs }) : t("entry.copyUsernameToast"));
  };
  const edit = () =>
    props.onEdit(entry.id, {
      title: entry.title,
      username: entry.username,
      password: entry.password,
      urls: entry.urls,
      notes: entry.notes,
      favorite: entry.favorite,
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
