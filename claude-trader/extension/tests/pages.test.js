import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { requiredPages, findTab, planOpen, checklist, summary, urlMatches, reloadAllowed } from "../lib/pages.js";

const M1 = "So11111111111111111111111111111111111111112";
const M2 = "7GCihgDB8fe6KNjn2MYtkzZcRjQy3t9GHdC8uHYmW2hr";
const on = { pages: { pumpFunHome: true, dexscreenerHeld: true } };
const req = (over = {}) => requiredPages({ settings: on, xmon: { enabled: true }, held: [{ mint: M1, symbol: "CAT" }], ...over });
const ids = (l) => l.map((p) => p.id);

test("the needed pages: pump.fun home, x.com and a chart per open position", () => {
  const r = req({ held: [{ mint: M1, symbol: "CAT" }, { mint: M2, symbol: "DOG" }] });
  assert.deepEqual(ids(r), ["pump-home", "x", `dex-${M1}`, `dex-${M2}`]);
  assert.equal(r[2].url, `https://dexscreener.com/solana/${M1}`);
  assert.equal(r[1].pinned, true);
  assert.equal(r[1].background, true);
  assert.equal(r.every((p) => !p.off), true);
});

test("pages whose toggle is off are listed as off and never opened", () => {
  const r = requiredPages({ settings: { pages: { pumpFunHome: false, dexscreenerHeld: false } }, xmon: { enabled: false }, held: [{ mint: M1, symbol: "CAT" }] });
  assert.deepEqual(r.map((p) => [p.id, p.off]), [["pump-home", true], ["x", true], ["dex", true]]);
  assert.deepEqual(planOpen(r, [], {}, true).open, [], "even a forced open skips pages that are off");
  assert.deepEqual(checklist(r, [], {}).map((i) => i.state), ["off", "off", "off"]);
  assert.deepEqual(summary(checklist(r, [], {})), { ok: 0, need: 0, missing: 0, loggedOut: 0 });
});

test("a bad coin address never becomes a page", () => {
  assert.deepEqual(ids(req({ held: [{ mint: "javascript:alert(1)", symbol: "X" }, { mint: "short" }] })), ["pump-home", "x"]);
});

test("an open tab is reused: the page is not opened again", () => {
  const tabs = [{ id: 1, url: "https://www.pump.fun/coin/abc" }, { id: 2, url: `https://dexscreener.com/solana/${M1}?x=1` }, { id: 3, url: "https://x.com/someone" }];
  const r = req();
  assert.equal(findTab(r[0], tabs).id, 1, "www variant counts");
  assert.equal(findTab(r[2], tabs).id, 2);
  assert.equal(findTab(r[1], tabs).id, 3);
  const p = planOpen(r, tabs, {}, true);
  assert.deepEqual(p.open, []);
  assert.deepEqual(checklist(r, tabs, { xLoggedIn: true }).map((i) => i.state), ["open", "open", "open"]);
  // a different coin's chart does not count
  assert.equal(findTab(r[2], [{ id: 9, url: `https://dexscreener.com/solana/${M2}` }]), null);
  // a lookalike host does not count
  assert.equal(findTab(r[0], [{ id: 9, url: "https://pump.fun.evil.example/" }]), null);
  assert.equal(findTab(r[0], [{ id: 9, url: "https://notpump.fun/" }]), null);
});

test("only what is missing is opened, each page once, even with duplicate entries", () => {
  const r = [...req(), ...req()];
  const p = planOpen(r, [{ id: 1, url: "https://pump.fun/" }], {}, true);
  assert.deepEqual(ids(p.open), ["x", `dex-${M1}`]);
  assert.equal(p.open.length, new Set(ids(p.open)).size);
});

test("at app start a page is tried once per browser session, the button opens all that are missing", () => {
  const r = req();
  const first = planOpen(r, [], {}, false);
  assert.deepEqual(ids(first.open), ["pump-home", "x", `dex-${M1}`]);
  // the tab was closed on purpose: the next automatic check leaves it closed
  assert.deepEqual(planOpen(r, [], first.gates, false).open, []);
  // "Open missing pages" opens it again
  assert.deepEqual(ids(planOpen(r, [], first.gates, true).open), ["pump-home", "x", `dex-${M1}`]);
});

