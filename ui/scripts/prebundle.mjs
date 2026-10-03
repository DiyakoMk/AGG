import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scripts = dirname(fileURLToPath(import.meta.url));
const root = join(scripts, "..", "..");
const destDir = join(scripts, "..", "src-tauri", "resources");
const dest = join(destDir, "agg-svc.exe");
mkdirSync(destDir, { recursive: true });

const placeholder = process.argv.includes("--placeholder");

if (placeholder) {
  // tauri.windows.conf.json lists this path; `tauri dev` validates it exists
  // even though the installer is the only consumer of the real binary.
  if (!existsSync(dest)) {
    writeFileSync(dest, Buffer.alloc(0));
  }
  process.exit(0);
}

if (process.platform !== "win32") {
  if (!existsSync(dest)) {
    writeFileSync(dest, Buffer.alloc(0));
  }
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
copyFileSync(src, dest);
