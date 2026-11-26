# Tips section: sources and accuracy notes

Used for the beginner text in `app/src/tips/content.ts`. Numbers about the app (exits, stops, fees, risk limits, score bar) are read from `config.json` at run time. Two constants live in code: 100 paper trades for the live lock (SPEC 11b item 28) and 85 SOL graduation (same value the engine uses).

Web research run 2026-11-26 (search results only; several sites such as arxiv.org and madeonsol.com could not be opened from the sandbox, so only search summaries were read):
- Bonding curve, graduation about 85 SOL, migration to PumpSwap (launched 2025-03-20, older guides say Raydium): DEXTools "What Is Pump.fun" guide, The Block "Pump.fun launches DEX called PumpSwap", Bitquery docs "pump fun to pump swap", Blockworks pump.fun analytics.
- Rug pull, honeypot, bundles, snipers, red flags: Coinpaprika "launchpad red flags", Cointelegraph and CoinMarketCap coverage of the Bubblemaps "Rugproof" case (162 wallets), BeInCrypto on HNUT, Ledger Academy "Dissecting a rug pull".
- Priority fees and slippage: Jupiter docs "landing transactions", QuickNode Jupiter transactions docs, Phantom swap settings help, Bybit "Why Solana transactions fail", dev.to "Slippage and priority fees".
- 98.7% of pump.fun tokens showing pump-and-dump or rug patterns: Solidus Labs, as recorded in SPEC 11b (pump.fun disputes it).

Not found by search: a source on paper results overstating live results. That tip is reasoning (fills, failures, speed, fees) and the app's own SPEC 11b item 28, not a cited study.

Accuracy doubts:
- Fee numbers (pump.fun 1.25%, PumpPortal 0.5%) are the SPEC's conservative model. Sources disagree (0.3% to 1%, one says 0.95%). Tips say "assumed, check before live".
- The Tips text says dev sell and honeypot checks exist; the honeypot sell simulation is NOT run in paper mode (STATUS M2). The Tips say so.
- Share of coins that graduate is not stated, because no source could be opened to confirm a number.
