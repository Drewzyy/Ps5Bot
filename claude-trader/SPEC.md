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

## 4b. Paper is practice for live (same rules, one code path)

Entry gates, Scam Shield, the Token Picker, the exit rules and the risk gates are ONE code path. Only the executor differs: `engine/src/exec.rs` defines the `Executor` trait (`quote_buy`, `buy`, `sell`, `skipped_checks`). `PaperExecutor` fills against the bonding-curve math with simulated delay and failures. The future live executor lives only under `engine/src/live/` behind the M4 gate and does not exist yet. There is no paper-only shortcut that changes a decision (audited 2026-11-27: `replay` only swaps the data source, `relax_egress` is a sandbox-only flag that is off in the app and is listed in the UI when on).

Things paper cannot run are listed in the UI as "skipped in paper", never silently different: the honeypot sell test (needs a real wallet and a real sell), real transaction errors, other buyers racing you. Test `trader::tests::same_market_same_decisions_with_paper_and_scripted_executors` runs one scripted market through the real entry gates, risk gates and exit loop with the paper executor and a second executor and asserts the decisions are identical. The `dip-40pct` lane is a paper-only comparison lane; it never changes what the main lane does, and the live executor only gets the main lane.

## 4c. Connections, updates, learning (added 2026-11-27)

- **Helius key** (`engine/src/secrets.rs`): kept in the OS vault (Windows Credential Manager through the `keyring` crate), encrypted file fallback (ChaCha20-Poly1305, key from a random salt plus user and computer name; weaker than the vault, so only a fallback). Never in config.json, logs, git or the page after saving; the UI gets only "saved, ends in ...ab12". The engine reads it at start and on save (`reload-rpc` hot swaps the RPC and the trade feed WebSocket); `HELIUS_API_KEY` and `CT_RPC_URL` override it. URLs in logs are masked.
- **Updates** (`engine/src/update.rs`): the app embeds the git SHA and build time at build time (`GITHUB_SHA`, else `git rev-parse`). "Check for updates" asks the public GitHub API for the newest successful `windows-build` run on the app's branch (else the latest release), compares with GitHub's compare endpoint (falls back to timestamps), and opens the run page in the browser. No token.
- **Learning journal** (`engine/src/learning.rs`): every lesson comes from recorded numbers; nothing is invented. A journal entry per closed trade; a review every 30 closed main-lane trades (groups by curve SOL, buy/sell ratio, buyers, score, exit reason; a pattern needs at least 10 trades in the group and 10 in the rest and a win-rate gap of at least 25 points); the proposal runs in a shadow paper lane for the next 30 main-lane trades and is applied only if it beats the main lane on net P/L and profit factor with at least 10 shadow trades. Allowed changes: a whitelist of entry/exit thresholds and the five score weights, never more than 20% from the shipped value. Never Scam Shield, trade size, fees or loss limits. Every change is logged with before/after, evidence and result and has Undo. In live mode a winning change waits for the user's approval. Efficiency notes: pick-to-fill time, failed-transaction cost, fee share of gross profit, missed coins (score 80 or more but blocked, with the next 5 minutes of price when seen).
- **Source links**: wherever the app shows a reason or signal from outside, it stores and shows a clickable `sourceUrl`. Today that is the coin's own website, X and Telegram from its metadata. X Monitor and Thesis Finder are not built (the X API is not free), so the UI only has the slot ("Source: post link"); no data is faked.

## 4d. Chrome extension and browser startup (added 2026-11-27)

An optional Manifest V3 Chrome extension (`extension/`) works with the desktop app. Paper only, no keys, no signing.

