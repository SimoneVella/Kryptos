import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
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
