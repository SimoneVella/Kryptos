import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { Fingerprint } from "lucide-react";
import { api, type BiometricStatus } from "../lib/api";
import { Logo, PasswordInput } from "../components/ui";
import { isMobile } from "../lib/utils";
import { useT, type TKey } from "../i18n";

/** What to tell the user after a fingerprint attempt; null = nothing to say. */
const BIOMETRIC_ERRORS: Record<string, TKey | null> = {
  cancelled: null,
  use_master: null,
  master_required: "unlock.biometricMasterRequired",
  invalidated: "unlock.biometricInvalidated",
  lockout: "unlock.biometricLockout",
};

export default function Unlock({ onDone }: { onDone: () => void }) {
  const { t, errorMessage } = useT();
  const [pw, setPw] = useState("");
  const [error, setError] = useState("");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [shake, setShake] = useState(0);
  const [bio, setBio] = useState<BiometricStatus | null>(isMobile() ? null : "unavailable");

  const unlockWithFingerprint = useCallback(async () => {
    setError("");
    try {
      await api.biometricUnlock();
      onDone();
    } catch (err) {
      const key = typeof err === "string" && err in BIOMETRIC_ERRORS ? BIOMETRIC_ERRORS[err] : "unlock.biometricFailed";
      if (key) setError(t(key));
      api.biometricStatus().then(setBio);
    }
  }, [onDone, t]);

  // Offer the fingerprint right away, once per lock, as soon as the screen is visible
  // (the vault may lock while Kryptos is in the background).
  const offered = useRef(false);
  useEffect(() => {
    if (!isMobile()) return;
    let status: BiometricStatus | null = null;
    const offer = () => {
      if (status !== "on" || offered.current || document.visibilityState !== "visible") return;
      offered.current = true;
      unlockWithFingerprint();
    };
    api.biometricStatus().then((s) => {
      status = s;
      setBio(s);
      if (s === "master_required") setNote(t("unlock.biometricMasterRequired"));
      offer();
    });
    document.addEventListener("visibilitychange", offer);
    return () => document.removeEventListener("visibilitychange", offer);
  }, [unlockWithFingerprint, t]);

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
          {/* With a fingerprint available the keyboard would only get in the way of the prompt. */}
          <PasswordInput value={pw} onChange={setPw} placeholder={t("unlock.placeholder")} autoFocus={bio !== null && bio !== "on"} />
          {error ? <p className="error">{error}</p> : note && <p className="muted">{note}</p>}
          <button className="btn btn-primary btn-block" disabled={busy || !pw}>
            {busy ? t("unlock.submitBusy") : t("unlock.submit")}
          </button>
          {bio === "on" && (
            <button type="button" className="btn btn-secondary btn-block" onClick={unlockWithFingerprint}>
              <Fingerprint size={20} />
              {t("unlock.biometricButton")}
            </button>
          )}
        </form>
      </div>
    </div>
  );
}
