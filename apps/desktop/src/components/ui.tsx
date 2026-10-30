import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { strengthOf } from "../lib/utils";
import { Icon } from "./Icon";
import { useBrandIcon } from "../lib/brand";
import { useT } from "../i18n";
import { useBackButton } from "../lib/back";

/* ───────── Avatar: deterministic colour from the title ───────── */
const HUES = [217, 262, 330, 12, 38, 150, 190, 290];
export function Avatar({ name, urls, size = "md" }: { name: string; urls?: string[]; size?: "md" | "lg" }) {
  const brand = useBrandIcon(name, urls);
  if (brand) {
    return (
      <div className={`avatar ${size === "lg" ? "avatar-lg" : ""} ${brand.ring ? "avatar-ring" : ""}`} style={{ background: brand.hex, ["--halo" as string]: brand.ring ? "transparent" : brand.hex }}>
        <svg viewBox="0 0 24 24" width="52%" height="52%" aria-hidden="true">
          <path d={brand.path} fill={brand.glyph} />
        </svg>
      </div>
    );
  }
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  const hue = HUES[h % HUES.length];
  const letter = (name.trim()[0] ?? "?").toUpperCase();
  return (
    <div
      className={`avatar ${size === "lg" ? "avatar-lg" : ""}`}
      style={{ background: `hsl(${hue} 38% 42%)`, ["--halo" as string]: `hsl(${hue} 38% 42%)` }}
    >
      {letter}
    </div>
  );
}

/**
 * The Kryptos mark, a safe's combination dial (docs/assets/brand). Small sizes use
 * the simplified version (bezel and wheel only); `large` draws the full dial, whose
 * graduations and wheel turn slowly, and spin while `spinning` (unlocking).
 */
export function Logo({ large, spinning }: { large?: boolean; spinning?: boolean }) {
  if (!large) {
    return (
      <svg className="logo" viewBox="0 0 1024 1024" role="img" aria-label="Kryptos">
        <rect width="1024" height="1024" rx="230" fill="#0B0C0E" />
        <circle cx="512" cy="512" r="370" fill="none" stroke="#D6A166" strokeWidth="64" />
        <g stroke="#D6A166" strokeWidth="72" strokeLinecap="round">
          <line x1="512" y1="512" x2="512" y2="300" />
          <line x1="512" y1="512" x2="328" y2="618" />
          <line x1="512" y1="512" x2="696" y2="618" />
        </g>
        <circle cx="512" cy="512" r="96" fill="#D6A166" />
        <circle cx="512" cy="512" r="38" fill="#0B0C0E" />
      </svg>
    );
  }
  const graduations = Array.from({ length: 24 }, (_, i) => i * 15).filter((a) => a !== 0);
  return (
    <svg className={`logo logo-lg dial-logo ${spinning ? "spinning" : ""}`} viewBox="0 0 1024 1024" role="img" aria-label="Kryptos">
      <rect width="1024" height="1024" rx="218" fill="#0B0C0E" />
      <circle cx="512" cy="512" r="344" fill="none" stroke="#D6A166" strokeWidth="18" />
      <circle cx="512" cy="512" r="317" fill="none" stroke="#D6A166" strokeWidth="5" opacity="0.92" />
      <g className="dial-graduations" stroke="#D6A166" strokeLinecap="round">
        {graduations.map((a) => (
          <line key={a} x1="512" y1="205" x2="512" y2={a % 45 ? 218 : 225} strokeWidth={a % 45 ? 5 : 8} transform={`rotate(${a} 512 512)`} />
        ))}
      </g>
      <path d="M500 194 L524 194 L512 216 Z" fill="#D6A166" />
      <circle cx="512" cy="512" r="236" fill="#0B0C0E" stroke="#D6A166" strokeWidth="12" />
      <circle cx="512" cy="512" r="207" fill="none" stroke="#D6A166" strokeWidth="3" opacity="0.52" />
      <g className="dial-wheel">
        <g stroke="#D6A166" strokeWidth="28" strokeLinecap="round">
          <line x1="512" y1="512" x2="512" y2="354" />
          <line x1="512" y1="512" x2="375" y2="591" />
          <line x1="512" y1="512" x2="649" y2="591" />
        </g>
        <g fill="#0B0C0E" stroke="#D6A166" strokeWidth="12">
          <circle cx="512" cy="348" r="25" />
          <circle cx="370" cy="594" r="25" />
          <circle cx="654" cy="594" r="25" />
        </g>
        <circle cx="512" cy="512" r="61" fill="#D6A166" />
        <circle cx="512" cy="512" r="28" fill="#0B0C0E" />
        <circle cx="512" cy="512" r="10" fill="#D6A166" />
      </g>
    </svg>
  );
}

