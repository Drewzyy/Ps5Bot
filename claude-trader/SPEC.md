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

The research log was supplied in chat and its contents are folded into this spec. For the reference repos, the builder clones them read-only into `claude-trader/.reference/` (gitignored) when needed, and only to read:
- github.com/chainstacklabs/pumpfun-bonkfun-bot (docs and cookbook only; its Issues are spammed with wallet-draining scam links)
- github.com/slightlyuseless/memecoin-trading-bot (read `config.json` only)
- github.com/pump-fun/pump-public-docs (official; layouts, fee and protocol notes. Read as raw text, never run)

**Removed from the reference list [2026-11-28]:** `DoradoDevs/solana-pumpfun-sniper-bot`. It is a single-commit repo that handles wallets and Jito racing with no history and no independent verification. The fade exits that came from its `smart_exit.py` are kept as ideas (SPEC 6 rules) but the repo is not a source anyone should clone or trust.

**Rules for every third-party repo [2026-11-28]:** never clone it into this repo, never install or run its code, never add it as a dependency, never send our coin lists, wallets or keys to a service it points at. Reimplement ideas ourselves. A repo whose description holds a t.me link or "contact on Telegram" is treated as malware until proven otherwise.

**Do-not-use list** (from the GitHub research of 2026-11-28; judged on metadata and README text, not a full source audit, so absence from this list is not a clearance):
- `mortdeus/solana-copy-sniper-mev-trading-bot` (4,523 stars, 0 issues, repo created 2013, now an ad-and-Telegram "MEV sniper"), `coffellas-cto/Solana-Copy-Trading-Bot` (created 2014, Telegram pitch).
- `bigmacman1129/solana-sniper-trading-mev-bot` and `solana-rust-sniper-bot` (repackaged copy under a new account, keyword-stuffed), `moonlight987/sniper_Rust_solana-pumpfun` (copy-paste description), `vvizardev/solana-pumpfun-sniper-rust` (keyword spam).
- Volume, maker and bundler tools that need a funded wallet in `.env`: `A5-/solana-market-maker-volume-bot`, `cicere/pumpswap-volume-bot`, `hexnome/pumpfun-raydium-cli-tools`, `yegor104/pumpfun-volume-bot` and `pumpfun-maker-bot`, `Trenchy-Tools/trenchytools-pumpfun-launcher`.
- Tools on the rugger side: `soldrift/bubblemap-bypass-engine-solana`, `soldrift/pumpfun-sniper`.
- Generic "sniper bot" clones with near-identical descriptions and no measured results: `Keidev-sol/Solana-Sniper-Rust-Bot`, `Snowstorm08/Ultra-Fast-Rust-PumpFun-Sniper-Bot`, `dexoryn-solana/pumpfun-sniper-bot`, `ferris007/solana-jito-shredstream-pumpfun-sniper`, `pumpfun-trader/pumpfun-trading-bot`, `ahk780/pumpfun-copy-trading-bot`, `bitman09/pumpfun-sniper-bot`, `jhcdx9999/pumpfun-sniper`.
- Promotional or pay-per-call: the `CabalSpy/*` SDK repos, `h100envy/gem-search` and gemsearch.fun (its own token and Telegram bot), `wyattpalm2-eng/x402-seller`, `chulinhcql-art/Solana_Rugpull_Detector`, `x402agent/SolanaOS` (mayhem sniper inside an agent monorepo).
- Auto-traders with wallet keys and no track record: `BaileyOnBlockchain/gary-solana-sniper`, `AiMemeHunter/Auto-Ai-Meme-token-bot`, `kontia1/gmgn-screener`, `vineetdev02/Memcoin-Sniper-Bot`, `DoradoDevs/solana-pumpfun-sniper-bot`.
- Provider SDKs with a large tip surface, never to be added as dependencies: `0xfnzero/sol-trade-sdk`, `0xfnzero/solana-streamer`, `0xfnzero/sol-parser-sdk`.

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
- **Learning journal** (`engine/src/learning.rs`): every lesson comes from recorded numbers; nothing is invented. A journal entry per closed trade; a review every 30 closed main-lane trades (groups by curve SOL, buy/sell ratio, buyers, score, exit reason; a pattern needs at least 10 trades in the group and 10 in the rest and a win-rate gap of at least 25 points); the proposal runs in a shadow paper lane for the next 30 main-lane trades. Those shadow trades are the HOLDOUT (the change never saw them) and it is applied only if there are at least `learning.promotionMinTrades` (20) of them, their mean return per trade is above 0, the lower end of the 95% bootstrap range of that mean is above 0 (4,000 resamples), and the mean is not below the main lane's [changed 2026-11-28, was: beats the main lane on net P/L and profit factor with 10 trades; a 10 against 10 split shows a gap from luck alone]. Allowed changes: a whitelist of entry/exit thresholds and the five score weights, never more than 20% from the shipped value. Never Scam Shield, trade size, fees or loss limits. Every change is logged with before/after, evidence and result and has Undo. In live mode a winning change waits for the user's approval. Efficiency notes: pick-to-fill time, failed-transaction cost, fee share of gross profit, missed coins (score 80 or more but blocked, with the next 5 minutes of price when seen).
- **Source links**: wherever the app shows a reason or signal from outside, it stores and shows a clickable `sourceUrl`. Today that is the coin's own website, X and Telegram from its metadata. X Monitor and Thesis Finder (section 4e) read X through the Chrome extension, so the links carry real data now ("X post", "X profile", "X community"); nothing is faked.

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

## 4e. X Monitor and Thesis Finder (added 2026-11-28)

