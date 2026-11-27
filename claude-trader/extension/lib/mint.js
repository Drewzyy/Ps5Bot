// Finds the coin address on a page. Plain script (no import), loaded before content.js and also read by the tests.
(function (root) {
  const B58 = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/;

  function isAddress(s) {
    return typeof s === "string" && B58.test(s);
  }

  /** pump.fun/coin/<mint> */
  function pumpMint(pathname) {
    const m = /^\/coin\/([1-9A-HJ-NP-Za-km-z]{32,44})\/?$/.exec(pathname);
    return m ? m[1] : null;
  }

  /** dexscreener.com/solana/<pair or token address> */
  function dexId(pathname) {
    const m = /^\/solana\/([1-9A-HJ-NP-Za-km-z]{32,44})\/?$/.exec(pathname);
    return m ? m[1] : null;
  }

  /** Token addresses found in link targets such as solscan.io/token/<mint>. */
  function mintsFromLinks(hrefs) {
    const out = [];
    for (const h of hrefs) {
      const m = /(?:solscan\.io|solana\.fm|explorer\.solana\.com|birdeye\.so)\/(?:token|address)\/([1-9A-HJ-NP-Za-km-z]{32,44})/.exec(h);
      if (m && !out.includes(m[1])) out.push(m[1]);
    }
    return out;
  }

  root.CTMint = { isAddress, pumpMint, dexId, mintsFromLinks };
})(typeof globalThis !== "undefined" ? globalThis : this);
