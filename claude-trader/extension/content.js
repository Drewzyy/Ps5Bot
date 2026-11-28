// Claude Trader: small coin card on pump.fun coin pages and dexscreener.com/solana pages.
// It asks the app what the bot thinks of the coin. "Check this coin" only runs a scam check. It cannot buy.
(function () {
  if (window.__ctOverlay) return;
  window.__ctOverlay = true;
  const M = globalThis.CTMint;
  const REFRESH_MS = 10000;
  let href = "";
  let mint = null;
  let host = null;
  let root = null;
  let timer = null;
  let busy = false;
  let last = null;
  let minimized = false;

  const css = `
  :host { all: initial; }
  .card { position: fixed; right: 16px; bottom: 16px; z-index: 2147483647; width: 270px; font: 13px/1.4 Inter, "Segoe UI", system-ui, sans-serif;
    --bg:#fff; --text:#374151; --head:#111827; --muted:#6b7280; --line:#e5e7eb; --green:#16a34a; --red:#dc2626; --amber:#b45309; --blue:#2563eb;
    background: var(--bg); color: var(--text); border: 1px solid var(--line); border-radius: 12px; box-shadow: 0 6px 24px rgba(17,24,39,.22); overflow: hidden; }
  @media (prefers-color-scheme: dark) { .card { --bg:#16191e; --text:#d1d5db; --head:#f3f4f6; --muted:#9ca3af; --line:#2b3038; --green:#22c55e; --red:#f87171; --amber:#fbbf24; --blue:#60a5fa; } }
  .top { display:flex; align-items:center; gap:8px; padding:8px 10px; cursor:pointer; border-bottom:1px solid var(--line); }
  .top b { color: var(--head); flex:1; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .chip { font-weight:700; font-size:11px; border-radius:999px; padding:2px 8px; border:1px solid currentColor; }
  .held { color: var(--green); } .watching { color: var(--blue); } .skipped,.closed,.unknown { color: var(--muted); } .blocked { color: var(--red); }
  .body { padding:8px 10px 10px; display:grid; gap:6px; }
  .row { display:flex; justify-content:space-between; gap:8px; } .k { color: var(--muted); }
  .why { color: var(--text); }
  .pass { color: var(--green); font-weight:700; } .block { color: var(--red); font-weight:700; }
  button { font: inherit; font-weight:700; border-radius:8px; padding:6px 10px; border:1px solid var(--green); background: transparent; color: var(--green); cursor:pointer; }
  button:disabled { opacity:.55; cursor:default; }
  .hint { color: var(--muted); font-size:11.5px; }
  a { color: var(--blue); }
  .off { color: var(--muted); }
  `;

  const logo = `<svg viewBox="0 0 12 9" width="22" height="17" shape-rendering="crispEdges" aria-hidden="true"><g fill="#D97757"><rect x="2" y="0" width="8" height="7"/><rect x="0" y="2" width="2" height="2"/><rect x="10" y="2" width="2" height="2"/><rect x="3" y="7" width="1" height="2"/><rect x="5" y="7" width="1" height="2"/><rect x="7" y="7" width="1" height="2"/><rect x="9" y="7" width="1" height="2"/></g><g fill="#141414"><rect x="3" y="1" width="1" height="1"/><rect x="8" y="1" width="1" height="1"/></g></svg>`;
  const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  const STATUS = { held: "Held", watching: "Watching", skipped: "Skipped", blocked: "Blocked", closed: "Closed", unknown: "Not seen" };

  function ask(msg) {
    return new Promise((res) => {
      try {
        chrome.runtime.sendMessage(msg, (r) => res(chrome.runtime.lastError ? { t: "error", error: "Extension was updated. Reload this page." } : r));
      } catch {
        res({ t: "error", error: "Extension was updated. Reload this page." });
      }
    });
  }

  function ensureRoot() {
    if (host && document.documentElement.contains(host)) return;
    host = document.createElement("div");
    host.id = "claude-trader-overlay";
    root = host.attachShadow({ mode: "open" });
    root.innerHTML = `<style>${css}</style><div class="card" part="card"></div>`;
    document.documentElement.appendChild(host);
  }
  function remove() {
    host?.remove();
    host = null;
    root = null;
  }

  // X community score and flags (from the X Monitor in the app). Unknown is shown as unknown, never as a pass.
  function xRows(x) {
    if (!x) return "";
    const link = typeof x.url === "string" && x.url.startsWith("https://x.com/") ? `<a href="${esc(x.url)}" target="_blank" rel="noreferrer noopener">${esc(x.linkLabel || "X link")}</a>` : "";
    if (x.status === "ready") {
      const flags = [...(x.flags || [])].slice(0, 4).map((f) => `<div class="hint">${esc(f)}</div>`).join("");
      return `<div class="row"><span class="k">X community</span><span><b>${Math.round(x.score)}</b> of 100 (+${x.points} pts)</span></div>${x.rank ? `<div class="row"><span class="k">Copy rank</span><span>${esc(x.rank)} of ${esc(x.clones)}</span></div>` : ""}${link ? `<div class="row"><span class="k">Source</span>${link}</div>` : ""}${flags}`;
    }
    if (x.status === "blocked") {
      return `<div class="row"><span class="k">X community</span><span class="block">Blocked</span></div>${(x.blocks || []).slice(0, 3).map((b) => `<div class="hint">${esc(b)}</div>`).join("")}${link ? `<div class="row"><span class="k">Source</span>${link}</div>` : ""}`;
    }
    const why = { unknown: x.reason || "X data is not ready. Scored without it.", nolink: "This coin has no X link.", off: "X Monitor is off." }[x.status] || "";
    return `<div class="row"><span class="k">X community</span><span class="off">${x.status === "unknown" ? "Unknown" : x.status === "nolink" ? "No X link" : "Off"}</span></div>${why ? `<div class="hint">${esc(why)}</div>` : ""}${link ? `<div class="row"><span class="k">Source</span>${link}</div>` : ""}`;
  }

  function render() {
    ensureRoot();
    const card = root.querySelector(".card");
    const v = last;
    if (!v) {
      card.innerHTML = `<div class="top">${logo}<b>Claude Trader</b></div><div class="body off">Looking up this coin...</div>`;
      return;
    }
    if (v.offline) {
      card.innerHTML = `<div class="top">${logo}<b>Claude Trader</b><span class="chip unknown">Engine offline</span></div><div class="body off">Open the Claude Trader app to see what the bot thinks of this coin.</div>`;
      return;
    }
    if (v.error) {
      card.innerHTML = `<div class="top">${logo}<b>Claude Trader</b></div><div class="body why">${esc(v.error)}</div>`;
      return;
    }
    const RISK = { LOW_SIGNAL: "low risk", ELEVATED: "elevated risk", HIGH: "high risk" };
    const risk = RISK[v.shieldLevel] ? ` (${RISK[v.shieldLevel]})` : "";
    const shield = v.shield === "pass" ? `<span class="pass">Passed${risk}</span>` : v.shield === "block" ? `<span class="block">Blocked${risk}</span>` : `<span class="off">Not checked yet</span>`;
    const score = v.score != null ? `<div class="row"><span class="k">Score</span><span>${Math.round(v.score)} of 100${v.minScore ? ` (needs ${Math.round(v.minScore)})` : ""}</span></div>` : "";
    card.innerHTML = `
      <div class="top" id="hd">${logo}<b>${esc(v.symbol)}</b><span class="chip ${esc(v.status)}">${esc(STATUS[v.status] || v.status)}</span></div>
      ${minimized ? "" : `<div class="body">
        <div class="row"><span class="k">Scam Shield</span>${shield}</div>
        ${score}
        ${xRows(v.x)}
        <div class="why">${esc(v.reason)}</div>
        <button id="chk" ${busy ? "disabled" : ""}>${busy ? "Checking..." : "Check this coin"}</button>
        <div class="hint">Only runs the scam check. The bot buys by its own rules, never from this button.</div>
      </div>`}`;
    card.querySelector("#hd").onclick = () => {
      minimized = !minimized;
      render();
    };
    const b = card.querySelector("#chk");
    if (b) b.onclick = check;
  }

  async function lookup() {
    if (!mint) return;
    const r = await ask({ t: "verdict", mint });
    last = r.verdict ? r.verdict : r.offline ? { offline: true } : { error: r.error || "No answer." };
    render();
  }

  async function check() {
    if (busy || !mint) return;
    busy = true;
    render();
    const r = await ask({ t: "check", mint });
    busy = false;
    last = r.verdict ? r.verdict : r.offline ? { offline: true } : { error: r.error || "The check did not finish." };
    render();
  }

  function hintForPage() {
    const h = location.hostname.replace(/^www\./, "");
    if (h === "pump.fun") return { host: h, pathMint: M.pumpMint(location.pathname) };
    const links = [...document.querySelectorAll("a[href]")].map((a) => a.href);
    return { host: h, pathId: M.dexId(location.pathname), domMints: M.mintsFromLinks(links) };
  }

  async function onPage() {
    href = location.href;
    clearInterval(timer);
    mint = null;
    last = null;
    remove();
    const hint = hintForPage();
    if (!(hint.pathMint || hint.pathId)) return; // not a coin page
    const r = await ask({ t: "resolve", hint });
    if (location.href !== href || !r || !r.mint) return;
    mint = r.mint;
    render();
    await lookup();
    timer = setInterval(() => {
      if (!document.hidden && !busy) lookup();
    }, REFRESH_MS);
  }

  onPage();
  // these sites change the address without reloading the page
  setInterval(() => {
    if (location.href !== href) onPage();
  }, 1500);
})();