User decision: read x.com with the Chrome extension and a spare logged-in X account. No paid API. Paper only. Research: `x-monitor-research.md` (scratch notes, not in the repo). **Every number below is a starting guess from that research with no measured backing.** They live in `config.json` under `xMonitor`, each with an i text in Settings, and the journal logs every part so they can be tuned after about 100 paper trades.

**Flow.** A coin that passes the cheap prefilter and the chain checks and has an X link in its metadata is queued (`xmon::XMon::queue_coin`). The X link only exists after the metadata fetch in the chain stage, so jobs are queued once the chain stage passes (about 90% of coins are blocked earlier, and the 120 pages an hour could not cover them). Every coin with an X link is still recorded for clustering (`x_coins`), scraped or not. The extension pulls jobs over the bridge (`xnext`), opens each page in one reused, unfocused background tab, and returns a structured result (`xresult`). Job states: queued, running, done, failed, stale (not started within `jobWaitSecs`, or running over 60 s). Pages per coin: profile link = profile + followers_you_follow; post link = post (author, views, replies) + followers_you_follow of the author; community link = community page.

**Extension.** A MAIN-world script at document_start on x.com (`xhook.js`) wraps fetch and XMLHttpRequest and copies the page's own GraphQL answers (UserByScreenName, UserTweets, TweetDetail, community operations, followers_you_follow) to an isolated script (`xcontent.js`) by postMessage. Parsers (`lib/xparse.js`) are defensive about rotating shapes and report drift. If no data arrives in 14 s the isolated script reads the DOM. The isolated script does nothing unless the background worker says the tab is the bot's job tab. It never posts, likes, follows or clicks (a test scans the source). It does not replay GraphQL calls. Pacing (`lib/xpace.js`): 8 to 15 s random gap, at most 120 loads an hour and 800 a day, data cached 30 minutes per handle, post or community, back-off 15 min then 1 h then the rest of the day after a rate-limit, lock or 429, shown as "X paused: account limited". The app enforces the same gap and hourly cap again. Hard floors: gap 5 s, 200 loads an hour.

**Community score (0-100).** Sum of parts, rescaled so the weights add to 100: smart (KOL) followers 30 (0 = 0, 1 = 8, 3 = 18, 5 = 25, 10 or one account with 200,000+ followers = 30), engagement quality 20 (interactions per view, or per follower: under 0.1% = 0, 0.3% = 5, 1 to 5% = 14 to 17, above 5% = 20 only if the repliers look varied, above 10% on a tiny account is suspicious), reply authenticity 10 (full at 70% real or better, zero at 30% or worse; needs 8 replies), organic follower growth 10 (follower counts per account id are kept in SQLite; with history, growth with healthy engagement is full, flat is half; without history only the account age is used), account and handle integrity 10 (age 30 days or more 4, no rename and no earlier coin 4, bio or post mentions the coin 2), X Community presence 10 (100+ members with recent posts), narrative strength 10 (author reach and age, times 1.0 for copy 1, 0.8 for copies 2 to 3, 0.4 for 4 to 5, 0 after). A profile link has no replies to sample and a community link has no account, so those coins can reach less than 100. Failed, missing or stale (older than `maxAgeMinutes` 30) data is **unknown**: 0 points, no block, never a pass. Unknown parts inside a score earn 0 and say so in `flags`.

**Hard blocks.** (1) The linked X account or post does not exist, is suspended or protected. (2) A renamed or reused account: the same numeric X user id was seen under another handle, or was the profile of an earlier coin, or the link names a different handle than the account has. (3) A post link by an account with 50,000+ followers whose text never names the coin's name, ticker or address. (4) A bought-follower pattern: followers up 3x within an hour with engagement under 0.3%, or more than 60% of the sampled replying accounts made inside one 30-day window with engagement under 0.3%. (5) 30% or more of at least 8 sampled replies look like bots (repeated text, generic or emoji-only, accounts under 7 days old, default avatars, handles ending in 6+ digits). (6) Later than 5th among coins linking the same post or community in 24 hours, or more than 10 such coins. A coin with no X link at all is not blocked by default (it earns no X points); `blockWhenNoXLink` turns the literal block on. Blocks are ignored while X Monitor cannot work (off, paused, extension offline, not logged in, account limited): the coin just loses the bonus.

**Picker.** `picker::with_x` adds `round(community * 0.15)` points, at most 15 (config cannot raise it, startup validation refuses it). X never changes an entry gate. X alone can never buy: the base score must already be at least `max(minScore - 15, minScore / 2)`. A block adds the gate "X Monitor block: ..." and stops the buy in the main, shadow and dip lanes. If X data is not ready at decision time the coin is scored without it. Settings has a switch (default on; it only acts once the extension is paired and x.com is logged in) and a pause button.

**Thesis Finder.** Coins are clustered by canonical post id, community id and author (links are normalised: twitter.com, x.com, mobile, www, query strings). Each coin shows a story: post text excerpt, author, reach, views, how much older the post was than the coin, copy rank and clone count. Several launches (3 by default) from one post, community or account within 60 minutes make a trending narrative.

**UI.** The X Monitor and Thesis Finder nodes show live status ("Checked 34 accounts this hour, 2 rising"). Dashboard cards: "X community" (rising accounts and the latest checks, each linked to the exact post, profile or community) and "Thesis Finder". The extension coin card shows the community score, copy rank, flags and blocks. Settings > X Monitor has directions (spare account, log in, follow a few hundred crypto KOLs, keep the extension paired), the switch, the pause button, the pacing numbers and the weights with i texts.

