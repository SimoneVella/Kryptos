import { useState, type FormEvent } from "react";
import { RefreshCw } from "lucide-react";
import { api, defaultGenerator, type EntryInput } from "../lib/api";
import { estimateBits } from "../lib/utils";
import { PasswordInput, Sheet, StrengthBar, Switch, useToast } from "../components/ui";
import type { Editing } from "./Shell";

export default function EditorSheet(props: { editing: Editing; onClose: () => void; onSaved: (id: string) => void }) {
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
      toast(isNew ? "Password salvata" : "Modifiche salvate");
      props.onSaved(id);
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  };

  return (
    <Sheet onClose={props.onClose} title={isNew ? "Nuova password" : "Modifica"}>
      <form className="form" onSubmit={submit} style={{ marginTop: 12 }}>
        <div>
          <label className="label">Nome</label>
          <input className="input" placeholder="es. Google, Netflix, Banca" value={v.title} onChange={(e) => set("title", e.target.value)} autoFocus />
        </div>
        <div>
          <label className="label">Sito web</label>
          <input className="input" placeholder="example.com" value={urls} onChange={(e) => setUrls(e.target.value)} spellCheck={false} />
        </div>
        <div>
          <label className="label">Nome utente o email</label>
          <input className="input" value={v.username} onChange={(e) => set("username", e.target.value)} spellCheck={false} autoComplete="off" />
        </div>
        <div>
          <label className="label">Password</label>
          <PasswordInput
            value={v.password}
            onChange={(p) => set("password", p)}
            mono
            trailing={
              <button
                type="button"
                className="icon-btn"
                title="Genera"
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
          <label className="label">Note</label>
          <textarea className="input" value={v.notes} onChange={(e) => set("notes", e.target.value)} />
        </div>
        <div className="toggle-row" style={{ padding: "4px 4px" }}>
          <span>Aggiungi ai preferiti</span>
          <Switch checked={v.favorite} onChange={(b) => set("favorite", b)} />
        </div>
        {error && <p className="error">{error}</p>}
        <div className="form-actions">
          <button type="button" className="btn btn-secondary" onClick={props.onClose}>Annulla</button>
          <button className="btn btn-primary" disabled={busy || !v.title.trim()}>{isNew ? "Salva" : "Salva modifiche"}</button>
        </div>
      </form>
    </Sheet>
  );
}
