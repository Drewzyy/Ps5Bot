# Claude Trader — STATUS

## Tips and first-run walkthrough: DONE (UI only, no trading or live code touched)

- `app/src/tips/content.ts` holds all text. Exit ladder, stops, hold time, fees, risk limits, score bar and curve SOL range are read from `config.json`. `app/src/tips/Tips.tsx` has the 10-step skippable walkthrough (highlights the real dashboard cards, docks on the side away from the highlight) and the Tips tab (5 topics, 24-term glossary, search, Replay walkthrough button). Top bar has a new Tips button (`#/tips`).
- "Seen" flag is in localStorage key `claudeTrader.walkthroughSeen.v1`, read and written inside try/catch. If storage fails the tour just shows again on next launch.
- Verified: `npm run build` ok, `cargo test --workspace` 108 passed, `scripts/check-live-gate.sh` ok. Screenshots `docs/screenshots/tips-*.png` (viewed; fixed the tour card covering the highlighted card and a lowercase sentence start). Sources and doubts: `docs/tips-sources.md`.

## M3 — Paper trader + widgets: DONE for the engine and UI; live market produced no pick (see below). Paper only, no keys.

### What was built (engine/src)
- `curve.rs` constant-product fills + fee model (pump.fun 1.25% + PumpPortal 0.5% per side, priority 0.0005 SOL, 5000 lamport tx fee, 2x/4x escalation capped 0.005). `paper.rs` executor: 400-1200 ms delay with the price re-read after the delay, buy slippage cap 5% (sells: config 15%), 10% buy / 15% sell failures (submit-failed / reverted / unconfirmed with 20 s balance check, retry escalation 2x, 4x, then sell half). Never mid-price: every fill walks the curve.
- `risk.rs` gates: size 1% of equity clamped 0.1-0.25, max 5 open, 5% exposure, 300 s cooldown, break-even gate (fees + priority + unrecovered rent + impact, TP1 must beat it by 5 points), fee gate (priority <= 1% of size), 30 trades/day, daily loss halt 7.5% and 3-loss stop (both sticky, stored in SQLite, cleared only by `engine-cli feed --clear-halt`), halve after 3 wins (floor 0.05), no size increase after a loss, regime pause (20-trade PF < 0.8 pauses 30 min), slippage pause (avg of last 10 buys > 6%).
- `exits.rs` Hold Timer: max hold 300 s, TP ladder 40/30/20 at +20/+40/+60, last 10% on 10% trailing, soft stop -10% with 15 s recovery timer, hard -15%, stop-at-entry after TP1, stall, bleeder, volume taper (sell 50%), momentum death, whale sell (>=3% of curve), cabal dump, dev sell, slow rug, graduation at 90% of 85 SOL, migration. Fade rules that read the absence of trades are skipped when the trade feed was not connected for the whole lookback. `last_price` never takes 0.0.
- `position.rs` state machine BUYING -> OPEN -> SELLING -> OPEN/CLOSED (second sell refused). `db.rs` positions table written BEFORE the paper buy; restart resumes OPEN/SELLING rows. `trader.rs` ties it together (1 s loop per position, RPC curve poll as fallback with a drift self-check). `report.rs` JSONL (exact SPEC 8 key order) + daily summary. Two lanes run the same engine: `momentum` and the paper variant `dip-40pct`; variant files go to `paper-logs/dip-40pct/`, so entry style is NOT added to the JSONL shape (it is in SQLite `positions.lane` and in every log line tag `TRADE[lane]`).
- `replay.rs` scripted markets (`hot`, `dump`) fed through the SAME picker, pre-buy checks, risk gates, executor, exit loop and JSONL: `engine-cli feed --replay hot`. UI: Open Positions, Position Chart (SVG candles from curve trades, entry/exit markers, SL/TP dashed lines, live line), Fees Paid split, Performance + equity curve, top bar P/L and trades today. Replayed data is tagged REPLAY in the UI.

