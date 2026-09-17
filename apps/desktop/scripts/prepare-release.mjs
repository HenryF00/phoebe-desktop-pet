import { execFileSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const npm = process.platform === "win32" ? "npm.cmd" : "npm";

execFileSync(npm, ["run", "build"], { cwd: root, stdio: "inherit" });
execFileSync(npm, ["run", "bundle", "-w", "@phoebe/agent"], { cwd: root, stdio: "inherit" });