- **Link.** A dedicated local WebSocket, `ws://127.0.0.1:8766` (`engine/src/bridge.rs`, `--ext-port` on engine-cli, always on in the app). The dashboard socket (8765) is unchanged and still refuses extension origins.
- **Origin.** The handshake is refused unless `Origin` is exactly `chrome-extension://jihnmdnpkeglfhoombfjhdfbmiopnljj`. The ID comes from the public `key` pinned in `extension/manifest.json`; a test checks the two agree. A missing or any other origin gets HTTP 403.
- **Pairing.** The app shows a one-time code (8 digits, valid 10 minutes, one use, stored only as a hash). The extension swaps it for a random 256-bit token that it keeps in `chrome.storage.local`; the app keeps only the token hash (`kv ext.token`). Five wrong tries in a row lock sign-in for 60 s. Unpair deletes the token. Reason for a code instead of a shipped secret: a secret inside the extension folder would be readable by anything on the PC and could not be revoked.
- **What the extension gets.** The same public events the dashboard shows (activity, positions without candle ticks, stats, kill state, learning journal) and the Startup settings. Never the Helius key or any secret.
- **What it can ask.** `verdict` (what the bot thinks of a coin: held, watching, skipped, blocked, closed, unknown, with Scam Shield result and score), `check` (Scam Shield checks only), and the commands `kill` and `resume`. There is no buy or sell message. Any other command is refused. `check` uses `Shield::manual_check` (mint account, RugCheck, holders), saves a decision with stage `manual-check` and has no handle to the trader, so it cannot open a position. The bot trades only by its own rules (Token Picker, risk gates). Tests: `bridge::tests`.
- **Pages.** pump.fun coin pages and dexscreener.com/solana pages show a small card. The coin address comes from the URL, a token link on the page, or the public Dexscreener pair API.
- **Tabs.** The extension opens pump.fun (once per browser session) and the Dexscreener chart of each coin bought, reuses tabs that already exist, and closes only tabs it opened, only when the setting is on and the coin has no other open trade.
- **Startup and browser (Settings).** Open Chrome at launch (chrome.exe in `%ProgramFiles%`, `%ProgramFiles(x86)%`, `%LocalAppData%`, else the default browser; skipped when the extension is already connected), pages to open, close a coin's tab on close, start with Windows (tauri-plugin-autostart), keep the PC awake while the app runs (`SetThreadExecutionState`, released on exit). Stored in SQLite `kv startup.settings`.
- **Install.** Developer mode > Load unpacked, once (Chrome cannot install extensions for an app). Steps are in the app and in `extension/README.md`. CI uploads the folder as `claude-trader-chrome-extension`; the installed app also carries the folder as a resource.

## 5. Pipeline

discovery → cheap prefilters (no RPC) → Scam Shield chain checks → observation window → score → entry rule → portfolio/risk gates → buy → persisted position → exit engine → sell → log.

Every rejection at any stage is written to the decisions table with stage + reason, and shown in the Watchlist.

## 6. Strategy config (contract)

Confirmed values. User decisions from 2026-11-25 are marked [user].

