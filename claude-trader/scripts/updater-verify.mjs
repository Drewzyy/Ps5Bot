// Checks for the in-app updater (Tauri v2, minisign signatures). No dependencies, Node 18+.
//
//   Before the release is created (the file and its .sig, against the public key in tauri.conf.json):
//     node scripts/updater-verify.mjs local --file setup.exe --sig setup.exe.sig --version 0.1.42
//   After the release is created (what the app will really download):
//     node scripts/updater-verify.mjs remote --repo owner/name --tag v0.1.42 --version 0.1.42 --asset setup.exe
//
// remote: latest.json is downloadable from releases/latest/download, has the right version and platform keys,
// its signature is exactly the uploaded .sig asset, its URL resolves and the downloaded installer verifies against
// the public key. Any failure exits 1 with a plain message.
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const b64 = (s) => Buffer.from(String(s).trim(), "base64");
const lines = (t) => String(t).replace(/\r/g, "").split("\n");

/** The public key as stored in tauri.conf.json (base64 of the minisign .pub file). */
export function parsePubkey(pubkeyB64) {
  const text = b64(pubkeyB64).toString("utf8");
  const l = lines(text).filter(Boolean);
  const raw = b64(l[1] ?? "");
  if (raw.length !== 42 || raw.subarray(0, 2).toString() !== "Ed") throw new Error("the public key in tauri.conf.json is not a minisign key");
  return { keyId: raw.subarray(2, 10), key: raw.subarray(10), keyIdHex: Buffer.from(raw.subarray(2, 10)).reverse().toString("hex").toUpperCase() };
}

/** A .sig file / latest.json signature: base64 of the minisign signature file. */
export function parseSignature(sigB64) {
  const text = b64(sigB64).toString("utf8");
  const l = lines(text);
  const raw = b64(l[1] ?? "");
  if (raw.length !== 74 || !["Ed", "ED"].includes(raw.subarray(0, 2).toString())) throw new Error("the signature is not a minisign signature (is it the contents of the .sig file?)");
  const tc = (l[2] ?? "").startsWith("trusted comment: ") ? l[2].slice("trusted comment: ".length) : null;
  if (tc === null) throw new Error("the signature has no trusted comment");
  const version = tc.split("\t").map((f) => f.match(/^version:(.*)$/)).find(Boolean)?.[1] ?? null;
  return { prehashed: raw.subarray(0, 2).toString() === "ED", keyId: raw.subarray(2, 10), sig: raw.subarray(10), trusted: tc, global: b64(l[3] ?? ""), version };
}

const SPKI = Buffer.from("302a300506032b6570032100", "hex");
const edKey = (raw) => crypto.createPublicKey({ key: Buffer.concat([SPKI, raw]), format: "der", type: "spki" });

/** Same checks as the Rust updater: key id, file signature (blake2b prehash), and the signature over the trusted comment. */
export function verifyMinisign(data, sigB64, pubkeyB64) {
  const pub = parsePubkey(pubkeyB64);
  const s = parseSignature(sigB64);
  if (!pub.keyId.equals(s.keyId)) {
    const sid = Buffer.from(s.keyId).reverse().toString("hex").toUpperCase();
    throw new Error(`the signing key does not match the public key in tauri.conf.json (signature key ${sid}, app key ${pub.keyIdHex}). The GitHub secret TAURI_SIGNING_PRIVATE_KEY must be the private half of that public key.`);
  }
  const msg = s.prehashed ? crypto.createHash("blake2b512").update(data).digest() : data;
  const pk = edKey(pub.key);
  if (!crypto.verify(null, msg, pk, s.sig)) throw new Error("the signature does not match the installer file");
  if (!crypto.verify(null, Buffer.concat([s.sig, Buffer.from(s.trusted)]), pk, s.global)) throw new Error("the trusted comment of the signature was changed");
  return s;
}

/** Shape checks on latest.json. Returns the platform entry. Throws a plain sentence. */
export function checkLatestShape(j, { version, platform = "windows-x86_64" } = {}) {
  if (!j || typeof j !== "object") throw new Error("latest.json is not a JSON object");
  if (!/^\d+\.\d+\.\d+$/.test(j.version ?? "")) throw new Error(`latest.json has a bad version: ${j.version}`);
  if (version && j.version !== version) throw new Error(`latest.json says version ${j.version}, expected ${version} (is "latest" pointing at another release?)`);
  if (Number.isNaN(new Date(j.pub_date).getTime())) throw new Error("latest.json has no valid pub_date");
  const p = j.platforms?.[platform];
  if (!p) throw new Error(`latest.json has no platforms.${platform} entry`);
  if (!p.signature || !/^https:\/\//.test(p.url ?? "")) throw new Error(`platforms.${platform} needs a signature and an https url`);
  return p;
}

const fail = (m) => {
  console.error(`::error title=Updater check failed::${m}`);
  console.error(`UPDATER CHECK FAILED: ${m}`);
  process.exit(1);
};
const arg = (n) => {
  const i = process.argv.indexOf(`--${n}`);
  return i > 0 ? process.argv[i + 1] : undefined;
};
const here = path.dirname(fileURLToPath(import.meta.url));
const confPubkey = () => JSON.parse(fs.readFileSync(path.join(here, "..", "app", "src-tauri", "tauri.conf.json"), "utf8")).plugins.updater.pubkey;

async function get(url, tries = 12) {
  let last = "";
  for (let i = 0; i < tries; i++) {
    try {
      const r = await fetch(url, { redirect: "follow", headers: { "user-agent": "claude-trader-ci" } });
      if (r.ok) return Buffer.from(await r.arrayBuffer());
      last = `HTTP ${r.status}`;
    } catch (e) {
      last = String(e.message ?? e);
    }
    await new Promise((r) => setTimeout(r, 10_000)); // a new release can take a short while to show up
  }
  throw new Error(`${url} could not be downloaded (${last})`);
}

async function main() {
  const mode = process.argv[2];
  const pubkey = confPubkey();
  if (mode === "local") {
    const file = fs.readFileSync(arg("file"));
    const sig = fs.readFileSync(arg("sig"), "utf8");
    const s = verifyMinisign(file, sig, pubkey);
    if (s.version && arg("version") && s.version !== arg("version")) throw new Error(`the signature says version ${s.version}, the build is ${arg("version")}`);
    console.log(`OK: signature matches the installer and the public key (${file.length} bytes)`);
  } else if (mode === "remote") {
    const repo = arg("repo");
    const tag = arg("tag");
    const version = arg("version");
    const asset = arg("asset");
    const j = JSON.parse((await get(`https://github.com/${repo}/releases/latest/download/latest.json`)).toString("utf8"));
    const p = checkLatestShape(j, { version });
    console.log(`latest.json: version ${j.version}, url ${p.url}`);
    const uploaded = (await get(`https://github.com/${repo}/releases/download/${tag}/${asset}.sig`)).toString("utf8");
    if (uploaded.trim() !== p.signature.trim()) throw new Error("latest.json signature is not the same as the uploaded .sig file");
    const exe = await get(p.url);
    if (exe.length < 1_000_000) throw new Error(`the installer at the url is only ${exe.length} bytes`);
    const s = verifyMinisign(exe, p.signature, pubkey);
    if (s.version && s.version !== j.version) throw new Error(`the signature says version ${s.version}, latest.json says ${j.version}`);
    console.log(`OK: latest.json downloads, its signature is the uploaded .sig, the url resolves (${exe.length} bytes) and the signature verifies against the app's public key.`);
  } else {
    throw new Error("usage: updater-verify.mjs local|remote ...");
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main().catch((e) => fail(e.message ?? String(e)));
