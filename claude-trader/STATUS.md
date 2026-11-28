# Claude Trader — STATUS

## M3h: everything on the D: drive (one data folder, installer folder page, move tool) (paper only, no keys, live stays locked)

### Installer (checked against the Tauri bundler's `installer.nsi`, dev branch, read 2026-11-28; NOT run on Windows)
- Directory page: shown in a normal install, skipped only in passive mode (`/P`, which the updater uses). So the user can pick `D:\...` on the folder page. Default when nothing is chosen: `%LOCALAPPDATA%\Claude Trader` (the bundler's per-user default; the exact default can differ by Tauri version).
- `bundle.windows.nsis.installMode` is now set explicitly to `currentUser` (it was already the default): no admin prompt, and installing under a folder the user owns on D: needs no admin. `perMachine` or `both` would need admin even for a per-user install, so they are not used.
- Updates go to the SAME folder: on install the NSIS script writes the install dir to `HKCU\Software\<publisher>\Claude Trader` (`WriteRegStr SHCTX "${MANUPRODUCTKEY}" "" $INSTDIR`) and `RestorePreviousInstallLocation` reads it back at start (`/D=` on the command line wins). The updater runs the installer passively, so it lands in the chosen folder without asking. Not tested on Windows. Uninstall note: the "delete app data" tick would remove `%APPDATA%\app.claudetrader.desktop` (the default data root and the pointer) but not a data root on D:.

### One data root (`engine/src/datadir.rs`)
- `DataLayout` is the only place that knows paths: `claude-trader.db`, `paper-logs/`, `recordings/`, the encrypted key fallback (`helius.key.enc` and the connector `*.enc` files), `chrome-extension/`, `logs/`, `cache/`, `webview/`. Audit (grep for `app_data_dir`, `local_data_dir`, `paper-logs`, `recordings`, `extinstall`): fixed (1) the installed app wrote `paper-logs` to a path relative to the working directory (so wherever Windows started it), (2) the extension folder was under `%LOCALAPPDATA%\Claude Trader` (now `<root>\chrome-extension`), (3) recordings and the key file derived the root by hand from the database path (now `EngineOpts::layout()`). `logs/` and `cache/` are reserved: nothing writes files there today (the engine log is in memory).
- Pointer file `data-location.json` (`{dataRoot, previousRoot}`) in the default app config folder; read, atomic write, missing, corrupt and unavailable-folder cases handled (an unavailable D: falls back to the default with a warning and leaves the pointer alone, so plugging the drive back in restores it).
- The WebView2 cache is under `<root>\webview`: `tauri.conf.json` has `create: false` and `lib.rs` builds the window with `WebviewWindowBuilder::from_config(...).data_directory(...)` (supported on Windows). On the very first launch the window uses a small `webview-boot` folder in the default root until the choice is made; it is deleted at the next start once the data is elsewhere. One-time effect: the window's browser storage (accordion state, theme, "tour seen") starts empty once, because the old WebView2 folder in `%LOCALAPPDATA%\app.claudetrader.desktop` is no longer used (that old folder is left and can be deleted by hand).
- First launch (no pointer, no database): the app asks "Where should Claude Trader keep its data?" with the default shown, Choose folder, Use the default. Nothing is created before the answer (`open_db` and `get_config` refuse while the question is open). Existing installs (database already in the default folder) are not asked.

### Settings > Storage and the move
- Shows the data folder, size, free space, what is stored where, Show folder, Move to another folder.
- Move (`storage_move`): checks the destination first (empty, or an existing data folder, or refused when not empty; not inside the current folder; free space against the size plus 10% and 32 MB; permission errors in plain words), pauses trading through the same `update-close` path as the updater (open paper positions are sold normally; if they will not close in 2 minutes nothing is moved), stops the engine and waits for it, copies (database via `VACUUM INTO`, the window cache is not copied), verifies (same files and sizes, `PRAGMA integrity_check`, same tables and row counts), writes the pointer LAST, restarts. Any failure before the pointer removes what was copied, leaves the old folder and pointer alone and starts the engine again. Choosing a folder that already holds `claude-trader.db` (a reinstall) switches to it without copying, after its integrity check.
- "Delete old copy" appears after the restart (the old path is kept in the pointer). It re-checks the new database first and removes only the app's own names (database, paper-logs, recordings, chrome-extension, logs, cache, webview, `*.enc`); other files are left and listed; the folder holding the pointer is never removed. "Keep it" forgets the old path.
- Extension: after a move the folder differs from the one saved at the last start, so a bar and the Storage section show "Chrome extension moved. Reload it once from the new folder" with 4 steps (remove, Load unpacked, pair again), Show folder and "Done, I reloaded it". Existing installs see this once too, because the extension folder moved from `%LOCALAPPDATA%\Claude Trader\chrome-extension` to `<root>\chrome-extension`.
- Tips entry "Where does the app keep its files, and can I put them on D:?" (what is stored where, how to move, and that Chrome's own downloads go where `chrome://settings/downloads` says, which the app cannot change). SPEC 4g.

### What stays on C: and why
- The pointer file (a few bytes, in `%APPDATA%\app.claudetrader.desktop`): it must be at a fixed place to find the rest.
- Windows Credential Manager entries (Helius key, webhook secrets): OS managed, cannot be moved. The encrypted file fallback IS in the data root.
- The app install folder unless chosen on the installer's folder page; the installer itself runs from where it was downloaded; Chrome (its profile and its downloads); the old WebView2 folder from before this version (unused).

### Verification (commands actually run)
- `cargo test --workspace`: 375 engine tests pass (was 359). Extension node tests 59 pass, `scripts/ci-scripts.test.mjs` 10 pass, `check-live-gate.sh` ok (unchanged; the only new `unsafe` is one `GetDiskFreeSpaceExW` call, not live code). New `datadir` tests (16): every path under the root, engine options use the layout, pointer read/write/missing/corrupt, resolve (default, first run, pointer, unavailable folder, corrupt pointer), destination checks, non-empty destination refused untouched, low disk space before copying, copy then verify then switch with the source untouched, failed copy removes the partial copy and keeps the pointer, unwritable pointer undoes the copy, existing-data folder switch, delete-old guards (never before a sound copy, only our names, pointer folder survives), WAL database copies whole, extension-moved detection.
- `npm run build` ok. DEMO screenshots viewed: `docs/screenshots/storage-section.png`, `storage-move-dialog.png`, `storage-move-not-empty.png`, `storage-first-run.png`, `storage-after-move.png`.

### NOT verified (needs Windows)
- The NSIS folder page, the registry read-back on update, a real install on D:, the folder picker, `data_directory` on real WebView2, `GetDiskFreeSpaceExW` (compiled only on Linux, where `df` is used), the move on a real second drive, Explorer's Show folder, the app restart after a move (`AppHandle::restart`), and stopping and restarting the engine in place after a failed move (the engine re-binds the extension bridge port; only compiled, not run).
- The backtester's default `--db` path (`<db folder>/recordings`) is unchanged and matches the new layout.

### The user's steps to move to D:
1. Update to this build. On the installer's "choose install location" page pick `D:\ClaudeTrader` if the app itself should be on D: too (the update button installs into the same folder afterwards).
2. Start the app. New install: answer the first-run question with Choose folder, pick `D:\ClaudeTraderData`. Existing install: Settings > Storage > Move to another folder, pick `D:\ClaudeTraderData` (a new or empty folder), press Move and restart.
3. After the restart check Settings > Storage shows the D: path, then press Delete old copy.
4. Follow "Chrome extension moved": chrome://extensions, remove the old one, Load unpacked from `D:\ClaudeTraderData\chrome-extension` (Show folder opens it), pair again.
5. Chrome downloads: chrome://settings/downloads > Location > Change to a D: folder (the app cannot do this).

## M3g — Feed correctness, evidence, learning rule, new signals and a deep-curve paper lane (paper only, no keys, live stays locked)

### Why
A GitHub research pass (2026-11-28) found that several independent small studies measured NEGATIVE average returns for buying a fresh pump.fun coin about a minute after launch and scalping out, that our trade decoder lacked two protections others found necessary, and that pump.fun shipped protocol changes (block 0 tax Nov 27, a 206 byte curve, holder-reward coins, non-SOL quotes). This milestone fixes the feed, records it, adds a way to judge strategies offline, makes the learning loop stricter, and adds paper lanes and signals to measure the rest. SPEC 13c has the evidence with sourced and unverified marked.

### Built
- **Feed correctness (SPEC 5b), checked against pump-fun/pump-public-docs and `idl/pump.json` read on 2026-11-28.** Trade events count only while pump.fun is on top of the invoke stack (`pump::pump_data_lines`); curve accounts of 125 to 206 bytes or more decode (`rpc::decode_curve_account_full`, offsets from the IDL); mayhem and non-SOL quote coins are blocked with `MAYHEM_MODE` and `NON_SOL_QUOTE` at the cheap stage, the chain stage, the pre-buy rules and `Trader::try_enter`; the Global account fee fields (95 + 30 bps, buyback, `block0_tax_bps` at 1096..1104) are read at start and feed the paper fee model (`FeeModel::trade_fees_in_creation_slot`, higher value wins); a completed curve is never entered; holder-reward coins are read and NOT excluded (the docs say trading is unchanged); `slotSubscribe` watchdog (6 s stall or 15 s without a first update reconnects, a refused subscribe turns it off) and a capped backfill (12 transactions per coin, 60 per reconnect) with a gap record.
- **Raw feed recorder (SPEC 4f)** `engine/src/recorder.rs`, Settings > Recording (on, 14 days, 2000 MB): compressed daily files of every trade, launch, creation slot, creation-slot trade and gap. New dependency: `flate2` (already in the lockfile through Tauri).
- **Offline backtester** `engine-cli backtest --data DIR --lane ...` (`engine/src/backtest.rs`): the real picker, gates, graded launch check, sprint rules and exit engine over a recording, fills priced after a delay, gap coins skipped, last 30% of time as the holdout with the promotion rule. Prints its own limits.
- **Learning promotion rule (SPEC 4c)**: a change is applied only if its shadow-lane trades (the holdout) are at least 20, have a mean return above 0, and the lower 95% bootstrap bound is above 0, and the mean is not below the main lane's. Bounds of 20% and every Scam Shield protection unchanged. Widget, Settings text and Tips updated.
- **Graded launch-slot check (SPEC 7b)**: counts launch-slot wallets, what they hold and how much SOL they could pull; blocks only above 16 SOL total, 11 SOL for one wallet, or 40% of supply; logs the graded values for every coin (`launch-grade` decision rows).
- **Creator funder-chain reputation (SPEC 7c)**: first funder via Helius RPC (2 calls, cached, 2 at a time, 30 s pause after a 429), judged from our own `funders` and `launch_outcomes` tables; `FUNDER_SERIAL_DEAD` block. Both tables start empty.
- **Deep-curve sprint lane (SPEC 6c), paper only**, lane `deep-sprint`, own stats, SOL-reserve stops (`exits.reserveStops`), watched from the coin's first seconds.
- **X Monitor**: `AboutAccountQuery` captured when the page fetches it (username changes, country, location flag, affiliate) and used in the rename block; operations matched by NAME including `CommunityQuery`, `CommunityTweetsTimeline`, `membersSliceTimeline_Query`, `moderatorsSliceTimeline_Query`; community `items_results` timelines read. Synthetic fixtures and 11 new tests.
- **Obsidian vault** (no-op when not connected): trade notes carry the lane tag (deep-sprint adds a paper-only line); Coins and Trades notes get a "Launch group, funder and sprint checks" table (funder verdict, graded launch-slot numbers, sprint entry); a serial dead-launcher funder writes a Situation note; Days notes count MAYHEM_MODE and NON_SOL_QUOTE skips; backtest runs with `--db` write Lessons notes with the bootstrap range and holdout verdict; promotion decisions already appear in the change notes with the numbers. Tests: a closed trade in each lane (momentum, dip-40pct, deep-sprint, shadow) writes a note.
- **Docs**: SPEC sections 4f, 5b, 6c, 7b, 7c and 13c (Evidence), DoradoDevs removed from the reference repos and a do-not-use list added (SPEC 3), Tips entry on expectancy, lane names in the UI.

### Decisions Claude made (user can change)
- Graded launch limits 16 SOL total, 11 SOL single wallet, 40% of supply; funder rule needs 2 other bad creators with at least 2 launches each at 80% dead; "dead" is judged from the 60 s watch (fewer than 5 buyers and no rise, or half the peak gone), not the research's 5 minutes.
- Sprint lane max hold is 600 s (paper only), and an unmeasurable launch group is not entered.
- The profile rename block fires on a username change in the last 90 days or 3 changes ever (starting guesses).
- Promotion needs 20 holdout trades (`learning.promotionMinTrades`, floor 10) and also that the test lane is not worse than the main lane.
- The mayhem and non-SOL exclusion happens at the cheap stage (no RPC spent on them).

### Verification (commands actually run)
- `cargo test --workspace`: 346 engine tests pass (was 264). New: invoke stack and fake router event 4, curve and Global decoding 3, recorder 6, watchdog and backfill 5, launch grade 8, funder 8 plus 4 through the Shield, graded and sprint end to end 8, promotion rule 3 plus statistics 3, exit reserve stops 2, sprint rules 5, backtest 11, About in the X score 3, config 3, fee model 1.
- `node --test tests/core.test.js tests/xmon.test.js` in `extension/`: 59 pass (was 48): hook relays by name (the real `xhook.js` in a fake page), hook and parser lists agree, About, community slices, items_results timelines, collector, runner.
- `node --test scripts/ci-scripts.test.mjs`: 3 pass. `npm run build` ok. `bash scripts/check-live-gate.sh` ok (script unchanged, no live code added).

### Verified against the official docs vs NOT verified
- Verified (official pump docs and IDL): BondingCurve and Global layouts and lengths, the TradeEvent tail fields (`mayhem_mode`, `quote_mint`, `holder_rewards_bps`, `block0_fee`), block 0 tax rules, holder-reward and synthetic-migration behaviour.
- Not verified: the invoke-stack rule against any pump doc (it is the generic Solana log format, tested only with synthetic logs); the real 2026-11-26 captured event is shorter than the IDL's current tail, so the tail parser is tolerant; claims about mayhem curves moving without events, the 8% non-SOL share, every number in SPEC 13c, the sprint thresholds, the funder statistics, the 16 SOL and 11 SOL limits; `AboutAccountQuery`'s nesting and when x.com fetches it; the community slice nesting. Nothing here ran against the real RPC, Helius, x.com or pump.fun (sandbox network). The Tauri app was only compiled on Linux.
- The block 0 tax and holder-reward changes cannot change paper results today (tax is 0, no lane trades in the creation slot).

### Known limits
- The backtester cannot replay RugCheck, authority, holder, funder, metadata or X checks, so it is an upper bound. It needs 1 to 2 weeks of recordings. Recordings before this version do not exist, and creation-slot trades are only recorded for coins that passed the chain stage.
- The funder check cannot block anything until the app has recorded several launches per funder.
- The launch-slot read costs up to 8 `getTransaction` calls per coin that has other creation-block transactions (about 2.5 coins a minute in the last live run), inside the 6 req/s limiter.
- The "unknown discriminator" counter idea (SPEC 5b item 8) is not built.

### The user's next steps
1. Update the app and reload the Chrome extension (chrome://extensions, Reload) so the new hook list is used. 2. Leave the app running 1 to 2 weeks (Settings > Recording shows the files). 3. Then run `engine-cli backtest --db <path to claude-trader.db>` or give it `--data` the recordings folder and read each lane's holdout verdict. 4. Watch the lanes on the dashboard; the Deep sprint will mostly sit idle.

## M3g: updater audit, extension link, stable extension folder, required pages (paper only)

### Updater audit (against Tauri 2.14.1 plugin source and docs)
- Checked: NSIS artifact `<productName>_<version>_x64-setup.exe` plus `.exe.sig` with `createUpdaterArtifacts: true` (no zip in v2); the signature field is the CONTENT of the .sig; the updater looks up `windows-x86_64-nsis` then `windows-x86_64`; passive install relaunches by itself (installer restarts the app); the plugin also compares a `version:` in the signed comment with latest.json `version`.
- Found and fixed: only one platform key was written (now both, like tauri-action); the MSI was built for nothing (CI now `--bundles nsis`); nothing checked the signature before publishing (a wrong key would have shipped a release that cannot install). New `scripts/updater-verify.mjs` (pure Node minisign verify, tested with the real Tauri CLI output): before the release it checks the .sig against the pubkey in tauri.conf.json; after it, it downloads latest.json from `releases/latest/download`, checks version, platform keys, that the signature equals the uploaded .sig, that the URL resolves and the installer verifies. A failing release is deleted so the previous one stays latest. Kept our own release step instead of tauri-action (same output, extension zip and verification already there).
- App: "Auto-update isn't set up yet. Add the signing key on GitHub (see Settings > About)" with steps on a 404 or missing release (`updaterText.ts`, tested); other errors are separate (offline, signature, unreadable file).
- Asset names have no spaces; `assetUrl` percent-encodes anyway.

### Extension link, stable folder, required pages
- Engine: `extinstall.rs` (version compare, copy only when the version differs, atomic writes, manifest last, key kept), bridge `info`/`reload`/`testping`/`pages`/`openpages`, hub `ext-test` and `ext-open-pages` (own handler so a slow test never blocks the kill command). App copies to `%LOCALAPPDATA%\Claude Trader\chrome-extension` at start. CI bumps the manifest version with the app version. Repo manifest version is now 0.1.0.
- UI: top-bar chip (hidden clock under 1400 px to fit), Settings > Startup and browser: versions, heartbeat, Test connection, updated notice, reload-once steps, checklist with Open missing pages. Side panel: connected line, updated line, checklist.
- Tests: engine 7 `extinstall` + 6 new `bridge` tests; extension `pages.test.js` (tab reuse, toggles, gates, login probe, reload gate); `scripts/ci-scripts.test.mjs` (latest.json shape, signatures); app `extStatus.test.ts`, `updaterText.test.ts`.
- Real run (Playwright Chromium, engine-cli feed, real extension, pairing): reload asked, manual-reload state, updated version announced, panel line, test ping answered in 1 ms, pages opened and not duplicated. Screenshots `docs/screenshots/ext-*`, `update-not-set-up-*` (chip states and the connection test are DEMO).

### Not verified
- No release exists and the secret is missing: real signing, release upload and the post-release check, the in-app download and passive install on Windows.
- `chrome.runtime.reload()` itself: headless Chromium with --load-extension disables the extension on reload, so the reload step was checked only up to the message (and the restart path). Test in real Chrome.
- x.com is unreachable here, so "Logged out" was tested on the content-script logic only, and the Windows paths (`local_data_dir`, `explorer.exe`).

### User steps
1. GitHub repo Settings > Secrets and variables > Actions > New secret `TAURI_SIGNING_PRIVATE_KEY` (your private key text, no password). Re-run windows-build.
2. Install the new build. Settings > Startup and browser > Show the extension folder. In chrome://extensions remove the old Claude Trader extension, Load unpacked from THAT folder, pair again. Never load the install folder again.

## M3f — X Monitor and Thesis Finder (paper only, no keys, live stays locked)

### Built
- **Extension** (`extension/`, manifest 0.2.0): `xhook.js` (MAIN world, document_start, x.com) copies x.com's own GraphQL answers to `xcontent.js` (isolated), which parses them with `lib/xparse.js` and only does so in the bot's own job tab. `xrunner.js` takes one job at a time from the app, opens it in ONE reused unfocused background tab, waits for the data (DOM fallback after 14 s), returns a structured result, then waits 8 to 15 s. `lib/xpace.js`: 120 loads an hour, 800 a day, 30 minute cache, back-off 15 min / 1 h / rest of the day, "X paused: account limited". Read only: nothing posts, likes, follows or clicks (source scan test). The app builds every address (four allowed shapes).
- **Engine** (`engine/src/xmon/`): `model.rs` (link parser, jobs, cleaned results), `score.rs` (pure community score and hard blocks), `mod.rs` (queue with queued/running/done/failed/stale, SQLite tables `x_coins`, `x_handles`, `x_follow`, `x_results`, evaluation, clone ranking, reused-account detection, follower history, rising accounts, stories, narratives, views). Bridge messages `xnext`, `xstatus`, `xresult` (bridge still has no buy or sell message). Picker `with_x`: `round(community * 0.15)` points, max 15, X alone can never buy, a block adds a gate. Journal, JSONL (`xCommunity`, last key, SPEC 8 updated), Obsidian trade note and a `decisions` row (stage `x-monitor`) carry every component.
- **App**: X Monitor and Thesis Finder nodes are live, dashboard cards "X community" and "Thesis Finder", Settings > X Monitor (directions, switch, pause button, pacing, weights with i texts marked as starting guesses), Tips entries, extension coin card shows score, copy rank, flags and blocks, side panel shows one X line.
- Weights and limits are in `config.json` under `xMonitor` (SPEC 4e). All are starting guesses from the research notes.

### Decisions Claude made (user can change)
- Pages are queued after the chain stage passes, not right after the cheap prefilter: the X link only exists after the metadata fetch in the chain stage, and queueing every coin would burn the 120 pages an hour on coins that Scam Shield blocks anyway. Every coin with an X link is still recorded for clone ranking and narratives.
- A coin with NO X link is not blocked (it gets no X points). `xMonitor.blockWhenNoXLink` switches the literal block on.
- Reuse/rename blocks apply to profile links (and a handle mismatch for post links is a flag). A post link's author being a KOL used by many coins is not a reuse block (copy rank covers it).
- A profile link has no replies to sample and a community link has no account, so such coins cannot reach 100.
- Follower growth needs two sightings at least 5 minutes apart; on a first look only the account age is used (young account with 1,000+ followers a day is flagged "fast-rising").

### Verification (commands actually run)
- `cargo test --workspace`: 264 engine tests pass (was 197; new ones include `xmon::score` 20, `xmon::model` 4, `xmon` 14, picker X rules 7, bridge X 2, engine X end to end 2, report line shape).
- `node --test tests/core.test.js tests/xmon.test.js` in `extension/`: 48 pass (parsers on synthetic fixtures, page detection, jobs, pacing with a fake clock, runner with a fake browser, read-only source scan, manifest).
- `npm run build` ok. `bash scripts/check-live-gate.sh` ok (script unchanged).
- Screenshots (DEMO data, viewed): `docs/screenshots/x-nodes.png`, `x-node-panel.png`, `x-community-card.png`, `x-thesis-card.png`, `x-settings.png`, `x-settings-info.png`, `x-ext-card-ready.png`, `x-ext-card-blocked.png`, `x-ext-card-unknown.png` (the extension card uses the real `content.js` on a stand-in page with a stubbed `chrome.runtime`, made-up data).

### NOT verified (this sandbox cannot reach x.com)
- Everything about the real site: the GraphQL shapes (fixtures are synthetic, made by `extension/tests/make-fixtures.mjs`), the community operation names (guessed, found by member count), whether a background tab loads all data, how fast the spare account is limited, whether the DOM fallback selectors still match.
- The Chrome extension was not loaded in a real Chrome in this session (no Playwright extension run for the X parts). The Tauri app was only compiled on Linux.

### The user's setup steps
1. Make a spare X account (not your main one). 2. Log into x.com in the Chrome profile the bot uses. 3. Optional: follow a few hundred crypto KOLs with it. 4. Reload the extension from the new folder (chrome://extensions, Reload) and keep it paired. 5. Settings > X Monitor (on by default). Watch the "X community" card.

## M3e — RugCheck outages, trade-feed health, "Why no trades?" (paper only, no keys, live stays locked)

### Why (from the user's 75-minute Windows run with a working Helius key)
321 coins seen, 307 blocked, 14 passed, 0 trades. 114 blocks were `UNDECIDED_RUGCHECK` ("rugcheck http 502", "error sending request", "RugCheck has no report yet"), decided 1-4 s after launch. A free website being down is not a scam signal.

### Built
- **RugCheck** (`engine/src/shield/net.rs`, `shield/mod.rs`, SPEC 11b #29): one shared client (2 in flight, 2 req/s, cache per mint), retries after 3, 8, 15 s for 502, network error, timeout, 429 and "no report yet". It runs in the background from the start of Scam Shield, in parallel with the 60 s watch; a report that arrives is judged at once (danger flag ends the watch early). Chain stage no longer waits for it.
- **On-chain fallback**: RugCheck still silent at pick time means our own checks must ALL pass (mint and freeze authority, Token-2022 extensions, dev snipe and bundle burst, top holders read from the chain with `getTokenLargestAccounts` + `getMultipleAccounts`, creator launch buy and live balance, creator blacklist). Recorded as "RugCheck unavailable, passed on-chain checks" (decisions stage `rugcheck-fallback`, verdict ELEVATED); trade size is half (`Pick.size_factor`). A RugCheck danger or rugged flag always blocks. `filters.requireRugcheck` (default false, ⓘ in Settings) restores "no report, no buy".
- **Trade feed health** (`rpc.rs`, `health.rs`): counters (trades decoded, last trade, subscriptions confirmed, last node error with the key masked), a subscribe the node never confirms is re-sent after 8 s (3 times), a refused subscribe is shown. Memecoin Monitor node line "Live. N trades/min on M coins", health block in the node panel and in Settings > Connections, red bar "No buy/sell data is arriving, so every coin scores 0." after 2 minutes of watching with 0 trades decoded. Health and funnel events keep only the latest in the hub history.
- **"Why no trades?" card** (first widget): real counts from SQLite (new `scores` table plus `decisions`, `tokens_seen`, `positions`): seen, blocked by Scam Shield with top reasons, scored (and how many had no trade at all), best score, gates behind the 10 best coins that were not picked, picked, bought, RugCheck-fallback counts.
- **X links**: chips say "X post" (a status link), "X profile", "X community", "Telegram" or "website".
- Tips: new walkthrough step, RugCheck and warning entries, glossary.

### Root cause of "0 buyers, curve 0.0 SOL" (could NOT be confirmed)
- Read the trade path end to end: the `logsSubscribe {mentions:[curve]}` filter, the TradeEvent decoder (offsets and discriminator now pinned to the pump.fun IDL by a test), and subscription timing (`hold_feed` runs right after the cheap prefilter, before any chain check, so the first trades are captured). No bug found. A replay test pushes Helius-shaped `logsNotification` messages through the same function the live task uses and a busy coin scores above 80 and is picked.
- The passed coin's own numbers say trades WERE decoded: the picker reports market cap 0.0 when it has no trade, and it showed 28.0, so the coin had at least one trade at about the launch price while the curve held under 0.05 SOL. That fits a dead coin with dust trades, not a dead feed. If the feed were dead, every scored coin would show 0 trades; the new card counts those exactly ("N of M had no buy or sell at all") and the red bar fires after 2 minutes.
- Not provable here: whether Helius's free plan limits `logsSubscribe` (this sandbox cannot reach Helius). If it does, the node's error is now shown in Settings > Connections ("The data connection says ...") and the subscription count ("confirmed by the node") will sit at 0.

### Verification (commands actually run)
- `cargo test --workspace`: 197 engine tests pass (was 166). New: RugCheck retry schedule 3/8/15 s, 502 + network error + "no report yet" retried to a report, give-up after 4 tries, cache per mint, max 2 in flight; fallback blocks on single holder, top holders, unreadable holder list, mint authority, freeze authority, transfer hook, creator live holding, blacklisted creator, missing dev snipe result; half size end to end through `observe_and_trade` (position size is half), full size with a good report, danger flag blocks and ends the watch early, `requireRugcheck` on blocks; health warning after 2 minutes, cleared by a trade, reset when nothing is watched; Helius-shaped notification replay into the picker, failed subscribe recorded with the key masked, IDL layout check, owners from `getMultipleAccounts`, funnel counts, hub keeps only the latest health.
- `npm run build` ok. `bash scripts/check-live-gate.sh` ok (script unchanged, no live code added).
- Screenshots (DEMO data, opened with `?demo=1`, `&nodata=1` for the warning), viewed: `docs/screenshots/funnel-card.png`, `funnel-card-dark.png`, `feed-health-warning.png`, `feed-health-panel.png`, `feed-health-node-warning.png`, `feed-health-node-ok.png`, `feed-health-connections.png`, `feed-health-connections-ok.png`. Fixed after viewing: node status text was cut off (now two lines, shorter text), health rows were misaligned.

### Stubbed, limited, or unsure
- Not run against the real RugCheck, Helius or pump.fun (sandbox network). The IDL fixture comes from `.reference/` (date unknown); the real transaction check of 2026-11-26 (`REAL_SELL_B64`) still decodes.
- The image check no longer gets a second chance from RugCheck's copy of the image (the chain stage does not wait for RugCheck any more).
- Coins that skip RugCheck are bought at half size and show up in the journal like any trade; compare them with full-size trades before trusting the fallback. The learning review does not separate them.
- The supply-bundle test (3 wallets hold most) now runs when the RugCheck report arrives, and does not run for fallback coins (top holders and single holder are checked instead).

## M3d — Chrome extension, Startup and browser settings (paper only, no keys, live stays locked)

### Built
- **Extension** (`extension/`, Manifest V3, plain JS, no build step): side panel (balance, P/L today and total, open positions, activity, "What it learned", kill switch with confirm, Resume, notification toggles, badge choice, Unpair), toolbar badge (today's P/L green/red or open count, "off" when the app is closed), coin card on pump.fun coin pages and dexscreener.com/solana pages (Scam Shield result, score, status held / watching / skipped / blocked / closed / not seen, reason, "Check this coin"), notifications (buy, sell, daily loss limit, kill switch, engine offline, each with a toggle), tab management (pump.fun once per browser session, the Dexscreener chart of each coin bought, reuse of existing tabs, closes only tabs it opened and only when the app setting is on). Same light/dark theme and pixel logo. Auto reconnect (1, 2, 3, 5 s), alarm every 30 s, app pings every 20 s. README with the install steps and security notes: `extension/README.md`.
- **Engine side** (`engine/src/bridge.rs`): own local WebSocket `ws://127.0.0.1:8766` (`--ext-port` on engine-cli, always on in the app). Origin must be exactly `chrome-extension://jihnmdnpkeglfhoombfjhdfbmiopnljj` (the ID of the public `key` pinned in the manifest). Pairing: one-time 8-digit code (10 minutes, single use, stored as hash) swapped for a 256-bit token (only its hash is stored). 5 wrong tries lock 60 s. The extension gets only the public dashboard events (no ticks) and the startup settings. It can send `verdict`, `check`, and the commands `kill` / `resume`. Nothing else exists. "Check this coin" runs `Shield::manual_check` (mint account, RugCheck, holders), records a `manual-check` decision, and has no handle to the trader.
- **App side**: Tauri commands `ext_pair_start / ext_unpair / ext_paired / get_startup / set_startup / set_autostart / open_chrome_now / open_extension_folder`. `engine/src/startup.rs` (settings in SQLite `kv startup.settings`, Chrome path finder). `app/src-tauri/src/startup.rs` (start Chrome, default browser fallback, keep awake with `SetThreadExecutionState` on a holder thread, released on exit). tauri-plugin-autostart. At launch: waits 4 s, and starts Chrome only if the extension has not already connected. The extension folder is bundled as a Tauri resource.
- **UI**: Settings > Startup and browser: extension status (live), Get pairing code (shown once, 10 minutes), Unpair, Open Chrome now, toggles with i texts (open Chrome at launch, pages, close tab on close, start with Windows, keep awake), and "Install the Chrome extension" steps with "Show the extension folder". Tips entry "What is the Chrome extension?".
- **CI**: `.github/workflows/windows-build.yml` runs the extension logic tests and uploads the folder as artifact `claude-trader-chrome-extension`.
- Replay mints renamed to valid base58 (`Rep1ayHot...pump`) so a replayed coin can be looked up.

### Verification (commands actually run)
- `cargo test --workspace` (CARGO_TARGET_DIR=/tmp/ct-target2): 165 engine tests pass (was 151). New: `bridge::tests` (11): only the pinned origin connects (8 bad origins refused with 403, including no origin and a lookalike ID), manifest key hashes to the pinned ID, right origin without pairing gets nothing (4 variants), code works once then the token signs in and Unpair revokes it, hash-only storage, expired code and 5-try lockout, events carry no ticks and skip token/verdict spam, verdict lookup (blocked, skipped, watching, held, unknown, bad address, after-restart read from SQLite), a block is never overwritten by a pass, "Check this coin" runs the checker once and nothing reaches the engine command handler for buy / sell / set-paper-fill / approve-change / reload-rpc (position rows stay 0; kill still passes), bridge source has no trader references, the real checker against a dead RPC records a `manual-check` block and opens no position. `startup::tests` (3). `node --test extension/tests/core.test.js`: 9 pass (notification diffs and baseline, badge, summary, mint finder, no buy message / no key handling in extension source).
- `npm run build` ok. `bash scripts/check-live-gate.sh` ok.
- Playwright Chromium 1243, `launchPersistentContext` with `--load-extension`, engine in replay mode on its own ports (ws 18765, bridge 8766) and own DB: extension ID matched the pinned ID; pairing failed with a plain message while the app was closed, then worked with the code; side panel showed the replayed trade as it opened; the card on a local `https://pump.fun/coin/<mint>` page (served by a Playwright route) showed Held, then Closed; an unknown coin showed "Not seen", Check this coin ran the scam check ("Blocked", this sandbox has no usable RPC) and the DB still had exactly 1 position (the replay one); the Dexscreener-shaped page found the coin; notifications `buy-1`, `sell-1`, `kill-...`, `offline` were created; the chart tab was opened once, recorded, and closed when the trade closed after the app setting `closeTabOnClose` was written to SQLite and pushed to the extension; kill switch with confirm and Resume worked from the panel; badge showed "off" when the engine stopped. Screenshots `docs/screenshots/ext-*.png` (panel: unpaired, pair-error, connected, kill-confirm, killed, dark, offline; card: held, unknown, checking, checked, dexscreener, dark, offline; app: `ext-app-settings.png`, `ext-app-settings-dark.png`, DEMO code). Viewed and fixed: "low signal" wording, an awkward offline line, demo code label.

### Not verified (needs Windows and a real Chrome)
- The Tauri build with tauri-plugin-autostart, the Windows registry autostart, `SetThreadExecutionState` (compiled only on Windows, the Linux build uses a no-op), starting chrome.exe from the standard paths, `explorer.exe` for the folder button, and the resource folder path in an installed app.
- The real pump.fun and Dexscreener pages (blocked here). The card works from the URL (pump.fun) and from the URL or a solscan token link or the Dexscreener pair API (Dexscreener). If Dexscreener changes its layout the API fallback still works.
- Chrome's side panel opening from the toolbar icon, desktop notifications on screen (checked that they are created), and Chrome's "disable developer mode extensions" notice.
- If Chrome is already open when the app starts and the extension needs more than 4 s to reconnect, Chrome may open a second pump.fun tab.

### The user's install steps
1. Install and open Claude Trader (the Windows build).
2. Settings > Startup and browser > Install the Chrome extension > Show the extension folder.
3. In Chrome open `chrome://extensions`, turn on Developer mode, press Load unpacked, pick that folder (it holds manifest.json).
4. Pin Claude Trader from the puzzle icon, click it to open the side panel.
5. In the app press Get pairing code, type the code in the side panel, press Connect. Done once.

## M3c — Helius key, update check, Settings accordion, one code path, learning journal (paper only)

### Built
- **Helius key** (Settings > Connections): password field, Save, Test connection (OK with ms, or a plain error), Remove key. `engine/src/secrets.rs` (OS vault via `keyring`, encrypted-file fallback, masked hint only), hot swap of RPC and trade-feed endpoint (`reload-rpc`), env var override, Tauri commands `helius_status/save_helius_key/remove_helius_key/test_helius_key`. Tips walkthrough step and Tips entries added.
- **Update check** (Settings > About and updates): version, commit, build time (`engine/build.rs` embeds `GITHUB_SHA` or `git rev-parse`), "Check for updates" against the public GitHub API, "Update available" with Open download page and 3 steps. `engine/src/update.rs`.
- **Settings accordion**: Entry, Scam Shield, Exits, Risk, Fees, Paper fill realism (preset Optimistic / Realistic / Harsh, saved in SQLite, hot applied), Learning, Connections, Appearance, About and updates. Collapsed by default, open state remembered in localStorage. Every old i is kept.
- **Same code path** (SPEC 4b): `Executor` trait, `ScriptedExecutor`, test that paper and scripted executors give identical decisions, "Skipped in paper" list in the Scam Shield card and in Paper fill realism.
- **Learning journal** ("What it learned" widget, SPEC 4c): journal per trade, review every 30 trades, shadow lane test, apply only if it wins, +-20% bounds, Undo, live-mode approval, efficiency notes, missed coins. All in SQLite (tables journal, trade_features, reviews, changes, learning_notes).
- **Source links**: `sourceUrl` on activity feed items, website/X/Telegram links on Watchlist rows and journal entries (from coin metadata). X Monitor and Thesis Finder slots only, no fake data.

### Verification
- `cargo test --workspace`: 151 engine tests pass (was 114). New: key storage with a mock keyring (save, fallback file is encrypted, remove, env override, bad keys), update comparison (same commit, compare status, time fallback, unknown build, API parsing, safe URLs), learning (observations carry the real numbers, 10-trade floor, gap threshold, +-20% bounds, Scam Shield/size/fees/loss limits not tunable, shadow win applies, shadow loss drops, live approval, undo, restore after restart, missed coin text, efficiency), same decisions with two executors, social link cleaning.
- `npm run build` ok. `bash scripts/check-live-gate.sh` ok.
- Screenshots in `docs/screenshots/` (all labelled DEMO where seeded, opened with `?demo=1`): `settings-accordion-collapsed.png`, `settings-paper-fill-realism.png`, `settings-connections-empty.png`, `settings-connections-saved.png`, `settings-about-updates.png`, `settings-connections-about-dark.png`, `settings-connections-narrow.png`, `learning-widget.png`, `learning-widget-dark.png`, `dashboard-demo.png`. Fixed after viewing: bold explanation text inside labels, feed source link squeezed the text, watchlist link prefix.

### Stubbed, limited, or unsure
- Not run on Windows: Credential Manager, `rundll32` browser open, the Tauri build itself. Linux build and tests pass; the vault path is tested with a mock.
- **(Done in M3d, see below.) Self-updating (tauri-plugin-updater) was NOT done at M3c.** It needs a signing keypair; a key I generated would be lost to you. Upgrade path: run `npx tauri signer generate -w ~/.tauri/claude-trader.key` once, put the public key in `tauri.conf.json` under `plugins.updater.pubkey`, add the private key as GitHub secret `TAURI_SIGNING_PRIVATE_KEY` (and its password as `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), publish GitHub Releases from the workflow. Never commit the private key.
- Artifacts on GitHub Actions need a GitHub login to download (the steps say so).
- Learning has had no real data: the live paper run made 0 trades, so every learning screenshot is DEMO. The pipeline is covered by unit tests only. The shadow lane makes a trade only if a coin passes the candidate's rules, so a test can take long.
- The trailing-stop give-back and bucket patterns are the only proposals; weights move only when the top bucket of ratio or buyers clearly wins.
- Browser-only preview cannot store a key (it says so). Changing the paper fill preset in browser mode against engine-cli works through the WebSocket.

### How the user enters the Helius key
1. Go to helius.dev, sign up free, open Dashboard > API Keys and copy the key.
2. In Claude Trader press Settings, open Connections.
3. Paste the key into Helius API key, press Save (it shows "saved, ends in ...xxxx"), then Test connection (expect OK with a time).
It is used right away, no restart. Remove key deletes it.

## M3b — SOL market-cap gate, full paper fee model, kill switch, long live run (paper only, no keys)

### Changes
- Entry gate is now `feed.minMarketCapSol = 30` (SOL, user approved). The USD gate needed ~98.5 SOL at SOL=$71 and could never pass with the chase rule (3x launch ~ 84 SOL). Changed in config schema (`engine/src/config.rs`), `config.json`, `picker.rs` (`score()` no longer takes a SOL price), `engine.rs` (trending mark), Settings i text, Tips glossary, SPEC section 6 (with reasoning). SOL/USD is now used only for display and the JSONL `entryMcapUsd`; no entry gate reads it. Test `picker::tests::market_cap_gate_is_in_sol_and_fits_under_the_chase_rule` proves gate and chase rule can both pass (45 SOL after 55 s passes both; 29.9 SOL is gated).
- SPEC fee inconsistency fixed: one "Fee model" rule in section 6 (1.25% pump.fun = 0.95 protocol + 0.30 creator, PumpPortal, base fee, priority, Jito, rent) with sources; the 0.30% mention is explained as the creator part.
- Full paper fee model: every component is its own line (Fees widget, `feeBreakdown` in JSONL, SPEC 8 updated), deducted from the paper balance: buy and sell % fees on the actual SOL, creator fee, 5000 lamport base fee, priority fee (x2, x4 on retries), optional Jito tip (`fees.jitoTipSol`, 0 now), fees of reverted/retried transactions, token-account rent 0.00203928 SOL out of the balance while open and back at close. PumpPortal raised to 1.0% per side (Solana Compass Oct 2026 lists 1% Lightning, 0.5% Local; higher used). A bug was found by the new lamport test: the Jito tip was missing from the buy fixed cost (fixed).
- Kill switch (SPEC section 11): red button in the top bar (both themes), confirm dialog, Ctrl+Shift+K, `engine-cli kill|resume|kill-state --db PATH`, persisted in SQLite `kv.killed`, "Resume trading" is separate with its own confirm. Tauri commands `kill_switch` / `resume_trading`; browser mode sends over the local WebSocket (origin checked). Closes use the normal sell path, exitReason `kill-switch`.

### Verification (commands run)
- `cargo test --workspace` -> `test result: ok. 114 passed; 0 failed` (3 consecutive runs, no flake). `npm run build` -> exit 0 (`built in 829ms`). `bash scripts/check-live-gate.sh` -> `live-gate ok`.
- New tests: SOL gate + chase; round trip balance change equals proceeds minus every cost to the lamport (with a Jito tip set); reverted buy costs base+priority+jito and opens no rent; rent locked while open and returned on close; kill closes every open position in both lanes and blocks entries (`KILLED`); a restart while killed stays stopped, still sells the open position, and a third start is still killed; resume is explicit and buying works again afterwards; failed sells are retried until closed (`sell_fails >= 6` while all fail, then closes with exitReason kill-switch with no new command).
- Kill switch end to end (replayed market, engine-cli + browser): `docs/screenshots/kill-switch-button-light.png`, `-button-dark.png`, `-confirm-light.png`, `-confirm-dark.png`, `-after.png` (all viewed). Log: `TRADE KILL SWITCH ON (from dashboard) ... selling 1 open position(s)`, then `FILL SELL RHOT reason=kill-switch`, `CLOSED RHOT reason=kill-switch pnl=+0.011612739 SOL`, banner "Kill switch on: all closed (1 sold). No new buys until you press Resume trading." Fixed in review: PAPER badge wrapped when the button changed to "Resume trading".

### Live paper run (public RPC, `--relax-metadata-egress`, `docs/m3b-live-run-log.txt`)
- 345 min total in 3 segments (06:25-07:40, 08:14-10:29, 18:50-21:05 UTC; container restarts cut the first one short). Coins seen 2024, checked 2024, blocked ~1450, passed Scam Shield ~590, scored 567 (log lines), score >= 80: 10, score >= 60: 28, picked 0, entered 0, failed buys 0, risk-gate blocks 0. No paper trade happened, so there are no live trades to list, no win rate, no drawdown (daily summary: trades=0, pnl 0, fees 0). The trade pipeline with the full fee model is covered by replay and unit tests only.
- Scam Shield blocks (both DBs): CREATOR_HOLDING 332, RUGCHECK_DANGER 313, UNDECIDED_RUGCHECK 303 (RugCheck had no answer for brand-new coins in time), DEV_SNIPE 265, UNDECIDED_DEV_SNIPE 80, METADATA_MISSING 49, SINGLE_HOLDER 40, CREATOR_BLACKLISTED 35, COPYCAT 27, NO_IMAGE 8, MINT_AUTHORITY 4, UNDECIDED_MINT_ACCOUNT 4, TOP_HOLDERS 4. Pre-buy: WASH_VOLUME 4. Observation: LOW_SCORE 558, entry-gate failures at score >= 80: 11.
- Entry gates failed, over all 567 scored coins: curve SOL 12-40 failed 565, buyers in 30 s 554, market cap 30 SOL 543 (mostly coins with no trades at all, market cap reads 0), volume not rising 542, buy/sell ratio 532, creator selling 82, WASH_VOLUME (pre-buy) 3. Among the 10 coins with score >= 80: curve SOL gate failed 8, buyers in 30 s 5, volume not rising 5, creator selling 4, buy/sell 4, WASH_VOLUME 3, market cap 1.
- Diagnosis: the gate that rejects the best candidates is `entry.curveSolMin = 12` (real SOL in the curve at 60 s). The best coin (HOMER, score 99.0) had 5.2 SOL; CHLOWIE 9.9, FCAT 11.2, almost all others 0-5. The new SOL market-cap gate is NOT the blocker any more (1 of 10 top coins). Near misses with only ONE failing gate: RUDOLPH (score 94.5, buy/sell 1.6 vs 2), Unbound (98.6, buy/sell 1.9 vs 2), CHLOWIE (94.0, curve 9.9 vs 12 plus WASH_VOLUME). Trading values were NOT loosened; the user should decide whether to touch curveSolMin 12.
- Worth knowing: (1) the chase rule (`shield::rules::chase_rule`) only blocks when `age <= 60 s`, but the pre-buy check runs after the 60 s observation, so the rule is effectively inert (age is >= 60.0). Not changed (changing it tightens entries). (2) Coins that fell 84-92% still scored 49-61 because buyers and volume count; the price-change part clamps at 0. (3) Public RPC dropped the trade feed repeatedly and RugCheck timed out on 303 brand-new coins; a Helius key would help both.

### M3b tuning and post-tuning run (2026-11-26/27)
- Applied (values decided by Claude, user-delegated): `entry.curveSolMin` 12 -> 4; chase rule fixed (price at decision time > 3x launch blocks at any age, it never fired before because it was limited to age <= 60 s); new momentum-lane pre-buy reason `DUMPED_FROM_PEAK` (price more than 40% below the observation-window peak). The dip-40pct lane is not affected (momentum-only checks run after the shared pre-buy). Buy/sell 2.0 and everything else unchanged. Tests: 116 passed (`chase_over_3x_blocks`, `coin_that_pumped_over_3x_during_observation_is_blocked_by_the_momentum_checks`, `dumped_from_peak_blocks_a_falling_knife`). SPEC section 6 and 11b #7/#7b updated with the data.
- Post-tuning live paper run: 201 min in three segments (21:09-22:10, 22:10-23:08, 01:00-02:23 UTC). The public RPC stopped delivering curve trade logs for the first two segments (285 subscriptions confirmed, 0 log messages, while 100 pump.fun signatures per second exist on chain), so those 2 hours scored 415 coins that all had zero trades. Only the last 83 min had a working trade feed. Entered 0, picked 0, failed buys 0, risk-gate blocks 0. No paper trade, so no win rate / P&L / drawdown (summary: trades 0, pnl 0, fees 0).
- Coins scored post-tuning: 509 (94 with the working feed). Score >= 80: 4, score >= 60: 20. Gate failures of the 4 coins with score >= 80: creator is selling 3, curve SOL (now 4-40) 3, buy/sell < 2 2, WASH_VOLUME 1, buyers in 30 s 1, volume not rising 1. With the working feed (part 6): 3 coins >= 80, failing creator-selling 3, curve SOL 2, buy/sell 2, buyers 1, volume 1.
- NEW top blocker: `creator is selling` (any sale by the creator, `dev_sold_pct > 0`) is the ONLY failing gate of the best coin of the whole run, MinerPig (score 100, 63 buyers, 20.5 SOL in the curve), and one of two for Firefly (79.7, 72 buyers, 8.5 SOL). The curve-SOL change to 4 lets those through that gate; the any-size creator-sell block now stops them. Not changed (values are the coordinator's call).
- Chase rule and DUMPED_FROM_PEAK fired 0 times (no coin got that far: no pick reached pre-buy).
- Learning journal: the engine binary used for the run was built before the journal commit (b9131cd), so no journal entries were generated (the run DB has no journal table). With zero trades there would be nothing to journal anyway.

### Cost of a 0.1 SOL trade (paper model, no Jito)
Buy: pump.fun protocol 0.00095 + creator 0.00030 + PumpPortal 0.00100 + base 0.000005 + priority 0.0005 = 0.002755 SOL. Sell of about 0.1 SOL gross: same percent fees on the proceeds (0.00225) + 0.000505 = 0.002755. Round trip about 0.00551 SOL = 5.5% of size (4.5% percent fees + 1.0% fixed), plus 0.00203928 SOL rent locked while open (returned on close). Break-even gate figure: about 7.6% before price impact; TP1 at +20% clears it. A reverted transaction costs 0.000505 SOL.

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


## M3d: one-click self-update, Obsidian memory, Discord and webhook connectors

### Self-update (tauri-plugin-updater)
- `tauri.conf.json` has `plugins.updater` (public key, endpoint `https://github.com/Drewzyy/Ps5Bot/releases/latest/download/latest.json`, Windows `installMode: "passive"`). The private key is NOT in the repo; it belongs in the GitHub secret `TAURI_SIGNING_PRIVATE_KEY` (no password).
- UI: a round update button (circular arrows, tooltip "Check for update") replaced the Tips tab in the top bar. One click: spins while checking, then "Up to date (vX)" toast, or "Updating to vX..." with a progress bar, install and restart. A red dot on the button and an "Update ready" badge on Settings appear when the quiet background check (4 s after start, then every 6 hours) finds one. It never installs without a click.
- Open paper positions: the dialog says "1 open paper position will be closed first. Continue?". On confirm the engine command `update-close` sells them through the normal exit path (exit reason `update`, normal fees and fail rates) and blocks new entries. This is NOT the kill switch: nothing persistent is set (test `closing_for_an_update_sells_normally_and_never_sets_the_kill_switch`). If the install fails, `update-close-cancel` lets trading carry on.
- Settings > About and updates: version shown as `0.1.<run>` plus the short commit; Check for updates, "You're up to date (version X)" or "Version Y is ready" with Update and restart; progress bar. On any error the old GitHub download page link and 3 steps are shown as a fallback.
- Settings, trade history, learning and keys live in the app data folder (`app.path().app_data_dir()`, `claude-trader.db` there), not the install folder, so the installer cannot touch them (read from `lib.rs` setup; the real Windows install is NOT tested here).
- Tips moved into Settings > Tips and help (small ? button next to the Settings title); Replay walkthrough kept. The old `#/tips` address opens Settings.
- CI (`.github/workflows/windows-build.yml`): sets version `0.1.<run_number>` in tauri.conf.json and Cargo.toml (`scripts/ci-set-version.mjs`), signs with `TAURI_SIGNING_PRIVATE_KEY` (password empty), enables `createUpdaterArtifacts`, and on pushes to `claude/trader-build-prompt-v2-9pmbsi` creates release `v0.1.<run>` (latest) with the NSIS setup.exe, its .sig, `latest.json` (`scripts/make-latest-json.mjs`) and the Chrome extension zip, using `gh release create` with the built-in GITHUB_TOKEN (`permissions: contents: write`). If the secret is missing it prints a warning, skips signing and the release, and still builds and uploads the normal artifacts. `node --test scripts/ci-scripts.test.mjs` covers the latest.json shape (version, notes, pub_date, platforms.windows-x86_64 {signature, url}) and the version edits.
- ONE STEP FOR YOU: add the repository secret `TAURI_SIGNING_PRIVATE_KEY` (the contents of your private key file). Until then CI builds unsigned and makes no release, and the update button shows the fallback link.

### Obsidian vault connector (Settings > Connectors)
- `engine/src/vault.rs`. Local markdown only. Choose vault folder (Tauri dialog), Test (writes and reads back `Claude Trader/Test note.md`), Disconnect, Backfill, per-type toggles (Trades, Coins, Days, Lessons, Situations). The path is in SQLite kv (`vault.settings`), not config.json.
- Writes `<vault>/Claude Trader/`: `Trades/YYYY-MM-DD SYMBOL id.md` (lane, times, prices, size, every fee line, net P&L, hold, exit reason, entry features, Scam Shield rows with a Solscan link, journal observations, source link, tags #win/#loss #exit/...), `Coins/<mint>.md`, `Days/YYYY-MM-DD.md` (summary, funnel counts, links), `Lessons/` (each review, each change with status, efficiency notes), `Situations/` (kill switch on/off, loss limit halt, feed warning, RugCheck outage once per hour, missed coins scoring 80 or more with what happened next), `Index.md`. YAML frontmatter and [[wikilinks]].
- Safety: temp file then rename; nothing is ever deleted; a note without our `generated-by` line is never overwritten; names are sanitised (no separators, reserved Windows names, length cap); nothing is written while disconnected. Only recorded data is used.
- Tests (engine, `vault::tests`): note content from data, names sanitised, atomic writes and keep-yours, disconnected writes nothing, connect/test/toggles/disconnect, close writes trade+day+index and honours toggles, backfill (twice is harmless), missed coins under 80 not written.

### Discord and generic webhook connectors (outgoing only)
- `engine/src/notify.rs`. Per-event toggles: buy, sell (with P&L), daily summary, learning change applied, kill switch, daily loss limit, feed/RugCheck warnings, update installed. Secrets (webhook URLs, optional secret header) are stored one by one in the OS secure storage (encrypted file fallback) like the Helius key, shown only as a masked hint, scrubbed from every error text, never in logs, SQLite or config.json.
- Discord: URL must be https://discord.com or discordapp.com `/api/webhooks/<id>/<token>`; embeds green for a win and red for a loss, with fields and the Dexscreener link; footer says "Paper results" while paper. Generic webhook: JSON POST (`claude-trader/event/v1`, schema in the card and below), https only (http only for localhost), optional `X-Claude-Trader-Secret` header.
- Each connector has its own bounded queue and worker: `emit` never blocks (a full queue drops), sends are paced (Discord 1.2 s, webhook 0.5 s), a 429 waits `retry_after`, 5xx and network errors retry up to 3 times, then the error is stored masked and shown on the card. A failing service cannot hold up trading.
- Daily summary: yesterday's real totals are sent once after midnight (only if there were trades). Update installed: sent after the restart when the version changed.
- Tests (`notify::tests`): URL validation, masking, payloads from data, paper label, rate limiting and 429, a failure that does not block, event toggles, secrets kept out of SQLite and status, test button error masking, day summary/version change. The X (Twitter) and Telegram connectors were dropped by decision; X as a reader is a later task.

### Webhook payload
`{"schema":"claude-trader/event/v1","event":"sell","mode":"paper","paper":true,"sent_at":ISO,"title":"Win CAT","data":{...recorded numbers, "event" repeated...},"links":{"dexscreener":...,"solscan":...}}`. Events: buy, sell, dailySummary, learningChange, killSwitch, lossLimit, health, updateInstalled, test.

### Screenshots (DEMO, viewed)
`docs/screenshots/`: `topbar-1280.png`, `dashboard-topbar-demo.png`, `update-downloading.png`, `update-closing-positions.png`, `update-confirm-open-position.png`, `update-up-to-date.png`, `update-error-fallback.png`, `settings-about-update-ready.png`, `settings-about-up-to-date.png`, `settings-tips-section.png`, `connectors-obsidian-card.png`, `connectors-discord-card.png`, `connectors-webhook-card.png`. Fixed after viewing: the top bar wrapped at 1280 px (tighter gaps under 1500 px), card screenshots had the sticky bar over them.

### Not verified
- The real update download, install and relaunch on Windows, the signature check against the real key, and the release created by CI (needs the secret).
- Real sends to Discord or a webhook (tests use a mock transport). The OAuth/Telegram code from the first draft was removed.
- Writing into a real Obsidian vault on Windows (tests use temp folders).
