// Builds latest.json, the file the in-app updater reads (Tauri v2 static JSON format):
// { version, notes, pub_date (RFC 3339), platforms: { "windows-x86_64": { signature, url } } }
// Usage: node scripts/make-latest-json.mjs --version 0.1.42 --sig-file setup.exe.sig --url https://... --notes-file notes.txt > latest.json
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export function makeLatestJson({ version, notes, pubDate, signature, url, platform = "windows-x86_64" }) {
  if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) throw new Error(`bad version: ${version}`);
  if (!signature || !signature.trim()) throw new Error("signature is empty");
  if (!/^https:\/\//.test(url ?? "")) throw new Error("url must be https");
  const d = new Date(pubDate);
  if (Number.isNaN(d.getTime())) throw new Error(`bad date: ${pubDate}`);
  return {
    version,
    notes: (notes ?? "").trim(),
    pub_date: d.toISOString(),
    platforms: { [platform]: { signature: signature.trim(), url } },
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
    url: arg("url"),
  });
  process.stdout.write(JSON.stringify(json, null, 2) + "\n");
}
