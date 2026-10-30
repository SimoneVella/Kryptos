import type { ReactElement, ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import { Icon } from "../components/Icon";
import { api, type Entry, type EntryInput, type TotpCode } from "../lib/api";
import { Action, Avatar, Sheet, useToast } from "../components/ui";
import { hostOf, isMac, isMobile } from "../lib/utils";
import { useT } from "../i18n";

/**
 * One entry. On phones and narrow windows it opens as a sheet over the list; on
 * wide windows (`inline`) it fills the detail pane next to the list.
 */
export default function EntrySheet(props: {
  id: string;
  inline?: boolean;
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
    // The pane stays mounted while another entry is picked: start each one hidden.
    setReveal(false);
    setConfirmDelete(false);
    api.getEntry(props.id).then(setEntry).catch(props.onClose);
  }, [props.id]);

  // Desktop shortcuts for the open entry, as in other password managers:
  // ⌘C username, ⇧⌘C password, ⌥⌘C 2FA code (Ctrl on Windows and Linux).
  // They stay out of the way of normal copying: not while typing in a field or
  // with text selected.
  const shortcut = useRef<(field: "username" | "password" | "totp") => void>(() => {});
  useEffect(() => {
    if (isMobile()) return;
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.code !== "KeyC") return;
      const el = document.activeElement;
      if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || window.getSelection()?.toString()) return;
      e.preventDefault();
      shortcut.current(e.altKey ? "totp" : e.shiftKey ? "password" : "username");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!entry || entry.id !== props.id) return null;
  const Frame = props.inline ? Pane : Sheet;

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
  shortcut.current = (field) => {
    if (field === "totp" && !entry.totp) return;
    if (field === "username" && !entry.username) return;
    copy(field);
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
      ai_access: entry.ai_access,
    });

  return (
    <Frame onClose={props.onClose}>
      <div className="sheet-hero">
        <Avatar name={entry.title} urls={entry.urls} size="lg" />
        <div>
          <h2 style={{ display: "flex", alignItems: "center", gap: 8, justifyContent: "center" }}>
            {entry.title}
            {entry.favorite && <Icon name="star" size={18} className="star" fill />}
          </h2>
          {entry.urls[0] && <p className="muted" style={{ marginTop: 4 }}>{hostOf(entry.urls[0])}</p>}
        </div>
      </div>

      <div className="sheet-actions">
        <Action icon={<Icon name="person" size={20} />} label={t("entry.actionUser")} onClick={() => copy("username")} />
        <Action icon={<Icon name="key" size={20} />} label={t("entry.actionPassword")} onClick={() => copy("password")} />
        <Action icon={<Icon name="edit" size={19} />} label={t("common.edit")} onClick={edit} />
      </div>

      <div className="detail-list">
        {entry.username && (
          <Item icon={<Icon name="person" size={18} />} label={t("entry.usernameLabel")} value={entry.username} onCopy={() => copy("username")} />
        )}
        <div className="detail-item">
          <Icon name="key" size={18} className="muted" />
          <div className="row-main">
            <span className="detail-label">{t("entry.passwordLabel")}</span>
            <span className="detail-value mono pw-value">{reveal ? <Chars text={entry.password} /> : "•".repeat(Math.min(entry.password.length, 16)) || "—"}</span>
          </div>
          <button className="icon-btn" onClick={() => setReveal(!reveal)} aria-label={reveal ? t("common.hide") : t("common.show")}>
            {reveal ? <Icon name="visibility_off" size={18} /> : <Icon name="visibility" size={18} />}
          </button>
          <button className="icon-btn" onClick={() => copy("password")} aria-label={t("entry.copyPasswordAria")}>
            <Icon name="content_copy" size={18} />
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
        {entry.history.length > 0 && <History entry={entry} clipboardClearSecs={props.clipboardClearSecs} />}
        {entry.urls.map((u) => (
          <Item key={u} icon={<Icon name="language" size={18} />} label={t("entry.websiteLabel")} value={u} />
        ))}
        {entry.notes && <Item icon={<Icon name="sticky_note_2" size={18} />} label={t("entry.notesLabel")} value={entry.notes} />}
      </div>

      {entry.urls.length > 0 && entry.password && (
        <button className="btn btn-secondary btn-block change-pw" onClick={() => api.openChangePassword(entry.id).catch(() => toast(t("entry.noChangePage")))}>
          <Icon name="open_in_new" size={18} /> {t("entry.changeOnSite")}
        </button>
      )}
      {entry.ai_access && (
        <p className="ai-badge"><Icon name="smart_toy" size={16} /> {t("entry.aiAllowed")}</p>
      )}
      <p className="muted" style={{ fontSize: 12, textAlign: "center", margin: "16px 0" }}>
        {t("entry.modifiedOn", { date: formatDate(entry.updated_at) })}
      </p>
      {!isMobile() && (
        <p className="shortcuts muted">
          <span><kbd className="kbd">{isMac() ? "⌘C" : "Ctrl+C"}</kbd> {t("entry.actionUser")}</span>
          <span><kbd className="kbd">{isMac() ? "⇧⌘C" : "Ctrl+Shift+C"}</kbd> {t("entry.actionPassword")}</span>
          {entry.totp && <span><kbd className="kbd">{isMac() ? "⌥⌘C" : "Ctrl+Alt+C"}</kbd> {t("entry.totpLabel")}</span>}
        </p>
      )}

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
          <Icon name="delete" size={18} /> {t("common.delete")}
        </button>
      )}
    </Frame>
  );
}

