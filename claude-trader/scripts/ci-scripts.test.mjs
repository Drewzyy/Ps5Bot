import test from "node:test";
import assert from "node:assert/strict";
import { makeLatestJson, assetUrl, PLATFORMS } from "./make-latest-json.mjs";
import { setCargoVersion, setConfVersion, setManifestVersion } from "./ci-set-version.mjs";
import crypto from "node:crypto";
import { verifyMinisign, parsePubkey, parseSignature, checkLatestShape } from "./updater-verify.mjs";
import fs from "node:fs";

const base = { version: "0.1.42", notes: "  Fix the exit timer\n", pubDate: "2026-11-27T12:34:56Z", signature: "dW50cnVzdGVk...\n", url: "https://github.com/Drewzyy/Ps5Bot/releases/download/v0.1.42/Claude-Trader_0.1.42_x64-setup.exe" };

test("latest.json has the Tauri v2 static shape", () => {
  const j = makeLatestJson(base);
  assert.deepEqual(Object.keys(j), ["version", "notes", "pub_date", "platforms"]);
  assert.equal(j.version, "0.1.42");
  assert.equal(j.notes, "Fix the exit timer");
  assert.equal(j.pub_date, "2026-11-27T12:34:56.000Z");
  // the updater tries "windows-x86_64-nsis" and then "windows-x86_64": both are written, with the same file and signature
  assert.deepEqual(Object.keys(j.platforms), ["windows-x86_64", "windows-x86_64-nsis"]);
  assert.deepEqual(PLATFORMS, Object.keys(j.platforms));
  for (const k of PLATFORMS) assert.deepEqual(j.platforms[k], { signature: "dW50cnVzdGVk...", url: base.url });
  assert.deepEqual(JSON.parse(JSON.stringify(j)), j);
});

test("latest.json refuses bad input", () => {
  assert.throws(() => makeLatestJson({ ...base, version: "v0.1.42" }), /bad version/);
  assert.throws(() => makeLatestJson({ ...base, signature: "  " }), /signature/);
  assert.throws(() => makeLatestJson({ ...base, url: "http://x" }), /https/);
  assert.throws(() => makeLatestJson({ ...base, pubDate: "nonsense" }), /bad date/);
  assert.throws(() => makeLatestJson({ ...base, url: "https://x/Claude Trader_0.1.42_x64-setup.exe" }), /space/);
  assert.throws(() => makeLatestJson({ ...base, signature: "C:\\keys\\setup.exe.sig" }), /content of the .sig/);
});

test("asset addresses are percent-encoded (the product name has a space)", () => {
  assert.equal(assetUrl("Drewzyy/Ps5Bot", "v0.1.42", "Claude-Trader_0.1.42_x64-setup.exe"), "https://github.com/Drewzyy/Ps5Bot/releases/download/v0.1.42/Claude-Trader_0.1.42_x64-setup.exe");
  assert.equal(assetUrl("o/r", "v1.0.0", "Claude Trader_1.0.0_x64-setup.exe"), "https://github.com/o/r/releases/download/v1.0.0/Claude%20Trader_1.0.0_x64-setup.exe");
});

test("the extension manifest version is bumped and nothing else changes", () => {
  const src = fs.readFileSync(new URL("../extension/manifest.json", import.meta.url), "utf8");
  const out = setManifestVersion(src, "0.1.77");
  assert.equal(JSON.parse(out).version, "0.1.77");
  assert.equal(JSON.parse(out).key, JSON.parse(src).key); // the pinned key (and so the extension id) is untouched
  assert.equal(out.replace("0.1.77", JSON.parse(src).version), src);
  assert.throws(() => setManifestVersion("{}", "0.1.1"));
});

// ---- signature checks (the CI step that runs before and after the release)

/** A tiny minisign signer for the tests: a fresh key pair, ED (blake2b prehash) signature, trusted comment. */
function sign(data, { version, keyId = crypto.randomBytes(8), pair = crypto.generateKeyPairSync("ed25519") } = {}) {
  const raw = pair.publicKey.export({ format: "der", type: "spki" }).subarray(12);
  const pubText = `untrusted comment: minisign public key: TEST\n${Buffer.concat([Buffer.from("Ed"), keyId, raw]).toString("base64")}\n`;
  const sig = crypto.sign(null, crypto.createHash("blake2b512").update(data).digest(), pair.privateKey);
  const trusted = `timestamp:1\tfile:setup.exe${version ? `\tversion:${version}` : ""}`;
  const global = crypto.sign(null, Buffer.concat([sig, Buffer.from(trusted)]), pair.privateKey);
  const sigText = `untrusted comment: signature from tauri secret key\n${Buffer.concat([Buffer.from("ED"), keyId, sig]).toString("base64")}\ntrusted comment: ${trusted}\n${global.toString("base64")}\n`;
  return { pub: Buffer.from(pubText).toString("base64"), sig: Buffer.from(sigText).toString("base64"), pair, keyId };
}

