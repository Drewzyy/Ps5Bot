import test from "node:test";
import assert from "node:assert/strict";
import { extChip, ago, pagesSummary, updatedNotice, versionLine } from "./extStatus.ts";
import type { ExtensionStatus } from "./types.ts";

const base: ExtensionStatus = { paired: true, connected: true, lastSeenMs: 1, version: "0.1.42", diskVersion: "0.1.42", appVersion: "0.1.42", heartbeatMs: 1, link: "ok", updatedTo: "", updatedAt: 0, pages: [], pagesTs: 0 };

test("the chip: connected (green), not paired (gray), offline (red)", () => {
  assert.equal(extChip(null), null);
  assert.deepEqual([extChip({ ...base, paired: false, connected: false })?.tone, extChip({ ...base, paired: false, connected: false })?.text], ["gray", "Extension not paired"]);
  assert.deepEqual([extChip({ ...base, connected: false })?.tone, extChip({ ...base, connected: false })?.text], ["off", "Extension offline"]);
  const ok = extChip(base)!;
  assert.deepEqual([ok.tone, ok.text, ok.attention], ["on", "Extension connected", false]);
  // connected but running old files: still connected, with a mark
  assert.equal(extChip({ ...base, link: "manual" })!.attention, true);
});

test("heartbeat age is plain", () => {
  assert.equal(ago(0, 5000), "never");
  assert.equal(ago(10_000, 13_000), "3 s ago");
  assert.equal(ago(0 + 1, 125_001), "2 min ago");
  assert.equal(ago(1000, 1000 + 3 * 3600_000), "3 h ago");
  assert.equal(ago(9000, 1000), "0 s ago");
});

test("versions, update notice and checklist counts", () => {
  assert.match(versionLine({ ...base, version: "0.1.41" }), /Extension v0.1.41, files on disk v0.1.42, app v0.1.42/);
  assert.match(versionLine({ ...base, version: "", diskVersion: "" }), /not reported yet.*none yet/);
  assert.equal(updatedNotice({ ...base, updatedTo: "0.1.42" }), "Extension updated to v0.1.42");
  assert.equal(updatedNotice(base), "");
  const s = pagesSummary([{ id: "a", label: "a", state: "open", url: "" }, { id: "b", label: "b", state: "missing", url: "" }, { id: "c", label: "c", state: "loggedout", url: "" }, { id: "d", label: "d", state: "off", url: "" }]);
  assert.deepEqual(s, { need: 3, ok: 1, missing: 1, loggedOut: 1 });
});
