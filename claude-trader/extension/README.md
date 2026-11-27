# Claude Trader Chrome extension

An optional add-on for the Claude Trader desktop app. Paper trading only.

What it does:
- Side panel: balance, P/L today and total, open positions, activity, "What it learned", kill switch (with a confirm) and Resume.
- Toolbar badge: today's P/L (green or red), or the number of open positions. Pick which in the side panel.
- Coin card on pump.fun coin pages and dexscreener.com/solana pages: Scam Shield result, score and what the bot is doing with the coin (held, watching, skipped, blocked). "Check this coin" runs a scam check only. It never buys. The bot trades only by its own rules.
- Alerts (buy, sell, daily loss limit, kill switch, engine offline), each with an on/off toggle.
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
- **No page access.** It only runs on pump.fun and dexscreener.com/solana. It reads the page address and the token links to find the coin. It does not read anything else on the page.

## Files

- `manifest.json` Manifest V3.
- `background.js` connection, badge, alerts, tabs.
- `content.js`, `lib/mint.js` the coin card.
- `sidepanel.html/.css/.js` the side panel.
- `lib/core.js` pure logic (tested in `tests/core.test.js` with `node --test tests/core.test.js`).

## Limits

- Chrome only (version 116 or newer, for the side panel).
- The Dexscreener page layout is not under our control. The coin is found from the address, or from a token link on the page, or from Dexscreener's public API (`api.dexscreener.com`).
- Chrome puts a service worker to sleep when idle. The app pings it every 20 seconds and an alarm wakes it every 30 seconds, so it reconnects within about half a minute at worst.