### Verification (commands actually run)
1. `cargo build --workspace` exit 0. `cargo test --workspace` exit 0: `test result: ok. 108 passed; 0 failed` (engine; was 58). `npm run build` exit 0 (`✓ built in 678ms`). `bash scripts/check-live-gate.sh` -> `live-gate ok`, exit 0.
2. LIVE run, 55 min (2026-11-26 03:19:30-04:14:30 UTC, `docs/m3-live-run-log.txt`, public RPC, `--relax-metadata-egress`): coins seen 233, blocked 181, passed Scam Shield 52, scored 53 (35 scored 0.0), picked 0, entered 0, risk-gate blocks 0, failed buys 0. Best score 85.4 (BOB) failed the entry gates (curve 1.4 SOL, 3 buyers in 30 s, market cap $2678, WASH_VOLUME). Trade feed reset 19 times (public RPC), the feed resubscribed each time. No live trade happened, so the full trade below is REPLAYED.
3. REPLAYED trade (scripted market, `docs/m3-evidence/replay-trade-log.txt`, JSONL `replay-trades.jsonl`): RHOT scored 100 and was picked by the real picker, signal -> fill (spent 0.1 SOL, 739 ms delay, fees 0.002255 SOL) -> TP1, TP2, TP3 -> trailing-stop. JSONL line: `{"ts":"2026-11-26T03:46:42+00:00","mint":"ReplayHot1111111111111111111111111111111pump","symbol":"RHOT","side":"buy","sizeSol":0.1,"entryPrice":5.9323116262815846e-8,"entryMcapUsd":8918,"exitPrice":1.0362714028590603e-7,"exitReason":"trailing-stop","pnlSol":0.06359217,"pnlPct":63.59,"holdSeconds":53,"feesSol":0.009783448,"mode":"paper"}`. A second replay (RDMP, dump) closed on `whale-sell` after 5 s at -0.009098643 SOL. Daily summary (`replay-summary.json`) vs hand recompute from the JSONL with a separate Python script: trades 2, win rate 50.0, avg win 0.06359217, avg loss -0.009098643, profit factor 6.9891927840228485, max drawdown 0.009098643, total 0.054493527, fees 0.014203001: identical.
4. Restart: replay hot, `kill -9` while the TP1 sell was in flight (DB row state SELLING, entry 5.9323116262815846e-08, tokens 1656184.067686677, realized 0). After restart: `RESUMED RHOT ... tranchesDone=0 heldSoFar=28s`, same entry and tokens, then TP1-3 and trailing-stop closed it at +0.048591475 SOL (`docs/m3-evidence/restart-db-dump.txt`, `restart-log.txt`). Unit test `open_position_survives_a_restart_with_same_entry_and_pnl` also covers it.
5. Screenshots `docs/screenshots/m3-position-chart.png` (closed replayed trade: BUY marker, TP1-3 exit markers, trailing exit, stop and TP dashed lines), `m3-dashboard.png` (2 replayed trades, equity curve, fees split), `m3-open-position.png` (open position mid-sell). All tagged REPLAY. Viewed and fixed: candle width, overlapping labels, fee bar colour and clipped numbers.
6. Unit tests added: 2-layer stop timing, TP ladder + trailing, 300 s max hold, stall, bleeder, volume taper, momentum death (and feed-gap suppression), dev/whale/cabal/slow-rug exits, graduation/migration, break-even gate, fee gate, daily loss halt, 3-loss halt, size never up after a loss (property test), halve after 3 wins, exposure/cooldown/30 per day, regime and slippage pauses, state machine no double sell (also concurrent), JSONL shape equals the SPEC 8 example byte for byte, summary math, restart, failed buy, executor fees/escalation/failure rates, dip signal.

