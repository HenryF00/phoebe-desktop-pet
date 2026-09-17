import { execFileSync, spawnSync } from "node:child_process";
import { copyFileSync, chmodSync, existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const tauriDir = resolve(root, "apps/desktop/src-tauri");
const binaryDir = resolve(tauriDir, "binaries");
const resourceDir = resolve(tauriDir, "resources");
let targetTriple;
try {
  targetTriple = execFileSync("rustc", ["--print", "host-tuple"], { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
} catch {
  const version = execFileSync("rustc", ["-Vv"], { encoding: "utf8" });
  targetTriple = version.match(/^host:\s*(.+)$/m)?.[1]?.trim();
}
const architecture = process.arch === "arm64" ? "aarch64" : process.arch === "x64" ? "x86_64" : null;
if (!architecture || !targetTriple.startsWith(`${architecture}-`)) {
  throw new Error(`Node runtime architecture does not match Rust host: ${process.arch} / ${targetTriple}`);
}
if (Number(process.versions.node.split(".")[0]) < 22) {
  throw new Error("Release Agent requires a Node.js 22 or newer build runtime");
}

mkdirSync(binaryDir, { recursive: true });
mkdirSync(resourceDir, { recursive: true });
const extension = process.platform === "win32" ? ".exe" : "";
const binary = resolve(binaryDir, `phoebe-agent-node-${targetTriple}${extension}`);
const script = resolve(resourceDir, "phoebe-agent.cjs");
const nodeLicense = [
  resolve(dirname(process.execPath), "LICENSE"),
  resolve(dirname(dirname(process.execPath)), "LICENSE"),
].find(existsSync);
if (!nodeLicense) throw new Error("The Node.js runtime license was not found; refusing to package it");

await build({
  entryPoints: [resolve(root, "packages/agent/src/sidecar.mjs")],
  outfile: script,
  bundle: true,
  platform: "node",
  format: "cjs",
  target: "node22",
  logLevel: "warning",
});
copyFileSync(process.execPath, binary);
if (process.platform !== "win32") chmodSync(binary, 0o755);
copyFileSync(nodeLicense, resolve(resourceDir, "NODE-LICENSE"));

const probe = spawnSync(binary, [script], {
  input: '{"type":"status"}\n',
  encoding: "utf8",
  env: {
    PATH: process.env.PATH || "",
    SYSTEMROOT: process.env.SYSTEMROOT || "",
    DEEPSEEK_API_KEY: "packaging-smoke-test-not-a-secret",
    PHOEBE_DEEPSEEK_MODEL: "deepseek-v4-flash",
  },
  timeout: 10000,
});
if (probe.status !== 0 || !probe.stdout.includes('"agent":"ready"')) {
  throw new Error(`Bundled Agent failed its offline status check: ${probe.stderr || probe.stdout}`);
}
console.log(`Prepared offline-checked Agent runtime and resources for ${targetTriple}`);
