import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { Fingerprint } from "lucide-react";
import { api, type FillRequest } from "../lib/api";
import { Avatar, PasswordInput, Sheet } from "../components/ui";
import { useT } from "../i18n";

/**
 * Desktop: the browser extension asked for a password and the app waits for the
 * user's OK before releasing it. Shown over whatever screen is open. In "system"
 * mode the Touch ID / Mac password prompt starts right away; the master password
 * is always available as an alternative.
 */
export default function FillConfirm() {
  const [req, setReq] = useState<FillRequest | null>(null);

  useEffect(() => {
    const un = api.onFillConfirm(setReq);
    const unDone = api.onFillConfirmDone((id) => setReq((r) => (r?.id === id ? null : r)));
    return () => {
      un.then((f) => f());
      unDone.then((f) => f());
    };
  }, []);

  // Keyed by request, so each one starts from a clean form.
  return req && <Dialog key={req.id} req={req} onClose={() => setReq(null)} />;
}

function Dialog({ req, onClose }: { req: FillRequest; onClose: () => void }) {
  const { t, errorMessage } = useT();
  const [mode, setMode] = useState(req.mode);
  const [pw, setPw] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [shake, setShake] = useState(0);

  const cancel = useCallback(() => {
    api.fillConfirmCancel(req.id);
    onClose();
  }, [req.id, onClose]);

  const askSystem = useCallback(async () => {
    setBusy(true);
    setError("");
    try {
      await api.fillConfirmSystem(req.id, t("fill.reason", { host: req.host }));
      onClose();
    } catch (err) {
      if (err === "timeout") onClose();
      else if (err !== "cancelled") setError(t("fill.systemFailed"));
    } finally {
      setBusy(false);
    }
  }, [req.id, req.host, onClose, t]);

  const asked = useRef(false);
  useEffect(() => {
    if (req.mode !== "system" || asked.current) return;
    asked.current = true;
    askSystem();
  }, [req.mode, askSystem]);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      await api.fillConfirmMaster(req.id, pw);
      onClose();
    } catch (err) {
      // "timeout": the request expired while the password was being checked.
      if (err === "timeout") return onClose();
      setError(err === "wrong_password" ? t("unlock.wrongPassword") : errorMessage(err));
      setShake((n) => n + 1);
      setBusy(false);
    }
  };

  return (
    <Sheet onClose={cancel}>
      <div className="sheet-hero">
        <Avatar name={req.title} urls={[req.host]} size="lg" />
        <div>
          <h2>{t("fill.title")}</h2>
          <p className="muted" style={{ marginTop: 6 }}>
            {t("fill.subtitle", { title: req.title, host: req.host })}
            {req.username && (
              <>
                <br />
                {req.username}
              </>
            )}
          </p>
        </div>
      </div>
      {mode === "system" ? (
        <div className="form">
          {error && <p className="error">{error}</p>}
          <button className="btn btn-primary btn-block" disabled={busy} onClick={askSystem}>
            <Fingerprint size={20} />
            {busy ? t("fill.waiting") : t("fill.confirmSystem")}
          </button>
          <button className="btn btn-secondary btn-block" disabled={busy} onClick={() => (setError(""), setMode("master"))}>
            {t("fill.useMaster")}
          </button>
        </div>
      ) : (
        <form className={shake ? "form shake" : "form"} onSubmit={submit} key={shake}>
          <PasswordInput value={pw} onChange={setPw} placeholder={t("unlock.placeholder")} autoFocus />
          {error && <p className="error">{error}</p>}
          <div className="form-actions">
            <button type="button" className="btn btn-secondary" onClick={cancel}>{t("common.cancel")}</button>
            <button className="btn btn-primary" disabled={busy || !pw}>{busy ? t("fill.checking") : t("fill.submit")}</button>
          </div>
        </form>
      )}
    </Sheet>
  );
}
