import type { Config } from "../types";

/** SPEC 11b item 28: live stays locked until this many paper trades. */
export const LIVE_MIN_PAPER_TRADES = 100;
/** pump.fun graduation target in SOL (the engine uses the same 85). */
export const GRADUATION_SOL = 85;

export interface Step {
  id: string;
  title: string;
  /** CSS selector of the dashboard area to highlight, or null for a centred card. */
  target: string | null;
  /** Heading text to look for inside .card h3 when the target is a card. */
  card?: string;
  what: string[];
  good: string;
  bad: string;
  todo: string;
}

export interface Entry { q: string; a: string[] }
export interface Topic { id: string; title: string; intro: string; entries: Entry[] }
export interface Term { term: string; def: string }

const secs = (s: number) => (s % 60 === 0 ? `${s / 60} minutes` : `${s} seconds`);

export function derive(c: Config) {
  const t = c.exits.tranches;
  const roundTrip = (c.fees.modelPumpFunPctPerSide + c.fees.pumpPortalPctPerSide) * 2; // % of size, before network and priority fees
  const ladder0 = t.map((x) => `sell ${x.sellPct}% at +${x.atPct}%`).join(", then ");
  const ladder = ladder0.charAt(0).toUpperCase() + ladder0.slice(1);
  const tail = 100 - t.reduce((a, x) => a + x.sellPct, 0);
  return { t, roundTrip, ladder, tail, hold: secs(c.exits.maxHoldSeconds) };
}

