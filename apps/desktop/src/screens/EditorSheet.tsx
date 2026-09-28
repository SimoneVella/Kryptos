import { useState, type FormEvent } from "react";
import { RefreshCw } from "lucide-react";
import { api, defaultGenerator, type EntryInput } from "../lib/api";
import { estimateBits } from "../lib/utils";
import { PasswordInput, Sheet, StrengthBar, Switch, useToast } from "../components/ui";
import { useT } from "../i18n";
import type { Editing } from "./Shell";

export default function EditorSheet(props: { editing: Editing; onClose: () => void; onSaved: (id: string) => void }) {
  const { t, errorMessage } = useT();
  const [v, setV] = useState<EntryInput>(props.editing.input);
  const [urls, setUrls] = useState(props.editing.input.urls.join(", "));
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const toast = useToast();
  const set = <K extends keyof EntryInput>(k: K, val: EntryInput[K]) => setV({ ...v, [k]: val });
  const isNew = props.editing.id === null;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    const input = { ...v, urls: urls.split(",").map((s) => s.trim()).filter(Boolean) };
    try {
      const id = isNew ? await api.addEntry(input) : (await api.updateEntry(props.editing.id!, input), props.editing.id!);
      toast(isNew ? t("editor.savedNewToast") : t("editor.savedEditToast"));
      props.onSaved(id);
    } catch (err) {
      setError(errorMessage(err));
      setBusy(false);
    }
  };

  return (
    <Sheet onClose={props.onClose} title={isNew ? t("editor.titleNew") : t("common.edit")}>
      <form className="form" onSubmit={submit} style={{ marginTop: 12 }}>
        <div>
          <label className="label">{t("editor.nameLabel")}</label>
          <input className="input" placeholder={t("editor.namePlaceholder")} value={v.title} onChange={(e) => set("title", e.target.value)} autoFocus />
        </div>
        <div>
          <label className="label">{t("entry.websiteLabel")}</label>
          <input className="input" placeholder={t("editor.websitePlaceholder")} value={urls} onChange={(e) => setUrls(e.target.value)} spellCheck={false} />
        </div>
        <div>
          <label className="label">{t("editor.usernameLabel")}</label>
          <input className="input" value={v.username} onChange={(e) => set("username", e.target.value)} spellCheck={false} autoComplete="off" />
        </div>
        <div>
          <label className="label">{t("entry.passwordLabel")}</label>
          <PasswordInput
            value={v.password}
            onChange={(p) => set("password", p)}
            mono
            trailing={
              <button
                type="button"
                className="icon-btn"
                title={t("common.generate")}
                onClick={async () => set("password", (await api.generatePassword(defaultGenerator)).password)}
              >
                <RefreshCw size={18} />
              </button>
            }
          />
          {v.password && (
            <div style={{ marginTop: 10 }}>
              <StrengthBar bits={estimateBits(v.password)} />
            </div>
          )}
        </div>
        <div>
          <label className="label">{t("entry.notesLabel")}</label>
          <textarea className="input" value={v.notes} onChange={(e) => set("notes", e.target.value)} />
        </div>
        <div className="toggle-row" style={{ padding: "4px 4px" }}>
          <span>{t("editor.favoriteToggle")}</span>
          <Switch checked={v.favorite} onChange={(b) => set("favorite", b)} />
        </div>
        {error && <p className="error">{error}</p>}
        <div className="form-actions">
          <button type="button" className="btn btn-secondary" onClick={props.onClose}>{t("common.cancel")}</button>
          <button className="btn btn-primary" disabled={busy || !v.title.trim()}>{isNew ? t("editor.saveNew") : t("editor.saveEdit")}</button>
        </div>
      </form>
    </Sheet>
  );
}