```json
{
  "mode": "paper",
  "paper": { "bankrollSol": 10 },
  "feed": { "minMarketCapSol": 30, "launchpads": ["pumpfun"] },
  "filters": {
    "mintAuthorityRevoked": true,
    "freezeAuthorityRevoked": true,
    "maxCreatorHoldingPct": 10,
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
    "curveSolMin": 4, "curveSolMax": 40,
    "minHolders": 15,
    "maxCreatorSoldPctOfSupply": 1.0,
    "maxCreatorSoldPctOfHolding": 25.0,
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
    "pumpFunCreatorPctPerSide": 0.30,
    "pumpPortalPctPerSide": 1.0,
    "priorityMicroLamports": 150000,
    "computeUnits": 150000,
    "pumpPortalPriorityFeeSol": 0.0005,
    "pumpPortalPriorityFeeCapSol": 0.005,
    "jitoTipSol": 0
  },
  "paperFill": {
    "buyFailPct": 10, "sellFailPct": 15,
    "delayMsMin": 400, "delayMsMax": 1200,
    "slippageCapPct": 15
  },
  "risk": {
    "maxPctBankrollPerTrade": 1.0,
    "minTradeSol": 0.1, "maxTradeSol": 0.25,
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
- Stop-loss [Claude, 2026-11-25, replaces the earlier -20/-25 choice]: soft stop -10% starts a 15 s recovery wait; hard stop -15% sells at once (checked every 1 s). Tightened because fees model at ~4.5% per round trip (Fee model rule) and the user asked for minimal losses; -15% hard stops are used by gmgn-screener (high-risk entries), OilFront and the Capital repo config.
- Style [user, 2026-11-25]: quick scalps. Every position is closed within 5 minutes, no exceptions.
- Take-profit [user]: 40% at +20%, 30% at +40%, 20% at +60%, last 10% on a 10% trailing stop from the peak.
- Holder concentration [user]: single holder ≤5%, top holders combined ≤15%, with bonding-curve and pool accounts excluded from both numbers.
- Max hold [user]: 5 minutes (300 s). Anything still open is sold in full.
- Fade exits [user] (from the Capital repo's smart_exit.py, untested there since its engine is never called): stall (under +3% between 60 and 90 s: sell all); bleeder (past 120 s, below -20%, peak never above +20%: sell all); volume taper (3 falling 10 s buy-volume windows and the last under 50% of baseline: sell 50%); momentum death (after 90 s, fewer than 0.5 buys/min over 45 s: sell all); whale sell of 3% or more of the curve, or any dev sell: sell all at once.
- Exit loop runs every 1 s on curve-account WebSocket pushes, RPC fallback.
- Curve SOL minimum [Claude, user-delegated, 2026-11-26]: `entry.curveSolMin` 12 -> 4 (max stays 40). Data from 345 min of live paper (2024 coins, 567 scored, 10 with score >= 80, 0 entries): the 12 SOL gate failed 8 of those 10 (HOMER 5.2, CHLOWIE 9.9, FCAT 11.2, most others 0-5), because real SOL in the curve at 60 s is almost always under 12. 4 SOL still needs real buys, and the 30 SOL market-cap gate, 5 buyers in 30 s, buy/sell >= 2.0 (unchanged), rising volume and Scam Shield still apply.
- Creator-selling entry gate, tiered [Claude, user-delegated, 2026-11-27]: the old gate blocked on ANY creator sale in the 60 s window (`devSold > 0`). Evidence from the live paper run (509 coins scored after tuning): it was the only failing gate for MinerPig (score 100, 63 buyers, 20.5 SOL in the curve) and one of two for Firefly (79.7, 72 buyers, 8.5 SOL), so the best coins of the run were rejected for a tiny sale. New rule: allow entry when the creator sold at most `entry.maxCreatorSoldPctOfSupply` = 1% of total supply AND less than `entry.maxCreatorSoldPctOfHolding` = 25% of their own holding (launch buy plus buys in the window); block otherwise. A creator who sold with no known holding counts as 100% and is blocked. Exit behaviour is unchanged: `exits.exitOnDevSell` still sells an open position at once on any dev sale. The dip-40pct lane keeps its own eligibility check (any sale blocks).
- Chase rule fix [Claude, 2026-11-26]: block if the price at decision time is more than 3x the launch price (initial curve), whatever the age. The old rule only applied at age <= 60 s, but the decision comes after the 60 s window, so it never fired. Falling knife: block entry (pre-buy reason `DUMPED_FROM_PEAK`) if the price is more than 40% below its peak of the observation window. Both apply to the momentum lane only; the dip-40pct paper variant has its own logic and is not affected.
- Market-cap entry gate [user approved 2026-11-26, replaces the USD gate]: `feed.minMarketCapSol` = 30 SOL, compared with market cap in SOL straight from the curve. Reasoning: the old $7000 gate depended on the SOL price. At SOL ~$71 it meant ~98.5 SOL, which can never pass together with the chase rule (block above 3x launch within 60 s; a launch starts near 28 SOL, so 3x is ~84 SOL), and a 55 minute live run made zero picks. A SOL-denominated gate does not move when SOL/USD moves. 30 SOL still skips brand-new coins nobody has bought (they sit near 28 SOL) and leaves a passing band of about 30-84 SOL. No entry gate reads the SOL/USD price; USD is only used for display and the JSONL `entryMcapUsd` field.
- Fees: all fees per side are listed once in the Fee model rule; TP1 must clear their sum. TP1 at +20% must still clear break-even at the chosen size; the break-even gate rejects the buy if not.
- Size: 1% of bankroll per trade, clamped to 0.05–0.25 SOL. Fixed size, no doubling after a loss, never add to a loser.
- `entry.style` options: `momentum`, `snipe`, `confirmation`, `dip-40pct`, `graduation-dip`. Default `momentum` [Claude]: the only entry with any track record found (Capital repo: 458 self-reported live trades, buy/sell ratio and volume slope were the strongest predictors; score gate gave 41.9% win rate). `dip-40pct` runs as a parallel paper-only variant so the two can be compared; live uses whichever wins in paper. The observation-window gate applies to every style.
- Break-even check: before a buy, compute break-even % for the trade size (fees, tips, slippage). Reject the buy if TP1 is not above break-even with margin.
- Daily loss limit halts trading for the rest of the local day. 3 stopped trades in a row also halts. After 3 wins in a row, halve size for the next trade.
- Scam blocks are automatic with no override: honeypot, creator holding over the limit, dev-snipe in first block, live mint/freeze authority, bundled/fake holders.
- Top-trader wallets: off in v1 (no free, verified smart-money source exists). `minProTraders` is dropped. Later version may build a local list from wallets with 3+ early buys on graduated tokens. Never copy-trade.
- Community check [Claude]: at least one social link and an image in metadata. Weak hard filter only, not a score driver (link presence is easy to fake). All metadata fields logged so their predictive value can be measured in paper.
- Fee model [Claude, research 2026-11-26; where sources disagree the higher value is used]. Each component is its own line in the Fees widget and in the JSONL `feeBreakdown`:
  - pump.fun bonding curve: 1.25% per side, flat, no tiers on the curve = 0.95% protocol + 0.30% creator (`modelPumpFunPctPerSide` 1.25 and `pumpFunCreatorPctPerSide` 0.30; code and config use only these). Sources: pump.fun help article "Transaction fees on pump.fun" (intercom.help/pumpfun-web, article 11002413); coinpaprika.com/education/launchpad-fees (cites pump.fun fee docs; says the dynamic market-cap tiers, from 1.25% down to 0.30%, apply AFTER graduation on PumpSwap, which this bot never holds into). Project Ascend / Dynamic Fees V1 (blockworks.co/news/pumpdotfun-fee-model, dextools.io Ascend guide) scaled creator fees by market cap; Cointelegraph reports a revamp (fee sharing across up to 10 wallets) is planned. A coin on the curve at 30-84 SOL market cap is below the cheapest tier cut either way, so 1.25% is the upper bound. One source quotes 0.30% alone: that is the creator part.
  - PumpPortal: 1.0% per side (`pumpPortalPctPerSide`). Sources disagree: Solana Compass (Oct 2026) lists 1% for the Lightning (hosted wallet) API and 0.5% for the Local API, older write-ups say 0.5% flat. Higher value used. M4 uses trade-local, so the real rate may be 0.5%.
  - Solana base fee: 5000 lamports per signature (0.000005 SOL), charged per transaction including reverted ones.
  - Priority fee: 0.0005 SOL per trade (PumpPortal default per its MCP wrapper docs), x2 then x4 on retries, cap 0.005 SOL.
  - Jito tip: `jitoTipSol` 0 (the bot lands trades with PumpPortal and the priority fee; Jito is only for M4 launch atomicity). If set, charged on every transaction including failed ones. Typical tips are 0.001-0.01 SOL [est].
  - Token-account rent 0.00203928 SOL: out of the balance while a position is open, back on close; it is not a fee and not in `feesSol`, but it is in the break-even gate.
  - About 4.5% per round trip in percent fees, plus ~0.00101 SOL fixed per round trip. Verify every rate above against live docs before M4; the creator fee and fee tiers have changed several times in 2025-2026.
- Volume [Claude]: the bot scans every launch nonstop but only buys what clears every gate. Up to 5 positions at once, 300 s cooldown per token.

## 7. Scam Shield

- Check order: cheap no-RPC checks first, chain checks just before buying. Each check has a 2 s budget and a `required` flag; a required check that times out or is undecided blocks the token.
- New mints: poll at `processed` commitment (8 × 750 ms) since they aren't visible at `confirmed` yet.
- Checks: RugCheck report, honeypot (sell simulation), mint/freeze authority, creator holding %, holder concentration (section 6), dev-snipe in first block, bundler rules (≥20 transfers from ≤5 payers; ≥15 burst transfers in one block; ≥80% supply from ≤3 wallets; tag ACTIVE if within last 15 s, else HISTORICAL), near-identical top-holder balances or clustered funding dates (one entity), liquidity.
- Rug detector on open positions, 1 s loop, points: holder exodus >30% (30), top-10 spike >50% (30), LP fully removed (100), liquidity drain >50% (30), entrapment >15% (20), creator >90% supply (25), fresh wallets >95% (15), price drop >40% (50), price-impact spike >3× in 60 s (40). Response: 10–29 watch, 30–49 warn, 50–74 sell 50%, 75+ full exit. Jupiter `NO_ROUTES` = exit at once.
- Output: one Exit-Liquidity Risk verdict (LOW_SIGNAL / ELEVATED / HIGH) with evidence links (Solscan tx URLs).

## 8. Paper mode

- Same rules engine as live; only the executor differs.
- Fills: bonding-curve constant-product formula, plus every cost in the Fee model (section 6), price impact, a fill delay, and a failed-fill rate. Never mark at mid-price. All costs come out of the paper SOL balance: buy spend (holds the buy-side percent fees), fixed fees, fees of reverted/retried transactions, token-account rent 0.00203928 SOL taken on the buy fill and returned only when a full sell closes the account (while open it is shown as "rent locked"), sell-side percent fees on the SOL actually received. The balance change of a closed trade equals `pnlSol` to the lamport (unit test).
- One JSONL line per closed trade in `paper-logs/YYYY-MM-DD.jsonl`, exactly this shape (`feeBreakdown` is the last key; its parts sum to `feesSol`; rent is not a fee):

```json
{"ts":"2026-11-25T15:02:11-08:00","mint":"7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgHR","symbol":"MOON","side":"buy","sizeSol":0.2,"entryPrice":0.00042,"entryMcapUsd":12400,"exitPrice":0.00081,"exitReason":"take-profit-1","pnlSol":0.176,"pnlPct":88.0,"holdSeconds":112,"feesSol":0.00551,"mode":"paper","feeBreakdown":{"pumpFun":0.0019,"creator":0.0006,"pumpPortal":0.002,"tx":0.00001,"priority":0.001,"jito":0.0}}
```

- Daily summary: win rate, avg win, avg loss, profit factor, max drawdown, total PnL, trades per day.
- Promotion to live: at least a few days and at least 100 trades, stats reviewed with the user, positive after fees and slippage.

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
- Start-with-Windows toggle (Settings > Startup and browser, section 4d) and tray icon.

## 11. Safety (enforced in code and config)

- Paper wallet only until M4 is approved. No real-wallet or signing code before M4.
- Private key only in OS secure storage (Windows Credential Manager via keyring) or an encrypted local file. Never in git, logs, chat, or any API other than local signing.
- RPC URLs and API keys masked in every log line.
- Main/Axiom wallet never connected.
- Kill switch [user, 2026-11-26]: a red "Kill switch" button always visible in the top bar (Ctrl+Shift+K, `engine-cli kill --db PATH`) after a confirm ("Stop all trading and sell every open position now?"). The engine at once stops new entries in every lane, cancels a buy that has not been sent, and sells every open position through the normal sell path (paper fees, fail rates, escalating retries of 11b #17; the 1 s loop keeps retrying until each position is closed, with the retry state shown). The state is persisted in SQLite (`kv.killed`), so a restart stays stopped, resumes the selling of any open position and never buys. Closes are logged with exitReason `kill-switch` (JSONL and activity feed). Only a separate "Resume trading" button with its own confirm (or `engine-cli resume`) clears it. Same code path in live mode; the live executor must implement the same sell-all-until-closed behaviour.
- Daily loss halt can't be lifted from the dashboard without the typed confirmation.
- `.claude/settings.json` with deny rules (no reading the key file / keystore paths) and a Stop hook that fails if a live-trade call path exists outside the Executor's live gate. Created in M1.
- Sell-path invariants: SUCCESS / REVERTED / UNCONFIRMED (re-check chain before re-selling); only SUBMIT_FAILED retries; never store 0.0 as last-known price.
- Every real send simulated first.

## 11b. Loss protections (from trap/scam research, 2026-11-25)

Context: Solidus Labs flags ~98.7% of pump.fun tokens as showing pump-and-dump or rug patterns (pump.fun disputes it). The bot treats every token as hostile until every check passes. Values marked [est] are starting values with no published source; paper mode tunes them.

Urgent, before any buy code (M3 paper sim and M4 live):
- pump.fun rotated its fee recipients (pump-fun/pump-public-docs FEE_RECIPIENT_ROTATION.md, cutoff Nov 25). Trades built with old recipients fail. Never hardcode fee recipients or account lists: let PumpPortal trade-local build the transaction, and cross-check against a recent successful on-chain trade. Startup self-test simulates a tiny buy and sell; 3 consecutive simulation failures halt all buying.

Token scams (pre-buy, all required):
1. Mint account parsed on-chain (not RugCheck alone). Reject if: mint authority set, freeze authority set, Token-2022 transferHook with a program, permanentDelegate, transferFee > 0 bps, closeAuthority. (Pump `create_v2` coins are all Token-2022, so inspect extensions, don't reject the program.)
2. Honeypot: simulateTransaction of a sell must succeed before buying.
3. Creator holds no more than 10% [est]. Exit at once if the creator sells more than 25% of their bag [est] (on top of the any-dev-sell exit).
4. Serial ruggers: local SQLite of creator wallets and outcomes. Blacklist a creator after 1 token that fell more than 70% within 5 min [est], and wallets that funded a blacklisted creator.
5. Wash volume: at least 15 distinct buyer wallets in the last 60 s, and the top 3 wallets under 40% of trades [est].
6. Copycats: skip if name or symbol is within Levenshtein 2 of a token seen trending in the last 24 h [est]; skip if the metadata URI is missing or doesn't resolve within 2 s.
7. Chase risk: no entry if the price at decision time is more than 3x the launch price (any age) [est]. 7b. Falling knife: no entry if the price is more than 40% below the observation-window peak [est] (`DUMPED_FROM_PEAK`). Momentum lane only.
8. Bot-bait: never buy in the first 60 s (minAgeSeconds), dev-snipe-first-block block stays on.
9. Graduation: sell everything when market cap reaches 90% of the graduation target [est]. Never hold through migration.
10. Cabal dump: 3 or more sells each above 3% of supply within 10 s means sell all at once [est].
11. Slow rug: sells above 60% of volume for 2 consecutive 30 s windows means sell all [est].

Execution:
12. Slippage cap 5% on buys [est]. Quote must be under 2 s old; re-quote from curve state before signing.
13. Realized slippage monitor: pause buying if average slippage over the last 10 trades exceeds 6% [est].
14. Fee gate: reject a buy if priority fee plus tip exceeds 1% of size. Break-even gate includes fees, tips and unrecovered rent. Minimum trade raised to 0.1 SOL so fixed costs stay small.
15. Close the token account after every full sell to recover rent (~0.002 SOL).
16. One in-flight order per mint. Per-mint state machine OPEN → SELLING → CLOSED with a lock. Confirm by signature within 20 s, then check wallet balance before any retry. Never resend a sell until the first is confirmed dead (blockhash expired).
17. Exit escalation: a failed sell retries with priority fee 2x then 4x (cap 0.005 SOL); after 3 failures, sell 50% first.
18. Verify every transaction PumpPortal returns before signing: only expected program IDs, and simulated SOL change no worse than trade size plus 5%.

Operational and security:
19. Dedicated hot wallet holding no more than 1 SOL [est]. Main wallet never touched.
20. Supply chain: commit lockfiles, install with `npm ci --ignore-scripts`, CI fails if any lockfile URL is not registry.npmjs.org (a 2025 fake "solana-pumpfun-bot" swapped a dependency to steal keys, per SlowMist). Pin exact versions, `npm audit` and `cargo audit` in CI, minimal dependencies. Never run third-party bot code with a real key.
21. Helius free tier: own rate limiter at 8 req/s and 1 sendTransaction/s, back off on HTTP 429, fallback RPC, halt new buys if RPC latency exceeds 2 s [est].
22. One shared PumpPortal socket, reconnect backoff 1/2/4 s up to 30 s, no buys if disconnected more than 5 s.
23. Clock check against RPC slot time at start and every 10 min; halt if drift exceeds 2 s.
24. Crash safety: write the position to SQLite before sending the buy. On startup reconcile against wallet token balances and sell any orphan at once.

Strategy:
25. Max 30 trades per day [est]. Daily loss halt and 3-loss stop need a manual restart.
26. Unit test proves size never increases after a loss.
27. Regime pause: rolling 20-trade profit factor below 0.8, or new-token rate under half its 7-day median [est].
28. Paper-to-live gate raised to at least 100 paper trades. Live starts at the minimum size. Halt live if realized PnL is more than 30% worse than paper over 20 trades. Recalibrate the paper fill model from the first 20 live fills.

All 28 protections are acceptance items: each needs a unit test or a recorded demo before its milestone closes (token checks and execution rules in M2/M3, live-only rules in M4).

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