test("a good signature verifies, a changed file or a wrong key does not", () => {
  const data = Buffer.from("installer bytes");
  const s = sign(data, { version: "0.1.9" });
  assert.equal(verifyMinisign(data, s.sig, s.pub).version, "0.1.9");
  assert.throws(() => verifyMinisign(Buffer.from("other bytes"), s.sig, s.pub), /does not match the installer/);
  const other = sign(data);
  assert.throws(() => verifyMinisign(data, other.sig, s.pub), /does not match the public key in tauri.conf.json/);
  // same key id but a different key: caught by the signature check itself
  const forged = sign(data, { keyId: s.keyId });
  assert.throws(() => verifyMinisign(data, forged.sig, s.pub), /does not match the installer/);
  assert.throws(() => verifyMinisign(data, "not base64 of a sig", s.pub), /not a minisign signature/);
});

test("a signature made by the real Tauri CLI verifies (fixture made with `tauri signer`)", () => {
  const pub = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDA0NkVBOEEwQkIzNjY4RkEKUldUNmFEYTdvS2h1QklEeHlYYnBVbE5nOWI3UlEyYTlYS3FpMndwMmJDaGV1M0RHVStSMUkwekEK";
  const sig = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUNmFEYTdvS2h1Qk9mQm8xcXZpTVYyVWJ2TXdwc0hYN2YzZENVSjFUZXJXSGlzUkI0a3VSWW9lNGNYN1FYSjA5elNHVUFtOThqd1kzOUxJb1BtL3plRFZFcDFWeUlpTncwPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk1ODQ1ODY2CWZpbGU6Zi5leGUKMlZucWFKUlQvRHFFcWUzdXZJd1FnbzNDb3ozdlBaeHhOMzFpZWlYTGE4alBneWRVOFF2MDIwMlYrZFJpeS96ZjdUZkNsaVUrQ0lMRnhpaXJIbGJZQXc9PQo=";
  assert.equal(verifyMinisign(Buffer.from("hello-bytes\n"), sig, pub).trusted, "timestamp:1795845866\tfile:f.exe");
  assert.throws(() => verifyMinisign(Buffer.from("hello-bytes"), sig, pub), /does not match the installer/);
});

test("the public key in tauri.conf.json is a usable minisign key", () => {
  const conf = JSON.parse(fs.readFileSync(new URL("../app/src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  assert.equal(parsePubkey(conf.plugins.updater.pubkey).key.length, 32);
  assert.deepEqual(conf.plugins.updater.endpoints, ["https://github.com/Drewzyy/Ps5Bot/releases/latest/download/latest.json"]);
  assert.equal(conf.plugins.updater.windows.installMode, "passive");
});

test("a tampered trusted comment is caught", () => {
  const data = Buffer.from("x");
  const s = sign(data, { version: "0.1.9" });
  const text = Buffer.from(s.sig, "base64").toString().replace("version:0.1.9", "version:0.1.8");
  assert.throws(() => verifyMinisign(data, Buffer.from(text).toString("base64"), s.pub), /trusted comment/);
  assert.equal(parseSignature(s.sig).version, "0.1.9");
});

test("latest.json checks: version, platform keys, signature and url", () => {
  const j = makeLatestJson(base);
  assert.equal(checkLatestShape(j, { version: "0.1.42" }).url, base.url);
  assert.throws(() => checkLatestShape(j, { version: "0.1.43" }), /expected 0.1.43/);
  assert.throws(() => checkLatestShape({ ...j, platforms: {} }), /no platforms.windows-x86_64/);
  assert.throws(() => checkLatestShape({ ...j, pub_date: "x" }), /pub_date/);
  assert.throws(() => checkLatestShape({ ...j, platforms: { "windows-x86_64": { signature: "", url: base.url } } }), /signature and an https url/);
});

test("version scripts change only the version", () => {
  const conf = setConfVersion(JSON.stringify({ productName: "X", version: "0.1.0", bundle: { active: true } }), "0.1.7", true);
  const c = JSON.parse(conf);
  assert.equal(c.version, "0.1.7");
  assert.equal(c.productName, "X");
  assert.deepEqual(c.bundle, { active: true, createUpdaterArtifacts: true });
  assert.equal(JSON.parse(setConfVersion(JSON.stringify({ version: "0.1.0", bundle: {} }), "0.1.8", false)).bundle.createUpdaterArtifacts, undefined);
  const toml = '[package]\nname = "a"\nversion = "0.1.0"\n\n[dependencies]\ntauri = { version = "2" }\n';
  assert.equal(setCargoVersion(toml, "0.1.9"), '[package]\nname = "a"\nversion = "0.1.9"\n\n[dependencies]\ntauri = { version = "2" }\n');
  assert.throws(() => setCargoVersion("[package]\nname = 'a'\n", "0.1.1"));
});
