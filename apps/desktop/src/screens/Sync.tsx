// Moving the vault between devices without any network: merge a backup file,
// or pass the encrypted vault as a loop of QR codes from one screen to the
// other device's camera. See src-tauri/src/sync.rs.
import { useEffect, useRef, useState, type FormEvent } from "react";
import jsQR from "jsqr";
import { Icon } from "../components/Icon";
import { api, type MergeReport, type StagedCopy } from "../lib/api";
import { PasswordInput, Sheet } from "../components/ui";
import { useT } from "../i18n";

/** Asks for the master password of a staged copy and merges it. */
export function MergeSheet({ copy, onClose, onDone }: { copy: StagedCopy; onClose: () => void; onDone: (r: MergeReport) => void }) {
  const { t, errorMessage } = useT();
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const cancel = () => {
    api.syncCancel();
    onClose();
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      onDone(await api.syncMerge(password));
    } catch (err) {
      setError(err === "wrong_password" ? t("sync.wrongPassword") : errorMessage(err));
      setBusy(false);
    }
  };

  return (
    <Sheet onClose={cancel} title={t("sync.mergeTitle")}>
      <form className="form" onSubmit={submit} style={{ marginTop: 12 }}>
        <p className="muted">
          {copy.file ? t("sync.fromFile", { file: copy.file }) : t("sync.fromQr")}{" "}
          {copy.same_vault ? t("sync.sameVaultHint") : t("sync.otherVaultHint")}
        </p>
        <PasswordInput value={password} onChange={setPassword} placeholder={t("sync.passwordPlaceholder")} autoFocus />
        {error && <p className="error">{error}</p>}
        <div className="form-actions">
          <button type="button" className="btn btn-secondary" onClick={cancel}>{t("common.cancel")}</button>
          <button className="btn btn-primary" disabled={busy || !password}>{busy ? t("sync.mergeBusy") : t("sync.merge")}</button>
        </div>
      </form>
    </Sheet>
  );
}

/** Frames per second of the QR loop: slow enough for any phone camera to catch each one. */
const SEND_FPS = 5;

/** Shows this vault (encrypted, as on disk) as a loop of QR codes. */
export function SendQrSheet({ onClose }: { onClose: () => void }) {
  const { t, errorMessage } = useT();
  const [frames, setFrames] = useState<string[] | null>(null);
  const [error, setError] = useState("");
  const [i, setI] = useState(0);
  useWakeLock();

  useEffect(() => {
    api.syncQrFrames().then(
      (r) => setFrames(r.frames.map((svg) => `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`)),
      (e) => setError(errorMessage(e)),
    );
  }, [errorMessage]);

  useEffect(() => {
    if (!frames?.length) return;
    const id = setInterval(() => setI((n) => (n + 1) % frames.length), 1000 / SEND_FPS);
    return () => clearInterval(id);
  }, [frames]);

  return (
    <Sheet onClose={onClose} title={t("sync.sendTitle")}>
      <div className="qr-send">
        {frames?.length ? <img className="qr-frame" src={frames[i]} alt="" /> : <div className="qr-frame" />}
        {frames && <p className="muted">{t("sync.frameOf", { n: i + 1, total: frames.length })}</p>}
        {error ? <p className="error">{error}</p> : <p className="muted qr-hint">{t("sync.sendHint")}</p>}
      </div>
    </Sheet>
  );
}

/** Keeps the screen on while a transfer runs (best effort: not every WebView has it). */
function useWakeLock() {
  useEffect(() => {
    let lock: { release: () => Promise<void> } | null = null;
    const nav = navigator as Navigator & { wakeLock?: { request: (t: "screen") => Promise<typeof lock> } };
    nav.wakeLock?.request("screen").then((l) => (lock = l), () => {});
    return () => void lock?.release().catch(() => {});
  }, []);
}

type Frame = { session: string; index: number; count: number; data: string };

/** `KRY1|<session>|<index>|<count>|<base64>` as written by sync_qr_frames. */
function parseFrame(text: string): Frame | null {
  const p = text.split("|");
  if (p.length !== 5 || p[0] !== "KRY1") return null;
  const [index, count] = [Number(p[2]), Number(p[3])];
  if (!Number.isInteger(index) || !Number.isInteger(count) || index < 0 || index >= count || count > 10_000) return null;
  return { session: p[1], index, count, data: p[4] };
}

/** Joins the chunks (each base64 on its own) into one base64 string. */
function joinChunks(chunks: string[]): string {
  const bytes = chunks.map((c) => Uint8Array.from(atob(c), (ch) => ch.charCodeAt(0)));
  const all = new Uint8Array(bytes.reduce((n, b) => n + b.length, 0));
  let at = 0;
  for (const b of bytes) {
    all.set(b, at);
    at += b.length;
  }
  let bin = "";
  for (const b of all) bin += String.fromCharCode(b);
  return btoa(bin);
}

