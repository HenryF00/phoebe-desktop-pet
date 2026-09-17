import { execFileSync } from "node:child_process";
import { mkdirSync, rmSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const desktop = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const root = resolve(desktop, "../..");
const tauri = resolve(root, "node_modules/.bin", process.platform === "win32" ? "tauri.cmd" : "tauri");
const config = resolve(desktop, "src-tauri/tauri.release.conf.json");

if (process.platform === "darwin") {
  // Tauri's decorative DMG helper is brittle with multi-gigabyte embedded runtimes.
  // Build the signed .app first, then let macOS create a conventional compressed image.
  execFileSync(tauri, ["build", "--bundles", "app", "--config", config], {
    cwd: desktop,
    stdio: "inherit",
  });
  const bundleRoot = resolve(root, "target/release/bundle");
  const source = resolve(bundleRoot, "macos");
  const architecture = process.arch === "arm64" ? "aarch64" : process.arch;
  const destination = resolve(bundleRoot, `dmg/Phoebe Assistant_0.1.0_${architecture}.dmg`);
  mkdirSync(dirname(destination), { recursive: true });
  rmSync(destination, { force: true });
  execFileSync("/usr/bin/hdiutil", [
    "create",
    "-volname", "Phoebe Assistant",
    "-srcfolder", source,
    "-format", "UDZO",
    destination,
  ], { cwd: root, stdio: "inherit" });
} else if (process.platform === "win32") {
  execFileSync(tauri, ["build", "--bundles", "nsis", "--config", config], {
    cwd: desktop,
    stdio: "inherit",
  });
} else {
  throw new Error("Phoebe release packaging currently supports macOS and Windows only");
}