**About data and operation names [2026-11-28].** The page hook copies GraphQL answers by operation NAME (the id in the URL rotates, the name has not so far): `AboutAccountQuery`, `UserByScreenName`, `UserByRestId`, `UserTweets`, `UserTweetsAndReplies`, `TweetDetail`, `TweetResultByRestId`, `CommunityQuery`, `CommunityByRestId`, `CommunityTweetsTimeline`, `membersSliceTimeline_Query`, `moderatorsSliceTimeline_Query` and any `*FollowersYouFollow*`. The names come from the maintained twscrape client's `api.py` (read as text only). Community timelines list posts as `items_results` with `slice_info.next_cursor`, not as entries, and the parser reads both. `AboutAccountQuery` carries an `about_profile` with `username_changes {count, last_changed_at_msec}`, `account_based_in`, `location_accurate`, `affiliate_username` and `source`. It is captured only if the page fetches it during a profile job (the extension never opens an About page of its own: the four address shapes above stay the only ones). A missing `username_changes` means unknown, not zero. In the rename block: a profile link whose account changed its username in the last 90 days, or 3 or more times at any time, is blocked (starting guesses, no data behind them); one older change is a flag. Country, "location may be inaccurate" and affiliate are flags only, never blocks.

**Not verified.** Everything about real x.com (query shapes, whether `AboutAccountQuery` is fetched while a profile page loads and which address triggers it, the nesting of the community slices, whether a background tab loads all data, how fast the spare account gets limited). Parsers are tested on synthetic fixtures only.

## 4f. Raw feed recording and offline replay (added 2026-11-28)

**Why.** The learning loop used to decide on 30 trades. Fills priced at an order book that is already bought out, buyer counts that include trades after the entry, and an "oracle" that was not a ceiling made fake profits in the one open replay study found (cavalheiroeth/pumpfun-sniping-lab, 0 stars, claims unverified, design sound). To judge any change we need the exact feed to replay.

**Recording** (`engine/src/recorder.rs`). Every trade of a watched coin (after the store drops repeats), every launch, the creation slot of each coin that got past the chain stage, the creation-slot trades read from the chain and explicit gap records are appended to `recordings/feed-YYYY-MM-DD.jsonl.gz` (UTC day) in the app data folder next to the database. Lines are buffered and flushed every 5 s as one gzip member each (concatenated members are one valid gzip stream), so a crash loses seconds. Gap record: written when the trade socket reconnects, from the time of the last message to the reconnect. Retention (Settings > Recording, `recorder` in config.json): delete files older than `retentionDays` (14), then the oldest until under `maxMegabytes` (2000). Nothing leaves the PC.

**Backtester** (`engine/src/backtest.rs`): `engine-cli backtest --data DIR [--lane momentum|dip-40pct|deep-sprint|all] [--config PATH] [--delay-ms N] [--from DAY] [--to DAY] [--holdout-pct N] [--json]`. It runs the real picker, the entry gates, the trade-only pre-buy rules, the graded launch check, the sprint rules and the real exit engine (`exits::evaluate_r`) over the recording. Entry is priced at the curve `delay` ms (default 2000) AFTER the decision. A coin that overlaps a recorded gap is skipped and counted. The newest 30% of trades (by time) are the holdout, judged once with the promotion rule above. What it cannot replay: RugCheck, authority and holder checks, the funder chain, metadata, X Monitor. So it is more permissive than the live bot and its numbers are an upper bound. It needs 1 to 2 weeks of recording before any lane has enough trades.

**Obsidian.** Everything in this milestone also feeds the vault when it is connected (SPEC 11c): lane tag on trade notes, funder, graded and sprint rows in Coins and Trades notes, a Situation for a serial dead-launcher funder, MAYHEM_MODE and NON_SOL_QUOTE counts in Days notes, and Lessons notes for backtests (`--db`) with their bootstrap range.

**Verified vs unverified.** The file format, retention, the replay and the holdout rule are covered by tests (STATUS). Whether the deep-curve window produces enough trades on live data is unknown until this recorder has run.

## 5. Pipeline

discovery → cheap prefilters (no RPC) → Scam Shield chain checks → observation window → score → entry rule → portfolio/risk gates → buy → persisted position → exit engine → sell → log.

Every rejection at any stage is written to the decisions table with stage + reason, and shown in the Watchlist.

