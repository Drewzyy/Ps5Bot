// Builds latest.json, the file the in-app updater reads (Tauri v2 static JSON format):
// { version, notes, pub_date (RFC 3339), platforms: { "windows-x86_64": { signature, url }, "windows-x86_64-nsis": { same } } }
// The updater looks for "<os>-<arch>-<installer>" first and then "<os>-<arch>", so both keys are written (same as tauri-action).
// `signature` is the CONTENT of the .sig file (not a path), `version` must equal the version the installer was signed for.
// Usage: node scripts/make-latest-json.mjs --version 0.1.42 --sig-file setup.exe.sig (--url https://... | --repo o/r --tag v0.1.42 --asset name.exe) --notes-file notes.txt > latest.json
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

/** Download address of a release asset. Names are percent-encoded, so a space or other odd character cannot break it. */
export function assetUrl(repo, tag, name) {
  return `https://github.com/${repo}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(name)}`;
}

export const PLATFORMS = ["windows-x86_64", "windows-x86_64-nsis"];

export function makeLatestJson({ version, notes, pubDate, signature, url, platforms = PLATFORMS }) {
  if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) throw new Error(`bad version: ${version}`);
  if (!signature || !signature.trim()) throw new Error("signature is empty");
  if (!/^https:\/\//.test(url ?? "")) throw new Error("url must be https");
  if (/[\s"<>]/.test(url)) throw new Error("url has a space or an unsafe character (encode it)");
  if (/^(untrusted comment|[./\\]|[A-Za-z]:\\)/.test(signature.trim())) throw new Error("signature must be the content of the .sig file, not a path");
  const d = new Date(pubDate);
  if (Number.isNaN(d.getTime())) throw new Error(`bad date: ${pubDate}`);
  return {
    version,
    notes: (notes ?? "").trim(),
    pub_date: d.toISOString(),
    platforms: Object.fromEntries(platforms.map((p) => [p, { signature: signature.trim(), url }])),
  };
}

function arg(name) {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : undefined;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const read = (f) => (f ? fs.readFileSync(f, "utf8") : "");
  const json = makeLatestJson({
    version: arg("version"),
    notes: read(arg("notes-file")),
    pubDate: arg("date") ?? new Date().toISOString(),
    signature: read(arg("sig-file")),
    url: arg("url") ?? assetUrl(arg("repo"), arg("tag"), arg("asset")),
  });
  process.stdout.write(JSON.stringify(json, null, 2) + "\n");
}
