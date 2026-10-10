#!/usr/bin/env node
// Bump all release identities together. Call once for each new user-facing
// edit/release batch; keep the resulting version unique for downloaded EXEs.
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const load = (path) => readFileSync(resolve(root, path), "utf8");
const updates = [];
const replaceOnce = (path, re, replacement) => {
  const before = load(path);
  const matches = [...before.matchAll(new RegExp(re.source, re.flags.includes("g") ? re.flags : re.flags + "g"))];
  if (matches.length !== 1) throw new Error(`Expected one version field in ${path}, found ${matches.length}.`);
  updates.push([path, before.replace(re, replacement)]);
};

const current = JSON.parse(load("package.json")).version;
if (!/^\d+\.\d+\.\d+$/.test(current)) throw new Error("Invalid current semantic version.");
const [major, minor, patch] = current.split(".").map(Number);
const newVersion = process.argv[2] ?? `${major}.${minor}.${patch + 1}`;
if (!/^\d+\.\d+\.\d+$/.test(newVersion)) throw new Error("Pass a numeric MAJOR.MINOR.PATCH version.");
const [nmaj, nmin, npat] = newVersion.split(".").map(Number);
if (!(nmaj > major || (nmaj === major && (nmin > minor || (nmin === minor && npat > patch))))) {
  throw new Error("New version must be strictly greater than current.");
}
replaceOnce("package.json", new RegExp(`("version"\\s*:\\s*")${current.replaceAll(".", "\\.")}("`), `$1${newVersion}$2`);
replaceOnce("src-tauri/tauri.conf.json", new RegExp(`("version"\\s*:\\s*")${current.replaceAll(".", "\\.")}("`), `$1${newVersion}$2`);
replaceOnce("src-tauri/Cargo.toml", new RegExp(`^(version = ")${current.replaceAll(".", "\\.")}(")`, "m"), `$1${newVersion}$2`);
replaceOnce("src-tauri/Cargo.lock", new RegExp(`(\\[\\[package\\]\\]\\s*\\nname = "claude-proxy"\\s*\\nversion = ")${current.replaceAll(".", "\\.")}(")`), `$1${newVersion}$2`);
replaceOnce("README.md", /^# Claude Proxy \d+\.\d+\.\d+/m, `# Claude Proxy ${newVersion}`);
for (const [path, next] of updates) writeFileSync(resolve(root, path), next);
console.log(`Bumped ${current} → ${newVersion}; run node scripts/check-version.mjs`);
