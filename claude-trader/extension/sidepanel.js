import { summarize, fmtSol } from "./lib/core.js";

const app = document.getElementById("app");
const connEl = document.getElementById("conn");
const modeEl = document.getElementById("mode");
let state = null;
let ask = null; // "kill" | "resume" | null
let busy = false;
let pairError = "";
let pairing = false;
let shownKey = "";

const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const cls = (n) => (n < 0 ? "neg" : n > 0 ? "pos" : "");
const hhmm = (ms) => new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
const send = (msg) => new Promise((res) => chrome.runtime.sendMessage(msg, (r) => res(r)));

const port = chrome.runtime.connect({ name: "panel" });
port.onMessage.addListener((m) => {
  if (m.t === "state") {
    state = m.state;
    render();
  }
});

function pill() {
  const c = state?.conn;
  connEl.className = "pill " + (c === "connected" ? "on" : c === "offline" ? "off" : "");
  connEl.textContent = c === "connected" ? "Connected" : c === "offline" ? "Engine offline" : c === "unpaired" ? "Not paired" : "Connecting";
  modeEl.textContent = state?.stats?.replay ? "REPLAY" : "PAPER";
}

function view() {
  if (!state) return `<div class="empty">Loading...</div>`;
  if (state.conn === "unpaired") return pairView();
  if (state.conn !== "connected") return offlineView();
  return mainView();
}

function pairView() {
  return `<section class="card hero">
    <h1>Connect to the app</h1>
    <p class="note">Open the Claude Trader app, go to Settings, Startup and browser, and press <b>Get pairing code</b>. Type the code here. You only do this once.</p>
    <input type="text" id="code" inputmode="numeric" autocomplete="off" placeholder="0000-0000" maxlength="9" aria-label="Pairing code">
    <button class="btn" id="pair" ${pairing ? "disabled" : ""}>${pairing ? "Connecting..." : "Connect"}</button>
    ${pairError ? `<div class="err" role="alert">${esc(pairError)}</div>` : state.error ? `<div class="err">${esc(state.error)}</div>` : ""}
  </section>`;
}

function offlineView() {
  return `<section class="card hero" data-testid="offline">
    <h1>Engine offline</h1>
    <p class="note">The Claude Trader app is closed or not responding. Open it and this panel connects by itself.</p>
    <button class="btn ghost" id="retry">Try again now</button>
    <p class="note">The bot only trades while the app is open.</p>
  </section>${settingsView()}`;
}

function mainView() {
  const s = summarize(state);
  const killed = !!state.kill?.killed;
  const open = s.open;
  const feed = (state.activity || []).slice(0, 14);
  const learn = state.learning?.journal || [];
  return `
  ${killed ? `<div class="banner" role="alert">Kill switch is on. No new buys. ${esc(state.kill?.text || "")}</div>` : ""}
  ${s.halted ? `<div class="banner">Daily loss limit reached: ${esc(s.halted)}</div>` : ""}
  <section class="stats" aria-label="Account">
    <div class="stat"><span class="k">Balance</span><span class="v">${s.balanceSol == null ? "-" : fmtSol(s.balanceSol)}</span></div>
    <div class="stat"><span class="k">P/L today</span><span class="v ${cls(s.pnlToday)}">${fmtSol(s.pnlToday, true)}</span></div>
    <div class="stat"><span class="k">P/L total</span><span class="v ${cls(s.pnlTotal)}">${fmtSol(s.pnlTotal, true)}</span></div>
  </section>
  <section class="killrow">
    ${ask ? confirmView() : killed ? `<button class="kill resume" id="ask-resume">Resume trading</button>` : `<button class="kill" id="ask-kill">Kill switch</button>`}
  </section>
  <section><h2>Open positions (${open.length})</h2>
    ${open.length ? `<div class="list">${open.map(posRow).join("")}</div>` : `<div class="empty">Nothing open right now.</div>`}
  </section>
  <section><h2>Activity</h2>
    ${feed.length ? `<ul class="feed">${feed.map((a) => `<li><span class="t">${hhmm(a.ts)}</span>${esc(a.text)}</li>`).join("")}</ul>` : `<div class="empty">Nothing yet.</div>`}
  </section>
  <section><h2>What it learned</h2>
    ${learn.length ? `<div class="learn">${learn.map((r) => `<div class="e"><b>${esc(r.symbol)}</b> <span class="${cls(r.pnlSol)}">${fmtSol(r.pnlSol, true)}</span><br>${esc(r.headline)}</div>`).join("")}</div>` : `<div class="empty">Nothing yet. It writes a note after each trade.</div>`}
    ${state.learning ? `<div class="note" style="margin-top:6px">Next review in ${esc(state.learning.nextReviewIn)} trades.${state.learning.demo ? " (demo data)" : ""}</div>` : ""}
  </section>
  ${xView()}
  ${settingsView()}`;
}

