import type { ChangeRow, EngineEvent, JournalRow, LearningSnapshot, PageStatus, UpdateCheck, XMonitorView } from "./types";

/**
 * Seeded DEMO data for screenshots (open the page with ?demo=1). None of it is real. Every screen that shows it
 * carries a DEMO label. The real app never uses this file's data: it only runs when ?demo=1 is in the address.
 */
const NOW = Date.now();
const min = 60_000;

function row(n: number, sym: string, pnl: number, reason: string, hold: number, curve: number, ratio: number, buyers: number, score: number, obs: string[], url: string): JournalRow {
  return {
    id: n, ts: NOW - (40 - n * 6) * min, positionId: n, lane: "momentum", tradeNo: n, symbol: sym, mint: `DEMO${n}`.padEnd(44, "x"),
    pnlSol: pnl, pnlPct: pnl * 1000, feesSol: 0.0056, grossSol: pnl + 0.0056, holdSeconds: hold, exitReason: reason, score, buyers, ratio, curveSol: curve,
    ageSeconds: 61, peakPct: 34, exitPct: 25, fillSeconds: 0.9, observations: obs, sourceUrl: url,
    headline: `Trade #${n}: ${pnl >= 0 ? "+" : ""}${pnl.toFixed(3)} SOL.`,
  };
}

const journal: JournalRow[] = [
  row(12, "MOONCAT", 0.012, "trailing-stop", 53, 4.2, 3.4, 9, 88.5, [
    "Exit fired at take profit 1, take profit 2 then the trailing stop; the peak was +34% and the last sell was at +25%, so the trailing stop gave back 9 points.",
    "Fees were 31% of the gross profit (0.0056 of 0.0176 SOL).",
    "Entered at curve 4.2 SOL; this lane's trades under 5 SOL are 2W/5L so far (counting this one).",
    "The buy filled 0.9 s after the pick, 1.20% above the quote.",
  ], "https://example.org/mooncat"),
  row(11, "PEPEX", -0.011, "hard-stop", 22, 4.8, 2.6, 7, 82.1, [
    "Exit fired at the hard stop; the peak was +3% and the last sell was at -16%.",
    "The price move alone made -0.005 SOL; fees of 0.0056 SOL took the result to -0.011 SOL.",
    "Entered at curve 4.8 SOL; this lane's trades under 5 SOL are 1W/5L so far (counting this one).",
  ], "https://x.com/pepex"),
  row(10, "GLOWB", 0.021, "take-profit-3", 71, 9.6, 4.1, 12, 93.4, [
    "Exit fired at take profit 1, take profit 2 then take profit 3; the peak was +71% and the last sell was at +60%.",
    "Fees were 21% of the gross profit (0.0056 of 0.0266 SOL).",
    "Entered at curve 9.6 SOL; this lane's trades 5 to 10 SOL are 5W/2L so far (counting this one).",
  ], "https://t.me/glowb"),
  row(9, "DOGGO", -0.012, "soft-stop", 38, 4.4, 2.2, 6, 80.3, [
    "Exit fired at the soft stop; the peak was +2% and the last sell was at -11%.",
    "Entered at curve 4.4 SOL; this lane's trades under 5 SOL are 1W/4L so far (counting this one).",
  ], "https://example.org/doggo"),
  row(8, "ZAPPY", 0.009, "max-hold", 300, 7.7, 3.0, 10, 86.0, [
    "Exit fired at take profit 1 then max hold; the peak was +26% and the last sell was at +12%.",
    "Fees were 38% of the gross profit (0.0056 of 0.0146 SOL).",
  ], ""),
];

