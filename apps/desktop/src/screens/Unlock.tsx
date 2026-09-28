import { useState, type FormEvent } from "react";
import { api } from "../lib/api";
import { Logo, PasswordInput } from "../components/ui";
import { useT } from "../i18n";

export default function Unlock({ onDone }: { onDone: () => void }) {
  const { t, errorMessage } = useT();
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
      setError(err === "wrong_password" ? t("unlock.wrongPassword") : errorMessage(err));
      setShake((n) => n + 1);
      setBusy(false);
    }
  };

  return (
    <div className="auth">
      <div className="auth-card">
        <Logo large />
        <h1 style={{ marginTop: 14 }}>{t("unlock.title")}</h1>
        <p className="muted">{t("unlock.subtitle")}</p>
        <form onSubmit={submit} key={shake} className={shake ? "shake" : ""}>
          <PasswordInput value={pw} onChange={setPw} placeholder={t("unlock.placeholder")} autoFocus />
          {error && <p className="error">{error}</p>}
          <button className="btn btn-primary btn-block" disabled={busy || !pw}>
            {busy ? t("unlock.submitBusy") : t("unlock.submit")}
          </button>
        </form>
      </div>
    </div>
  );
}
