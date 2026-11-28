// The pages the bot needs open in Chrome, and the checklist the app and the side panel show.
// Pure functions (no Chrome APIs), tested in tests/pages.test.js.
//
//   pump.fun home            where the new coins appear                     (Settings toggle "pump.fun home")
//   x.com                    logged in with the spare account, X Monitor     (only while the X Monitor is on)
//   a Dexscreener chart      one per open position                           (Settings toggle "Dexscreener charts")
//
// Rules: a page that is already open is reused (never opened twice), a page whose toggle is off is never opened,
// and the extension only ever opens addresses it builds itself from this list.

export const PUMP_HOME = { id: "pump-home", label: "pump.fun home", url: "https://pump.fun/", patterns: ["https://pump.fun/*", "https://www.pump.fun/*"] };
export const X_PAGE = { id: "x", label: "x.com (logged in, for the X Monitor)", url: "https://x.com/home", patterns: ["https://x.com/*"] };

const dexUrl = (mint) => `https://dexscreener.com/solana/${mint}`;
const mintOk = (m) => typeof m === "string" && /^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(m);

/** Tab address against a pattern like "https://pump.fun/*" (only a trailing * is supported). */
export function urlMatches(url, pattern) {
  if (typeof url !== "string") return false;
  return pattern.endsWith("*") ? url.startsWith(pattern.slice(0, -1)) : url === pattern;
}

/**
 * What the bot needs right now. `settings` is the app's startup settings ({ pages: { pumpFunHome, dexscreenerHeld } }),
 * `xmon` the X Monitor switch ({ enabled }), `held` the open positions as [{ mint, symbol }].
 * Pages switched off are returned with `off: true` so the checklist can say so; they are never opened.
 */
export function requiredPages({ settings, xmon, held }) {
  const pages = settings?.pages || {};
  const out = [];
  out.push({ ...PUMP_HOME, off: pages.pumpFunHome === false });
  out.push({ ...X_PAGE, off: !xmon?.enabled, background: true, pinned: true });
  if (pages.dexscreenerHeld === false) {
    out.push({ id: "dex", label: "Dexscreener chart of each open position", url: "", patterns: [], off: true });
  } else {
    for (const h of held || []) {
      if (!mintOk(h.mint)) continue;
      out.push({ id: `dex-${h.mint}`, label: `Dexscreener chart: ${String(h.symbol || h.mint.slice(0, 6)).slice(0, 20)}`, url: dexUrl(h.mint), patterns: [`https://dexscreener.com/solana/${h.mint}*`, `https://www.dexscreener.com/solana/${h.mint}*`], mint: h.mint, off: false });
    }
  }
  return out;
}

/** First open tab that shows this page, or null. `tabs` is [{ id, url }]. */
export function findTab(page, tabs) {
  if (page.off) return null;
  for (const t of tabs || []) if (page.patterns.some((p) => urlMatches(t.url, p))) return t;
  return null;
}

/**
 * Which pages should be opened now. A page that is open is never opened again (existing tab reused).
 * Without `force` (app start) a page is opened only once per browser session: `gates[id]` is true after the first try, so a tab
 * you closed on purpose stays closed. With `force` ("Open missing pages") every missing page is opened.
 * Returns the pages to open (each once) and the new gates.
 */
export function planOpen(required, tabs, gates, force) {
  const next = { ...(gates || {}) };
  const open = [];
  const seen = new Set();
  for (const p of required) {
    if (p.off || seen.has(p.id) || !p.url) continue;
    seen.add(p.id);
    if (findTab(p, tabs)) continue;
    if (!force && next[p.id]) continue;
    next[p.id] = true;
    open.push(p);
  }
  return { open, gates: next };
}

/**
 * The checklist lines: [{ id, label, state, url }], state "open" | "missing" | "loggedout" | "off".
 * `xLoggedIn` is true, false or null (not known yet). x.com is "loggedout" only when X showed a login page.
 */
export function checklist(required, tabs, { xLoggedIn = null } = {}) {
  return required.map((p) => {
    let state;
    if (p.off) state = "off";
    else if (!findTab(p, tabs)) state = "missing";
    else if (p.id === "x" && xLoggedIn === false) state = "loggedout";
    else state = "open";
    return { id: p.id, label: p.label, state, url: p.url || "" };
  });
}

/** Short summary for a badge or a line: "2 of 3 pages open". Pages that are off do not count. */
export function summary(items) {
  const need = items.filter((i) => i.state !== "off");
  const ok = need.filter((i) => i.state === "open").length;
  return { ok, need: need.length, missing: need.filter((i) => i.state === "missing").length, loggedOut: need.filter((i) => i.state === "loggedout").length };
}

/** A reload asked for by the app is honoured at most once every 2 minutes, so a confused app can never put the extension in a loop. */
export function reloadAllowed(now, lastReloadAt) {
  return !lastReloadAt || now - lastReloadAt > 2 * 60 * 1000;
}