const changes: ChangeRow[] = [
  {
    id: 3, ts: NOW - 20 * min, key: "entry.minBuySellRatio", label: "Buy/sell ratio needed to buy", before: 2, after: 2.4, status: "shadow",
    why: "Change buy/sell ratio needed to buy from 2 to 2.4 (the most allowed is 20% from the shipped value).",
    evidence: ["buy/sell ratio at entry, under 3: 11 trades, 27% won, average -0.006 SOL. All other trades: 19, 68% won, average +0.008 SOL."],
    result: "", startMs: NOW - 20 * min, mainStart: 30, target: 30, doneMs: 0,
  },
  {
    id: 2, ts: NOW - 300 * min, key: "entry.curveSolMin", label: "Lowest SOL in the curve to buy", before: 4, after: 4.8, status: "applied",
    why: "Change lowest sol in the curve to buy from 4 to 4.8 (the most allowed is 20% from the shipped value).",
    evidence: ["curve SOL at entry, under 5 SOL: 12 trades, 17% won, average -0.008 SOL. All other trades: 18, 72% won, average +0.009 SOL."],
    result: "The test lane won on both. Test lane: 12 trades, net +0.071 SOL, profit factor 3.10. Main lane: 30 trades, net +0.012 SOL, profit factor 1.08.",
    startMs: NOW - 600 * min, mainStart: 0, target: 30, doneMs: NOW - 300 * min,
  },
  {
    id: 1, ts: NOW - 900 * min, key: "exits.trailingStopPct", label: "Trailing stop (%)", before: 10, after: 8, status: "rejected",
    why: "Change trailing stop (%) from 10 to 8 (the most allowed is 20% from the shipped value).",
    evidence: ["Trailing stop exits: 11 trades gave back 15.1 points on average from the peak, 3.4 more than the 10% rule intends."],
    result: "The main lane did better. Test lane: 11 trades, net -0.004 SOL, profit factor 0.91. Main lane: 30 trades, net +0.006 SOL, profit factor 1.05.",
    startMs: NOW - 1200 * min, mainStart: 0, target: 30, doneMs: NOW - 900 * min,
  },
];

export const demoLearning: LearningSnapshot = {
  ts: NOW, enabled: true, liveMode: false, demo: true, closedTrades: 12, reviewEvery: 30, nextReviewIn: 18,
  journal, changes, shadow: changes[0],
  shadowProgress: { mainDone: 12, target: 30, shadow: { trades: 5, netSol: 0.018, wins: 3, profitFactor: 2.4 }, main: { trades: 12, netSol: 0.011, wins: 6, profitFactor: 1.3 } },
  lastReview: {
    ts: NOW - 20 * min, tradesReviewed: 30,
    result: {
      tradesReviewed: 30,
      findings: [{ text: "buy/sell ratio at entry, under 3: 11 trades, 27% won, average -0.006 SOL. All other trades: 19, 68% won, average +0.008 SOL." }],
      exitLines: ["Exit trailing stop: 11 trades, 64% won, average +0.004 SOL."],
      proposal: { label: "Buy/sell ratio needed to buy", before: 2, after: 2.4, why: "", evidence: [] },
      notes: [],
    },
  },
  efficiency: {
    fills: 12, avgPickToFillS: 1.1, failedBuys: 2, failedTxCostSol: 0.0021, feesSol: 0.067, grossProfitSol: 0.121, feeSharePct: 33,
    missed: 3,
    notes: [
      { id: 2, ts: NOW - 50 * min, kind: "missed", text: "Missed HOMER (score 99.0): blocked by curve holds 3.1 SOL (needs 4-40). Over the next 5 minutes (41 trades seen) the price ended +12% and peaked at +30%.", sourceUrl: "https://example.org/homer" },
      { id: 1, ts: NOW - 95 * min, kind: "missed", text: "Missed RUDOLPH (score 94.5): blocked by buy/sell 1.6 below 2. No price data was seen in the next 5 minutes, so what it did is unknown.", sourceUrl: "" },
    ],
  },
};

