import test from "node:test";
import assert from "node:assert/strict";
import { classifyUpdateError, updateErrorMessage, NOT_SET_UP, NOT_SET_UP_STEPS } from "./updaterText.ts";

test("no release or a 404 on latest.json gives the exact set-up message", () => {
  for (const raw of ["Could not fetch a valid release JSON from the remote", "Could not fetch a valid release JSON from the remote (HTTP 404)", "request failed with status 404 Not Found", "ReleaseNotFound"]) {
    assert.equal(classifyUpdateError(raw), "not-set-up", raw);
    assert.equal(updateErrorMessage(raw).message, "Auto-update isn't set up yet. Add the signing key on GitHub (see Settings > About)");
  }
  assert.equal(NOT_SET_UP, "Auto-update isn't set up yet. Add the signing key on GitHub (see Settings > About)");
  assert.ok(NOT_SET_UP_STEPS.length >= 4);
  assert.ok(NOT_SET_UP_STEPS.some((s) => s.includes("TAURI_SIGNING_PRIVATE_KEY")));
});

test("other problems are not called 'not set up'", () => {
  assert.equal(classifyUpdateError("error sending request for url (https://github.com/x/latest.json)"), "offline");
  assert.equal(classifyUpdateError("dns error: failed to lookup address information"), "offline");
  assert.equal(classifyUpdateError("the signature could not be verified"), "signature");
  assert.equal(classifyUpdateError("Signature verification failed"), "signature");
  assert.equal(classifyUpdateError("missing field `version` at line 1 column 20"), "bad-release");
  assert.equal(classifyUpdateError("something odd"), "other");
  assert.match(updateErrorMessage("something odd").message, /Could not check for updates: something odd/);
  assert.match(updateErrorMessage("the signature could not be verified").message, /signature does not match/);
});

test("no message uses an em dash", () => {
  for (const raw of ["404", "dns error", "signature", "missing field", "x"]) assert.ok(!updateErrorMessage(raw).message.includes("—"));
  assert.ok(NOT_SET_UP_STEPS.every((s) => !s.includes("—")));
});