/** The detail pane of wide windows: same content as the sheet, no overlay. */
function Pane({ children }: { onClose: () => void; children: ReactNode }) {
  return (
    <section className="pane-detail">
      <div className="pane-inner">{children}</div>
    </section>
  );
}

/** A revealed password with digits and symbols tinted, so look-alikes (0/O, l/1) stand apart. */
function Chars({ text }: { text: string }) {
  return (
    <>
      {[...text].map((c, i) =>
        /\d/.test(c) ? <span key={i} className="ch-d">{c}</span> : /[^\p{L}]/u.test(c) ? <span key={i} className="ch-s">{c}</span> : c,
      )}
    </>
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
  const ending = !!totp && totp.remaining <= 5;
  return (
    <div className="detail-item totp-item">
      <Icon name="verified_user" size={18} className="muted" />
      <div className="row-main">
        <span className="detail-label">{t("entry.totpLabel")}</span>
        <span className={`detail-value mono totp-code ${ending ? "ending" : ""}`}>{error ? "—" : code}</span>
      </div>
      {totp && (
        <span className="mono muted totp-secs" aria-label={t("entry.totpSeconds", { secs: totp.remaining })}>
          {totp.remaining}s
        </span>
      )}
      {totp && <span className={`totp-bar ${ending ? "ending" : ""}`} style={{ width: `${left * 100}%` }} />}
      <button className="icon-btn" onClick={onCopy} disabled={error} aria-label={t("entry.copyTotpAria")}>
        <Icon name="content_copy" size={18} />
      </button>
    </div>
  );
}

/** Earlier passwords, newest first: hidden until asked, each can be copied. */
function History({ entry, clipboardClearSecs }: { entry: Entry; clipboardClearSecs: number }) {
  const { t, formatDate } = useT();
  const toast = useToast();
  const [open, setOpen] = useState(false);
  return (
    <>
      <div className="detail-item">
        <Icon name="history" size={18} className="muted" />
        <div className="row-main">
          <span className="detail-label">{t("entry.historyLabel")}</span>
          <span className="detail-value">{t("entry.historyCount", { n: entry.history.length })}</span>
        </div>
        <button className="icon-btn" onClick={() => setOpen(!open)} aria-label={open ? t("common.hide") : t("common.show")}>
          <Icon name={open ? "visibility_off" : "visibility"} size={18} />
        </button>
      </div>
      {open &&
        entry.history.map((h, i) => (
          <div key={i} className="detail-item backup-code">
            <span className="row-main">
              <span className="detail-value mono pw-value">{h.password}</span>
              <span className="detail-label">{t("entry.historyUntil", { date: formatDate(h.replaced_at) })}</span>
            </span>
            <button
              className="icon-btn"
              aria-label={t("entry.copyPasswordAria")}
              onClick={async () => {
                await api.copyHistory(entry.id, i);
                toast(t("common.copiedClearing", { secs: clipboardClearSecs }));
              }}
            >
              <Icon name="content_copy" size={18} />
            </button>
          </div>
        ))}
    </>
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
        <Icon name="receipt_long" size={18} className="muted" />
        <div className="row-main">
          <span className="detail-label">{t("entry.backupCodesLabel")}</span>
          <span className="detail-value">{t("entry.backupCodesLeft", { left, total: codes.length })}</span>
        </div>
        <button className="icon-btn" onClick={() => setReveal(!reveal)} aria-label={reveal ? t("common.hide") : t("common.show")}>
          {reveal ? <Icon name="visibility_off" size={18} /> : <Icon name="visibility" size={18} />}
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
              {c.used && <Icon name="check" size={14} />}
            </button>
            <span className="detail-value mono row-main">{c.code}</span>
            {!c.used && (
              <button className="icon-btn" onClick={() => onCopy(c.code)} aria-label={t("entry.backupCodeCopy")}>
                <Icon name="content_copy" size={18} />
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
          <Icon name="content_copy" size={18} />
        </button>
      )}
    </div>
  );
}
