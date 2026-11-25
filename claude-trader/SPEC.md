# Claude Trader — SPEC

Status: Phase 0 output. Source: build prompt v2 (2026-11-25) plus user answers given 2026-11-25.
Build from this file in a fresh session, one milestone at a time. Anything here overrides the build prompt where they differ.

## 1. Goal

A Windows desktop app (Tauri: Rust backend, web frontend) that watches every new pump.fun launch in real time, scam-checks it, scores it, buys the good ones, manages each position (staged exits, stops, max hold), sells, and logs everything. It runs locally, starts with Windows, lives in the system tray. Paper mode is the default and the only mode until Milestone 4 is approved by the user.

The user is not technical. Every label, hint and error is plain language. The dashboard shows real P/L, real fees and real stats, losing days included.

## 2. Location and repo layout

- The repo already holds an unrelated PS5 bot (README.md, PS5bot-master.zip). Leave those files alone.
- All Claude Trader code lives in `claude-trader/`.
- `claude-trader/STATUS.md` is the handoff doc, updated at the end of every milestone.
- `claude-trader/paper-logs/` holds paper-trade JSONL (gitignored).
- `claude-trader/config.json` holds the strategy config (section 6).

## 3. Missing inputs (known gaps)

The build prompt references files that are not in this environment:
- `~/workspace/trading-bot/strategy-videos.md` (playbook + full starting config)
- `~/workspace/trading-bot/pumpfun-scalper-spec.md`
- `~/workspace/trading-bot/research-repos/` (3 cloned reference repos)

The research log was supplied in chat and its contents are folded into this spec. For the reference repos, the builder clones them read-only into `claude-trader/.reference/` (gitignored) when needed:
- github.com/chainstacklabs/pumpfun-bonkfun-bot
- github.com/slightlyuseless/memecoin-trading-bot
- github.com/DoradoDevs/solana-pumpfun-sniper-bot (single-commit repo: verify any code before trusting it)

If the playbook file turns up later, any value in it that conflicts with section 6 goes back to the user as a question. Section 6 wins until then.

## 4. Stack (free only)

| Piece | Choice |
|---|---|
| Shell | Tauri 2 (Rust core, TypeScript + React frontend) |
| New launches | PumpPortal WS `wss://pumpportal.fun/api/data`, one connection only (`subscribeNewToken`, `subscribeMigration`) |
| Backup feed | Anaxer free tier |
| Prices | Dexscreener `GET /latest/dex/tokens/{mint}` + local bonding-curve math |
| RPC | Helius free tier (1M credits/mo, 10 req/s) |
| Scam data | RugCheck API (3 req/s), Helius DAS, Birdeye free tier as backup (1 req/s, 30K CU/mo), Solscan free key |
| Curve execution | PumpPortal trade-local, signed locally (M4 only) |
| Migrated execution | Jupiter Swap API v2 (M4 only) |
| Landing | Helius send + priority fee; Jito bundles only for launch+buy atomicity (M4 only) |
| X signals | twscrape (no paid X API) |
| Alerts | Telegram Bot API, outbox-only messages to the user |
| Storage | SQLite (positions, decisions, wallet history) + JSONL trade log |

No LLM calls anywhere in the trade path. No paid API, gated server or private signal feed.

## 5. Pipeline

discovery → cheap prefilters (no RPC) → Scam Shield chain checks → observation window → score → entry rule → portfolio/risk gates → buy → persisted position → exit engine → sell → log.

Every rejection at any stage is written to the decisions table with stage + reason, and shown in the Watchlist.

## 6. Strategy config (contract)

Confirmed values. User decisions from 2026-11-25 are marked [user].

