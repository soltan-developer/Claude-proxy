#!/usr/bin/env node
// Validate that all app identities have one version. No network or writes.
import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const read = (path) => readFileSync(resolve(root, path), "utf8");
const version = JSON.parse(read("package.json")).version;
const versions = {
  "package.json": version,
  "src-tauri/tauri.conf.json": JSON.parse(read("src-tauri/tauri.conf.json")).version,
  "src-tauri/Cargo.toml": read("src-tauri/Cargo.toml").match(/^version = "([^"]+)"/m)?.[1],
  "src-tauri/Cargo.lock": read("src-tauri/Cargo.lock").match(/\[\[package\]\]\s*\nname = "claude-proxy"\s*\nversion = "([^"]+)"/)?.[1],
  "README.md": read("README.md").match(/^# Claude Proxy (\d+\.\d+\.\d+)/m)?.[1],
};
if (!/^\d+\.\d+\.\d+$/.test(version)) {
  throw new Error("The application version must be numeric MAJOR.MINOR.PATCH.");
}
for (const [path, actual] of Object.entries(versions)) {
  if (actual !== version) throw new Error(`VERSION_MISMATCH: ${path} = ${actual ?? "missing"}; expected ${version}`);
}
const requestedBase = process.argv.indexOf("--base");
const baseSha = requestedBase >= 0 ? (process.argv[requestedBase + 1] || "") : "";
if (baseSha) {
  // The PR's base must be strictly older: changes cannot silently reuse a
  // version already built or published from the same release ancestry.
  if (!/^[a-f0-9]{40}$/.test(baseSha)) throw new Error("Invalid base commit SHA.");
  const prior = JSON.parse(execFileSync("git", ["show", `${baseSha}:package.json`], { cwd: root, encoding: "utf8" })).version;
  const parts = (v) => v.split(".").map(Number);
  const [ma, mi, pa] = parts(version);
  const [ba, bi, bp] = parts(prior);
  if (!(ma > ba || (ma === ba && (mi > bi || (mi === bi && pa > bp))))) {
    throw new Error(`VERSION_NOT_BUMPED: ${version} must be greater than PR base ${prior}. Run: node scripts/bump-version.mjs`);
  }
}
console.log(`Version OK: v${version} (package, Tauri, Cargo, Cargo.lock, README)`);