/** Reads the QR loop shown by another device with the camera. */
export function ScanQrSheet({ onClose, onReceived }: { onClose: () => void; onReceived: (base64: string) => void }) {
  const { t } = useT();
  const video = useRef<HTMLVideoElement>(null);
  const [progress, setProgress] = useState<{ got: number; total: number } | null>(null);
  const [error, setError] = useState("");
  useWakeLock();

  useEffect(() => {
    let stream: MediaStream | null = null;
    let raf = 0;
    let stopped = false;
    const canvas = document.createElement("canvas");
    const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
    let session = "";
    let chunks: string[] = [];

    const tick = () => {
      if (stopped) return;
      const v = video.current;
      if (v && v.readyState >= v.HAVE_CURRENT_DATA && v.videoWidth) {
        // Decode at most 720 px wide: plenty for a QR filling part of the frame, and fast.
        const scale = Math.min(1, 720 / v.videoWidth);
        canvas.width = Math.round(v.videoWidth * scale);
        canvas.height = Math.round(v.videoHeight * scale);
        ctx.drawImage(v, 0, 0, canvas.width, canvas.height);
        const img = ctx.getImageData(0, 0, canvas.width, canvas.height);
        const f = jsQR(img.data, img.width, img.height, { inversionAttempts: "dontInvert" });
        const frame = f && parseFrame(f.data);
        if (frame) {
          if (frame.session !== session || chunks.length !== frame.count) {
            // A new transfer (or the sender reopened its screen): start over.
            session = frame.session;
            chunks = new Array(frame.count);
          }
          chunks[frame.index] ??= frame.data;
          const got = chunks.filter(Boolean).length;
          setProgress({ got, total: frame.count });
          if (got === frame.count) {
            stopped = true;
            onReceived(joinChunks(chunks));
            return;
          }
        }
      }
      raf = requestAnimationFrame(tick);
    };

    navigator.mediaDevices
      ?.getUserMedia({ video: { facingMode: "environment", width: { ideal: 1280 } }, audio: false })
      .then((s) => {
        if (stopped) return s.getTracks().forEach((tr) => tr.stop());
        stream = s;
        if (video.current) {
          video.current.srcObject = s;
          video.current.play().catch(() => {});
        }
        raf = requestAnimationFrame(tick);
      })
      .catch((e) => setError(e?.name === "NotFoundError" ? t("sync.noCamera") : t("sync.cameraDenied")));
    if (!navigator.mediaDevices) setError(t("sync.noCamera"));

    return () => {
      stopped = true;
      cancelAnimationFrame(raf);
      stream?.getTracks().forEach((tr) => tr.stop());
    };
  }, [onReceived, t]);

  return (
    <Sheet onClose={onClose} title={t("sync.scanTitle")}>
      <div className="qr-send">
        <video ref={video} className="qr-camera" muted playsInline />
        {progress && (
          <div className="strength-bar qr-progress">
            <div style={{ width: `${(100 * progress.got) / progress.total}%`, background: "var(--accent)" }} />
          </div>
        )}
        {error ? (
          <p className="error">{error}</p>
        ) : (
          <p className="muted qr-hint">{progress ? t("sync.frameOf", { n: progress.got, total: progress.total }) : t("sync.scanHint")}</p>
        )}
      </div>
    </Sheet>
  );
}

/** Decodes a 2FA QR code (`otpauth://...`) from an image or pasted screenshot. */
async function decodeQrImage(blob: Blob): Promise<{ uri?: string; wrong?: boolean }> {
  const url = URL.createObjectURL(blob);
  try {
    const img = await new Promise<HTMLImageElement>((resolve, reject) => {
      const el = new Image();
      el.onload = () => resolve(el);
      el.onerror = () => reject(new Error("image_load_failed"));
      el.src = url;
    });
    const max = Math.max(img.naturalWidth, img.naturalHeight);
    if (!max) return { wrong: true };
    const scale = Math.min(1, 1200 / max);
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(img.naturalWidth * scale));
    canvas.height = Math.max(1, Math.round(img.naturalHeight * scale));
    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    if (!ctx) return { wrong: true };
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
    const data = ctx.getImageData(0, 0, canvas.width, canvas.height);
    const code = jsQR(data.data, data.width, data.height, { inversionAttempts: "attemptBoth" });
    if (code?.data.toLowerCase().startsWith("otpauth://")) {
      return { uri: code.data };
    }
    return { wrong: true };
  } catch {
    return { wrong: true };
  } finally {
    URL.revokeObjectURL(url);
  }
}

/**
 * Reads a site's 2FA QR code with the camera, from a chosen/pasted screenshot
 * image, or (when `manual` is set) from a pasted secret / `otpauth://` URI.
 * Everything runs locally; nothing leaves the device.
 */