```json
{
  "mode": "paper",
  "paper": { "bankrollSol": 10 },
  "feed": { "minMarketCapUsd": 7000, "launchpads": ["pumpfun"] },
  "filters": {
    "mintAuthorityRevoked": true,
    "freezeAuthorityRevoked": true,
    "maxCreatorHoldingPct": 15,
    "maxInsidersPct": 20,
    "maxSingleHolderPct": 5,
    "maxTopHoldersCombinedPct": 15,
    "excludeCurveAndPoolAccounts": true,
    "rejectDevSnipeFirstBlock": true,
    "rejectBundle": true,
    "requireCommunityChecks": { "minSocialLinks": 1, "requireImage": true }
  },
  "observation": {
    "minScore": 80, "minUniqueBuyers": 5, "minBuySellRatio": 2.0, "minPriceChangePct": 5
  },
  "entry": {
    "style": "momentum",
    "minBuySellRatio": 2.0,
    "minUniqueBuyers30s": 5,
    "requireRisingVolumeSlope": true,
    "curveSolMin": 12, "curveSolMax": 40,
    "minHolders": 15,
    "minAgeSeconds": 60,
    "paperVariant": { "style": "dip-40pct", "minDipPct": 40, "maxDipPct": 50, "confirmVolumeExpanding": true }
  },
  "exits": {
    "tranches": [
      { "atPct": 20, "sellPct": 40 },
      { "atPct": 40, "sellPct": 30 },
      { "atPct": 60, "sellPct": 20 }
    ],
    "trailingStopPct": 10,
    "softStopPct": -10,
    "softStopRecoverySeconds": 15,
    "hardStopPct": -15,
    "sellBelowAverageEntry": true,
    "maxHoldSeconds": 300,
    "stallExit": { "fromSeconds": 60, "toSeconds": 90, "minPct": 3 },
    "bleederExit": { "afterSeconds": 120, "belowPct": -20, "peakNeverAbovePct": 20 },
    "volumeTaper": { "windowSeconds": 10, "decliningWindows": 3, "lastBelowBaselinePct": 50, "sellPct": 50 },
    "momentumDeath": { "afterHoldSeconds": 90, "lookbackSeconds": 45, "minBuysPerMinute": 0.5 },
    "whaleSellPctOfCurve": 3,
    "exitOnDevSell": true,
    "sellOnMigration": true,
    "exitLoopMs": 1000
  },
  "fees": {
    "modelPumpFunPctPerSide": 1.25,
    "pumpPortalPctPerSide": 0.5,
    "priorityMicroLamports": 150000,
    "computeUnits": 150000,
    "pumpPortalPriorityFeeSol": 0.0005,
    "pumpPortalPriorityFeeCapSol": 0.005
  },
  "paperFill": {
    "buyFailPct": 10, "sellFailPct": 15,
    "delayMsMin": 400, "delayMsMax": 1200,
    "slippageCapPct": 15
  },
  "risk": {
    "maxPctBankrollPerTrade": 1.0,
    "minTradeSol": 0.05, "maxTradeSol": 0.25,
    "maxOpenPositions": 5, "maxTotalExposurePct": 5,
    "dailyLossLimitPct": 7.5,
    "consecutiveLossStop": 3,
    "reduceSizeAfterWins": 3,
    "cooldownSecondsPerToken": 300,
    "liveMinSolReserve": 0.02
  }
}
```

