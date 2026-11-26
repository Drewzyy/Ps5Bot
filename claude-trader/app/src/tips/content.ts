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
  const roundTrip = (c.fees.modelPumpFunPctPerSide + c.fees.pumpPortalPctPerSide) * 2;
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
      ],
      good: "PAPER badge in green. Profit after fees over many trades.",
      bad: "A red LIVE badge before you meant to switch. Or good paper profit from only a handful of trades, which is mostly luck.",
      todo: "Stay on paper. Do not go live until you have read the Tips page on risk and the results hold up.",
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
      ],
      good: "Many blocks, with clear reasons listed.",
      bad: "Zero blocks over a long time, or every coin passing. That would be suspicious.",
      todo: "Nothing. Read the reasons if you are curious. Verdicts: LOW SIGNAL means no red flags found, ELEVATED means some, HIGH means serious.",
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
      id: "settings", title: "Settings and the i icons", target: ".nav", 
      what: [
        "Open Settings to see every rule as a number. Each row has an i icon.",
        "Click it for a plain explanation: what the setting does, what a good value is, and how to change it.",
        "The numbers were chosen from research. Paper mode is where they get tested. Changing them without knowing why usually makes results worse.",
      ],
      good: "Leave defaults alone while you learn.",
      bad: "Raising trade size or loosening stops after a loss.",
      todo: "Read a few i icons for fun. Replay this tour or read more under the Tips tab any time.",
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
        { q: "Market cap vs curve SOL", a: ["Curve SOL is the real SOL buyers have paid into the curve so far.", "Market cap is price times all coins, shown in dollars. It is always bigger than curve SOL and moves when the SOL price moves."] },
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
        { q: "The live lock", a: [`Live mode stays locked until ${LIVE_MIN_PAPER_TRADES} paper trades exist and results are positive after fees. Never put money in that you cannot lose.`] },
      ],
    },
    {
      id: "reading", title: "Reading the dashboard", intro: "What the numbers mean.",
      entries: [
        { q: "Token Picker score", a: [`0 to 100. The app wants ${c.observation.minScore} or more. It rewards more buyers than sellers, many different buyers, rising price and rising volume.`] },
        { q: "Exit rules", a: [`${d.ladder}. The last ${d.tail}% uses a ${c.exits.trailingStopPct}% trailing stop. Soft stop ${c.exits.softStopPct}%, hard stop ${c.exits.hardStopPct}%, maximum hold ${d.hold}.`] },
        { q: "Win rate, profit factor, drawdown", a: ["Win rate: share of winning trades. Profit factor: total wins divided by total losses, above 1 is good. Drawdown: the biggest fall from a high point."] },
        { q: "Why 0 trades for a long time?", a: ["Strict rules. It is normal and safe. Do not loosen rules to get action."] },
        { q: "The i icons", a: ["Open Settings and click i beside any value for a plain explanation."] },
      ],
    },
  ];
}

export const GLOSSARY: Term[] = [
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
  { term: "Market cap", def: "Price times all coins. A rough size of the coin in dollars." },
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
