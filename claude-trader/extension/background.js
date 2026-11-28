// Claude Trader extension: background worker.
// Talks to the desktop app over ws://127.0.0.1:8766 (only this extension may connect, and only after pairing).
// It never holds a key or secret: just a pairing token. It cannot buy or sell. It can run a scam check and press the kill switch.
import { DEFAULT_PREFS, emptyState, applyEvent, resetPrimed, badgeFor, diffNotifications, closedMints, heldMints, dexUrl } from "./lib/core.js";
import { createXRunner } from "./xrunner.js";
import { DEFAULT_PACE } from "./lib/xpace.js";

const URL_WS = "ws://127.0.0.1:8766";
const BACKOFF = [1000, 2000, 3000, 5000];
const OFFLINE_NOTICE_MS = 15000;

let state = emptyState();
let prefs = { ...DEFAULT_PREFS };
let ws = null;
let attempt = 0;
let retryTimer = null;
let offlineTimer = null;
let wasConnected = false;
let reqId = 0;
const pending = new Map();
const ports = new Set();
let pairWaiter = null;
let booted = null;

const store = chrome.storage;

async function boot() {
  const { prefs: p } = await store.local.get("prefs");
  prefs = { ...DEFAULT_PREFS, ...(p || {}) };
  const { cache } = await store.session.get("cache");
  if (cache && state.conn === "connecting") state = { ...state, ...cache, conn: "connecting", primed: state.primed };
}
booted = boot();

// ------------------------------------------------------------------ state, badge, panel

let pushTimer = null;
function changed() {
  if (pushTimer) return;
  pushTimer = setTimeout(() => {
    pushTimer = null;
    const view = publicState();
    for (const p of ports) {
      try {
        p.postMessage({ t: "state", state: view });
      } catch {
        ports.delete(p);
      }
    }
    store.session.set({ cache: { stats: state.stats, positions: state.positions, activity: state.activity, kill: state.kill, learning: state.learning, xmon: state.xmon } }).catch(() => {});
    paintBadge();
  }, 150);
}

function publicState() {
  return { ...state, prefs };
}

function paintBadge() {
  const b = badgeFor(state, prefs.badge);
  chrome.action.setBadgeText({ text: b.text });
  chrome.action.setBadgeBackgroundColor({ color: b.color });
  if (chrome.action.setBadgeTextColor) chrome.action.setBadgeTextColor({ color: "#ffffff" });
}

function setConn(conn, error = "") {
  state = { ...state, conn, error };
  changed();
}

// ------------------------------------------------------------------ connection

function schedule(ms) {
  clearTimeout(retryTimer);
  retryTimer = setTimeout(connect, ms);
}

async function connect() {
  await booted;
  if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) return;
  const { token } = await store.local.get("token");
  if (!token) {
    setConn("unpaired");
    return;
  }
  openSocket({ t: "auth", token });
}

function openSocket(first) {
  let sock;
  try {
    sock = new WebSocket(URL_WS);
  } catch {
    return schedule(BACKOFF[Math.min(attempt++, BACKOFF.length - 1)]);
  }
  ws = sock;
  let authed = false;
  sock.onopen = () => sock.send(JSON.stringify(first));
  sock.onmessage = async (m) => {
    let v;
    try {
      v = JSON.parse(m.data);
    } catch {
      return;
    }
    if (v.t === "paired") {
      await store.local.set({ token: v.token });
      authed = true;
      onSignedIn();
      pairWaiter?.({ ok: true });
      pairWaiter = null;
    } else if (v.t === "auth") {
      if (v.ok) {
        authed = true;
        onSignedIn();
      } else {
        const notPaired = /Not paired|wrong or has expired/i.test(v.error || "");
        if (first.t === "pair") {
          pairWaiter?.({ ok: false, error: v.error });
          pairWaiter = null;
        } else if (notPaired) {
          await store.local.remove("token");
          setConn("unpaired", "The app does not know this extension any more. Pair again.");
        }
        sock.close();
      }
    } else if (!authed) {
      return;
    } else if (v.t === "ping") {
      sock.send(JSON.stringify({ t: "pong" }));
    } else if (v.t === "event") {
      onEvent(v.e);
    } else if (v.t === "settings") {
      state = { ...state, settings: v.startup };
      if (v.xmonitor) xSettings = { enabled: !!v.xmonitor.enabled, paused: !!v.xmonitor.paused, pace: { ...DEFAULT_PACE, ...(v.xmonitor.pace || {}) } };
      changed();
      tabsOnSettings();
    } else if (v.id != null && pending.has(v.id)) {
      const r = pending.get(v.id);
      pending.delete(v.id);
      r(v);
    }
  };
  sock.onclose = () => {
    if (ws === sock) ws = null;
    for (const r of pending.values()) r({ t: "error", error: "Engine offline" });
    pending.clear();
    pairWaiter?.({ ok: false, error: "Could not reach the Claude Trader app. Is it open?" });
    pairWaiter = null;
    state = resetPrimed(state);
    if (state.conn !== "unpaired") {
      setConn("offline");
      if (wasConnected && !offlineTimer) {
        offlineTimer = setTimeout(() => {
          offlineTimer = null;
          if (state.conn === "offline" && prefs.offline) notify({ id: "offline", title: "Claude Trader is offline", message: "The app is closed or not responding. Open Claude Trader to keep the bot running." });
        }, OFFLINE_NOTICE_MS);
      }
    }
    if (state.conn !== "unpaired") schedule(BACKOFF[Math.min(attempt++, BACKOFF.length - 1)]);
  };
  sock.onerror = () => {};
}

