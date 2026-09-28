import { useState, type FormEvent } from "react";
import { api } from "../lib/api";
import { Logo, PasswordInput, StrengthBar } from "../components/ui";
import { estimateBits } from "../lib/utils";

export default function Setup({ onDone }: { onDone: () => void }) {
  const [pw, setPw] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (pw.length < 8) return setError("Usa almeno 8 caratteri.");
    if (pw !== confirm) return setError("Le password non coincidono.");
    setBusy(true);
    try {
      await api.createVault(pw);
      onDone();
    } catch (err) {
      setError(String(err));
      setBusy(false);
    }
  };

  return (
    <div className="auth">
      <div className="auth-card">
        <Logo large />
        <h1 style={{ marginTop: 14 }}>Crea il tuo vault</h1>
        <p className="muted">
          Scegli una master password. Resta solo nella tua testa: non viene mai salvata né inviata, e nessuno può recuperarla.
        </p>
        <form onSubmit={submit}>
          <PasswordInput value={pw} onChange={setPw} placeholder="Master password" autoFocus />
          {pw && <StrengthBar bits={estimateBits(pw)} />}
          <PasswordInput value={confirm} onChange={setConfirm} placeholder="Ripeti la password" />
          {error && <p className="error">{error}</p>}
          <button className="btn btn-primary btn-block" disabled={busy || !pw || !confirm} style={{ marginTop: 8 }}>
            {busy ? "Protezione in corso…" : "Crea vault"}
          </button>
        </form>
      </div>
    </div>
  );
}
