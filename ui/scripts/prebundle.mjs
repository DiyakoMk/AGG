import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scripts = dirname(fileURLToPath(import.meta.url));
const root = join(scripts, "..", "..");
const destDir = join(scripts, "..", "src-tauri", "resources");
mkdirSync(destDir, { recursive: true });

if (process.platform !== "win32") {
  process.exit(0);
}

const r = spawnSync("cargo", ["build", "-p", "agg-svc", "--release"], {
  cwd: root,
  stdio: "inherit",
  shell: true,
});
if (r.status !== 0) process.exit(r.status ?? 1);

const src = join(root, "target", "release", "agg-svc.exe");
if (!existsSync(src)) {
  console.error("agg-svc.exe missing after release build");
  process.exit(1);
}
copyFileSync(src, join(destDir, "agg-svc.exe"));
