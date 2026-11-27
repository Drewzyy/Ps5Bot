// Pure logic shared by the background worker, the side panel and the tests. No Chrome APIs in here.

export const DEFAULT_PREFS = { buy: true, sell: true, limit: true, kill: true, offline: true, badge: "pnl" };

export function emptyState() {
  return {
    conn: "connecting", // connecting | connected | offline | unpaired
    error: "",
    stats: null,
    positions: [],
    activity: [],
    kill: null,
    learning: null,
    settings: null,
    primed: { positions: false, kill: false, halt: false },
  };
}

/** Apply one message from the app. Returns a new state (the old one is not changed). */
export function applyEvent(state, e) {
  const s = { ...state, primed: { ...state.primed } };
  switch (e.type) {
    case "activity": {
      const key = `${e.ts}|${e.text}`;
      if (!s.activity.some((a) => a.key === key)) {
        s.activity = [{ key, ts: e.ts, text: e.text, mint: e.mint ?? null, sourceUrl: e.sourceUrl ?? null }, ...s.activity].sort((a, b) => b.ts - a.ts).slice(0, 40);
      }
      break;
    }
    case "positions":
      s.positions = e.positions || [];
      s.primed.positions = true;
      break;
    case "stats":
      s.stats = { balanceSol: e.balanceSol, solUsd: e.solUsd, replay: e.replay, lanes: e.lanes || [] };
      s.primed.halt = true;
      break;
    case "kill":
      s.kill = { killed: !!e.killed, text: e.text || "" };
      s.primed.kill = true;
      break;
    case "learning":
      s.learning = { journal: (e.journal || []).slice(0, 5), closedTrades: e.closedTrades, nextReviewIn: e.nextReviewIn, reviewEvery: e.reviewEvery, demo: !!e.demo };
      break;
    default:
  }
  return s;
}

/** After a lost connection the next snapshot is a fresh baseline: nothing in it counts as news. */
export function resetPrimed(state) {
  return { ...state, primed: { positions: false, kill: false, halt: false } };
}

export function summarize(state) {
  const main = state.stats?.lanes?.[0];
  const open = state.positions.filter((p) => p.state !== "CLOSED");
  return {
    balanceSol: state.stats?.balanceSol ?? null,
    pnlToday: main?.pnlToday ?? 0,
    pnlTotal: main?.totalPnlSol ?? 0,
    tradesToday: main?.tradesToday ?? 0,
    open,
    replay: !!state.stats?.replay,
    halted: state.stats?.lanes?.find((l) => l.halted)?.halted ?? null,
  };
}

export function fmtSol(n, sign = false) {
  if (n == null || Number.isNaN(n)) return "-";
  const a = Math.abs(n);
  const body = a < 1 && a !== 0 ? a.toFixed(4) : a.toFixed(2);
  const sg = n < 0 ? "-" : sign && n > 0 ? "+" : "";
  return `${sg}${body} SOL`;
}

/** Short text for the toolbar badge (about 4 characters fit). */
export function pnlBadgeText(v) {
  const a = Math.abs(v);
  if (a < 0.005) return "0";
  const sg = v < 0 ? "-" : "+";
  if (a < 1) return `${sg}${a.toFixed(2).slice(1)}`; // +.12
  if (a < 10) return `${sg}${a.toFixed(1)}`; // +1.2
  return `${sg}${Math.round(a)}`; // +12
}

export const BADGE_GREEN = "#16a34a";
export const BADGE_RED = "#dc2626";
export const BADGE_GREY = "#6b7280";
export const BADGE_BRAND = "#D97757";

/** What the toolbar icon shows: { text, color }. */
export function badgeFor(state, mode) {
  if (state.conn !== "connected") return { text: state.conn === "unpaired" ? "?" : "off", color: BADGE_GREY };
  const sum = summarize(state);
  if (mode === "open") return { text: sum.open.length ? String(sum.open.length) : "", color: BADGE_BRAND };
  const t = pnlBadgeText(sum.pnlToday);
  return { text: t, color: t === "0" ? BADGE_GREY : sum.pnlToday > 0 ? BADGE_GREEN : BADGE_RED };
}

/**
 * Notifications that the change from `prev` to `next` deserves. Returns [{ id, kind, title, message }].
 * Nothing is reported until a baseline exists (the snapshot that arrives right after connecting).
 */
export function diffNotifications(prev, next, prefs = DEFAULT_PREFS) {
  const out = [];
  if (prev.primed.positions && next.primed.positions) {
    const before = new Map(prev.positions.map((p) => [p.id, p]));
    for (const p of next.positions) {
      const was = before.get(p.id);
      if (!was && p.state !== "CLOSED" && prefs.buy) {
        out.push({ id: `buy-${p.id}`, kind: "buy", title: `Bought ${p.symbol}`, message: `Paper buy of ${fmtSol(p.sizeSol)}. The bot decided this by its own rules.` });
      }
      if (was && was.state !== "CLOSED" && p.state === "CLOSED" && prefs.sell) {
        out.push({ id: `sell-${p.id}`, kind: "sell", title: `Sold ${p.symbol}`, message: `${fmtSol(p.pnlSol, true)} (${p.pnlPct >= 0 ? "+" : ""}${Number(p.pnlPct).toFixed(1)}%). Reason: ${p.exitReason || "exit rule"}.` });
      }
    }
  }
  if (prev.primed.halt && next.primed.halt && prefs.limit) {
    const was = prev.stats?.lanes?.find((l) => l.halted)?.halted;
    const now = next.stats?.lanes?.find((l) => l.halted)?.halted;
    if (now && !was) out.push({ id: `limit-${Date.now()}`, kind: "limit", title: "Daily loss limit reached", message: `${now}. The bot stops buying until you clear it.` });
  }
  if (prev.primed.kill && next.primed.kill && prefs.kill && !prev.kill?.killed && next.kill?.killed) {
    out.push({ id: `kill-${Date.now()}`, kind: "kill", title: "Kill switch is on", message: next.kill.text || "All trading stopped and open positions are being sold." });
  }
  return out;
}

/** Positions that just closed (for closing coin tabs). */
export function closedMints(prev, next) {
  if (!prev.primed.positions || !next.primed.positions) return [];
  const was = new Map(prev.positions.map((p) => [p.id, p]));
  const stillOpen = new Set(next.positions.filter((p) => p.state !== "CLOSED").map((p) => p.mint));
  return [...new Set(next.positions.filter((p) => p.state === "CLOSED" && was.get(p.id) && was.get(p.id).state !== "CLOSED" && !stillOpen.has(p.mint)).map((p) => p.mint))];
}

export function heldMints(state) {
  return [...new Set(state.positions.filter((p) => p.state !== "CLOSED").map((p) => p.mint))];
}

export const STATUS_TEXT = {
  held: "Held",
  watching: "Watching",
  skipped: "Skipped",
  blocked: "Blocked",
  closed: "Closed",
  unknown: "Not seen",
};

export function dexUrl(mint) {
  return `https://dexscreener.com/solana/${mint}`;
}
