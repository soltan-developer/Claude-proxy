#!/usr/bin/env node
// Increase the version once per user-delivered change batch (no credential writes).
// Prevalidates every file before making any changes.
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const read = (path) => readFileSync(resolve(root, path), "utf8");
const current = JSON.parse(read("package.json")).version;
if (!/^\d+\.\d+\.\d+$/.test(current)) throw new Error("Invalid current semantic version.");
const [major, minor, patch] = current.split(".").map(Number);
const next = process.argv[2] ?? `${major}.${minor}.${patch + 1}`;
if (!/^\d+\.\d+\.\d+$/.test(next)) throw new Error("Version must be numeric MAJOR.MINOR.PATCH.");
const [nm, nn, np] = next.split(".").map(Number);
if (!(nm > major || (nm === major && (nn > minor || (nn === minor && np > patch))))) {
  throw new Error(`Version ${next} must be greater than ${current}.`);
}

const changes = [];
function stage(path, regex) {
  const before = read(path);
  const matches = [...before.matchAll(new RegExp(regex.source, regex.flags.includes("g") ? regex.flags : regex.flags + "g"))];
  if (matches.length !== 1) throw new Error(`Expected one version marker in ${path}, found ${matches.length}.`);
  if (matches[0][2] !== current) throw new Error(`Version mismatch in ${path}: ${matches[0][2]} vs ${current}.`);
  changes.push([path, before.replace(regex, (_, prefix, _old, suffix) => prefix + next + suffix)]);
}
stage("package.json", /^(\s*"version": ")(\d+\.\d+\.\d+)(")/m);
stage("src-tauri/tauri.conf.json", /^(\s*"version": ")(\d+\.\d+\.\d+)(")/m);
stage("src-tauri/Cargo.toml", /^(version = ")(\d+\.\d+\.\d+)(")/m);
stage("src-tauri/Cargo.lock", /(\[\[package\]\]\nname = "claude-proxy"\nversion = ")(\d+\.\d+\.\d+)(")/);
stage("README.md", /^(# Claude Proxy )(\d+\.\d+\.\d+)()$/m);

for (const [path, newText] of changes) writeFileSync(resolve(root, path), newText);
console.log(`Version bumped: ${current} -> ${next}. Verify: node scripts/check-version.mjs`);
