export interface Explain {
  label: string;
  what: string;
  good: string;
  how: string;
}

const e = (label: string, what: string, good: string, how: string): Explain => ({ label, what, good, how });

const EDIT_LATER = "Editing from this screen comes in a later version. For now the value is set in config.json and the app reads it at start.";

/** Sections in display order. Each key is a top-level key of config.json. */
export const SECTIONS: { key: string; title: string; blurb: string }[] = [
  { key: "mode", title: "Mode", blurb: "Whether the app trades pretend money or real money." },
  { key: "paper", title: "Paper wallet", blurb: "The pretend money used while practising." },
  { key: "feed", title: "New token feed", blurb: "Which new coins the app looks at." },
  { key: "filters", title: "Scam Shield filters", blurb: "Checks a coin must pass before it can be bought." },
  { key: "observation", title: "Watching window", blurb: "What a coin must show while we watch it before buying." },
  { key: "entry", title: "Buying rules", blurb: "The exact moment and conditions for a buy." },
  { key: "exits", title: "Selling rules", blurb: "When and how a position is sold. Every position closes within 5 minutes." },
  { key: "fees", title: "Fees", blurb: "The costs the app expects to pay on each trade." },
  { key: "paperFill", title: "Paper fill realism", blurb: "Makes pretend trades behave like real ones, including failures." },
  { key: "risk", title: "Risk limits", blurb: "Hard limits that protect your money." },
];