function xView() {
  const x = state.xmon;
  if (!x) return "";
  const dot = x.state === "working" ? "pos" : x.state === "paused" || x.state === "not-logged-in" ? "neg" : "";
  return `<section class="card" data-testid="xmon"><h2>X Monitor</h2>
    <div class="note"><span class="${dot}"><b>${esc(x.line || "Off")}</b></span></div>
    <div class="note" style="margin-top:4px">${x.state === "off" ? "Turn it on in the app: Settings, X Monitor." : `Pages opened this hour: ${esc(x.loadsLastHour)} of ${esc(x.maxPerHour)}. It only reads x.com in one background tab.`}</div>
  </section>`;
}

function posRow(p) {
  return `<div class="pos-row">
    <button class="sym" data-coin="${esc(p.mint)}" title="Open the chart">${esc(p.symbol)}</button>
    <span class="pnl ${cls(p.pnlSol)}">${fmtSol(p.pnlSol, true)} (${p.pnlPct >= 0 ? "+" : ""}${Number(p.pnlPct).toFixed(1)}%)</span>
    <span class="sub">${esc(p.state === "SELLING" ? "Selling now" : p.nextAction || "Holding")} · ${Math.round(p.holdSeconds)} s</span>
  </div>`;
}

function confirmView() {
  const kill = ask === "kill";
  return `<div class="confirm" role="alertdialog" aria-label="Confirm">
    <b>${kill ? "Stop all trading and sell every open position now?" : "Allow the bot to trade again?"}</b>
    <div class="btns"><button class="btn ${kill ? "danger" : ""}" id="yes" ${busy ? "disabled" : ""}>${kill ? "Yes, kill it" : "Yes, resume"}</button><button class="btn ghost" id="no">Cancel</button></div>
  </div>`;
}

function settingsView() {
  const p = state?.prefs || {};
  const box = (k, label) => `<label class="chk"><input type="checkbox" data-pref="${k}" ${p[k] ? "checked" : ""}><span>${label}</span></label>`;
  return `<section class="card"><h2>Notifications</h2>
    ${box("buy", "When the bot buys")}${box("sell", "When the bot sells")}${box("limit", "Daily loss limit reached")}${box("kill", "Kill switch pressed")}${box("offline", "Engine goes offline")}
    <h2 style="margin-top:10px">Icon badge shows</h2>
    <div class="seg" role="radiogroup" aria-label="Badge"><button role="radio" aria-checked="${p.badge !== "open"}" data-badge="pnl">P/L today</button><button role="radio" aria-checked="${p.badge === "open"}" data-badge="open">Open positions</button></div>
    ${state?.conn !== "unpaired" ? `<div style="margin-top:10px"><button class="btn ghost small" id="unpair">Unpair from the app</button></div>` : ""}
  </section>`;
}

function render() {
  pill();
  // do not rebuild the page under someone typing a code
  const typing = document.activeElement && document.activeElement.id === "code";
  const key = state?.conn === "unpaired" ? "unpaired" : "";
  if (typing && shownKey === key) return;
  shownKey = key;
  app.innerHTML = view();
  wire();
}

function wire() {
  const $ = (id) => document.getElementById(id);
  $("pair")?.addEventListener("click", async () => {
    const code = $("code").value;
    pairing = true;
    pairError = "";
    render();
    const r = await send({ t: "pair", code });
    pairing = false;
    if (!r?.ok) pairError = r?.error || "Could not pair.";
    render();
  });
  $("code")?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") $("pair").click();
  });
  $("retry")?.addEventListener("click", () => send({ t: "reconnect" }));
  $("ask-kill")?.addEventListener("click", () => ((ask = "kill"), render()));
  $("ask-resume")?.addEventListener("click", () => ((ask = "resume"), render()));
  $("no")?.addEventListener("click", () => ((ask = null), render()));
  $("yes")?.addEventListener("click", async () => {
    busy = true;
    render();
    await send({ t: "cmd", cmd: ask });
    busy = false;
    ask = null;
    render();
  });
  $("unpair")?.addEventListener("click", () => send({ t: "unpair" }));
  document.querySelectorAll("[data-coin]").forEach((b) => b.addEventListener("click", () => send({ t: "openCoin", mint: b.dataset.coin })));
  document.querySelectorAll("[data-pref]").forEach((c) => c.addEventListener("change", () => send({ t: "setPrefs", prefs: { [c.dataset.pref]: c.checked } })));
  document.querySelectorAll("[data-badge]").forEach((b) => b.addEventListener("click", () => send({ t: "setPrefs", prefs: { badge: b.dataset.badge } })));
}

send({ t: "getState" }).then((r) => {
  if (r?.state && !state) {
    state = r.state;
    render();
  }
});
