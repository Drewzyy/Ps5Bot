import test from "node:test";
import assert from "node:assert/strict";
import { makeLatestJson } from "./make-latest-json.mjs";
import { setCargoVersion, setConfVersion } from "./ci-set-version.mjs";

const base = { version: "0.1.42", notes: "  Fix the exit timer\n", pubDate: "2026-11-27T12:34:56Z", signature: "dW50cnVzdGVk...\n", url: "https://github.com/Drewzyy/Ps5Bot/releases/download/v0.1.42/Claude-Trader_0.1.42_x64-setup.exe" };

test("latest.json has the Tauri v2 static shape", () => {
  const j = makeLatestJson(base);
  assert.deepEqual(Object.keys(j), ["version", "notes", "pub_date", "platforms"]);
  assert.equal(j.version, "0.1.42");
  assert.equal(j.notes, "Fix the exit timer");
  assert.equal(j.pub_date, "2026-11-27T12:34:56.000Z");
  assert.deepEqual(Object.keys(j.platforms), ["windows-x86_64"]);
  assert.deepEqual(j.platforms["windows-x86_64"], { signature: "dW50cnVzdGVk...", url: base.url });
  assert.deepEqual(JSON.parse(JSON.stringify(j)), j);
});

test("latest.json refuses bad input", () => {
  assert.throws(() => makeLatestJson({ ...base, version: "v0.1.42" }), /bad version/);
  assert.throws(() => makeLatestJson({ ...base, signature: "  " }), /signature/);
  assert.throws(() => makeLatestJson({ ...base, url: "http://x" }), /https/);
  assert.throws(() => makeLatestJson({ ...base, pubDate: "nonsense" }), /bad date/);
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