### Findings and honest limits
- PICKS ARE NOT POSSIBLE AT TODAY'S SOL PRICE. SOL was $71 during the run. `feed.minMarketCapUsd` $7000 then needs a market cap of about 98.5 SOL, which is 3.5x a fresh launch (28 SOL), but the chase rule blocks anything above 3x launch within 60 s. The two SPEC values only fit near SOL = $150 (replay pins $150 for that reason, `--sol-usd`). Not changed here; the user should decide (e.g. express the market-cap gate in SOL, or relax chase risk). Separately, 52 chain checks timed out (UNDECIDED_DEV_SNIPE) on the public RPC.
- The dip-40pct lane ran in no real or replayed trade (rare: needs score >= 80 AND a 40-50% dip). Only its signal function is unit tested.
- Curve account polling (RPC fallback) was only checked offline plus a manual look at live accounts: layout matches for normal coins, but some live coins show different virtual-SOL offsets than 30, so the poll is ignored when it drifts over 25% from a fresh trade. Not exercised in a live position.
- Regime pause: the "new-token rate under half the 7-day median" half is not built (needs 7 days of history). Day = UTC (`CT_TZ_OFFSET_MINUTES` overrides). Rent is only counted inside the break-even gate, not in paper balance.
- Exit loop ticks every 1 s (not woken by each trade event). Paper wallet balance = bankroll + realized + unrealized.
- Randomness showed: 2 of 5 demo replays lost the buy to the 10% simulated failure (reruns fine).
- Tauri build still unrun here beyond `cargo build --workspace`; Windows job still not run.

### Next
- M4 only after the user approves paper stats; user decision needed on the market-cap/chase-rule conflict and a free Helius key (public RPC dropped the trade feed 19 times in 55 min and timed out many dev-snipe checks).

## M2 — Live feed + Scam Shield + Token Picker: DONE (paper only, no keys needed)

### What works (all run here, live)
- `engine-cli feed --minutes N [--ws-port P] [--db PATH] [--rpc URL] [--relax-metadata-egress]` runs the pipeline headless, logs to stdout, writes every decision to the SQLite `decisions` table, and serves events on ws://127.0.0.1:P.
- Memecoin Monitor: ONE PumpPortal socket (subscribeNewToken + subscribeMigration), own CONNECT-proxy tunnel when HTTPS_PROXY is set (direct otherwise), extra CA from SSL_CERT_FILE or /root/.ccr/ca-bundle.crt, reconnect backoff 1/2/4..30 s, "no buys" flag after 5 s down.
- Scam Shield (`engine/src/shield/`): stage A cheap (metadata present, creator holding from initialBuy, creator blacklist table, copycat), stage B chain (Token-2022 mint parse with transferHook/permanentDelegate/transferFee/closeAuthority + mint/freeze authority at `processed` 8x750 ms, RugCheck report at 3 req/s, holders excl. curve/pool, insiders, bundle burst/supply, dev-snipe in first block via getSignaturesForAddress slots, metadata URI + community), pre-buy (wash volume, chase risk, bundle payers on real wallets). Each check has a 2 s budget and a `required` flag; undecided required = block.
- Token Picker: 60 s observation, score 0-100 (ratio 25, unique buyers 25, price change 20, volume slope 15, wallets 15) plus entry gates (curve SOL 12-40, buyers in 30 s, rising slope, market cap, creator not selling).
- UI: Watchlist with status/reason, Scam Shield widget (counts by reason, LOW_SIGNAL/ELEVATED/HIGH badge, Solscan/RugCheck evidence links), activity feed (handoffs, blocks, scores), pulses on handoff, real agent statuses. Tauri forwards engine events (`engine-event`); browser mode keeps the mock unless opened with `?engine=8765`.