function onSignedIn() {
  attempt = 0;
  wasConnected = true;
  clearTimeout(offlineTimer);
  offlineTimer = null;
  setConn("connected");
}

function onEvent(e) {
  const prev = state;
  state = applyEvent(state, e);
  for (const n of diffNotifications(prev, state, prefs)) notify(n);
  for (const mint of closedMints(prev, state)) tabsOnClosed(mint);
  if (e.type === "positions") tabsOnPositions(prev);
  changed();
}

function request(msg, timeoutMs = 35000) {
  return new Promise((resolve) => {
    if (!ws || ws.readyState !== WebSocket.OPEN || state.conn !== "connected") return resolve({ t: "error", error: "Engine offline" });
    const id = ++reqId;
    const timer = setTimeout(() => {
      pending.delete(id);
      resolve({ t: "error", error: "No answer from the app. Try again." });
    }, timeoutMs);
    pending.set(id, (v) => {
      clearTimeout(timer);
      resolve(v);
    });
    ws.send(JSON.stringify({ ...msg, id }));
  });
}

async function pair(code) {
  await booted;
  if (ws) {
    try {
      ws.close();
    } catch {
      /* already closed */
    }
    ws = null;
  }
  clearTimeout(retryTimer);
  const done = new Promise((resolve) => {
    pairWaiter = resolve;
    setTimeout(() => {
      if (pairWaiter === resolve) {
        pairWaiter = null;
        resolve({ ok: false, error: "Could not reach the Claude Trader app. Is it open?" });
      }
    }, 10000);
  });
  openSocket({ t: "pair", code: String(code || "").trim() });
  return done;
}

async function unpair() {
  await store.local.remove("token");
  try {
    ws?.close();
  } catch {
    /* ignore */
  }
  ws = null;
  state = resetPrimed({ ...emptyState(), conn: "unpaired" });
  changed();
}

// ------------------------------------------------------------------ X Monitor (reads x.com in one background tab)
// The app queues jobs; this runner opens each page (8 to 15 s apart), collects the page's own data and sends it back.
// It never clicks, posts, likes or follows. Off until the app says it is on.

let xSettings = { enabled: false, paused: false, pace: { ...DEFAULT_PACE } };
const xRunner = createXRunner({
  chrome,
  ask: (m) => request(m, 12000),
  connected: () => state.conn === "connected",
  now: () => Date.now(),
  sleep: (ms) => new Promise((r) => setTimeout(r, ms)),
  rand: Math.random,
  getSettings: () => xSettings,
  store: {
    get: async (k) => (await store.session.get(k))[k],
    set: (k, v) => store.session.set({ [k]: v }),
  },
  log: (t) => console.log(t),
});
xRunner.start();
chrome.tabs.onRemoved.addListener((id) => {
  store.session.get("xTab").then((r) => {
    if (r.xTab === id) store.session.remove("xTab");
  });
});

// ------------------------------------------------------------------ notifications

function notify(n) {
  try {
    chrome.notifications.create(n.id, { type: "basic", iconUrl: "icons/icon128.png", title: n.title, message: n.message, priority: 1 });
  } catch {
    /* notifications can be blocked by the system */
  }
}

// ------------------------------------------------------------------ tabs
// Rules: reuse a tab that is already open, never open the same page twice, and only ever close tabs this extension opened.

// one tab job at a time, so two quick events can never open the same page twice
let tabQueue = Promise.resolve();
const serial = (fn) => (tabQueue = tabQueue.then(fn).catch(() => {}));

async function tabsGet() {
  const { tabs } = await store.session.get("tabs");
  return tabs || { opened: {}, done: [], home: false };
}
async function tabsSet(t) {
  await store.session.set({ tabs: t });
}

async function findTab(patterns) {
  try {
    const found = await chrome.tabs.query({ url: patterns });
    return found[0] || null;
  } catch {
    return null;
  }
}

async function ensureHome() {
  const t = await tabsGet();
  if (t.home || !state.settings?.pages?.pumpFunHome) return;
  t.home = true; // once per browser session: if you close it, it stays closed
  await tabsSet(t);
  if (await findTab(["https://pump.fun/*", "https://www.pump.fun/*"])) return;
  await chrome.tabs.create({ url: "https://pump.fun/", active: false });
}

async function ensureCoinTab(mint) {
  if (!state.settings?.pages?.dexscreenerHeld) return;
  const t = await tabsGet();
  if (t.done.includes(mint)) return;
  t.done = [...t.done, mint].slice(-200);
  const existing = await findTab([`https://dexscreener.com/solana/${mint}*`, `https://www.dexscreener.com/solana/${mint}*`]);
  if (!existing) {
    const tab = await chrome.tabs.create({ url: dexUrl(mint), active: false });
    t.opened[mint] = tab.id; // ours: allowed to close later
  }
  await tabsSet(t);
}

