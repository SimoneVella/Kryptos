// Builds the desktop sidecars, kryptos-native-host (browser extension),
// kryptos-mcp (AI assistants) and kryptos (the command line), and places them where Tauri's `externalBin`
// expects them (src-tauri/binaries/<name>-<target-triple>), so they ship inside
// the app bundle, where the app's peer check looks for them.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "../../..");
const release = process.argv.includes("--release");
const triple =
  process.env.TAURI_ENV_TARGET_TRIPLE ||
  execFileSync("rustc", ["-vV"]).toString().match(/host: (\S+)/)[1];

// The browser bridge only exists on desktop.
if (/android|ios/.test(triple)) process.exit(0);

const BINS = ["kryptos-native-host", "kryptos-mcp", "kryptos"];
const args = ["build", "-p", "kryptos-native-host", "-p", "kryptos-mcp", "-p", "kryptos-cli", "--target", triple];
if (release) args.push("--release");
execFileSync("cargo", args, { cwd: root, stdio: "inherit" });

const ext = triple.includes("windows") ? ".exe" : "";
const out = join(root, "apps/desktop/src-tauri/binaries");
mkdirSync(out, { recursive: true });
for (const bin of BINS) {
  copyFileSync(join(root, "target", triple, release ? "release" : "debug", `${bin}${ext}`), join(out, `${bin}-${triple}${ext}`));
}