export function demoUpdate(): UpdateCheck {
  return {
    verdict: "update-available",
    message: "Update available. A newer build (9f8e7d6) is ready from 2026-11-28. (DEMO)",
    current: { version: "0.1.0", sha: "a1b2c3d", shortSha: "a1b2c3d", branch: "claude/trader-build-prompt-v2-9pmbsi", builtAt: 1795700000, builtAtText: "2026-11-27 (DEMO)" },
    latest: { sha: "9f8e7d6", shortSha: "9f8e7d6", url: "https://github.com/Drewzyy/Ps5Bot/actions", kind: "build", title: "DEMO build", createdAt: "2026-11-28T09:00:00Z", createdUnix: 1795900000 },
    steps: [
      "Press Open download page. Sign in to GitHub if it asks.",
      "Scroll to Artifacts at the bottom and download claude-trader-windows. Unzip it.",
      "Close Claude Trader, then run the new setup file from the zip. Your settings and trade history stay.",
    ],
    demo: true,
  };
}

/** DEMO X Monitor view for screenshots. Every handle, number and post here is made up. */
export function demoXmon(): XMonitorView {
  const post = "https://x.com/alphacalls/status/1861500000000000001";
  return {
    ts: NOW, enabled: true, paused: false, mode: "ready", state: "working",
    statusLine: "Checked 34 accounts this hour, 2 rising",
    thesisLine: "1 narrative trending: Same post (3 coins)",
    extAlive: true, loggedIn: true, pausedReason: "", pausedUntilMs: 0, loadsLastHour: 71, maxPerHour: 120,
    jobs: { queued: 1, running: 1, done: 48, failed: 3, stale: 2 },
    checkedHour: 34,
    rising: [
      { handle: "moonbearsol", name: "Moon Bear", url: "https://x.com/moonbearsol", followers: 8431, gain: 2100, gainPct: 33.2, minutes: 38, why: "up 33% in 38 minutes", symbol: "MOONB" },
      { handle: "glowbear_x", name: "Glow Bear", url: "https://x.com/glowbear_x", followers: 3120, gain: 0, gainPct: 0, minutes: 0, why: "about 1,560 followers a day", symbol: "GLOWB" },
    ],
    latest: [
      { ts: NOW - 40_000, mint: "DEMO1", symbol: "MOONB", kind: "profile", handle: "moonbearsol", url: "https://x.com/moonbearsol", linkLabel: "X profile", status: "ready", score: 72.4, points: 11, reason: "", flags: ["6 smart follower(s) (@cobie_fan_acc, @solana_whale_x, @degen_daily)"], blocks: [], rank: 0, clones: 0 },
      { ts: NOW - 95_000, mint: "DEMO2", symbol: "COPY2", kind: "status", handle: "alphacalls", url: post, linkLabel: "X post", status: "blocked", score: 61.0, points: 0, reason: "Copy #6 of 7 coins linking the same post.", flags: [], blocks: ["Copy #6 of 7 coins linking the same post. Only the first 5 count."], rank: 6, clones: 7 },
      { ts: NOW - 150_000, mint: "DEMO3", symbol: "PEPEX", kind: "profile", handle: "pepex", url: "https://x.com/pepex", linkLabel: "X profile", status: "blocked", score: 0, points: 0, reason: "This X account used to be @pepe_old (same account id). It was renamed.", flags: [], blocks: ["This X account used to be @pepe_old (same account id). It was renamed."], rank: 0, clones: 0 },
      { ts: NOW - 210_000, mint: "DEMO4", symbol: "GLOWB", kind: "community", handle: "", url: "https://x.com/i/communities/1861700000000000007", linkLabel: "X community", status: "unknown", score: 0, points: 0, reason: "X data is not ready yet (the page is waiting its turn).", flags: [], blocks: [], rank: 1, clones: 1 },
    ],
    stories: [
      { mint: "DEMO1", symbol: "MOONB", name: "Moon Bear", ts: NOW - 40_000, kind: "profile", url: "https://x.com/moonbearsol", handle: "moonbearsol", author: "moonbearsol", text: "gm bears. building in public, no promises. $MOONB", reach: 8431, views: 9800, tweetAgeMin: null, rank: 0, clones: 0, cluster: "author:moonbearsol" },
      { mint: "DEMO5", symbol: "COPY1", name: "Moon Bear 1", ts: NOW - 300_000, kind: "status", url: post, handle: "alphacalls", author: "alphacalls", text: "New one to watch: $MOONB, the bear that went to the moon. CA below.", reach: 182000, views: 215000, tweetAgeMin: 25, rank: 1, clones: 3, cluster: "tweet:1861500000000000001" },
      { mint: "DEMO6", symbol: "COPY2", name: "Moon Bear 2", ts: NOW - 240_000, kind: "status", url: post, handle: "alphacalls", author: "alphacalls", text: "New one to watch: $MOONB, the bear that went to the moon. CA below.", reach: 182000, views: 215000, tweetAgeMin: 29, rank: 2, clones: 3, cluster: "tweet:1861500000000000001" },
    ],
    narratives: [
      { key: "tweet:1861500000000000001", label: "Same post", kind: "post", url: post, count: 3, symbols: ["COPY1", "COPY2", "COPY3"], firstMs: NOW - 300_000, lastMs: NOW - 120_000, text: "New one to watch: $MOONB, the bear that went to the moon. CA below." },
    ],
    demo: true,
  };
}