function tabsOnSettings() {
  return serial(async () => {
    await ensureHome();
    for (const m of heldMints(state)) await ensureCoinTab(m);
  });
}

function tabsOnPositions(prev) {
  const before = new Set(heldMints(prev));
  const fresh = heldMints(state).filter((m) => !prev.primed.positions || !before.has(m));
  if (!fresh.length) return;
  return serial(async () => {
    for (const m of fresh) await ensureCoinTab(m);
  });
}

function tabsOnClosed(mint) {
  return serial(() => closeCoinTab(mint));
}

async function closeCoinTab(mint) {
  const t = await tabsGet();
  t.done = t.done.filter((x) => x !== mint); // a later buy of the same coin may open its chart again
  const id = state.settings?.closeTabOnClose ? t.opened[mint] : null;
  if (id != null) delete t.opened[mint];
  await tabsSet(t);
  if (id == null) return;
  try {
    await chrome.tabs.remove(id); // only a tab this extension opened
  } catch {
    /* already closed */
  }
}

async function openCoin(mint) {
  const existing = await findTab([`https://dexscreener.com/solana/${mint}*`, `https://www.dexscreener.com/solana/${mint}*`]);
  if (existing) {
    await chrome.tabs.update(existing.id, { active: true });
    await chrome.windows.update(existing.windowId, { focused: true });
  } else {
    const tab = await chrome.tabs.create({ url: dexUrl(mint), active: true });
    const t = await tabsGet();
    t.opened[mint] = tab.id;
    await tabsSet(t);
  }
}

// ------------------------------------------------------------------ page awareness (content script asks us)

async function resolveMint(hint) {
  const A = (s) => typeof s === "string" && /^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(s);
  if (hint.host.endsWith("pump.fun")) return A(hint.pathMint) ? hint.pathMint : null;
  // Dexscreener: a token link on the page is the best answer; otherwise the URL id is a pair address (ask Dexscreener) or a token address
  for (const m of hint.domMints || []) if (A(m)) return m;
  if (!A(hint.pathId)) return null;
  try {
    const r = await fetch(`https://api.dexscreener.com/latest/dex/pairs/solana/${hint.pathId}`);
    const j = await r.json();
    const base = j?.pairs?.[0]?.baseToken?.address;
    if (A(base)) return base;
  } catch {
    /* offline or blocked: fall through */
  }
  return hint.pathId;
}

// ------------------------------------------------------------------ messages from the panel and the pages

chrome.runtime.onMessage.addListener((msg, sender, send) => {
  (async () => {
    await booted;
    // messages from the x.com content script (only the bot's job tab is answered)
    if (sender.tab && typeof msg.t === "string" && msg.t.startsWith("x")) return send(xRunner.onMessage(msg, sender));
    switch (msg.t) {
      case "getState":
        return send({ state: publicState() });
      case "pair":
        return send(await pair(msg.code));
      case "unpair":
        await unpair();
        return send({ ok: true });
      case "reconnect":
        attempt = 0;
        connect();
        return send({ ok: true });
      case "cmd":
        if (msg.cmd !== "kill" && msg.cmd !== "resume") return send({ t: "error", error: "Not allowed" });
        return send(await request({ t: "cmd", cmd: msg.cmd }, 10000));
      case "setPrefs":
        prefs = { ...prefs, ...msg.prefs };
        await store.local.set({ prefs });
        changed();
        return send({ ok: true });
      case "openCoin":
        await openCoin(String(msg.mint));
        return send({ ok: true });
      case "resolve":
        return send({ mint: await resolveMint(msg.hint) });
      case "verdict":
        if (state.conn !== "connected") return send({ t: "error", error: "Engine offline", offline: true });
        return send(await request({ t: "verdict", mint: msg.mint }, 8000));
      case "check":
        if (state.conn !== "connected") return send({ t: "error", error: "Engine offline", offline: true });
        return send(await request({ t: "check", mint: msg.mint }, 35000));
      default:
        return send({ t: "error", error: "Unknown" });
    }
  })();
  return true;
});

chrome.runtime.onConnect.addListener((port) => {
  if (port.name !== "panel") return;
  ports.add(port);
  port.onDisconnect.addListener(() => ports.delete(port));
  port.postMessage({ t: "state", state: publicState() });
  connect();
});

// ------------------------------------------------------------------ start up and stay up

chrome.sidePanel?.setPanelBehavior?.({ openPanelOnActionClick: true }).catch(() => {});
chrome.runtime.onInstalled.addListener(() => {
  chrome.sidePanel?.setPanelBehavior?.({ openPanelOnActionClick: true }).catch(() => {});
  connect();
});
chrome.runtime.onStartup.addListener(connect);
chrome.alarms.create("keepalive", { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener((a) => {
  if (a.name === "keepalive") connect();
});
connect();
paintBadge();
