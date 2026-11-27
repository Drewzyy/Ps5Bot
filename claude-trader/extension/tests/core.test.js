import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import { emptyState, applyEvent, resetPrimed, summarize, badgeFor, pnlBadgeText, diffNotifications, closedMints, heldMints, DEFAULT_PREFS, fmtSol } from "../lib/core.js";

const pos = (id, state, extra = {}) => ({ id, lane: "momentum", mint: `M${id}`, symbol: `S${id}`, state, sizeSol: 0.1, pnlSol: 0.02, pnlPct: 20, exitReason: "", ...extra });
const stats = (over = {}) => ({ type: "stats", balanceSol: 10, solUsd: 150, replay: false, lanes: [{ pnlToday: 0.12, totalPnlSol: 0.3, tradesToday: 2, halted: null, ...over }] });
const feed = (s, ...events) => events.reduce((st, e) => applyEvent(st, e), s);

test("a snapshot right after connecting is a baseline, not news", () => {
  const a = emptyState();
  const b = feed(a, { type: "positions", ts: 1, positions: [pos(1, "OPEN")] }, { type: "kill", killed: true, text: "x" }, stats({ halted: "limit" }));
  assert.deepEqual(diffNotifications(a, b), []);
});

test("buy, sell, daily loss limit and kill switch each notify once", () => {
  let s = feed(emptyState(), { type: "positions", ts: 1, positions: [] }, { type: "kill", killed: false, text: "" }, stats());
  let n = feed(s, { type: "positions", ts: 2, positions: [pos(1, "OPEN")] });
  assert.equal(diffNotifications(s, n)[0].kind, "buy");
  assert.match(diffNotifications(s, n)[0].message, /own rules/);
  s = n;
  n = feed(s, { type: "positions", ts: 3, positions: [pos(1, "CLOSED", { exitReason: "trailing-stop", pnlSol: 0.05 })] });
  const sell = diffNotifications(s, n);
  assert.equal(sell.length, 1);
  assert.equal(sell[0].kind, "sell");
  assert.match(sell[0].message, /\+0\.0500 SOL/);
  assert.deepEqual(closedMints(s, n), ["M1"]);
  // same state again: nothing
  assert.deepEqual(diffNotifications(n, n), []);
  s = n;
  n = feed(s, stats({ halted: "Daily loss 7.5% reached" }));
  assert.equal(diffNotifications(s, n)[0].kind, "limit");
  s = n;
  n = feed(s, { type: "kill", killed: true, text: "closing 1" });
  assert.equal(diffNotifications(s, n)[0].kind, "kill");
});

test("each notification type can be switched off", () => {
  const s = feed(emptyState(), { type: "positions", ts: 1, positions: [] });
  const n = feed(s, { type: "positions", ts: 2, positions: [pos(1, "OPEN")] });
  assert.equal(diffNotifications(s, n, { ...DEFAULT_PREFS, buy: false }).length, 0);
});

test("after a lost connection the next snapshot is quiet again", () => {
  const s = feed(emptyState(), { type: "positions", ts: 1, positions: [pos(1, "OPEN")] });
  const r = resetPrimed(s);
  const n = feed(r, { type: "positions", ts: 2, positions: [pos(1, "OPEN"), pos(2, "OPEN")] });
  assert.deepEqual(diffNotifications(r, n), []);
});

test("badge: P/L colour and text, open count, offline", () => {
  const base = { ...feed(emptyState(), stats({ pnlToday: 0.12 }), { type: "positions", ts: 1, positions: [pos(1, "OPEN"), pos(2, "CLOSED")] }), conn: "connected" };
  assert.deepEqual(badgeFor(base, "pnl"), { text: "+.12", color: "#16a34a" });
  assert.equal(badgeFor(feed(base, stats({ pnlToday: -0.05 })), "pnl").color, "#dc2626");
  assert.equal(badgeFor(feed(base, stats({ pnlToday: 0 })), "pnl").text, "0");
  assert.deepEqual(badgeFor(base, "open").text, "1");
  assert.equal(badgeFor({ ...base, conn: "offline" }, "pnl").text, "off");
  assert.equal(pnlBadgeText(1.234), "+1.2");
  assert.equal(pnlBadgeText(-12.6), "-13");
});

test("summary uses the main lane and ignores closed positions", () => {
  const s = feed(emptyState(), stats(), { type: "positions", ts: 1, positions: [pos(1, "OPEN"), pos(2, "CLOSED")] });
  const m = summarize(s);
  assert.equal(m.open.length, 1);
  assert.equal(m.balanceSol, 10);
  assert.deepEqual(heldMints(s), ["M1"]);
  assert.equal(fmtSol(-0.5, true), "-0.5000 SOL");
});

test("activity is de-duplicated and kept newest first", () => {
  const e = { type: "activity", ts: 5, text: "hello", mint: null };
  const s = feed(emptyState(), e, e, { type: "activity", ts: 9, text: "later" });
  assert.deepEqual(s.activity.map((a) => a.text), ["later", "hello"]);
});

test("mint finder: pump.fun path, dexscreener path, token links", () => {
  const ctx = {};
  vm.runInNewContext(fs.readFileSync(new URL("../lib/mint.js", import.meta.url), "utf8"), ctx);
  const M = ctx.CTMint;
  const mint = "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU";
  assert.equal(M.pumpMint(`/coin/${mint}`), mint);
  assert.equal(M.pumpMint("/board"), null);
  assert.equal(M.pumpMint(`/coin/${mint}/extra`), null);
  assert.equal(M.dexId(`/solana/${mint}`), mint);
  assert.equal(M.dexId("/solana"), null);
  assert.deepEqual([...M.mintsFromLinks(["https://x.com/a", `https://solscan.io/token/${mint}`, `https://solscan.io/token/${mint}`])], [mint]);
  assert.equal(M.isAddress("nope"), false);
});

test("the extension has no buy or sell message and holds no secrets", () => {
  const files = ["background.js", "content.js", "sidepanel.js", "lib/core.js"].map((f) => fs.readFileSync(new URL(`../${f}`, import.meta.url), "utf8")).join("\n");
  assert.ok(!/t:\s*["'](buy|sell)["']/.test(files), "no buy or sell request exists");
  assert.ok(!/helius|api-key|apikey|secretKey|privateKey|seed phrase/i.test(files), "no key handling");
  const bg = fs.readFileSync(new URL("../background.js", import.meta.url), "utf8");
  assert.ok(/msg\.cmd !== "kill" && msg\.cmd !== "resume"/.test(bg), "only kill and resume pass through");
});