/** DEMO extension link. ?ext= connected | ok | offline | unpaired (default) | updated | reload | reloading. */
export function demoExtension(mode: string | null): EngineEvent {
  const base = { type: "extension" as const, ts: NOW, paired: true, connected: true, lastSeenMs: Date.now(), version: "0.1.42", diskVersion: "0.1.42", appVersion: "0.1.42", heartbeatMs: Date.now() - 3000, link: "ok" as const, updatedTo: "", updatedAt: 0, pagesTs: Date.now() - 5000, pages: [] as PageStatus[] };
  const mixed: PageStatus[] = [
    { id: "pump-home", label: "pump.fun home", state: "open", url: "https://pump.fun/" },
    { id: "x", label: "x.com (logged in, for the X Monitor)", state: "loggedout", url: "https://x.com/home" },
    { id: "dex-MOONCAT", label: "Dexscreener chart: MOONCAT", state: "missing", url: "https://dexscreener.com/solana/DEMO" },
  ];
  const allOpen = mixed.map((p) => ({ ...p, state: "open" as const }));
  switch (mode) {
    case "connected":
      return { ...base, pages: mixed };
    case "ok":
      return { ...base, pages: allOpen };
    case "offline":
      return { ...base, connected: false, heartbeatMs: Date.now() - 95_000, pages: allOpen };
    case "updated":
      return { ...base, pages: allOpen, updatedTo: "0.1.42", updatedAt: Date.now() - 20_000 };
    case "reloading":
      return { ...base, version: "0.1.41", link: "reloading", pages: allOpen };
    case "reload":
      return { ...base, version: "0.1.41", link: "manual", pages: allOpen };
    default:
      return { type: "extension", ts: NOW, paired: false, connected: false, lastSeenMs: 0, version: "", diskVersion: "0.1.42", appVersion: "0.1.42", heartbeatMs: 0, link: "ok", updatedTo: "", updatedAt: 0, pages: [], pagesTs: 0 };
  }
}