/* ───────── Sheet (modal) ───────── */
export function Sheet({ onClose, children, title }: { onClose: () => void; children: ReactNode; title?: string }) {
  const { t } = useT();
  useBackButton(onClose);
  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [onClose]);
  return (
    <div className="scrim" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="sheet" role="dialog" aria-modal="true">
        <div className="sheet-top">
          <h3>{title}</h3>
          <button className="icon-btn" onClick={onClose} aria-label={t("common.close")}>
            <Icon name="close" size={20} />
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}

/* ───────── Round quick action ───────── */
export function Action({ icon, label, onClick }: { icon: ReactNode; label: string; onClick: () => void }) {
  return (
    <button className="action" onClick={onClick}>
      <div className="action-circle">{icon}</div>
      <span>{label}</span>
    </button>
  );
}

/* ───────── Inputs ───────── */
export function Switch({ checked, onChange }: { checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="switch">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span />
    </label>
  );
}

export function PasswordInput(props: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  autoFocus?: boolean;
  mono?: boolean;
  trailing?: ReactNode;
}) {
  const { t } = useT();
  const [show, setShow] = useState(false);
  return (
    <div className="input-wrap">
      <input
        className={`input ${props.mono ? "mono" : ""}`}
        type={show ? "text" : "password"}
        value={props.value}
        placeholder={props.placeholder}
        autoFocus={props.autoFocus}
        autoComplete="off"
        spellCheck={false}
        onChange={(e) => props.onChange(e.target.value)}
        style={props.trailing ? { paddingRight: 96 } : undefined}
      />
      <div className="input-trail">
        {props.trailing}
        <button type="button" className="icon-btn" onClick={() => setShow(!show)} aria-label={show ? t("common.hide") : t("common.show")}>
          {show ? <Icon name="visibility_off" size={18} /> : <Icon name="visibility" size={18} />}
        </button>
      </div>
    </div>
  );
}

export function StrengthBar({ bits }: { bits: number }) {
  const { t } = useT();
  const s = strengthOf(bits);
  return (
    <div className="strength" style={{ padding: 0 }}>
      <div className="strength-bar">
        <div style={{ width: `${s.pct}%`, background: s.color }} />
      </div>
      <div className="strength-meta">
        <span style={{ color: s.color }}>{t(`strength.${s.key}`)}</span>
        <span className="muted">{t("common.bits", { n: Math.round(bits) })}</span>
      </div>
    </div>
  );
}

/* ───────── Toast ───────── */
const ToastCtx = createContext<(msg: string) => void>(() => {});
export const useToast = () => useContext(ToastCtx);

export function ToastProvider({ children }: { children: ReactNode }) {
  const [msg, setMsg] = useState<string | null>(null);
  const timer = useRef<number>(undefined);
  const show = useCallback((m: string) => {
    setMsg(m);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setMsg(null), 2600);
  }, []);
  return (
    <ToastCtx.Provider value={show}>
      {children}
      {msg && (
        <div className="toast" role="status" key={msg + Date.now()}>
          <div className="toast-dot">
            <Icon name="check" size={14} />
          </div>
          {msg}
        </div>
      )}
    </ToastCtx.Provider>
  );
}
