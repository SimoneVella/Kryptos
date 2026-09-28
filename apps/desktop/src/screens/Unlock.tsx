import { useState, type FormEvent } from "react";
import { api } from "../lib/api";
import { Logo, PasswordInput } from "../components/ui";

export default function Unlock({ onDone }: { onDone: () => void }) {
  const [pw, setPw] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [shake, setShake] = useState(0);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      await api.unlock(pw);
      setPw("");
      onDone();
    } catch (err) {
      setError(err === "wrong_password" ? "Password errata. Riprova." : String(err));
      setShake((n) => n + 1);
      setBusy(false);
    }
  };

  return (
    <div className="auth">
      <div className="auth-card">
        <Logo large />
        <h1 style={{ marginTop: 14 }}>Bentornato</h1>
        <p className="muted">Inserisci la master password per sbloccare il vault.</p>
        <form onSubmit={submit} key={shake} className={shake ? "shake" : ""}>
          <PasswordInput value={pw} onChange={setPw} placeholder="Master password" autoFocus />
          {error && <p className="error">{error}</p>}
          <button className="btn btn-primary btn-block" disabled={busy || !pw}>
            {busy ? "Sblocco…" : "Sblocca"}
          </button>
        </form>
      </div>
    </div>
  );
}