Rules:
- Stop-loss [Claude, 2026-11-25, replaces the earlier -20/-25 choice]: soft stop -10% starts a 15 s recovery wait; hard stop -15% sells at once (checked every 1 s). Tightened because fees model at ~3.5% per round trip and the user asked for minimal losses; -15% hard stops are used by gmgn-screener (high-risk entries), OilFront and the Capital repo config.
- Style [user, 2026-11-25]: quick scalps. Every position is closed within 5 minutes, no exceptions.
- Take-profit [user]: 40% at +20%, 30% at +40%, 20% at +60%, last 10% on a 10% trailing stop from the peak.
- Holder concentration [user]: single holder ≤5%, top holders combined ≤15%, with bonding-curve and pool accounts excluded from both numbers.
- Max hold [user]: 5 minutes (300 s). Anything still open is sold in full.
- Fade exits [user] (from the Capital repo's smart_exit.py, untested there since its engine is never called): stall (under +3% between 60 and 90 s: sell all); bleeder (past 120 s, below -20%, peak never above +20%: sell all); volume taper (3 falling 10 s buy-volume windows and the last under 50% of baseline: sell 50%); momentum death (after 90 s, fewer than 0.5 buys/min over 45 s: sell all); whale sell of 3% or more of the curve, or any dev sell: sell all at once.
- Exit loop runs every 1 s on curve-account WebSocket pushes, RPC fallback.
- Fees: pump.fun 0.30% per side plus PumpPortal 0.5% per side plus priority fee. TP1 at +20% must still clear break-even at the chosen size; the break-even gate rejects the buy if not.
- Size: 1% of bankroll per trade, clamped to 0.05–0.25 SOL. Fixed size, no doubling after a loss, never add to a loser.
- `entry.style` options: `momentum`, `snipe`, `confirmation`, `dip-40pct`, `graduation-dip`. Default `momentum` [Claude]: the only entry with any track record found (Capital repo: 458 self-reported live trades, buy/sell ratio and volume slope were the strongest predictors; score gate gave 41.9% win rate). `dip-40pct` runs as a parallel paper-only variant so the two can be compared; live uses whichever wins in paper. The observation-window gate applies to every style.
- Break-even check: before a buy, compute break-even % for the trade size (fees, tips, slippage). Reject the buy if TP1 is not above break-even with margin.
- Daily loss limit halts trading for the rest of the local day. 3 stopped trades in a row also halts. After 3 wins in a row, halve size for the next trade.
- Scam blocks are automatic with no override: honeypot, creator holding over the limit, dev-snipe in first block, live mint/freeze authority, bundled/fake holders.
- Top-trader wallets: off in v1 (no free, verified smart-money source exists). `minProTraders` is dropped. Later version may build a local list from wallets with 3+ early buys on graduated tokens. Never copy-trade.
- Community check [Claude]: at least one social link and an image in metadata. Weak hard filter only, not a score driver (link presence is easy to fake). All metadata fields logged so their predictive value can be measured in paper.
- Fees [Claude]: model pump.fun at 1.25% per side (conservative end of conflicting sources), PumpPortal 0.5% per side, so ~3.5% per round trip plus priority fee. Verify both against live docs before M4.
- Volume [Claude]: the bot scans every launch nonstop but only buys what clears every gate. Up to 5 positions at once, 300 s cooldown per token.

## 7. Scam Shield

- Check order: cheap no-RPC checks first, chain checks just before buying. Each check has a 2 s budget and a `required` flag; a required check that times out or is undecided blocks the token.
- New mints: poll at `processed` commitment (8 × 750 ms) since they aren't visible at `confirmed` yet.
- Checks: RugCheck report, honeypot (sell simulation), mint/freeze authority, creator holding %, holder concentration (section 6), dev-snipe in first block, bundler rules (≥20 transfers from ≤5 payers; ≥15 burst transfers in one block; ≥80% supply from ≤3 wallets; tag ACTIVE if within last 15 s, else HISTORICAL), near-identical top-holder balances or clustered funding dates (one entity), liquidity.
- Rug detector on open positions, 1 s loop, points: holder exodus >30% (30), top-10 spike >50% (30), LP fully removed (100), liquidity drain >50% (30), entrapment >15% (20), creator >90% supply (25), fresh wallets >95% (15), price drop >40% (50), price-impact spike >3× in 60 s (40). Response: 10–29 watch, 30–49 warn, 50–74 sell 50%, 75+ full exit. Jupiter `NO_ROUTES` = exit at once.
- Output: one Exit-Liquidity Risk verdict (LOW_SIGNAL / ELEVATED / HIGH) with evidence links (Solscan tx URLs).

## 8. Paper mode

- Same rules engine as live; only the executor differs.
- Fills: bonding-curve constant-product formula, plus 0.5% PumpPortal fee, SOL tx fee, priority-fee estimate, price impact, a fill delay, and a failed-fill rate. Never mark at mid-price.
- One JSONL line per closed trade in `paper-logs/YYYY-MM-DD.jsonl`, exactly this shape:

```json
{"ts":"2026-11-25T15:02:11-08:00","mint":"7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgHR","symbol":"MOON","side":"buy","sizeSol":0.2,"entryPrice":0.00042,"entryMcapUsd":12400,"exitPrice":0.00081,"exitReason":"take-profit-1","pnlSol":0.176,"pnlPct":88.0,"holdSeconds":112,"feesSol":0.0042,"mode":"paper"}
```

- Daily summary: win rate, avg win, avg loss, profit factor, max drawdown, total PnL, trades per day.
- Promotion to live: at least a few days and ~50 trades, stats reviewed with the user, positive after fees and slippage.

## 9. Dashboard and UI

Theme: white background, white cards, green accents (#16a34a primary, #14532d text/headers, light green tints). Red only for losses, sells, danger. No dark mode. No other colors.

- Top bar: local clock; trades today (hover: buys/sells); SOL balance + USD portfolio value; P/L today and total (SOL + USD, green/red); mode badge `PAPER` (green outline) or `LIVE` (solid red).
- Network graph: central Claude bot node (larger, app name under it) with 9 agent nodes: Scam Shield, Token Picker, Executor, Hold Timer, Thesis Finder, Memecoin Monitor, X Monitor, Top Trader Monitor, Top Gainers. Each has icon, name, status dot (green working / amber waiting / grey idle / red blocked), one-line live status. Idle lines thin light grey; on a message the line pulses green with a travelling dot. Click a node for a panel: current task, last 10 decisions, settings.
- Activity feed under the graph: timestamped, newest first, clickable to token detail.
- Widgets: Watchlist, Open Positions, Position Chart (candles, green entry marker, red exit marker, red dashed SL line, green dashed TP lines, live price line), Scam Shield (blocked count + breakdown by reason + verdict), Fees Paid (today/all-time, split PumpPortal / tx / priority / Jito, per-trade fee), Performance (win rate, avg win/loss, profit factor, max drawdown, best/worst, trades/day, equity curve), Settings page.
- Every setting has an ⓘ with a plain explanation: what it does, good value, how to set it up.
- UI updates throttled to a few per second max.

## 10. Setup and onboarding

- First-run walkthrough with real download/signup links (Helius signup, Telegram @BotFather, etc.) and a green check per completed step.
- Wallet (M4 only): generated in-app, address + funding screen, plain warning to never use a main wallet.
- Start-with-Windows toggle and tray icon.

## 11. Safety (enforced in code and config)

- Paper wallet only until M4 is approved. No real-wallet or signing code before M4.
- Private key only in OS secure storage (Windows Credential Manager via keyring) or an encrypted local file. Never in git, logs, chat, or any API other than local signing.
- RPC URLs and API keys masked in every log line.
- Main/Axiom wallet never connected.
- Daily loss halt can't be lifted from the dashboard without the typed confirmation.
- `.claude/settings.json` with deny rules (no reading the key file / keystore paths) and a Stop hook that fails if a live-trade call path exists outside the Executor's live gate. Created in M1.
- Sell-path invariants: SUCCESS / REVERTED / UNCONFIRMED (re-check chain before re-selling); only SUBMIT_FAILED retries; never store 0.0 as last-known price.
- Every real send simulated first.

## 12. Out of scope

Live trading before the M4 gate; real keys in repo/logs/fixtures; paid APIs (Birdeye streaming, LaserStream, Solscan Pro, Jupiter Developer, X API reads); Axiom; copy-trade execution; auto-posting to X/Telegram; EVM, leverage, perps; anything needing a gated server or paid signal feed.

## 13. Milestones and acceptance criteria

One git commit per milestone. A milestone is done only when every box is checked by a command that was run, with output quoted in STATUS.md.

**M1 — Scaffold + dashboard shell**
- [ ] Build command exits 0 (output quoted).
- [ ] Screenshot of the running UI shows theme, top bar with PAPER badge, central node + 9 agents with idle grey lines, empty activity feed, 7 widget cards.
- [ ] Settings page: every setting has a working ⓘ.
- [ ] `.claude/settings.json` deny rules + Stop hook in place.
- [ ] No paid key needed.
Note: this container is Linux. Build and screenshot the web frontend here (Vite + Playwright); the Windows Tauri bundle is built via a GitHub Actions `windows-latest` job, and its exit status is quoted.

**M2 — Feed + agents + Scam Shield**
- [ ] 10 consecutive live new-token events with mint + timestamp, no reconnect errors (log lines quoted). If the live feed is blocked from this environment, replay a recorded session and say so.
- [ ] A known-bad token (bundle or live mint authority) blocked with the right reason.
- [ ] A known-clean token passes and reaches the Picker with a score.
- [ ] Every skip logged with reason.
- [ ] Agent lines pulse on handoff; feed logs it.

**M3 — Paper trader + widgets**
- [ ] One full paper trade (signal → fill → staged exits) completes; JSONL line matches section 8 exactly.
- [ ] Daily summary reconciles with JSONL by hand recompute.
- [ ] Restart mid-position: position survives with same entry and P/L.
- [ ] Position chart screenshot shows entry, SL, TP markers.
- [ ] Unit tests for: 2-layer stop timing, TP ladder, 10-min max hold, break-even gate, daily loss halt, consecutive-loss halt.

**M4 — Live executor (gated; only after user approves paper stats)**
- [ ] Gate blocks live mode without typed confirmation (demonstrated).
- [ ] One real 0.05 SOL buy + sell, signatures logged and checkable on Solscan.
- [ ] Daily loss limit halt proven by test, not real money.
- [ ] Key scan over files and git history finds no key (output quoted).
If live isn't approved, stop after the gate demo.

**M5 — Final verification**
- [ ] Clean-checkout build + tests pass (quoted).
- [ ] Fresh subagent reviews against this SPEC, rechecking paper-only, no-keys, free-APIs-only rules; all gaps fixed and re-verified.
- [ ] Setup walkthrough completes; every ⓘ opens a correct explanation; start-with-Windows and tray work.

## 13b. Research basis

Sources for the scalp values: kontia1/gmgn-screener README (stops, ladder shape); moonkeemoo/oilfront research doc (fees, base rates: win rate went 22% to 36% only after adding filters); madeonsol surge tracker, search snippet only (65 surges, median peak 1.45x at ~4 min, 60% peaked within 5 min); DoradoDevs repo smart_exit.py (fade exits). Medium, alchemii.io and the vercel counter-study were blocked by the network and are unread.

## 14. Open items

Trading values are decided by Claude from research (user delegated this on 2026-11-25). Only these need the user:
1. Before M2: free Helius API key.
2. Before M4: approval of paper stats; live per-trade size.
3. Before M4: confirm pump.fun and PumpPortal fees against their docs (hosts were blocked during research).

Paper-fill numbers (fail rates, delay) are conservative estimates with no published source; recalibrate from real M4 fills.