export function buildSteps(c: Config): Step[] {
  const d = derive(c);
  return [
    {
      id: "welcome", title: "Welcome. Here is what this app does", target: null,
      what: [
        "This app watches new coins on pump.fun, a site where anyone can launch a joke coin in seconds.",
        "It checks each coin for scams, scores the ones that look alive, and buys a few with pretend money. Then it sells within " + d.hold + ".",
        "You do not need to trade or click anything. The app runs on its own.",
      ],
      good: "Most days you will mostly watch numbers and read.",
      bad: "Do not expect big wins. Most memecoins lose money and many go to zero. This app is a test to see if a careful method can beat that.",
      todo: "Nothing. Skim the next few steps, then leave it running.",
    },
    {
      id: "mode", title: "Paper mode and the live lock", target: ".topbar .badge",
      what: [
        "The green PAPER badge means every trade is pretend. No real money, no wallet.",
        `Live mode stays locked until the app has made ${LIVE_MIN_PAPER_TRADES} paper trades and the results look good after fees.`,
        "Paper results look better than real ones. Real trades get worse prices, fail more, and compete with faster bots.",
        "The red Kill switch button (or Ctrl+Shift+K) stops all trading and sells every open position at once.",
      ],
      good: "PAPER badge in green. Profit after fees over many trades.",
      bad: "A red LIVE badge before you meant to switch. Or good paper profit from only a handful of trades, which is mostly luck.",
      todo: "Stay on paper. Do not go live until you have read the risk tips (Settings > Tips and help) and the results hold up.",
    },
    {
      id: "graph", title: "The agents and the network", target: ".graph-wrap",
      what: [
        "Each box is a small worker. Memecoin Monitor finds new coins. Scam Shield checks them. Token Picker scores them. Executor buys and sells. Hold Timer watches each trade and decides when to exit.",
        "Lines light up when one worker hands a coin to the next. Dots show status: green working, amber waiting, grey idle, red blocked.",
      ],
      good: "Memecoin Monitor green and lines pulsing now and then.",
      bad: "Memecoin Monitor red or amber for a long time. That means the coin feed dropped. It usually reconnects by itself.",
      todo: "Usually nothing. Click a box to read its last decisions.",
    },
    {
      id: "shield", title: "Scam Shield and why most coins get blocked", target: ".card", card: "Scam Shield",
      what: [
        "Scam Shield looks for tricks: the creator still holding a lot, hidden fees, bundled wallets, a creator who rugged before.",
        "Most coins fail a check. A high blocked count is normal and good. It is not a bug.",
        "A check that cannot finish in time also blocks the coin. The app prefers to miss a good coin than to buy a bad one.",
        "The one exception is RugCheck, a free outside website that rates coins. It is often busy or has not seen a brand new coin yet, so the app keeps asking it while it watches the coin. If it still has no answer, the app runs its own checks on the blockchain and, if every one passes, may buy at half the normal size. A RugCheck danger flag always blocks.",
      ],
      good: "Many blocks, with clear reasons listed.",
      bad: "Zero blocks over a long time, or every coin passing. That would be suspicious.",
      todo: "Nothing. Read the reasons if you are curious. Verdicts: LOW SIGNAL means no red flags found, ELEVATED means some, HIGH means serious.",
    },
    {
      id: "funnel", title: "Why no trades? Where coins drop off", target: ".card", card: "Why no trades?",
      what: [
        "This card counts today's real coins at each step: seen, blocked by Scam Shield (with the top reasons), watched and scored, best score, and picked or bought.",
        "It also shows which rule stopped the best-scoring coins, so you can see whether the app is too strict, the market is quiet, or something is broken.",
        "If it says coins were watched but not one buy or sell was seen, the buy/sell data is not arriving. Look at the red bar at the top and at Settings > Connections.",
      ],
      good: "Most coins stop at Scam Shield, a few reach scoring, and the best ones are stopped by a clear rule.",
      bad: "A red warning that no buy/sell data is arriving. Every coin scores 0 until that is fixed.",
      todo: "Add a free Helius key if the warning appears. Do not loosen the rules to force trades.",
    },
    {
      id: "picker", title: "Watchlist and the Token Picker score", target: ".card", card: "Watchlist",
      what: [
        "Every coin the app looks at shows up in the Watchlist, with a status and the reason.",
        `Coins that pass Scam Shield get watched for a minute and scored from 0 to 100. The bar to buy is ${c.observation.minScore}. The score uses buyers vs sellers, how many different wallets are buying, and whether price and volume are rising.`,
        `Even a high score must also pass entry rules, such as curve SOL between ${c.entry.curveSolMin} and ${c.entry.curveSolMax}.`,
      ],
      good: "A few coins reach a high score. Most score 0, because most new coins get almost no real buyers.",
      bad: "A high score is not a promise. It only means the coin looked active in its first minute.",
      todo: "Nothing.",
    },
    {
      id: "positions", title: "Open positions and the exit rules", target: ".card", card: "Open positions",
      what: [
        "An open position is a coin the app currently holds. You see entry price, current profit or loss, and the time left.",
        `Exit rules in plain words: ${d.ladder.charAt(0).toLowerCase() + d.ladder.slice(1)}. The last ${d.tail}% rides a trailing stop that sells if price falls ${c.exits.trailingStopPct}% from its peak.`,
        `Stop loss: at ${c.exits.softStopPct}% the app waits ${c.exits.softStopRecoverySeconds} seconds for a bounce, at ${c.exits.hardStopPct}% it sells at once.`,
        `Time limit: everything is sold after ${d.hold}, no exceptions. It also sells early if the creator sells, a whale dumps, or the coin is about to graduate.`,
      ],
      good: "Positions that close on their own within a few minutes, win or lose.",
      bad: "A position older than the time limit. That should never happen.",
      todo: "Nothing. Do not try to hold longer. Losses are small on purpose.",
    },
    {
      id: "perf", title: "P&L, win rate, profit factor, drawdown", target: ".card", card: "Performance",
      what: [
        "P&L is profit and loss in SOL, after all fees.",
        "Win rate is the share of trades that made money. A win rate under 50% can still profit if wins are bigger than losses.",
        "Profit factor is total wins divided by total losses. Above 1 means ahead. Below 1 means behind. Treat anything under about 30 trades as noise.",
        "Max drawdown is the biggest drop from a high point. It tells you how bad the bad stretches get.",
      ],
      good: "Profit factor above 1 after many trades, with a small drawdown.",
      bad: "Profit factor below 1 over 30 or more trades, or a drawdown that keeps growing.",
      todo: "Check once a day at most. Losing days are normal.",
    },
    {
      id: "fees", title: "Fees", target: ".card", card: "Fees paid",
      what: [
        `Every buy and every sell costs money. The app models about ${d.roundTrip.toFixed(1)}% for a round trip, plus a small priority fee and network fee.`,
        "On small trades these costs eat a big part of any gain. A coin has to rise a few percent just to break even.",
      ],
      good: "Fees small compared to profit.",
      bad: "Fees larger than profit. Then the method does not work at this size.",
      todo: "Nothing. The app already refuses trades where fees are too large.",
    },
    {
      id: "zero", title: "When nothing happens for a long time", target: ".card", card: "Open positions",
      what: [
        "0 trades for hours is normal. The app only buys when a coin passes every check and has a high score.",
        "Slow markets and strict rules mean some days have no trades at all. Do not loosen settings to force trades.",
        "If the status says the feed is down, check your internet and wait a minute.",
      ],
      good: "A few trades per day or none. Patience is the point.",
      bad: "Feed red for more than a few minutes.",
      todo: "Nothing.",
    },
    {
      id: "helius", title: "Add your free Helius key", target: ".nav",
      what: [
        "The app works without it, but the public connection drops often and misses trades. A free Helius key gives a steadier line.",
        "Get a free key at helius.dev, then Dashboard > API Keys. Copy it.",
        "In the app open Settings, then Connections. Paste the key, press Save, then Test connection. You should see OK with a time.",
        "The key is kept in your computer's secure storage. It is never shown again, only its last four characters.",
      ],
      good: "OK with a time under about 500 ms.",
      bad: "An error message. The usual cause is a missing character when copying. Paste it again.",
      todo: "Do this once. It takes effect right away, no restart.",
    },
    {
      id: "learning", title: "What it learned", target: ".card", card: "What it learned",
      what: [
        "After each closed trade the app writes a short note with the real numbers: profit after fees, how long it held, why it sold, and how the coin looked when it was bought.",
        "Every 30 trades it looks for patterns. It only reports one if at least 10 trades back it up and the gap is large.",
        "A change is tried in a separate pretend lane first and used only if it beats the normal one. It can never loosen Scam Shield, raise trade size or loosen loss limits, and every change has an Undo button.",
      ],
      good: "Short notes with numbers, and few changes.",
      bad: "Many changes in a short time. That means the app is chasing luck. Use Undo.",
      todo: "Read it now and then. With no closed trades it says so.",
    },
    {
      id: "settings", title: "Settings and the i icons", target: ".nav", 
      what: [
        "Open Settings to see every rule as a number. Each row has an i icon.",
        "Click it for a plain explanation: what the setting does, what a good value is, and how to change it.",
        "The numbers were chosen from research. Paper mode is where they get tested. Changing them without knowing why usually makes results worse.",
      ],
      good: "Leave defaults alone while you learn.",
      bad: "Raising trade size or loosening stops after a loss.",
      todo: "Read a few i icons for fun. Replay this tour or read more under Settings > Tips and help (the ? button) any time.",
    },
  ];
}

