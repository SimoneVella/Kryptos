import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { strengthOf } from "../lib/utils";
import { Check, Eye, EyeOff, X } from "lucide-react";
import logoUrl from "../assets/logo.png";

/* ───────── Avatar: deterministic colour from the title ───────── */
const HUES = [217, 262, 330, 12, 38, 150, 190, 290];
export function Avatar({ name, size = "md" }: { name: string; size?: "md" | "lg" }) {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  const hue = HUES[h % HUES.length];
  const letter = (name.trim()[0] ?? "?").toUpperCase();
  return (
    <div
      className={`avatar ${size === "lg" ? "avatar-lg" : ""}`}
      style={{ background: `linear-gradient(135deg, hsl(${hue} 85% 62%), hsl(${(hue + 30) % 360} 80% 50%))` }}
    >
      {letter}
    </div>
  );
}

export function Logo({ large }: { large?: boolean }) {
  return <img className={`logo ${large ? "logo-lg" : ""}`} src={logoUrl} alt="Kryptos" draggable={false} />;
}

/* ───────── Sheet (modal) ───────── */
export function Sheet({ onClose, children, title }: { onClose: () => void; children: ReactNode; title?: string }) {
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
          <button className="icon-btn" onClick={onClose} aria-label="Chiudi">
            <X size={20} />
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
        <button type="button" className="icon-btn" onClick={() => setShow(!show)} aria-label={show ? "Nascondi" : "Mostra"}>
          {show ? <EyeOff size={18} /> : <Eye size={18} />}
        </button>
      </div>
    </div>
  );
}

export function StrengthBar({ bits }: { bits: number }) {
  const s = strengthOf(bits);
  return (
    <div className="strength" style={{ padding: 0 }}>
      <div className="strength-bar">
        <div style={{ width: `${s.pct}%`, background: s.color }} />
      </div>
      <div className="strength-meta">
        <span style={{ color: s.color }}>{s.label}</span>
        <span className="muted">{Math.round(bits)} bit</span>
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
            <Check size={14} strokeWidth={3} />
          </div>
          {msg}
        </div>
      )}
    </ToastCtx.Provider>
  );
}
