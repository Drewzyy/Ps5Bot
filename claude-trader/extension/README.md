# Claude Trader Chrome extension

An optional add-on for the Claude Trader desktop app. Paper trading only.

What it does:
- Side panel: balance, P/L today and total, open positions, activity, "What it learned", kill switch (with a confirm) and Resume.
- Toolbar badge: today's P/L (green or red), or the number of open positions. Pick which in the side panel.
- Coin card on pump.fun coin pages and dexscreener.com/solana pages: Scam Shield result, score and what the bot is doing with the coin (held, watching, skipped, blocked). "Check this coin" runs a scam check only. It never buys. The bot trades only by its own rules.
- Alerts (buy, sell, daily loss limit, kill switch, engine offline), each with an on/off toggle.
- X Monitor (optional, off until the app turns it on): reads the X (Twitter) page a coin links to, in one background tab, with a spare logged-in X account. Read-only. See below.
- Tabs: opens pump.fun and the Dexscreener chart of each coin the bot buys, reuses tabs that are already open, and can close a coin's tab when its trade closes (a setting in the app). It only closes tabs it opened itself.

## Install (once)

Chrome does not allow apps to install extensions for you, so this is by hand. It takes about a minute.

1. Get the folder. Easiest: in the Claude Trader app open **Settings > Startup and browser > Install the Chrome extension** and press **Show the extension folder**. (Or download the `claude-trader-chrome-extension` artifact from the GitHub Actions run, and unzip it.)
2. In Chrome type `chrome://extensions` in the address bar and press Enter.
3. Turn on **Developer mode** (top right).
4. Press **Load unpacked** and choose the folder that holds `manifest.json`.
5. Click the puzzle icon in Chrome and pin **Claude Trader**. Click its icon to open the side panel.
6. In the app press **Get pairing code** (same Settings section). Type the code in the side panel and press **Connect**.

After that it connects by itself whenever the app is open. If the app is closed the panel says **Engine offline** and connects again when you start the app.

## How it is kept safe

- **Fixed ID.** `manifest.json` holds a public `key`, so the extension ID is always `jihnmdnpkeglfhoombfjhdfbmiopnljj`. The app only accepts connections whose browser origin is exactly `chrome-extension://jihnmdnpkeglfhoombfjhdfbmiopnljj`. A website cannot set that origin, so websites cannot connect. (The `key` is only the public half. There is no private key in this repository, and none is needed to load the extension unpacked. If you ever publish it in the Chrome Web Store, Google uses its own key and the ID changes, so `EXTENSION_ID` in `engine/src/bridge.rs` and this file must be updated together. A test checks that the manifest key and the ID in the app agree.)
- **Local only.** The app listens on `ws://127.0.0.1:8766` (this computer only).
- **Pairing.** The app shows a one-time code (valid 10 minutes, works once). The extension swaps it for a random token and keeps the token in Chrome's extension storage. The app keeps only a hash of the token. Without the code or token the app sends nothing. Five wrong tries lock sign-in for a minute. **Unpair** in the app or in the side panel ends it.
- **No secrets.** The extension never receives the Helius key or anything else secret. It sees the same numbers the dashboard shows.
- **No trading power.** The extension can ask for a coin's verdict, ask for a scam check, press the kill switch and press Resume. There is no message that buys or sells, and the app refuses any other command (tested).
- **Page access.** The coin card only runs on pump.fun and dexscreener.com/solana and reads the page address and token links. The X Monitor scripts run on x.com only, and do nothing unless the background worker says the tab is the bot's own job tab, so your normal x.com browsing is never read.

## X Monitor (spare X account, no paid API)

The app queues the X pages it wants read (a coin's profile, post or community, and who follows it). The extension opens each one in one reused, unfocused background tab and sends back what the page showed.

Set up (once):
1. Make a **spare X account**. Do not use your main one. X bans automated reading and the account could get locked.
2. Log into x.com in the Chrome profile the bot uses.
3. Optional but it makes the score much better: with that account, follow a few hundred crypto KOLs. The score counts how many of them follow a coin's account (the page `x.com/<handle>/followers_you_follow`).
4. Keep the extension paired. In the app open Settings > X Monitor (on by default, with a pause button and the pacing numbers).

How it works and stays safe:
- `xhook.js` runs in the page (MAIN world, document_start, x.com only). It wraps fetch and XMLHttpRequest and copies x.com's own GraphQL answers by operation NAME (the id in the URL rotates): AboutAccountQuery (username changes, country, affiliate), UserByScreenName, UserByRestId, UserTweets, TweetDetail, CommunityQuery, CommunityTweetsTimeline, membersSliceTimeline_Query, moderatorsSliceTimeline_Query and followers_you_follow. About data is kept only if the page fetches it by itself while a profile loads; the extension never opens an About page to `xcontent.js`. It never changes a request. It never sends one.
- `lib/xparse.js` parses those answers defensively (X changes the shapes every few weeks) and reports drift. If nothing arrives in 14 seconds, `xcontent.js` reads the page text instead.
- **Read only.** Nothing here posts, likes, follows, types or clicks (a test scans the source). The app can only ask for four kinds of address: `x.com/<handle>`, `x.com/<handle>/followers_you_follow`, a post and a community. Anything else is refused.
- **Pacing** (`lib/xpace.js`): one page every 8 to 15 seconds (random), at most 120 pages an hour and 800 a day, the same page is not read again for 30 minutes. Never faster than 5 seconds. If X shows a rate-limit or lock page, or answers 429, the extension stops for 15 minutes, then 1 hour, then the rest of the day, and the app shows "X paused: account limited". A login page marks x.com as logged out and pages stop.
- The app, not the extension, builds the addresses, so a web page cannot make the extension open anything.

Not verified here: the real x.com (this sandbox cannot reach it). The parsers are tested on synthetic fixtures in `tests/fixtures` (made by `tests/make-fixtures.mjs`). The first real run is the real test. Community page operation names are guesses.

## Files

- `manifest.json` Manifest V3.
- `background.js` connection, badge, alerts, tabs.
- `content.js`, `lib/mint.js` the coin card.
- `sidepanel.html/.css/.js` the side panel.
- `lib/core.js` pure logic (tested in `tests/core.test.js`).
- `xhook.js`, `xcontent.js`, `xrunner.js`, `lib/xparse.js`, `lib/xjob.js`, `lib/xpace.js` the X Monitor (tested in `tests/xmon.test.js`). Run everything with `npm test` in this folder.

## Limits

- Chrome only (version 116 or newer, for the side panel).
- The Dexscreener page layout is not under our control. The coin is found from the address, or from a token link on the page, or from Dexscreener's public API (`api.dexscreener.com`).
- Chrome puts a service worker to sleep when idle. The app pings it every 20 seconds and an alarm wakes it every 30 seconds, so it reconnects within about half a minute at worst.