export function buildTopics(c: Config): Topic[] {
  const d = derive(c);
  return [
    {
      id: "basics", title: "The basics", intro: "What is going on, in plain words.",
      entries: [
        { q: "What is pump.fun?", a: ["A site where anyone can create a coin in seconds, with no code and almost no cost. Thousands launch every day. Nearly all are jokes or scams."] },
        { q: "What is a bonding curve?", a: ["A formula that sets the price. Early buyers pay little. Each buy pushes the price up, and each sell pushes it down.", "There is no order book. You trade against the curve itself."] },
        { q: "What is graduation or migration?", a: [`When about ${GRADUATION_SOL} SOL has been paid into a coin's curve, it graduates. Its liquidity moves to a normal exchange pool (PumpSwap).`, "Few coins get there. This app sells before that point, so it never holds through the move."] },
        { q: "Market cap vs curve SOL", a: ["Curve SOL is the real SOL buyers have paid into the curve so far.", `Market cap is price times all coins. The app counts it in SOL (a new coin starts near 28 SOL), so the entry rule does not change when the dollar price of SOL moves. It is always bigger than curve SOL. The minimum is ${c.feed.minMarketCapSol} SOL.`] },
        { q: "What is a scalp?", a: [`A very short trade. This app holds for at most ${d.hold}. Quick trades mean many trades, and every trade pays fees.`] },
      ],
    },
    {
      id: "scams", title: "Scams and traps", intro: "What Scam Shield looks for.",
      entries: [
        { q: "Rug pull", a: ["The creator or insiders sell everything and the price collapses. Buyers are left holding coins nobody wants."] },
        { q: "Honeypot", a: ["A coin you can buy but cannot sell. The app tests this before buying once live mode exists. In paper mode this check is not available yet, and the dashboard says so."] },
        { q: "Dev sell", a: ["The creator sells their share. It is a strong warning. The app exits at once if it sees one."] },
        { q: "Bundles", a: ["The creator uses many wallets to buy at launch so the coin looks popular and spread out. In fact one person controls much of the supply. Blocked automatically."] },
        { q: "Snipers", a: ["Bots that buy in the first moments after launch, often with insider knowledge. They sell to later buyers. The app blocks coins sniped in the first block and never buys in the first minute."] },
        { q: "RugCheck", a: ["A free website that rates coins and lists the biggest holders. It is often down (HTTP 502), slow, or has not indexed a new coin yet. That says nothing about the coin, so the app retries after 3, 8 and 15 seconds while it keeps watching, and never treats an outage as a scam sign.", "If it still has no answer when a coin would be bought, the app uses its own blockchain checks: mint and freeze authority gone, no hidden Token-2022 tricks, no big holders, creator holding small, no first-block sniping, creator not on the blacklist. All must pass, and the trade is half size. A danger flag from RugCheck always blocks. Switch on Settings > Scam Shield filters > requireRugcheck to block every coin RugCheck cannot rate."] },
        { q: "Why is most of it blocked?", a: ["One security firm (Solidus Labs) flagged about 98.7% of pump.fun coins as showing pump and dump or rug patterns. pump.fun disputes the figure. Either way, most coins are bad, so blocking most is correct."] },
      ],
    },
    {
      id: "costs", title: "Costs and prices", intro: "Why small trades are hard.",
      entries: [
        { q: "Fees", a: [`The app assumes about ${d.roundTrip.toFixed(1)}% round trip (pump.fun ${c.fees.modelPumpFunPctPerSide}% and PumpPortal ${c.fees.pumpPortalPctPerSide}% each way). The real rates may differ and are to be checked before live.`] },
        { q: "Priority fee", a: ["A small tip paid to get your transaction processed sooner. Too low and it may fail or arrive late. Too high and it eats the profit. The app caps it."] },
        { q: "Slippage", a: ["The gap between the price you expected and the price you got. On thin coins the price can move between your click and the fill."] },
        { q: "Why fees eat small trades", a: [`Fixed costs hit small trades hardest. The app buys ${c.risk.minTradeSol} to ${c.risk.maxTradeSol} SOL per trade and refuses a trade if the first profit target would not clear fees by a safe margin.`] },
      ],
    },
    {
      id: "risk", title: "Honest risk", intro: "Read this before you ever think about live mode.",
      entries: [
        { q: "Why do most memecoins go to zero?", a: ["There is no business behind them. Price depends only on new buyers showing up. When they stop, it falls. Insiders often sell first."] },
        { q: "Why paper profit does not promise live profit", a: ["Paper trades are simulated. The real world has slower fills, failed transactions, bots that react faster, and prices that move against you. The app adds delays, failures and fees to paper, but that is still a guess."] },
        { q: "Is scalping risky?", a: ["Yes. Many small trades mean many chances to lose and many fees to pay. A few bad trades can erase many small wins."] },
        { q: "How the app limits damage", a: [`Each trade is about ${c.risk.maxPctBankrollPerTrade}% of bankroll. The app stops for the day after a ${c.risk.dailyLossLimitPct}% loss, or after ${c.risk.consecutiveLossStop} losing trades in a row. It holds at most ${c.risk.maxOpenPositions} coins at once.`, "These limits reduce losses. They do not make losses impossible."] },
        { q: "Kill switch", a: ["The red Kill switch button at the top (or Ctrl+Shift+K) stops all trading: no new buys, and every open position is sold now. It stays off, even after a restart, until you press Resume trading and confirm."] },
        { q: "The live lock", a: [`Live mode stays locked until ${LIVE_MIN_PAPER_TRADES} paper trades exist and results are positive after fees. Never put money in that you cannot lose.`] },
      ],
    },
    {
      id: "reading", title: "Reading the dashboard", intro: "What the numbers mean.",
      entries: [
        { q: "Token Picker score", a: [`0 to 100. The app wants ${c.observation.minScore} or more. It rewards more buyers than sellers, many different buyers, rising price and rising volume.`] },
        { q: "Exit rules", a: [`${d.ladder}. The last ${d.tail}% uses a ${c.exits.trailingStopPct}% trailing stop. Soft stop ${c.exits.softStopPct}%, hard stop ${c.exits.hardStopPct}%, maximum hold ${d.hold}.`] },
        { q: "Win rate, profit factor, drawdown", a: ["Win rate: share of winning trades. Profit factor: total wins divided by total losses, above 1 is good. Drawdown: the biggest fall from a high point."] },
        { q: "What is the 'Why no trades?' card?", a: ["A funnel of today's real counts: coins seen, blocked by Scam Shield, scored, best score, picked, bought. It shows where coins drop off and which rule stopped the best ones."] },
        { q: "The red 'No buy/sell data is arriving' bar", a: ["The app learns how a coin trades from the buys and sells it reads from the Solana network. If none arrive for 2 minutes while coins are being watched, every coin scores 0 and nothing can be bought. Settings > Connections shows trades read in the last minute, how many watched coins the server confirmed, the last trade and the last error. A free Helius key usually fixes it."] },
        { q: "Why 0 trades for a long time?", a: ["Strict rules. It is normal and safe. Do not loosen rules to get action."] },
        { q: "The i icons", a: ["Open Settings, open a section, and click i beside any value for a plain explanation."] },
        { q: "How do I add my Helius key?", a: ["Get a free key at helius.dev, then Dashboard > API Keys. In the app open Settings > Connections, paste the key, press Save, then Test connection.", "It is stored in your computer's secure storage and shown only as 'saved, ends in ...ab12'. Remove key deletes it."] },
        { q: "How do I update the app?", a: ["Press the round update button (circular arrows) at the top right, or open Settings > About and updates and press Check for updates. When a newer version is ready, press Update and restart. The app closes any open paper position the normal way first, downloads the update, installs it and starts again by itself. Your settings, trade history and learning stay.","A small red dot on the button and an Update ready badge on Settings mean the app found a newer version in the background. It never installs by itself. If automatic updating fails, a link to the GitHub download page is shown."] },
        { q: "What does 'What it learned' show?", a: ["A note per closed trade, built only from recorded numbers. Every 30 trades it may propose one small change, tests it in a separate pretend lane, and applies it only if it wins. Every change can be undone."] },
        { q: "What is the Obsidian vault connector?", a: ["Obsidian is a free note app (obsidian.md) that keeps notes as plain files in a folder called a vault. In Settings > Connectors you can point the app at your vault. It then writes readable notes into a Claude Trader folder there: one note per closed trade, per coin, per day, the lessons the learning feature found, and notable events such as the kill switch or a missed coin.","Notes contain only numbers and text the app really recorded, with [[links]] between them. The app never deletes your notes and never overwrites a note you wrote. Turn each note type on or off, or press Disconnect to stop. Backfill writes notes for trades that closed before you connected."] },
        { q: "Discord and webhook connectors", a: ["In Settings > Connectors you can have the app send messages out: a Discord webhook posts a coloured card (green for a win, red for a loss) with the numbers and a Dexscreener link; a generic webhook sends the same facts as JSON to Zapier, Make, n8n or your own server. Both are outgoing only.","Pick which events to send: buy, sell with P&L, daily summary, learning change applied, kill switch, daily loss limit, feed or RugCheck warnings, update installed. Every message says Paper results while the app is paper only, and contains only numbers the app recorded.","The webhook link or secret is stored in your computer's secure storage like the Helius key, shown only as a masked hint, and never goes into logs or config.json. Sending is paced and queued. If Discord or your server is down, the message is dropped after three tries and trading is never held up. Send test message checks the connection."] },
        { q: "What is X Monitor?", a: ["X Monitor checks the X (Twitter) page a coin links to. It uses the Chrome extension and a spare X account that is logged in to x.com. There is no paid X service. The extension opens one page at a time in a background tab, 8 to 15 seconds apart and never more than 120 an hour, and only reads. It never posts, likes, follows or clicks.","From what it reads it makes a community score from 0 to 100: how many well-known crypto accounts follow the coin's account (30), how real the engagement is (20), how real the replies look (10), how the followers grew (10), whether the account and handle look honest (10), a real X Community (10) and how far the source post reached (10). These weights are starting guesses and the app logs every part so they can be tuned.","The score adds at most 15 points to the coin's score and can never buy a coin alone. It can block a coin: a missing, suspended or private X account, a renamed or reused account, a famous post that never mentions the coin, bought followers, 30 percent bot replies, or a late copy of the same post. If the page could not be read in time, the answer is unknown. Unknown adds nothing and blocks nothing."] },
        { q: "How do I set up X Monitor?", a: ["1. Make a spare X account. Do not use your main account. X bans automated reading and the account could get locked. 2. Log into x.com in the Chrome profile the bot uses. 3. Optional but helpful: with that account, follow a few hundred crypto KOLs. 4. Keep the Chrome extension paired. Then open Settings > X Monitor. It is on by default and starts once the extension is paired and x.com is logged in. There is a pause button and the pacing numbers.","If X limits the account, the app shows X paused: account limited, and coins are scored without X until the pause ends."] },
        { q: "What is the Thesis Finder?", a: ["It finds the story behind each coin: the X post, account or community the coin links to, who wrote it, its reach, how old it was when the coin was made, and which copy of the story the coin is (first, second, and so on). When several coins launch from the same post or account within an hour, it shows that as a trending narrative. The first coin on a post usually gets the buyers, and later copies are mostly traps."] },
        { q: "What is the Chrome extension?", a: ["An optional add-on for Chrome. It shows your balance, open positions and the kill switch in a side panel, shows what the bot thinks of a coin on pump.fun and Dexscreener pages, and sends alerts. It can run a scam check on a coin, but it can never make the bot buy.", "Install it once from Settings > Startup and browser > Install the Chrome extension. It connects to the app only on this computer, with a code you paste once."] },
      ],
    },
  ];
}