Timing [2026-11-27]: the coin's trade subscription (`logsSubscribe {mentions:[bondingCurve]}` on the RPC WebSocket, commitment `processed`) starts right after the cheap prefilter, BEFORE the chain checks, so the 60 s window holds the coin's first trades. RugCheck runs in parallel with the watch (11b #29), not before it. A subscribe the node never confirms is re-sent after 8 s (3 times); a subscribe the node refuses is shown as the last error in Settings > Connections. The trade event decoder is pinned to the pump.fun IDL `TradeEvent` layout by a test (`engine/tests/fixtures/pump_trade_event_layout.json`; a moved field or a new discriminator fails the test), and a replay test pushes `logsNotification` messages through the same function the live feed uses, into the picker.

## 5b. Feed correctness and protocol drift (added 2026-11-28)

Each item below was checked against the official pump-fun/pump-public-docs (README.md, docs/PUMP_PROGRAM_README.md, docs/BLOCK0_TAX.md, docs/HOLDER_REWARDS_README.md, docs/SYNTHETIC_MIGRATION.md and `idl/pump.json`, read as raw text on 2026-11-28) unless marked unverified.

1. **Invoke-stack attribution** (`pump::pump_data_lines`). A `Program data:` line counts only while the pump program is on top of the stack built from `Program X invoke [n]` and `Program X success|failed` lines. A router or aggregator that calls pump.fun can print a look-alike TradeEvent itself; those lines are ignored. After a `Log truncated` line nothing counts. This is the standard Solana log format, not something the pump docs describe (unverified against pump docs; tested with a router log that fakes an event).
2. **Curve account length**: any length from 125 bytes up (`MIN_CURVE_ACCOUNT_LEN`); the docs list 125, 151, 166 and 206 and say missing trailing fields read as zero. Offsets taken from the IDL: reserves 8..48, `complete` 48, `creator` 49..81, `is_mayhem_mode` 81, `quote_mint` 83..115, `is_holder_reward` 124, `created_at_slot` 166..174, `tax_exempt_creator` 174..206.
3. **Mayhem and non-SOL quote**: both excluded, with reason codes `MAYHEM_MODE` and `NON_SOL_QUOTE`, at the cheap stage (the create message's `is_mayhem_mode`), at the chain stage (the curve account), in the pre-buy rules (flags read from the tail of each TradeEvent: `mayhem_mode`, `quote_mint`) and as a last guard in `Trader::try_enter` for every lane. The account fields exist in the docs; the claim that mayhem curves move without trade events (a measured median 77% error on the constant-product invariant) and that about 8% of curves have a non-SOL quote come from the sniping-lab README: unverified.
4. **Global fee fields** (`4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf`): `fee_basis_points` 105, `creator_fee_basis_points` 154, `buyback_basis_points` 997, `block0_tax_bps` 1096..1104. Read at start. The paper fee model takes the higher of config and chain for the per-side percent, and carries the block 0 tax: when it is above 0, `FeeModel::trade_fees_in_creation_slot` adds it to the protocol fee for a non-creator trading in the coin's creation slot (`current_slot <= created_at_slot`). The buyback share is part of the protocol fee, so it is logged and not added. No lane trades in the creation slot, so the tax has no effect on paper results today (it is 0 and "stays 0 until further notice").
5. **Holder-reward coins**: the docs say there are no trade interface changes and fees are computed as before, only the recipient of the creator fee differs, so these coins are NOT excluded. The flag is read (curve byte 124, `holder_rewards_bps` in the TradeEvent) and carried in the recording. Cashback coins are deprecated but old ones still trade.
6. **Synthetic migration**: the buy that completes the curve can be bigger than what is left and is priced at the future pool's price; the price is continuous, so "sell on migration" paper fills stay fair. After it the curve is `complete` and every trade fails until `migrate_v2` lands, so the engine treats a complete curve as migrated (exit) and never enters one.
7. **Liveness**: `slotSubscribe` on the same socket (about 2.5 updates a second). No slot update for 6 s, or none within 15 s of connecting, drops and reopens the socket (a half-open socket never fires `close`). A node that refuses `slotSubscribe` switches the watchdog off with a log line. On every reconnect a gap record is written and, for every watched coin, `getSignaturesForAddress` plus `getTransaction` read the missed trades (at most 12 transactions per coin and 60 per reconnect), through the same decoder, into the same store (repeats dropped).
8. Not done: an "unknown discriminator counter" alert. Fee recipients (rotated 2025-11-25) only matter for M4 live trades built by us, which PumpPortal handles.

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
    "requireRugcheck": false,
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

New config sections [2026-11-28], all optional (a config without them loads with these defaults): `filters.graded` (SPEC 7b), `filters.funder` (SPEC 7c), `fees.block0TaxBps` (set by the engine from the chain, SPEC 5b), `learning.promotionMinTrades` 20 and `learning.bootstrapResamples` 4000 (SPEC 4c), `recorder` (SPEC 4f), `sprint` (SPEC 6c).

Rules:
- Stop-loss [Claude, 2026-11-25, replaces the earlier -20/-25 choice]: soft stop -10% starts a 15 s recovery wait; hard stop -15% sells at once (checked every 1 s). Tightened because fees model at ~4.5% per round trip (Fee model rule) and the user asked for minimal losses; -15% hard stops are used by gmgn-screener (high-risk entries), OilFront and a single-commit repo's config (now on the do-not-use list, SPEC 3).
- Style [user, 2026-11-25]: quick scalps. Every position is closed within 5 minutes, no exceptions.
- Take-profit [user]: 40% at +20%, 30% at +40%, 20% at +60%, last 10% on a 10% trailing stop from the peak.
- Holder concentration [user]: single holder ≤5%, top holders combined ≤15%, with bonding-curve and pool accounts excluded from both numbers.
- Max hold [user]: 5 minutes (300 s). Anything still open is sold in full.
- Fade exits [user] (an idea taken from the smart_exit.py of a repo that is now on the do-not-use list, SPEC 3; it was untested there since its engine is never called): stall (under +3% between 60 and 90 s: sell all); bleeder (past 120 s, below -20%, peak never above +20%: sell all); volume taper (3 falling 10 s buy-volume windows and the last under 50% of baseline: sell 50%); momentum death (after 90 s, fewer than 0.5 buys/min over 45 s: sell all); whale sell of 3% or more of the curve, or any dev sell: sell all at once.
- Exit loop runs every 1 s on curve-account WebSocket pushes, RPC fallback.
- Curve SOL minimum [Claude, user-delegated, 2026-11-26]: `entry.curveSolMin` 12 -> 4 (max stays 40). Data from 345 min of live paper (2024 coins, 567 scored, 10 with score >= 80, 0 entries): the 12 SOL gate failed 8 of those 10 (HOMER 5.2, CHLOWIE 9.9, FCAT 11.2, most others 0-5), because real SOL in the curve at 60 s is almost always under 12. 4 SOL still needs real buys, and the 30 SOL market-cap gate, 5 buyers in 30 s, buy/sell >= 2.0 (unchanged), rising volume and Scam Shield still apply.
- Creator-selling entry gate, tiered [Claude, user-delegated, 2026-11-27]: the old gate blocked on ANY creator sale in the 60 s window (`devSold > 0`). Evidence from the live paper run (509 coins scored after tuning): it was the only failing gate for MinerPig (score 100, 63 buyers, 20.5 SOL in the curve) and one of two for Firefly (79.7, 72 buyers, 8.5 SOL), so the best coins of the run were rejected for a tiny sale. New rule: allow entry when the creator sold at most `entry.maxCreatorSoldPctOfSupply` = 1% of total supply AND less than `entry.maxCreatorSoldPctOfHolding` = 25% of their own holding (launch buy plus buys in the window); block otherwise. A creator who sold with no known holding counts as 100% and is blocked. Exit behaviour is unchanged: `exits.exitOnDevSell` still sells an open position at once on any dev sale. The dip-40pct lane keeps its own eligibility check (any sale blocks).
- Chase rule fix [Claude, 2026-11-26]: block if the price at decision time is more than 3x the launch price (initial curve), whatever the age. The old rule only applied at age <= 60 s, but the decision comes after the 60 s window, so it never fired. Falling knife: block entry (pre-buy reason `DUMPED_FROM_PEAK`) if the price is more than 40% below its peak of the observation window. Both apply to the momentum lane only; the dip-40pct paper variant has its own logic and is not affected.
- Market-cap entry gate [user approved 2026-11-26, replaces the USD gate]: `feed.minMarketCapSol` = 30 SOL, compared with market cap in SOL straight from the curve. Reasoning: the old $7000 gate depended on the SOL price. At SOL ~$71 it meant ~98.5 SOL, which can never pass together with the chase rule (block above 3x launch within 60 s; a launch starts near 28 SOL, so 3x is ~84 SOL), and a 55 minute live run made zero picks. A SOL-denominated gate does not move when SOL/USD moves. 30 SOL still skips brand-new coins nobody has bought (they sit near 28 SOL) and leaves a passing band of about 30-84 SOL. No entry gate reads the SOL/USD price; USD is only used for display and the JSONL `entryMcapUsd` field.
- Fees: all fees per side are listed once in the Fee model rule; TP1 must clear their sum. TP1 at +20% must still clear break-even at the chosen size; the break-even gate rejects the buy if not.
- Size: 1% of bankroll per trade, clamped to 0.05–0.25 SOL. Fixed size, no doubling after a loss, never add to a loser.
- `entry.style` options: `momentum`, `snipe`, `confirmation`, `dip-40pct`, `graduation-dip`. Default `momentum` [Claude]: the only entry with any track record found (one single-commit repo, now on the do-not-use list, SPEC 3, claimed 458 live trades where buy/sell ratio and volume slope were the strongest predictors and the score gate gave a 41.9% win rate: unverified self-report, and SPEC 13c says the whole scalp may have negative expectancy). `dip-40pct` runs as a parallel paper-only variant so the two can be compared; live uses whichever wins in paper. The observation-window gate applies to every style.
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

## 6c. Deep-curve sprint lane, PAPER ONLY (added 2026-11-28)

A second paper lane next to the dip lane (`sprint` in config.json, lane name `deep-sprint`). It never changes what the main lane does and the live executor never gets it. Source: one tiny backtest (WhoIsTyfur/makefun-memecoin-backtest-48h, 1 star): **every number below is UNVERIFIED** (7 sprint trades, thresholds tuned on the window, the replay closes at the completing trade with no price impact, graduation itself may be bundle manipulated). Its claimed result (about +27 SOL on 10 SOL in 24 h) is NOT in our numbers and is not a promise. The lane exists to measure it.

- **Entry** (watched from the coin's first seconds by `sprint_watch`, once per second, up to 150 s): real reserves 39 to 60 SOL, coin 40 to 120 slots old (slots = latest trade slot minus the creation slot), at least 30 distinct buyer wallets, launch-slot wallets able to pull at most 16 SOL and none of them more than 11 SOL (SPEC 7b), creator has not sold, not mayhem or non-SOL, fewer than 3 sprint positions open, Scam Shield pre-buy rules, RugCheck or the on-chain fallback (half size) and X Monitor blocks all still apply. One entry per coin. A coin that got deep but was not entered gets one `sprint` decision row with the closest set of reasons.
- **Exit**: the percent stops, the take-profit ladder and the fade exits are off. Sell all if the real reserve falls 20 SOL below its entry value or 40 SOL below its peak since entry (graduating curves gave back up to 32 SOL on the way, tighter stops sold them early: unverified), at 90% of graduation (76.5 SOL, SPEC 11b #9), on a dev sell, a cabal dump, a slow rug or a migration, and at the lane's own maximum hold of 600 s (the 300 s of the main lane is too short for a coin to climb from 39 to 76 SOL; this lane is paper only, so the "every position within 5 minutes" rule of the live strategy is not applied to it).
- **Price**: from 39 to 76.5 SOL real the price of the constant-product curve rises about 2.4x ((30+76.5)/(30+39) squared); entry fees round trip about 4.5% plus fixed costs.
- **Stats** are kept apart like the dip lane's (own risk state, positions, journal rows, JSONL).
- Not built: trimming a position above 45% of equity (one position is 0.1 to 0.25 SOL of 10).

## 7. Scam Shield

- Check order: cheap no-RPC checks first, chain checks just before buying. Each check has a 2 s budget and a `required` flag; a required check that times out or is undecided blocks the token.
- New mints: poll at `processed` commitment (8 × 750 ms) since they aren't visible at `confirmed` yet.
- Checks: RugCheck report, honeypot (sell simulation), mint/freeze authority, creator holding %, holder concentration (section 6), dev-snipe in first block, bundler rules (≥20 transfers from ≤5 payers; ≥15 burst transfers in one block; ≥80% supply from ≤3 wallets; tag ACTIVE if within last 15 s, else HISTORICAL), near-identical top-holder balances or clustered funding dates (one entity), liquidity.
- Rug detector on open positions, 1 s loop, points: holder exodus >30% (30), top-10 spike >50% (30), LP fully removed (100), liquidity drain >50% (30), entrapment >15% (20), creator >90% supply (25), fresh wallets >95% (15), price drop >40% (50), price-impact spike >3× in 60 s (40). Response: 10–29 watch, 30–49 warn, 50–74 sell 50%, 75+ full exit. Jupiter `NO_ROUTES` = exit at once.
- Output: one Exit-Liquidity Risk verdict (LOW_SIGNAL / ELEVATED / HIGH) with evidence links (Solscan tx URLs).

### 7b. Graded launch-slot check (added 2026-11-28)

The old rule blocked a coin if ANY other transaction landed in its creation block. The sniping-lab README says professional snipers buy block 0 on most launches and about 62% of new coins have a third-party trade within a second (unverified), so the rule blocked nearly every coin and said nothing about danger. Now (`shield/launch.rs`, `filters.graded`): the chain stage COUNTS the creation-block transactions and reads the trades of the creation slot and the next one with `getTransaction` (at most 8 calls per coin, kept apart from the live store so observation counts do not change). At the end of the watch the engine measures, for every coin, the non-creator wallets that bought in those two slots, what they still hold, and how much SOL they could pull by selling all of it through the current curve (capped by the real reserve). It blocks only above the limits: **launch-slot wallets able to pull more than 16 SOL** (`LAUNCH_SLOT_PULL`), **one wallet able to pull more than 11 SOL** (`LAUNCH_SLOT_SINGLE`), or **the group holding 40% or more of the supply** (`LAUNCH_GROUP_SUPPLY`). 16 and 11 are the sprint research's numbers (UNVERIFIED, one small sample); 40% is the "one-slot collapse" figure from the same research. Every coin that reaches the end of the watch gets a `launch-grade` decision row with the graded values (wallets, creation-slot buys, SOL they could pull, biggest wallet, supply share) whether it blocks or not, so the real blockers can be counted. The extreme tests stay as they were: 15 or more transactions in one block (`BUNDLE_BURST`), 20 or more from 5 or fewer payers, 3 wallets holding 80%. `filters.graded.enabled=false` restores the hard first-block rule. An unmeasurable group (no creation slot, no curve state) is logged as not measured and does not block.

### 7c. Creator funder-chain reputation (added 2026-11-28)

`funder.rs`, `filters.funder`, SQLite tables `funders` and `launch_outcomes`. For each creator wallet that passes the cheap stage, find its first funder: `getSignaturesForAddress` (25 signatures or more, or a first transaction older than 48 h, means the wallet is not new and is not judged) then `getTransaction` on the oldest one for the first system transfer of at least 0.01 SOL into it (2 RPC calls). The answer is cached in SQLite (7 days; a failed lookup 60 s), at most 2 lookups run at once and none for 30 s after the node says 429. A funder is a serial dead-launcher funder when at least 2 OTHER creators it funded each have at least 2 recorded launches of which at least 80% died. A creator funded by such a funder is blocked (`FUNDER_SERIAL_DEAD`, a Scam Shield block with a Solscan link to the funder). A funder that funded more than 25 creators is a hub (exchange, faucet) and is never judged; no exchange address list is hard-coded because none can be verified here. A coin "died" when its 60 s watch ended with fewer than 5 buyers and no price rise, or with half its peak gone: an approximation of the research's "80% dead after 5 minutes". Unknown, old, failed and skipped lookups pass with a note and never block. **Both tables start empty, so the block can only fire after the app has seen several launches from one funder.** Research numbers (TrenchTruth, 15,008 non-mayhem launches, hackathon repo, UNVERIFIED): fresh creators funded by a clean wallet were 78% dead in 5 minutes (n=4,712), by a serial dead-launcher funder 91% (n=2,093), by a good funder 59% (n=122).

## 8. Paper mode

- Same rules engine as live; only the executor differs.
- Fills: bonding-curve constant-product formula, plus every cost in the Fee model (section 6), price impact, a fill delay, and a failed-fill rate. Never mark at mid-price. All costs come out of the paper SOL balance: buy spend (holds the buy-side percent fees), fixed fees, fees of reverted/retried transactions, token-account rent 0.00203928 SOL taken on the buy fill and returned only when a full sell closes the account (while open it is shown as "rent locked"), sell-side percent fees on the SOL actually received. The balance change of a closed trade equals `pnlSol` to the lamport (unit test).
- One JSONL line per closed trade in `paper-logs/YYYY-MM-DD.jsonl`, exactly this shape (`feeBreakdown`'s parts sum to `feesSol`; rent is not a fee; `xCommunity` is the last key, added 2026-11-28, and is `null` when X Monitor had nothing for the coin or the line is older):

```json
{"ts":"2026-11-25T15:02:11-08:00","mint":"7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgHR","symbol":"MOON","side":"buy","sizeSol":0.2,"entryPrice":0.00042,"entryMcapUsd":12400,"exitPrice":0.00081,"exitReason":"take-profit-1","pnlSol":0.176,"pnlPct":88.0,"holdSeconds":112,"feesSol":0.00551,"mode":"paper","feeBreakdown":{"pumpFun":0.0019,"creator":0.0006,"pumpPortal":0.002,"tx":0.00001,"priority":0.001,"jito":0.0},"xCommunity":null}
```

- `xCommunity` (SPEC 4e) holds every part of the X Monitor score so the weights can be tuned from real trades: `{"status":"ready","reason":"","score":72.4,"points":11,"components":{"kol":18.0,"engagement":12.6,"replies":0.0,"growth":5.0,"integrity":8.0,"community":0.0,"narrative":3.0},"flags":["6 smart follower(s) (@a, @b, @c)"],"blocks":[],"rank":0,"clones":0,"kind":"profile","handle":"moonbearsol","tweetId":"","url":"https://x.com/moonbearsol","ageMin":2}`. `status` is ready, unknown, blocked, nolink or off. The same numbers go into the learning journal entry and the Obsidian trade note, and a `decisions` row with stage `x-monitor` is written for every coin X Monitor looked at (it is not counted as a Scam Shield block).
- Daily summary: win rate, avg win, avg loss, profit factor, max drawdown, total PnL, trades per day.
- Promotion to live: at least a few days and at least 100 trades, stats reviewed with the user, positive after fees and slippage.

## 9. Dashboard and UI

Theme: white background, white cards, green accents (#16a34a primary, #14532d text/headers, light green tints). Red only for losses, sells, danger. No dark mode. No other colors.

- Top bar: local clock; trades today (hover: buys/sells); SOL balance + USD portfolio value; P/L today and total (SOL + USD, green/red); mode badge `PAPER` (green outline) or `LIVE` (solid red).
- Network graph: central Claude bot node (larger, app name under it) with 9 agent nodes: Scam Shield, Token Picker, Executor, Hold Timer, Thesis Finder, Memecoin Monitor, X Monitor, Top Trader Monitor, Top Gainers. Each has icon, name, status dot (green working / amber waiting / grey idle / red blocked), one-line live status. Idle lines thin light grey; on a message the line pulses green with a travelling dot. Click a node for a panel: current task, last 10 decisions, settings. The X Monitor and Thesis Finder nodes are live (section 4e).
- Activity feed under the graph: timestamped, newest first, clickable to token detail.
- "Why no trades?" card (first widget): real counts for today (UTC day) from SQLite: coins seen, blocked by Scam Shield with the top reasons, passed and still being checked, watched and scored (and how many had no trade at all), best score and its coin, the entry gates that stopped the 10 best-scoring coins that were not picked, picked and bought, and how many coins skipped RugCheck (passed or blocked on-chain). Plain text above it states the biggest drop-off in one sentence. Tables: `decisions`, `scores`, `tokens_seen`, `positions`. Nothing is estimated.
- Buy/sell data health (SPEC 6): a coin scores 0 without trades, so the Memecoin Monitor node shows "Live. N trades/min on M coins" and its panel and Settings > Connections show: both feeds connected, trades read in the last minute, coins watched and subscriptions the node confirmed, last trade seen, last error from the node (keys masked), RugCheck requests that worked. If coins have been watched for 2 minutes with 0 trades decoded, a red bar says "No buy/sell data is arriving, so every coin scores 0." (`engine/src/health.rs`).
- Widgets: X community card (rising accounts, latest checks, links to the exact post or profile), Thesis Finder card (stories and narratives), Watchlist, Open Positions, Position Chart (candles, green entry marker, red exit marker, red dashed SL line, green dashed TP lines, live price line), Scam Shield (blocked count + breakdown by reason + verdict), Fees Paid (today/all-time, split PumpPortal / tx / priority / Jito, per-trade fee), Performance (win rate, avg win/loss, profit factor, max drawdown, best/worst, trades/day, equity curve), Settings page.
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

29. RugCheck is not a single point of failure [Claude, user-reported, 2026-11-27]. A 75-minute run on a Windows PC with a working Helius key saw 321 coins and blocked 307; 114 of those blocks were `UNDECIDED_RUGCHECK` ("rugcheck http 502", "error sending request", "RugCheck has no report yet"). The checks ran 1-4 s after launch, so a third of all coins were refused only because a free website was down, rate limited or had not indexed a brand-new coin yet. That is not a scam signal. New rule:
   - One shared RugCheck client (`shield/net.rs`): at most 2 requests in flight, 2 requests per second, a cache per mint (10 min).
   - It runs in the background from the moment Scam Shield starts, in parallel with the 60 s watch. A 502, network error, timeout, 429 or "no report yet" is retried after 3 s, 8 s and 15 s (4 tries in about 26 s). The coin keeps being watched meanwhile. A report that arrives is judged at once: a rugged or danger flag, holder concentration or the creator's live balance end the watch early with a block.
   - If RugCheck is still silent when the coin would otherwise be picked, the app judges it on its own chain data. ALL must pass: mint authority revoked, freeze authority revoked, no dangerous Token-2022 extensions (transfer hook, permanent delegate, transfer fee, close authority), no first-block snipe and no burst bundle, top holders (single wallet <= `maxSingleHolderPct`, top 10 <= `maxTopHoldersCombinedPct`, bonding curve excluded, read with `getTokenLargestAccounts` + `getMultipleAccounts`), creator holding (launch buy and live balance <= `maxCreatorHoldingPct`), creator not blacklisted. Any failed or unreadable check blocks. The decision is recorded as "RugCheck unavailable, passed on-chain checks" (stage `rugcheck-fallback`, verdict ELEVATED) and the trade is HALF the normal size (`Pick.size_factor = 0.5`, the risk gates still apply to the halved size).
   - A RugCheck danger flag (or "rugged") ALWAYS blocks, whatever the switch says.
   - `filters.requireRugcheck` (default false): true restores the old behavior (no report, no buy: `UNDECIDED_RUGCHECK`).
   - Why this is safe enough: the on-chain checks cover what RugCheck's report adds for pump.fun coins (authorities and holders; its other "danger" flags on bonding-curve coins are mostly curve artifacts, see `parse_rugcheck`), half size limits the loss on the coins that skipped the second opinion, and the on-chain list is stricter than "no report". What is lost: RugCheck's insider-network and creator-history signals for those coins. Paper mode will show whether half-size fallback coins do worse than full-size ones (the decision rows make them countable).
   - Image check: the coin's metadata file decides; RugCheck's copy of the image is no longer a second chance because the chain stage no longer waits for it.

All 29 protections are acceptance items: each needs a unit test or a recorded demo before its milestone closes (token checks and execution rules in M2/M3, live-only rules in M4).

## 11c. Updates, vault and connectors (M3d)

- Self-update: signed releases only (`tauri-plugin-updater`, public key in tauri.conf.json), installed only after a click. Open paper positions are sold through the normal exit path first (exit reason `update`), never through the persistent kill switch.
- Obsidian vault: markdown in `<vault>/Claude Trader/` (Trades, Coins, Days, Lessons, Situations, Index), YAML frontmatter, wikilinks, atomic writes, nothing deleted, foreign notes never overwritten, content only from recorded data.
- Outgoing connectors (Discord webhook, generic webhook): secrets in OS secure storage, masked everywhere, paced and queued, a failed send never blocks trading, "Paper results" label while paper, payload `claude-trader/event/v1`. No inbound connector can place a trade.

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

Sources for the scalp values: kontia1/gmgn-screener README (stops, ladder shape); moonkeemoo/oilfront research doc (fees, base rates: win rate went 22% to 36% only after adding filters); madeonsol surge tracker, search snippet only (65 surges, median peak 1.45x at ~4 min, 60% peaked within 5 min); one single-commit repo's smart_exit.py (fade exits; that repo is now on the do-not-use list). Medium, alchemii.io and the vercel counter-study were blocked by the network and are unread.

## 13c. Evidence (added 2026-11-28)

Source of everything in this section: the GitHub research of 2026-11-28 (`github-improvements.md`, scratch notes, not in the repo) plus the official pump-fun/pump-public-docs. Almost all numbers come from tiny repos (0 to 3 stars, some hackathon or student projects), are self-reported and were not cross-checked. **Sourced** means read in the official pump docs; everything else is **unverified**.

| Claim | Number | Status |
|---|---|---|
| Coins dead within 5 minutes | 82% (n=15,008 non-mayhem launches) | unverified (TrenchTruth) |
| A sniper never reaches even 1.3x on 82% of launches | 82% (n=250, checked 40 to 120 min later) | unverified (rug-radar) |
| Buy 30 s after launch, sell at 5 minutes | average -11% to -16%, median -25% to -32% before fees, even with a learned score that picked survivors 3x better (top 10% survived 54% against 18%) | unverified (TrenchTruth) |
| Same strategy live, with a fast server and shred feed, every exit rule | average -3.5% to -22% before fees | unverified (TrenchTruth) |
| A momentum bot that looked positive on the chart | closed at a loss once 1 to 3 s of fill delay was real; one +24% in 4 min session gave it all back over 32 min with about 90 round trips | unverified (charllysb) |
| Graduation rate | 1.82% (583 of 32,000); published estimates range from 0.2% to 6% depending on what is counted | unverified (student capstone) |
| Median trades in a coin's first 30 minutes | about 26 | unverified |
| Transactions touching the program that fail | about 60% | unverified (sniping lab) |
| Real cost per side on the curve | 1.25% (0.95% protocol + 0.30% creator) | matches our config; the exact split is in the pump docs' fee fields, read live at start |
| Creator with 2 or more earlier launches and 80% dead | 94% dead in 5 min (n=4,527); under 50% dead: 20% (n=289) | unverified |
| Fresh creator funded by a serial dead-launcher funder | 91% dead (n=2,093) against 78% (clean funder, n=4,712) and 59% (good funder, n=122) | unverified |
| Deep curve sprint (39 to 60 SOL, 40 to 120 slots) | 7 trades, 5 graduated, 2 stopped, about +27 SOL on 10 SOL in 24 h; "a quiet day earns about nothing"; each failure costs 35% to 50% of the stake | unverified, tiny sample, tuned on its window |
| Block 0 tax, curve and Global layouts, holder rewards, synthetic migration, mayhem and quote-mint fields | as in section 5b | sourced (official docs) |

**What this means, honestly.** The core strategy of this app (momentum scalp: buy a coin about a minute after launch, sell within 5 minutes) may have NEGATIVE expectancy. Several independent small studies found that buying fresh pump.fun coins soon after launch and scalping out loses on average after costs, even when the entry filter picks better coins than chance, because about 8 in 10 coins are dead within 5 minutes and the rest do not pay for the losers plus 4.5% in fees per round trip. Our own live paper run made 0 trades, which fits a market where almost nothing passes our gates. None of this proves our version loses: our gates are stricter than the studies'. It means nobody should assume it wins. The paper lanes (momentum, dip-40pct, deep-sprint, the learning test lane) exist to MEASURE this with real fills and fees. Go-live stays gated by SPEC 8: at least 100 paper trades, positive after fees, and the promotion rule of section 4c (a holdout whose mean is above 0 with the lower 95% bound above 0). If the lanes show that no lane has positive expectancy, the right outcome is to not go live.

## 14. Open items

Trading values are decided by Claude from research (user delegated this on 2026-11-25). Only these need the user:
1. Before M2: free Helius API key.
2. Before M4: approval of paper stats; live per-trade size.
3. Before M4: confirm pump.fun and PumpPortal fees against their docs (hosts were blocked during research).

Paper-fill numbers (fail rates, delay) are conservative estimates with no published source; recalibrate from real M4 fills.
