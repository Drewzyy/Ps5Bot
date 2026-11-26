# Claude Trader — STATUS

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