### Verification (commands actually run)
- `cargo build --workspace` -> exit 0. `cargo test --workspace` -> exit 0, `test result: ok. 58 passed; 0 failed` (engine). `npm run build` -> exit 0, `✓ built in 701ms`. `bash scripts/check-live-gate.sh` -> `live-gate ok`, exit 0 (one comment containing a forbidden word had to be reworded first).
- Live run, 14 min (`docs/m2-feed-log.txt`, 2026-11-26 00:32-00:46 UTC): 58 new-token events; the PumpPortal socket had 0 disconnects/reconnects (the last 10 consecutive events with mint + ms timestamp are in the log, e.g. `2026-11-26T00:46:31.426Z NEW-TOKEN mint=BB7U39Tm...`). Summary: checked=57 blocked=49 passed=8; blocks: RUGCHECK_DANGER 14 (creator history of rugging), CREATOR_HOLDING 12 (e.g. "Creator holds 17.7% of the supply"), DEV_SNIPE 11 (e.g. "5 other transaction(s) landed in the same block as the launch"), UNDECIDED_DEV_SNIPE 10, SINGLE_HOLDER 2, METADATA_MISSING 1.
- Real picks: 8 coins passed Scam Shield and reached the Picker with scores (e.g. TARIFF 77.3, DEAD 54.7, RASMR 21.3; others 0). None reached 80 plus all gates, so nothing was handed to the Executor in the live run. A hot-token fixture test (`picker::tests::hot_token_scores_high_and_is_picked`) shows score >= 80 picked.
- Known-bad: live mint authority is blocked as MINT_AUTHORITY / HIGH by `shield::tests::known_bad_fixture_is_blocked_as_high_with_mint_authority_reason` (fixture: no real pump coin keeps mint authority). Real known-bad blocks from the live log: CREATOR_HOLDING, DEV_SNIPE (bundle-style), RUGCHECK_DANGER, SINGLE_HOLDER.
- After the 14 min run, `UNDECIDED_DEV_SNIPE` was cut by retrying the signature list up to 3x; a 4 min re-run showed checked=24 blocked=12 passed=12, UNDECIDED_DEV_SNIPE x1. The saved 14 min log predates that one tweak.
- Screenshot `docs/screenshots/m2-dashboard.png` taken against the live engine-cli (real Watchlist, feed, Scam Shield); viewed and fixed (time wrapping, duplicated reason text, stage-A passes removed from the evidence list).

### Findings that change the plan
- PumpPortal `subscribeTokenTrade` needs an API key funded with 0.02 SOL (verified). Not used. Trades come free from RPC `logsSubscribe` on each bonding curve, decoding the pump.fun TradeEvent (verified against live data). The public RPC closes this socket every 1-4 min; the feed resubscribes but loses trades in the gap. A Helius key will fix that.
- Dexscreener indexes pump coins only ~80-120 s after launch, too late for a 60 s window. pump.fun frontend APIs are blocked by the sandbox proxy.
- Most new coins get no organic trades in 60 s (score 0), so picks are rare. Expect to tune weights in paper (M3).
- RugCheck needs 1-4 attempts for brand-new coins (HTTP 400/404 first). RugCheck "danger" risks that are bonding-curve artifacts (low liquidity, holder % including the curve) only warn; our own rules cover them.

### Stubbed or limited (honest list)
- Honeypot sell simulation: NOT run. It needs a wallet that holds the coin and a PumpPortal-built transaction (M4). Shown as a `not available` flag on every pass; does not raise the verdict.
- Metadata URI + social links: this container's proxy blocks every metadata host (j7m.io, ipfs gateways, pinata), so the live run used `--relax-metadata-egress`: those checks show as flagged passes ("unverifiable") and the image check falls back to RugCheck. On the user's PC the flag is off: unreadable metadata = undecided = block, as SPEC requires.
- Bundle rule "20 transfers from <= 5 payers" runs on wallets seen in the observation trades (pre-buy), not on historic transfers; burst/supply rules run at chain stage. Funding-graph blacklist (wallets that funded a blacklisted creator) is not built.
- Copycat uses tokens that reached the feed minimum market cap as "trending", with tighter distance for short names (len <= 3 exact, 4-5 one edit, 6+ two edits).
- Executor / Hold Timer stay idle (M3). Helius key not used (public RPC, configurable via `CT_RPC_URL` or `HELIUS_API_KEY`, masked in logs). Windows build still not run.

