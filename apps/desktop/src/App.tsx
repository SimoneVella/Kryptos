import { useEffect, useState } from "react";
import { api, type Status } from "./lib/api";
import Setup from "./screens/Setup";
import Unlock from "./screens/Unlock";
import Shell from "./screens/Shell";
import { ToastProvider } from "./components/ui";
import { setPlatform } from "./lib/utils";

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
      window.removeEventListener("pointerdown", activity);
      window.removeEventListener("keydown", activity);
    };
  }, []);

  if (!status) return null;
  return (
    <ToastProvider>
      {!status.exists ? (
        <Setup onDone={refresh} />
      ) : !status.unlocked ? (
        <Unlock onDone={refresh} />
      ) : (
        <Shell onLock={() => api.lock().then(refresh)} />
      )}
    </ToastProvider>
  );
}