test("the checklist says Open, Missing or Logged out", () => {
  const r = req();
  const tabs = [{ id: 3, url: "https://x.com/home" }];
  assert.deepEqual(checklist(r, tabs, { xLoggedIn: false }).map((i) => i.state), ["missing", "loggedout", "missing"]);
  assert.deepEqual(checklist(r, tabs, { xLoggedIn: true }).map((i) => i.state), ["missing", "open", "missing"]);
  assert.equal(checklist(r, tabs, { xLoggedIn: null })[1].state, "open", "login not known yet is not called logged out");
  assert.equal(checklist(r, [], { xLoggedIn: false })[1].state, "missing", "no x.com tab: missing, not logged out");
  assert.deepEqual(summary(checklist(r, tabs, { xLoggedIn: false })), { ok: 0, need: 3, missing: 2, loggedOut: 1 });
});

test("urlMatches only treats a trailing * as a wildcard", () => {
  assert.equal(urlMatches("https://pump.fun/", "https://pump.fun/*"), true);
  assert.equal(urlMatches("https://pump.fun", "https://pump.fun/*"), false);
  assert.equal(urlMatches(undefined, "https://pump.fun/*"), false);
  assert.equal(urlMatches("https://a/", "https://a/"), true);
});

test("a reload from the app is honoured at most once every 2 minutes", () => {
  assert.equal(reloadAllowed(1_000_000, 0), true);
  assert.equal(reloadAllowed(1_000_000, 1_000_000 - 119_000), false);
  assert.equal(reloadAllowed(1_000_000, 1_000_000 - 121_000), true);
});

test("the background worker only opens pages it builds from the list, and reload is gated", () => {
  const bg = fs.readFileSync(new URL("../background.js", import.meta.url), "utf8");
  assert.match(bg, /reloadAllowed/);
  assert.match(bg, /chrome\.runtime\.reload\(\)/);
  assert.match(bg, /case "reload"|v\.t === "reload"/);
  assert.equal((bg.match(/chrome\.runtime\.reload\(/g) || []).length, 1, "one place reloads the extension");
});

// ---- the x.com login probe in the content script (the checklist's "Logged out")

import vm from "node:vm";
function runXContent(url, text) {
  const listeners = [];
  const ctx = {
    window: { addEventListener() {}, location: { origin: "https://x.com" } },
    location: { href: url, pathname: new URL(url).pathname, origin: "https://x.com" },
    document: { body: { innerText: text }, querySelector: () => null },
    chrome: { runtime: { id: "me", sendMessage() {}, onMessage: { addListener: (f) => listeners.push(f) } } },
    setTimeout() {}, setInterval() { return 1; }, clearInterval() {},
  };
  ctx.globalThis = ctx;
  vm.createContext(ctx);
  vm.runInContext(fs.readFileSync(new URL("../lib/xparse.js", import.meta.url), "utf8"), ctx);
  vm.runInContext(fs.readFileSync(new URL("../xcontent.js", import.meta.url), "utf8"), ctx);
  return (msg, senderId = "me") => {
    let out;
    for (const f of listeners) f(msg, { id: senderId }, (r) => (out = JSON.parse(JSON.stringify(r))));
    return out;
  };
}

test("the x.com login probe says login for the sign-in page and ok for a logged-in page", () => {
  assert.deepEqual(runXContent("https://x.com/home", "Sign in to X\nPhone, email")({ t: "xlogin" }), { state: "login" });
  assert.deepEqual(runXContent("https://x.com/i/flow/login", "")({ t: "xlogin" }), { state: "login" });
  assert.deepEqual(runXContent("https://x.com/home", "Home\nFor you\nFollowing")({ t: "xlogin" }), { state: "ok" });
  // only this extension may ask
  assert.equal(runXContent("https://x.com/home", "Home")({ t: "xlogin" }, "someone-else"), undefined);
});
