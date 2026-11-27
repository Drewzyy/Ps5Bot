// Sets the app version for one CI build so every build is newer than the one before.
// Usage: node scripts/ci-set-version.mjs 0.1.42 [--updater]
//   edits app/src-tauri/tauri.conf.json (version, and with --updater bundle.createUpdaterArtifacts = true)
//   and the [package] version in app/src-tauri/Cargo.toml.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export function setConfVersion(json, version, updater) {
  const c = JSON.parse(json);
  c.version = version;
  if (updater) c.bundle = { ...c.bundle, createUpdaterArtifacts: true };
  return JSON.stringify(c, null, 2) + "\n";
}

export function setCargoVersion(toml, version) {
  let done = false;
  const out = toml.replace(/^version\s*=\s*"[^"]*"/m, () => {
    done = true;
    return `version = "${version}"`;
  });
  if (!done) throw new Error("no version line in Cargo.toml");
  return out;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const version = process.argv[2];
  if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) {
    console.error("usage: ci-set-version.mjs MAJOR.MINOR.PATCH [--updater]");
    process.exit(2);
  }
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "app", "src-tauri");
  const conf = path.join(root, "tauri.conf.json");
  const cargo = path.join(root, "Cargo.toml");
  fs.writeFileSync(conf, setConfVersion(fs.readFileSync(conf, "utf8"), version, process.argv.includes("--updater")));
  fs.writeFileSync(cargo, setCargoVersion(fs.readFileSync(cargo, "utf8"), version));
  console.log(`app version set to ${version}${process.argv.includes("--updater") ? " (updater artifacts on)" : ""}`);
}
