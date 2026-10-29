import { useEffect, useState } from "react";
import { api, type Status } from "./lib/api";
import Setup from "./screens/Setup";
import Unlock from "./screens/Unlock";
import Shell from "./screens/Shell";
import FillConfirm from "./screens/FillConfirm";
import { ToastProvider } from "./components/ui";
import { setPlatform } from "./lib/utils";
import { I18nProvider } from "./i18n";

export default function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const refresh = () =>
    api.status().then((s) => {
      setPlatform(s.platform);
      setStatus(s);
    });

  useEffect(() => {
    refresh();
    const un = api.onLocked(refresh);
    const unUnlocked = api.onUnlocked(refresh);
    // Any interaction postpones auto-lock (throttled).
    let last = 0;
    const activity = () => {
      const now = Date.now();
      if (now - last > 15_000) {
        last = now;
        api.touch();
      }
    };
    window.addEventListener("pointerdown", activity);
    window.addEventListener("keydown", activity);
    return () => {
      un.then((f) => f());
      unUnlocked.then((f) => f());
      window.removeEventListener("pointerdown", activity);
      window.removeEventListener("keydown", activity);
    };
  }, []);

  if (!status) return null;
  return (
    <I18nProvider>
      <ToastProvider>
        {!status.exists ? (
          <Setup onDone={refresh} />
        ) : !status.unlocked ? (
          <Unlock onDone={refresh} />
        ) : (
          <Shell onLock={() => api.lock().then(refresh)} />
        )}
        {/* Browser fills wait for an OK here, whatever screen is open (desktop only). */}
        {status.platform !== "android" && status.platform !== "ios" && <FillConfirm />}
      </ToastProvider>
    </I18nProvider>
  );
}