export function demoEvents(): EngineEvent[] {
  const ev: EngineEvent[] = [demoExtension(typeof window !== "undefined" ? new URLSearchParams(window.location.search).get("ext") : null)];
  const coins: [string, string, string, string, string][] = [
    ["MOONCAT", "Moon Cat", "https://example.org/mooncat", "https://x.com/mooncat", "https://t.me/mooncat"],
    ["PEPEX", "Pepe X", "", "https://x.com/pepex", ""],
    ["GLOWB", "Glow Bear", "", "", "https://t.me/glowb"],
    ["NOLINK", "No Link Coin", "", "", ""],
  ];
  coins.forEach(([sym, name, web, x, tg], i) => {
    const mint = `DEMO${i + 20}`.padEnd(44, "y");
    const ts = NOW - (i + 1) * 40_000;
    ev.push({ type: "token", ts, mint, symbol: sym, name, creator: "creator", mcapSol: 30 });
    if (web || x || tg) ev.push({ type: "links", mint, website: web, twitter: x, telegram: tg });
    ev.push({ type: "verdict", ts: ts + 5000, mint, symbol: sym, stage: "chain", outcome: "pass", reason: "", detail: "11 checks done, none blocked.", verdict: "LOW_SIGNAL", evidence: [] });
    ev.push({ type: "activity", ts: ts + 1000, agent: "scam-shield", text: `DEMO: ${name} handed to Scam Shield`, mint, sourceUrl: web || x || tg || null });
  });
  ev.push({ type: "skipped", executor: "paper", checks: [
    { name: "Honeypot sell test", why: "Needs a real wallet that holds the coin and a real sell. Paper cannot do this, so it is skipped. Live mode will run it before every buy." },
    { name: "Real transaction errors", why: "Paper imitates failed and slow trades with random chances. Real ones can fail for other reasons." },
    { name: "Other buyers racing you", why: "In paper your buy does not move the price for anyone else and nobody beats you to a fill." },
  ] });
  ev.push({ type: "learning", ...demoLearning });
  ev.push({ type: "x-monitor", ...demoXmon() });
  ev.push({ type: "status", agent: "x-monitor", color: "green", line: "Checked 34 accounts this hour, 2 rising" });
  ev.push({ type: "status", agent: "thesis-finder", color: "green", line: "1 narrative trending: Same post (3 coins)" });
  // ?nodata=1 shows the "no buy/sell data" warning, otherwise a healthy feed. Both are DEMO numbers.
  const nodata = typeof window !== "undefined" && new URLSearchParams(window.location.search).get("nodata") === "1";
  ev.push(nodata
    ? { type: "status", agent: "memecoin-monitor", color: "red", line: "No buy/sell data arriving: every coin scores 0" }
    : { type: "status", agent: "memecoin-monitor", color: "green", line: "Live. 142 trades/min on 6 coins" });
  ev.push(nodata
    ? { type: "feed-health", ts: NOW, coinsConnected: true, tradesConnected: true, watched: 6, subsActive: 0, tradesLastMin: 0, tradesTotal: 0, lastTradeMs: 0, lastError: "DEMO: the node answered that this subscription is not allowed on the plan", warn: "No buy/sell data is arriving, so every coin scores 0.", rugRequests: 40, rugFailures: 17 }
    : { type: "feed-health", ts: NOW, coinsConnected: true, tradesConnected: true, watched: 6, subsActive: 6, tradesLastMin: 142, tradesTotal: 5310, lastTradeMs: NOW - 2000, lastError: "", warn: "", rugRequests: 40, rugFailures: 17 });
  ev.push({
    type: "funnel", ts: NOW, demo: true, seen: 321, blocked: 292, inProgress: 3, scored: 26, scoredNoTrades: 9, bestScore: 91, bestSymbol: "MOONCAT", minScore: 80,
    blockedReasons: [
      { reason: "UNDECIDED_RUGCHECK", count: 31 }, { reason: "RUGCHECK_DANGER", count: 80 }, { reason: "CREATOR_HOLDING", count: 49 },
      { reason: "NO_IMAGE", count: 42 }, { reason: "METADATA_MISSING", count: 13 }, { reason: "DEV_SNIPE", count: 5 },
    ],
    bestBlockedBy: [{ reason: "Not enough SOL in the curve", count: 5 }, { reason: "Too few buyers in the last 30 s", count: 3 }, { reason: "Buy/sell ratio too low", count: 2 }],
    picked: 1, bought: 1, rugFallbackPassed: 12, rugFallbackBlocked: 6,
  });
  return ev;
}