export const GLOSSARY: Term[] = [
  { term: "RugCheck", def: "A free website that rates new coins. When it is busy or has no report yet, the app checks the coin on the blockchain itself and buys at half size." },
  { term: "X post / X profile", def: "A link to one post on X (Twitter) or to a coin's X account page. The app labels each link so you know which." },
  { term: "X Monitor", def: "Reads a coin's X page through the Chrome extension and scores its community from 0 to 100. It adds at most 15 points and can block copies, reused accounts and bot crowds." },
  { term: "Smart follower (KOL)", def: "A well-known crypto account that follows a coin's X account. The spare X account must follow a few hundred KOLs for this to work." },
  { term: "Narrative", def: "Several coins launched from the same X post, account or community. The Thesis Finder shows these." },
  { term: "Helius", def: "A free service that gives the app a steadier line to the Solana network. You add a free key in Settings." },
  { term: "Test lane", def: "A separate pretend lane that tries a proposed change next to the normal one. The change is used only if the test lane wins." },
  { term: "Bankroll", def: "The pretend money the app trades with." },
  { term: "Bonding curve", def: "The price formula on pump.fun. Buying pushes price up, selling pushes it down." },
  { term: "Bundle", def: "Many wallets controlled by one person buying at launch to fake interest." },
  { term: "Curve SOL", def: "Real SOL paid into a coin's curve so far." },
  { term: "Dev", def: "The creator of the coin." },
  { term: "Drawdown", def: "The biggest drop from a high point in your balance." },
  { term: "Graduation", def: "A coin fills its curve and moves to a regular exchange pool." },
  { term: "Honeypot", def: "A coin you can buy but not sell." },
  { term: "Lamport", def: "The smallest piece of SOL. One SOL is 1,000,000,000 lamports." },
  { term: "Liquidity", def: "How much money sits behind a coin so people can sell." },
  { term: "Market cap", def: "Price times all coins. A rough size of the coin, counted in SOL by this app." },
  { term: "Mint authority", def: "A key that can create more coins. It should be gone." },
  { term: "P&L", def: "Profit and loss." },
  { term: "Paper trading", def: "Pretend trading with real prices and no real money." },
  { term: "Priority fee", def: "A tip to get a transaction processed faster." },
  { term: "Profit factor", def: "Total wins divided by total losses." },
  { term: "Rug pull", def: "Insiders sell everything and the price collapses." },
  { term: "Slippage", def: "The difference between expected and actual price." },
  { term: "Sniper", def: "A bot that buys in the first moments after launch." },
  { term: "SOL", def: "Solana's coin. Prices and fees here are in SOL." },
  { term: "Stop loss", def: "A rule that sells when a trade is down by a set amount." },
  { term: "Take profit", def: "A rule that sells part of a trade when it is up by a set amount." },
  { term: "Trailing stop", def: "A stop that follows the peak price up and sells if price falls a set amount from it." },
  { term: "Win rate", def: "Share of trades that made money." },
];
