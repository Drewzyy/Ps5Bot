# Claude Trader — STATUS

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
