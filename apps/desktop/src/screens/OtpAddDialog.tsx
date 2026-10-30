import { useEffect, useState } from "react";
import { Icon } from "../components/Icon";
import { api, type OtpAddRequest } from "../lib/api";
import { Sheet } from "../components/ui";
import { useT } from "../i18n";

/**
 * Desktop: the browser extension read a 2FA QR code on a page. The user picks
 * the entry that gets it (or a new one); closing the sheet stores nothing.
 */
export default function OtpAddDialog() {
  const [req, setReq] = useState<OtpAddRequest | null>(null);
  useEffect(() => {
    const un = api.onOtpAdd(setReq);
    const unDone = api.onOtpAddDone((id) => setReq((r) => (r?.id === id ? null : r)));
    return () => {
      un.then((f) => f());
      unDone.then((f) => f());
    };
  }, []);
  return req && <Dialog key={req.id} req={req} onClose={() => setReq(null)} />;
}

function Dialog({ req, onClose }: { req: OtpAddRequest; onClose: () => void }) {
  const { t } = useT();
  const [choice, setChoice] = useState(req.candidates[0]?.id ?? "new");
  const answer = (c: string | null) => {
    api.otpAddAnswer(req.id, c);
    onClose();
  };
  const label = [req.issuer, req.account].filter(Boolean).join(" · ") || req.host;
  return (
    <Sheet onClose={() => answer(null)} title={t("otpAdd.title")}>
      <p className="muted" style={{ margin: "6px 0 16px" }}>{t("otpAdd.subtitle", { label, host: req.host })}</p>
      <div className="choice-list">
        {req.candidates.map((c) => (
          <label key={c.id} className={`choice ${choice === c.id ? "on" : ""}`}>
            <input type="radio" name="otp-entry" checked={choice === c.id} onChange={() => setChoice(c.id)} />
            <Icon name="key" size={18} />
            <span className="row-main">
              <span className="row-title">{c.title}</span>
              <span className="row-sub">{c.username || "—"}</span>
            </span>
          </label>
        ))}
        <label className={`choice ${choice === "new" ? "on" : ""}`}>
          <input type="radio" name="otp-entry" checked={choice === "new"} onChange={() => setChoice("new")} />
          <Icon name="add" size={18} />
          <span className="row-main">
            <span className="row-title">{t("otpAdd.newEntry")}</span>
            <span className="row-sub">{req.issuer || req.host}</span>
          </span>
        </label>
      </div>
      <div className="form-actions" style={{ marginTop: 18 }}>
        <button className="btn btn-secondary" onClick={() => answer(null)}>{t("common.cancel")}</button>
        <button className="btn btn-primary" onClick={() => answer(choice)}>{t("otpAdd.confirm")}</button>
      </div>
    </Sheet>
  );
}
