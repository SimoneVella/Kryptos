import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { execSync } from "node:child_process";
import pkg from "./package.json";

// Shown in Settings, so you can tell which build is installed: the commit, "+"
// when built with uncommitted changes, and when it was built.
function buildId() {
  const git = (cmd: string) => {
    try {
      return execSync(`git ${cmd}`, { stdio: ["ignore", "pipe", "ignore"] }).toString().trim();
    } catch {
      return "";
    }
  };
  const commit = git("rev-parse --short HEAD") || "dev";
  const dirty = git("status --porcelain") ? "+" : "";
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${commit}${dirty} · ${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export default defineConfig({
  plugins: [react()],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
    __BUILD_ID__: JSON.stringify(buildId()),
  },
  clearScreen: false,
  // `tauri android dev` sets TAURI_DEV_HOST so the emulator/device can reach Vite.
  server: {
    port: 1420,
    strictPort: true,
    host: process.env.TAURI_DEV_HOST || "127.0.0.1",
    hmr: process.env.TAURI_DEV_HOST ? { protocol: "ws", host: process.env.TAURI_DEV_HOST, port: 1421 } : undefined,
  },
  build: { target: "es2022", sourcemap: false },
});
