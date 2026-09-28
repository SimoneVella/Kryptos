// Builds kryptos-native-host and places it where Tauri's `externalBin` expects it
// (src-tauri/binaries/<name>-<target-triple>), so it ships inside the app bundle.
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

const args = ["build", "-p", "kryptos-native-host", "--target", triple];
if (release) args.push("--release");
execFileSync("cargo", args, { cwd: root, stdio: "inherit" });

const ext = triple.includes("windows") ? ".exe" : "";
const out = join(root, "apps/desktop/src-tauri/binaries");
mkdirSync(out, { recursive: true });
copyFileSync(
  join(root, "target", triple, release ? "release" : "debug", `kryptos-native-host${ext}`),
  join(out, `kryptos-native-host-${triple}${ext}`),
);