export function ScanOtpSheet({
  onClose,
  onFound,
  onSave,
  manual,
}: {
  onClose: () => void;
  onFound?: (uri: string) => void;
  onSave?: (secret: string) => Promise<void>;
  manual?: boolean;
}) {
  const { t, errorMessage } = useT();
  const video = useRef<HTMLVideoElement>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const [cameraError, setCameraError] = useState("");
  const [saveError, setSaveError] = useState("");
  const [wrong, setWrong] = useState(false);
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState(false);
  const onFoundRef = useRef(onFound);
  const onSaveRef = useRef(onSave);
  onFoundRef.current = onFound;
  onSaveRef.current = onSave;
  useWakeLock();

  const deliver = async (val: string) => {
    const trimmed = val.trim();
    if (!trimmed) return;
    if (onSaveRef.current) {
      setBusy(true);
      setSaveError("");
      try {
        await onSaveRef.current(trimmed);
      } catch (err) {
        setSaveError(errorMessage(err));
        setBusy(false);
      }
    } else {
      onFoundRef.current?.(trimmed);
    }
  };
  const deliverRef = useRef(deliver);
  deliverRef.current = deliver;

  const handleBlob = async (blob: Blob) => {
    setWrong(false);
    setSaveError("");
    const res = await decodeQrImage(blob);
    if (res.uri) {
      await deliverRef.current(res.uri);
    } else if (res.wrong) {
      setWrong(true);
    }
  };
  const handleBlobRef = useRef(handleBlob);
  handleBlobRef.current = handleBlob;

  useEffect(() => {
    const onPaste = (e: ClipboardEvent) => {
      const items = e.clipboardData?.items;
      if (!items) return;
      for (const item of items) {
        if (item.type.startsWith("image/")) {
          const file = item.getAsFile();
          if (file) {
            e.preventDefault();
            void handleBlobRef.current(file);
            return;
          }
        }
      }
    };
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
  }, []);

  useEffect(() => {
    let stream: MediaStream | null = null;
    let raf = 0;
    let stopped = false;
    const canvas = document.createElement("canvas");
    const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
    const tick = () => {
      if (stopped) return;
      const v = video.current;
      if (v && v.readyState >= v.HAVE_CURRENT_DATA && v.videoWidth) {
        const scale = Math.min(1, 720 / v.videoWidth);
        canvas.width = Math.round(v.videoWidth * scale);
        canvas.height = Math.round(v.videoHeight * scale);
        ctx.drawImage(v, 0, 0, canvas.width, canvas.height);
        const img = ctx.getImageData(0, 0, canvas.width, canvas.height);
        const code = jsQR(img.data, img.width, img.height, { inversionAttempts: "attemptBoth" });
        if (code?.data.toLowerCase().startsWith("otpauth://")) {
          stopped = true;
          void deliverRef.current(code.data);
          return;
        }
        // A QR that isn't a 2FA one (a menu, a link): say so instead of staying silent.
        if (code) setWrong(true);
      }
      raf = requestAnimationFrame(tick);
    };
    navigator.mediaDevices
      ?.getUserMedia({ video: { facingMode: "environment", width: { ideal: 1280 } }, audio: false })
      .then((s) => {
        if (stopped) return s.getTracks().forEach((tr) => tr.stop());
        stream = s;
        if (video.current) {
          video.current.srcObject = s;
          video.current.play().catch(() => {});
        }
        raf = requestAnimationFrame(tick);
      })
      .catch((e) => setCameraError(e?.name === "NotFoundError" ? t("sync.noCamera") : t("sync.cameraDenied")));
    if (!navigator.mediaDevices) setCameraError(t("sync.noCamera"));
    return () => {
      stopped = true;
      cancelAnimationFrame(raf);
      stream?.getTracks().forEach((tr) => tr.stop());
    };
  }, [t]);

  const submitManual = async (e: FormEvent) => {
    e.preventDefault();
    await deliver(secret);
  };

  return (
    <Sheet onClose={onClose} title={t("otpScan.title")}>
      <div className="qr-send">
        {!cameraError && <video ref={video} className="qr-camera" muted playsInline />}
        {cameraError ? (
          <p className="muted qr-hint">{cameraError}</p>
        ) : (
          <p className="muted qr-hint">{wrong ? t("otpScan.notOtp") : t("otpScan.hint")}</p>
        )}
        {cameraError && wrong && <p className="error">{t("otpScan.notOtp")}</p>}
        <input
          ref={fileInput}
          type="file"
          accept="image/*"
          hidden
          onChange={(e) => {
            const file = e.target.files?.[0];
            e.target.value = "";
            if (file) void handleBlob(file);
          }}
        />
        <button type="button" className="btn btn-secondary" disabled={busy} onClick={() => fileInput.current?.click()}>
          <Icon name="description" size={16} /> {t("otpScan.fromImage")}
        </button>
        <p className="muted qr-hint" style={{ fontSize: 12 }}>{t("otpScan.pasteHint")}</p>
        {manual && (
          <form className="form" style={{ width: "100%", marginTop: 4 }} onSubmit={submitManual}>
            <div style={{ display: "flex", gap: 8 }}>
              <input
                className="input mono"
                style={{ flex: 1, minWidth: 0 }}
                placeholder={t("otpScan.manualPlaceholder")}
                value={secret}
                onChange={(e) => setSecret(e.target.value)}
                spellCheck={false}
                autoComplete="off"
                autoCapitalize="off"
              />
              <button className="btn btn-primary" disabled={busy || !secret.trim()}>{t("common.save")}</button>
            </div>
          </form>
        )}
        {saveError && <p className="error">{saveError}</p>}
      </div>
    </Sheet>
  );
}