### Next
- M3: paper executor, exits, JSONL, position chart. User still needs to provide a free Helius key for a steadier trade feed.

## M1 — Scaffold + dashboard shell: DONE (Windows CI job written but not run)

### Done
- Cargo workspace (`Cargo.toml`): `engine` (library + `engine-cli` binary) and `app/src-tauri` (Tauri 2).
- `config.json` = SPEC section 6 exactly. Engine serde structs load and validate it (`engine/src/config.rs`, 3 unit tests).
- Engine event enum + deterministic demo generator (`engine/src/events.rs`, `demo.rs`). The Tauri shell emits these as `engine-event`.
- React + TypeScript + Vite frontend (`app/src`). Runs in Tauri (events/commands) or in a plain browser with a mock source (`app/src/source.ts`).
- Dashboard: top bar (clock, trades today with buy/sell hover, balance, P/L today/total, PAPER badge), SVG network graph (center Claude Trader node + 9 agents, grey idle lines, pulse animation, click-for-panel), empty activity feed, 7 cards (Watchlist, Open Positions, Position Chart, Scam Shield, Fees Paid, Performance, Settings summary).
- Settings page: all 81 config.json leaves grouped in 10 sections, each with a working ⓘ (what it does / good value / how to set).
- Safety: `.claude/settings.json` (deny Read on key/wallet/.env paths + Stop hook), `scripts/check-live-gate.sh`.
- `.github/workflows/windows-build.yml` at repo root (windows-latest, `npm ci --ignore-scripts`, `tauri build`).

### Verification (commands actually run)
- `cargo build --workspace` -> exit 0 (Linux; apt-get update then `apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev` succeeded, so src-tauri builds here).
- `cargo test --workspace` -> exit 0; `test result: ok. 5 passed; 0 failed` (engine: config parses/validates/roundtrips/rejects bad, demo deterministic, event serialization).
- `cargo run -p claude-trader-engine --bin engine-cli` -> prints `engine ok` then the loaded config JSON.
- `npm ci --ignore-scripts` (in app/) -> exit 0; lockfile has 0 non-registry.npmjs.org URLs.
- `npm run build` (tsc --noEmit + vite build) -> exit 0, `✓ built in 635ms`.
- Playwright (chromium 1440x900, dev server): 0 console/page errors; clicked all 81 ⓘ buttons, 0 missing explanations, 0 em dashes.
- `bash scripts/check-live-gate.sh` -> `live-gate ok`, exit 0; planted `send_transaction` in engine/src -> `live-gate FAILED`, exit 1 (then removed).
- Screenshots: `docs/screenshots/m1-dashboard.png`, `docs/screenshots/m1-settings.png`; both viewed and checked against SPEC 9.
- Windows build: NOT run here. Workflow exists; its exit status is still to be quoted after the first GitHub Actions run.

### Next
- M2: PumpPortal feed (one socket), Scam Shield checks, Picker score, handoff pulses + feed entries. Needs a free Helius key from the user first.

### Key files
`SPEC.md`, `config.json`, `engine/src/{config,events,demo}.rs`, `engine/src/bin/engine-cli.rs`, `app/src/{App,source,useEngine,settingsMeta,agents}.ts(x)`, `app/src/components/*`, `app/src-tauri/src/lib.rs`, `scripts/check-live-gate.sh`, `.claude/settings.json`.

### Decisions
- Frontend imports `../../config.json` directly (single source of truth for the Settings page and mock data).
- Seventh widget card is a Settings summary card linking to the full Settings page (SPEC lists 6 data widgets plus Settings).
- Settings values are read only in M1.
- Top bar P/L and trades are zeros; USD uses a labelled demo SOL price of $150 until a price feed exists.
- Mock/demo pulses only (no activity-feed entries) so the feed stays empty in M1.
- Pinned exact npm versions; Tauri CLI/API 2.12.5 (2.5.x no longer on the registry).
- Open questions: SPEC.md section 14.