export const META: Record<string, Explain> = {
  mode: e(
    "Trading mode",
    "Chooses between paper (pretend money, nothing real is spent) and live (real money). The app only ever runs in paper mode until you approve live trading.",
    "paper. Stay on paper until you have at least 100 paper trades and the results look good after fees.",
    EDIT_LATER + " Live mode stays locked and needs a typed confirmation."
  ),
  "paper.bankrollSol": e(
    "Paper bankroll (SOL)",
    "How much pretend SOL the practice wallet starts with. Trade sizes and loss limits are worked out as a share of this number.",
    "10 SOL is a good round number. Use roughly what you would really put in later, so the practice results mean something.",
    "Type the amount of SOL you would be comfortable risking. Nothing real is used."
  ),
  "feed.minMarketCapSol": e(
    "Minimum market cap (SOL)",
    "Coins worth less than this many SOL are ignored. Market cap is the price of the coin times how many exist, counted in SOL so it does not shift when the dollar price of SOL moves.",
    "30 SOL. Every new coin starts near 28 SOL, so this skips coins nobody has bought yet. Do not go far above it: the chase rule blocks coins that already tripled (about 84 SOL).",
    "Raise it a little if you see too many dead coins. Keep it well under 84 SOL or no coin can pass both rules."
  ),
  "feed.launchpads": e(
    "Launchpads",
    "The websites where new coins are created that the app will watch. For now only pump.fun is supported.",
    "pump.fun only. It is the only one the app has been built and checked for.",
    "Nothing to set yet. More launchpads may be added in a later version."
  ),
  "filters.mintAuthorityRevoked": e(
    "Mint authority must be revoked",
    "If someone still holds the mint authority they can create unlimited new coins and crash the price. This rule blocks any coin where that power is still active.",
    "On. There is almost never a good reason to turn this off.",
    "Leave it switched on. This block cannot be overridden by hand."
  ),
  "filters.freezeAuthorityRevoked": e(
    "Freeze authority must be revoked",
    "If someone still holds the freeze authority they can lock your coins so you cannot sell. This rule blocks any coin where that power is still active.",
    "On. Turning it off risks getting stuck with coins you cannot sell.",
    "Leave it switched on."
  ),
  "filters.maxCreatorHoldingPct": e(
    "Max creator holding (%)",
    "The person who made the coin is called the creator. If they hold a big share, they can dump it on buyers. Coins above this share are blocked.",
    "10 percent or lower. Lower is safer.",
    "Type a percentage. Smaller numbers block more coins and keep you safer."
  ),
  "filters.maxInsidersPct": e(
    "Max insiders (%)",
    "Insiders are wallets that look connected to the creator, such as ones funded by the same source. If they hold more than this share together, the coin is blocked.",
    "20 percent or lower.",
    "Type a percentage. Lower is stricter."
  ),
  "filters.maxSingleHolderPct": e(
    "Max single holder (%)",
    "No single wallet may hold more than this share of the coin. One big wallet can crash the price by selling. The coin's own pool accounts are not counted.",
    "5 percent or lower.",
    "Type a percentage. Lower is stricter."
  ),
  "filters.maxTopHoldersCombinedPct": e(
    "Max top holders combined (%)",
    "The biggest holders together may not own more than this share. It catches coins where a small group controls most of the supply. Pool accounts are not counted.",
    "15 percent or lower.",
    "Type a percentage. Lower is stricter."
  ),
  "filters.excludeCurveAndPoolAccounts": e(
    "Ignore pool accounts in holder checks",
    "The coin's own pool holds a lot of coins by design. This setting leaves the pool out when the holder shares are added up, so the checks only look at real people.",
    "On. Without it almost every coin would look too concentrated.",
    "Leave it switched on."
  ),
  "filters.rejectDevSnipeFirstBlock": e(
    "Block if creator bought in the first block",
    "Some creators buy their own coin in the very first moment under another wallet to secretly collect a big share. This blocks coins that show that pattern.",
    "On.",
    "Leave it switched on. This block cannot be overridden by hand."
  ),
  "filters.rejectBundle": e(
    "Block bundled coins",
    "Bundling means many fake buyer wallets were set up by one person to make a coin look popular. This blocks coins that look bundled.",
    "On.",
    "Leave it switched on. This block cannot be overridden by hand."
  ),
  "filters.requireCommunityChecks.minSocialLinks": e(
    "Minimum social links",
    "How many social links (website, X, Telegram) the coin's page must list. It is only a weak check, because links are easy to fake.",
    "1. It removes the laziest coins without being strict.",
    "Type a whole number. 0 turns the check off."
  ),
  "filters.requireCommunityChecks.requireImage": e(
    "Coin must have an image",
    "Blocks coins that have no picture. Real projects almost always have one. It is a weak check.",
    "On.",
    "Switch on to require a picture."
  ),
  "observation.minScore": e(
    "Minimum score",
    "While we watch a coin, the app gives it a score from 0 to 100. Only coins at or above this score can be bought.",
    "80. Higher means fewer but better trades.",
    "Type a number from 0 to 100. If the app never buys, lower it a little. If it loses too often, raise it."
  ),
  "observation.minUniqueBuyers": e(
    "Minimum different buyers",
    "How many different wallets must have bought during the watching window. Many different buyers is a healthier sign than a few wallets trading back and forth.",
    "5 or more.",
    "Type a whole number. Higher is stricter."
  ),
  "observation.minBuySellRatio": e(
    "Minimum buy to sell ratio",
    "Compares buying to selling. A value of 2 means there must be twice as much buying as selling.",
    "2 or higher.",
    "Type a number. Higher means you only join coins with strong demand."
  ),
  "observation.minPriceChangePct": e(
    "Minimum price rise (%)",
    "The price must have risen at least this much during the watching window, so we only buy coins that are already moving up.",
    "5 percent.",
    "Type a percentage. Higher waits for a clearer move."
  ),
  "entry.style": e(
    "Entry style",
    "The method used to decide the exact moment to buy. Momentum buys coins that are rising with strong demand. Other styles are: snipe, confirmation, dip-40pct and graduation-dip.",
    "momentum. It was the only style with any track record in the research.",
    "Type one of the style names. The dip style also runs in paper mode on the side so the two can be compared."
  ),
  "entry.minBuySellRatio": e(
    "Entry buy to sell ratio",
    "At the moment of buying, buying must beat selling by at least this multiple. This is one of the best early signs of a coin that keeps rising.",
    "2 or higher.",
    "Type a number. Higher is stricter."
  ),
  "entry.minUniqueBuyers30s": e(
    "Different buyers in last 30 seconds",
    "How many different wallets must have bought in the last 30 seconds. It shows demand is happening right now.",
    "5 or more.",
    "Type a whole number. Higher is stricter."
  ),
  "entry.requireRisingVolumeSlope": e(
    "Trading volume must be rising",
    "Requires that the amount being traded is growing, not shrinking. Growing volume means more people are arriving.",
    "On.",
    "Switch on to require growing volume."
  ),
  "entry.curveSolMin": e(
    "Pool size minimum (SOL)",
    "Each new coin has a pool that fills up as people buy. A pool that is too empty means the coin has not caught on yet. Coins below this are skipped.",
    "About 4 SOL. That still needs real money paid in, and the other entry rules still apply.",
    "Type a number of SOL. Lower lets in earlier, riskier coins."
  ),
  "entry.curveSolMax": e(
    "Pool size maximum (SOL)",
    "A pool that is almost full means the coin is close to the end of its run and has less room to rise. Coins above this are skipped.",
    "About 40 SOL.",
    "Type a number of SOL. It must be larger than the minimum."
  ),
  "entry.minHolders": e(
    "Minimum holders",
    "How many wallets must hold the coin. Very few holders means the coin is not spread out.",
    "15 or more.",
    "Type a whole number. Higher is stricter."
  ),
  "entry.minAgeSeconds": e(
    "Minimum coin age (seconds)",
    "The app never buys a coin in its first moments, because that is when bots and creators play tricks. It waits at least this long.",
    "60 seconds or more.",
    "Type a number of seconds. Lower is riskier."
  ),
  "entry.paperVariant.style": e(
    "Practice variant style",
    "A second buying method that only runs in paper mode, so we can compare it against the main style with no risk.",
    "dip-40pct.",
    "Type a style name. Live mode only ever uses whichever style wins in paper."
  ),
  "entry.paperVariant.minDipPct": e(
    "Variant minimum dip (%)",
    "For the dip variant: the coin must have fallen at least this much from its high before the app considers buying.",
    "40 percent.",
    "Type a percentage. It must be below the maximum dip."
  ),
  "entry.paperVariant.maxDipPct": e(
    "Variant maximum dip (%)",
    "For the dip variant: if the coin has fallen further than this it is probably dying, so it is skipped.",
    "50 percent.",
    "Type a percentage. It must be above the minimum dip."
  ),
  "entry.paperVariant.confirmVolumeExpanding": e(
    "Variant needs growing volume",
    "For the dip variant: only buy the dip if trading volume is growing again, which hints at a bounce.",
    "On.",
    "Switch on to require growing volume."
  ),
  "exits.tranches.atPct": e(
    "Take profit at (%)",
    "When the price is this far above what you paid, part of the position is sold to lock in gains.",
    "20, 40 and 60 percent for the three steps.",
    "Type a percentage for each step. Step 1 must still beat the trading fees."
  ),
  "exits.tranches.sellPct": e(
    "Sell this share of the position (%)",
    "How much of the original position is sold at that step. The rest keeps running. Whatever is left after the last step rides on the trailing stop.",
    "40, 30 and 20 percent, leaving 10 percent to ride.",
    "Type a percentage per step. The steps together must not add up to more than 100."
  ),
  "exits.trailingStopPct": e(
    "Trailing stop (%)",
    "After the last profit step, the final part follows the price up. If the price falls this far from its highest point, it is sold.",
    "10 percent.",
    "Type a percentage. Smaller locks in gains sooner but sells on small dips."
  ),
  "exits.softStopPct": e(
    "Soft stop (%)",
    "First warning level for a losing trade. When the loss reaches this, the app waits a short time to see if the price recovers.",
    "-10 percent.",
    "Type a negative percentage. It must be above the hard stop."
  ),
  "exits.softStopRecoverySeconds": e(
    "Soft stop wait (seconds)",
    "How long the app waits after the soft stop is hit. If the price has not recovered by then, it sells.",
    "15 seconds.",
    "Type a number of seconds."
  ),
  "exits.hardStopPct": e(
    "Hard stop (%)",
    "The last line of defense. If the loss reaches this, the position is sold immediately with no waiting.",
    "-15 percent. Fees take about 4.5 percent per round trip, so tighter stops would trigger too often.",
    "Type a negative percentage. It must be at or below the soft stop."
  ),
  "exits.sellBelowAverageEntry": e(
    "Sell if price falls under entry after gains",
    "After some profit has been taken, if the price drops back under what you paid, the rest is sold so a winner never turns into a bigger loss.",
    "On.",
    "Switch on to protect winners."
  ),
  "exits.maxHoldSeconds": e(
    "Maximum hold time (seconds)",
    "Every position is closed by this time, no matter what. This keeps trades short and stops you from getting stuck.",
    "300 seconds (5 minutes).",
    "Type a number of seconds. Longer holds expose you to more risk."
  ),
  "exits.stallExit.fromSeconds": e(
    "Stall check starts (seconds)",
    "A coin that goes nowhere is usually about to fade. The stall check starts looking this many seconds after the buy.",
    "60 seconds.",
    "Type a number of seconds. It must be before the end time."
  ),
  "exits.stallExit.toSeconds": e(
    "Stall check ends (seconds)",
    "The stall check window ends at this many seconds after the buy. If the gain is still too small by then, everything is sold.",
    "90 seconds.",
    "Type a number of seconds."
  ),
  "exits.stallExit.minPct": e(
    "Stall minimum gain (%)",
    "The gain a position must show inside the stall window to be kept. Less than this counts as stalled.",
    "3 percent.",
    "Type a percentage."
  ),
  "exits.bleederExit.afterSeconds": e(
    "Slow bleed check starts (seconds)",
    "A position that has never really gone up and keeps sinking is a slow bleed. This check starts after this many seconds.",
    "120 seconds.",
    "Type a number of seconds."
  ),
  "exits.bleederExit.belowPct": e(
    "Slow bleed loss level (%)",
    "If the position is down more than this, and it never rose much, it is sold.",
    "-20 percent.",
    "Type a negative percentage."
  ),
  "exits.bleederExit.peakNeverAbovePct": e(
    "Slow bleed peak limit (%)",
    "The check only applies to positions whose best gain stayed under this level, so real winners that pulled back are handled by the other rules.",
    "20 percent.",
    "Type a percentage."
  ),
  "exits.volumeTaper.windowSeconds": e(
    "Volume check window (seconds)",
    "Buying activity is measured in slices of this length. Several shrinking slices in a row mean interest is fading.",
    "10 seconds.",
    "Type a number of seconds."
  ),
  "exits.volumeTaper.decliningWindows": e(
    "Shrinking slices in a row",
    "How many slices in a row must show less buying than the one before for the app to react.",
    "3.",
    "Type a whole number. Higher reacts later."
  ),
  "exits.volumeTaper.lastBelowBaselinePct": e(
    "Latest slice below normal (%)",
    "The most recent slice must also be below this percentage of the normal buying level before the app sells.",
    "50 percent.",
    "Type a percentage."
  ),
  "exits.volumeTaper.sellPct": e(
    "Share to sell on fading volume (%)",
    "How much of the position is sold when buying activity fades.",
    "50 percent.",
    "Type a percentage."
  ),
  "exits.momentumDeath.afterHoldSeconds": e(
    "Momentum check starts (seconds)",
    "After holding this long, the app checks whether people are still buying.",
    "90 seconds.",
    "Type a number of seconds."
  ),
  "exits.momentumDeath.lookbackSeconds": e(
    "Momentum look-back (seconds)",
    "How far back the app looks when counting recent buys.",
    "45 seconds.",
    "Type a number of seconds."
  ),
  "exits.momentumDeath.minBuysPerMinute": e(
    "Minimum buys per minute",
    "If fewer buys than this are happening, the coin is dead in the water and the whole position is sold.",
    "0.5 buys per minute.",
    "Type a number. Higher sells dead coins sooner."
  ),
  "exits.whaleSellPctOfCurve": e(
    "Big seller alarm (% of pool)",
    "A whale is a wallet with a very large holding. If one sells this share of the pool in one go, the app sells everything at once.",
    "3 percent.",
    "Type a percentage. Lower is more cautious."
  ),
  "exits.exitOnDevSell": e(
    "Sell if the creator sells",
    "If the creator of the coin sells any of their coins, the app sells the whole position right away. Creators selling is the most common sign of a rug pull.",
    "On.",
    "Leave it switched on."
  ),
  "exits.sellOnMigration": e(
    "Sell when the coin graduates",
    "When a coin fills its pool it moves to a new market (called migration), and prices get wild. The app sells before that point.",
    "On.",
    "Leave it switched on."
  ),
  "exits.exitLoopMs": e(
    "Exit check interval (ms)",
    "How often the app re-checks every open position, in thousandths of a second. 1000 means once per second.",
    "1000.",
    "Type a number of milliseconds. Lower checks more often but uses more of your free data allowance."
  ),
  "fees.modelPumpFunPctPerSide": e(
    "pump.fun fee per side (%)",
    "The fee the app assumes pump.fun takes on each buy and each sell. It is set on the cautious side so results are not too rosy.",
    "1.25 percent (0.95 to pump.fun and 0.30 to the coin's creator).",
    "Type a percentage. We will check it against the official docs before any live trading."
  ),
  "fees.pumpFunCreatorPctPerSide": e(
    "Creator share of the pump.fun fee (%)",
    "Part of the pump.fun fee above that is paid to the person who made the coin. You still pay it, so it is shown as its own line in Fees paid.",
    "0.30 percent.",
    "Type a percentage. It must not be more than the pump.fun fee above."
  ),
  "fees.jitoTipSol": e(
    "Jito tip per trade (SOL)",
    "An optional extra tip some traders pay so a trade lands first. The app does not use it today, but if you set a number it is charged on every trade, including failed ones.",
    "0 SOL. Trades go through PumpPortal, which lands them with the priority fee alone.",
    "Leave at 0 unless trades keep missing. Typical tips are 0.001 to 0.01 SOL."
  ),
  "fees.pumpPortalPctPerSide": e(
    "PumpPortal fee per side (%)",
    "The fee charged by the service that builds the trades, on each buy and each sell.",
    "1.0 percent. This is the higher of the two published rates, so results are not too rosy.",
    "Type a percentage. PumpPortal lists 0.5 percent for trades you sign yourself and 1 percent for its hosted wallet. We use 1 until checked."
  ),
  "fees.priorityMicroLamports": e(
    "Priority fee rate",
    "A small tip paid to the network so a trade is processed quickly. This is the rate for that tip.",
    "150000.",
    "Leave it unless trades are being missed. Higher costs more."
  ),
  "fees.computeUnits": e(
    "Compute budget",
    "How much work a trade is allowed to use on the network. It sets the size of the priority fee.",
    "150000.",
    "Leave it as it is unless you know why you are changing it."
  ),
  "fees.pumpPortalPriorityFeeSol": e(
    "Priority fee per trade (SOL)",
    "The tip in SOL that is added to each trade through PumpPortal.",
    "0.0005 SOL.",
    "Type an amount of SOL. Keep it small, because it counts against profit."
  ),
  "fees.pumpPortalPriorityFeeCapSol": e(
    "Priority fee limit (SOL)",
    "If a sell fails, the app retries with a higher tip. This is the most it will ever tip.",
    "0.005 SOL.",
    "Type an amount of SOL."
  ),
  "paperFill.buyFailPct": e(
    "Pretend buys that fail (%)",
    "In real trading some buys do not go through. Paper mode makes this share of pretend buys fail so the results are not too good to be true.",
    "10 percent.",
    "Type a percentage. We will fine tune it from real trades later."
  ),
  "paperFill.sellFailPct": e(
    "Pretend sells that fail (%)",
    "The share of pretend sells that fail on the first try, like real ones sometimes do.",
    "15 percent.",
    "Type a percentage."
  ),
  "paperFill.delayMsMin": e(
    "Fastest fill delay (ms)",
    "Real trades take a moment. Pretend trades wait at least this long, in thousandths of a second, before filling.",
    "400.",
    "Type a number of milliseconds."
  ),
  "paperFill.delayMsMax": e(
    "Slowest fill delay (ms)",
    "Pretend trades wait no longer than this before filling. The delay is picked at random between the two numbers.",
    "1200.",
    "Type a number of milliseconds. It must be above the fastest delay."
  ),
  "paperFill.slippageCapPct": e(
    "Price slip limit (%)",
    "Slippage is the gap between the price you expected and the price you got. Pretend trades are filled with realistic slippage, up to this limit.",
    "15 percent.",
    "Type a percentage."
  ),
  "risk.maxPctBankrollPerTrade": e(
    "Trade size (% of bankroll)",
    "How much of your bankroll goes into one trade. Small bets mean one bad coin cannot hurt much.",
    "1 percent.",
    "Type a percentage. The result is then kept between the minimum and maximum trade size."
  ),
  "risk.minTradeSol": e(
    "Smallest trade (SOL)",
    "No trade is smaller than this, because very small trades lose too much to fixed costs.",
    "0.1 SOL.",
    "Type an amount of SOL."
  ),
  "risk.maxTradeSol": e(
    "Largest trade (SOL)",
    "No trade is bigger than this, even if the bankroll grows.",
    "0.25 SOL.",
    "Type an amount of SOL."
  ),
  "risk.maxOpenPositions": e(
    "Max open positions",
    "How many coins can be held at the same time.",
    "5.",
    "Type a whole number. Fewer is safer."
  ),
  "risk.maxTotalExposurePct": e(
    "Max total exposure (%)",
    "The total share of your bankroll that may be in open trades at once.",
    "5 percent.",
    "Type a percentage."
  ),
  "risk.dailyLossLimitPct": e(
    "Daily loss limit (%)",
    "If you lose this share of your bankroll in one day, the app stops trading until tomorrow. Lifting the stop needs a typed confirmation.",
    "7.5 percent.",
    "Type a percentage. Lower stops sooner."
  ),
  "risk.consecutiveLossStop": e(
    "Stop after losses in a row",
    "If this many trades in a row end in a stop-loss, the app pauses so a bad streak cannot snowball.",
    "3.",
    "Type a whole number."
  ),
  "risk.reduceSizeAfterWins": e(
    "Halve size after wins in a row",
    "After this many wins in a row, the next trade is half size. Winning streaks end, and this protects the gains.",
    "3.",
    "Type a whole number."
  ),
  "risk.cooldownSecondsPerToken": e(
    "Cooldown per coin (seconds)",
    "After trading a coin, the app will not trade it again for this long.",
    "300 seconds.",
    "Type a number of seconds."
  ),
  "risk.liveMinSolReserve": e(
    "Keep in wallet (SOL)",
    "Live mode only. This much SOL is always left in the wallet to pay network fees and cannot be traded.",
    "0.02 SOL.",
    "Type an amount of SOL."
  ),
};

export interface Row {
  section: string;
  path: string; // dotted path, array index replaced by "N" for META lookup
  display: string; // dotted path with real indexes
  value: unknown;
  meta?: Explain;
  suffix?: string;
}

export function flatten(cfg: Record<string, unknown>): Row[] {
  const rows: Row[] = [];
  const walk = (v: unknown, path: string[], metaPath: string[], section: string) => {
    if (Array.isArray(v) && v.length && typeof v[0] === "object") {
      v.forEach((item, i) => walk(item, [...path, String(i + 1)], metaPath, section));
    } else if (v && typeof v === "object" && !Array.isArray(v)) {
      for (const [k, x] of Object.entries(v as Record<string, unknown>)) walk(x, [...path, k], [...metaPath, k], section);
    } else {
      const mp = metaPath.join(".");
      const meta = META[mp];
      const idx = path.find((p) => /^\d+$/.test(p));
      rows.push({
        section,
        path: mp,
        display: path.join("."),
        value: v,
        meta: meta && idx ? { ...meta, label: `${meta.label} (step ${idx})` } : meta,
      });
    }
  };
  for (const [k, v] of Object.entries(cfg)) walk(v, [k], [k], k);
  return rows;
}
