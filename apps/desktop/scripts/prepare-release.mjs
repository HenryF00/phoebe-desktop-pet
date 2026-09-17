import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const npm = process.platform === "win32" ? "npm.cmd" : "npm";

execFileSync(npm, ["run", "build"], { cwd: root, stdio: "inherit" });
execFileSync(npm, ["run", "bundle", "-w", "@phoebe/agent"], { cwd: root, stdio: "inherit" });

const output = resolve(root, "apps/desktop/src-tauri/resources/voice-runtime");
const required = ["manifest.json", "python-env.tar.gz", "gpt-sovits.tar.gz", "GPT-SoVITS-LICENSE"];
const targetPlatform = { darwin: "macos", win32: "windows" }[process.platform];
const targetArchitecture = { arm64: "aarch64", x64: "x86_64" }[process.arch];
let ready = required.every(name => existsSync(resolve(output, name)));
if (ready) {
  try {
    const manifest = JSON.parse(readFileSync(resolve(output, "manifest.json"), "utf8"));
    ready = manifest.platform === targetPlatform && manifest.architecture === targetArchitecture;
  } catch {
    ready = false;
  }
}
if (!ready || process.env.PHOEBE_REBUILD_VOICE_RUNTIME === "1") {
  const source = resolve(process.env.PHOEBE_GPTSOVITS_ROOT || resolve(root, "../GPT-SoVITS"));
  const defaultEnvironment = resolve(homedir(), "miniconda3/envs/GPTSoVits");
  const environment = resolve(process.env.PHOEBE_GPTSOVITS_ENV || defaultEnvironment);
  const python = process.env.PHOEBE_GPTSOVITS_PYTHON || (process.platform === "win32"
    ? resolve(environment, "python.exe")
    : resolve(environment, "bin/python"));
  if (!existsSync(python)) throw new Error(`GPT-SoVITS Python was not found: ${python}`);
  execFileSync(python, [resolve(root, "apps/desktop/scripts/prepare-voice-runtime.py"),
    "--source", source, "--environment", environment, "--output", output], {
    cwd: root, stdio: "inherit",
  });
} else {
  console.log("Reusing prepared platform voice runtime; set PHOEBE_REBUILD_VOICE_RUNTIME=1 to refresh it");
}
