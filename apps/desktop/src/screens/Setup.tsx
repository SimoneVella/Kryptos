import { useState, type FormEvent } from "react";
import { api } from "../lib/api";
import { Logo, PasswordInput, StrengthBar } from "../components/ui";
import { estimateBits } from "../lib/utils";
import { useT } from "../i18n";

export default function Setup({ onDone }: { onDone: () => void }) {
  const { t, errorMessage } = useT();
  const [pw, setPw] = useState("");
  const [confirm, setConfirm] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (pw.length < 8) return setError(t("common.errorTooShort"));
    if (pw !== confirm) return setError(t("setup.errorMismatch"));
    setBusy(true);
    try {
      await api.createVault(pw);
      onDone();
    } catch (err) {
      setError(errorMessage(err));
      setBusy(false);
    }
  };

  return (
    <div className="auth">
      <div className="auth-card">
        <Logo large />
        <h1 style={{ marginTop: 14 }}>{t("setup.title")}</h1>
        <p className="muted">{t("setup.subtitle")}</p>
        <form onSubmit={submit}>
          <PasswordInput value={pw} onChange={setPw} placeholder={t("setup.placeholderMaster")} autoFocus />
          {pw && <StrengthBar bits={estimateBits(pw)} />}
          <PasswordInput value={confirm} onChange={setConfirm} placeholder={t("setup.placeholderConfirm")} />
          {error && <p className="error">{error}</p>}
          <button className="btn btn-primary btn-block" disabled={busy || !pw || !confirm} style={{ marginTop: 8 }}>
            {busy ? t("setup.submitBusy") : t("setup.submit")}
          </button>
        </form>
      </div>
    </div>
  );
}
