import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { api } from "../source";
import { readThemePref, saveThemePref } from "../theme";
import type { ThemePref } from "../theme";
import type { BuildInfo, Config, ConnTest, KeyStatus, StartupView, UpdateCheck } from "../types";
import type { EngineState } from "../useEngine";
import { SECTIONS, flatten } from "../settingsMeta";
import type { Row } from "../settingsMeta";
import { SourceLink } from "./links";
import { FeedHealthLine } from "./Health";

function show(v: unknown): string {
  if (typeof v === "boolean") return v ? "On" : "Off";
  if (Array.isArray(v)) return v.join(", ");
  return String(v);
}

function SettingRow({ row }: { row: Row }) {
  const [open, setOpen] = useState(false);
  const id = `exp-${row.display}`;
  return (
    <div className="set-row" data-setting={row.display}>
      <div className="set-main">
        <span className="lab">
          {row.meta?.label ?? row.display}
          <span className="key">{row.display}</span>
        </span>
        <span className="val">{show(row.value)}</span>
        <button className="info" aria-expanded={open} aria-controls={id} aria-label={`Explain ${row.meta?.label ?? row.display}`} onClick={() => setOpen(!open)}>
          i
        </button>
      </div>
      {open && (
        <div className="explain" id={id}>
          {row.meta ? (
            <>
              <p><b>What it does.</b> {row.meta.what}</p>
              <p><b>A good value.</b> {row.meta.good}</p>
              <p><b>How to set it.</b> {row.meta.how}</p>
            </>
          ) : (
            <p>No explanation written yet for this setting.</p>
          )}
        </div>
      )}
    </div>
  );
}

/** A small ⓘ button with its own explanation, for the parts of Settings that are not plain config values. */
function Info({ label, children }: { label: string; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button className="info inline-i" aria-expanded={open} aria-label={`Explain ${label}`} onClick={() => setOpen(!open)}>i</button>
      {open && <div className="explain" style={{ borderRadius: 10, border: "1px solid var(--line)", marginTop: 6 }}>{children}</div>}
    </>
  );
}

// ------------------------------------------------------------------ accordion

const OPEN_KEY = "claudeTrader.settingsOpen.v1";

function readOpen(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(OPEN_KEY) ?? "[]");
    return Array.isArray(v) ? v.filter((x) => typeof x === "string") : [];
  } catch {
    return [];
  }
}

function Section({ id, title, summary, open, toggle, children }: { id: string; title: string; summary: string; open: boolean; toggle: () => void; children: ReactNode }) {
  return (
    <section className="acc" data-section={id}>
      <h2>
        <button className="acc-head" aria-expanded={open} aria-controls={`acc-${id}`} onClick={toggle}>
          <span>{title}</span>
          <span className="sum">{summary}</span>
          <span className="chev" aria-hidden="true">&#9656;</span>
        </button>
      </h2>
      {open && <div className="acc-body" id={`acc-${id}`}>{children}</div>}
    </section>
  );
}

const GROUPS: { id: string; title: string; keys: string[]; blurb: string }[] = [
  { id: "entry", title: "Entry", keys: ["feed", "observation", "entry"], blurb: "Which coins the app looks at, what a coin must show while it is watched, and the exact moment of a buy." },
  { id: "shield", title: "Scam Shield", keys: ["filters"], blurb: "Checks a coin must pass before it can be bought. These cannot be loosened by the learning feature." },
  { id: "exits", title: "Exits", keys: ["exits"], blurb: "When and how a position is sold. Every position closes within 5 minutes." },
  { id: "risk", title: "Risk", keys: ["mode", "paper", "risk"], blurb: "Pretend wallet and hard limits that protect your money. The learning feature can never raise trade size or loosen these." },
  { id: "fees", title: "Fees", keys: ["fees"], blurb: "The costs the app expects to pay on each trade." },
  { id: "fill", title: "Paper fill realism", keys: ["paperFill"], blurb: "Makes pretend trades behave like real ones, including failures." },
  { id: "learning", title: "Learning", keys: ["learning"], blurb: "How often the app reviews its closed trades, and how long it tests a change before using it." },
];

// ------------------------------------------------------------------ paper fill presets

export const PRESETS = {
  optimistic: { buyFailPct: 5, sellFailPct: 8, delayMsMin: 200, delayMsMax: 600, slippageCapPct: 25 },
  realistic: { buyFailPct: 10, sellFailPct: 15, delayMsMin: 400, delayMsMax: 1200, slippageCapPct: 15 },
  harsh: { buyFailPct: 20, sellFailPct: 25, delayMsMin: 800, delayMsMax: 2500, slippageCapPct: 10 },
} as const;
type PresetName = keyof typeof PRESETS;

function currentPreset(c: Config): PresetName | "custom" {
  const p = c.paperFill;
  for (const [name, v] of Object.entries(PRESETS)) {
    if (v.buyFailPct === p.buyFailPct && v.sellFailPct === p.sellFailPct && v.delayMsMin === p.delayMsMin && v.delayMsMax === p.delayMsMax && v.slippageCapPct === p.slippageCapPct) return name as PresetName;
  }
  return "custom";
}

function FillPreset({ config, onApplied, engine }: { config: Config; onApplied: (c: Config) => void; engine: EngineState }) {
  const cur = currentPreset(config);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const pick = async (name: PresetName) => {
    try {
      const r = await api.setPaperFill({ preset: name });
      if (r !== "saved" && r !== "sent") throw new Error(r);
      onApplied({ ...config, paperFill: { ...PRESETS[name] } } as Config);
      setMsg({ ok: true, text: "Saved. It applies from the next pretend trade." });
    } catch (e) {
      setMsg({ ok: false, text: String(e instanceof Error ? e.message : e) });
    }
  };
  return (
    <div className="acc-pad">
      <div className="field">
        <div className="lblrow">
          <label htmlFor="fill-preset">Preset</label>
          <Info label="Paper fill preset">
            <p><b>What it does.</b> Fills the five numbers below in one step. Optimistic is easy: few failures and fast fills. Realistic is the default. Harsh makes pretend trades fail and slip more.</p>
            <p><b>A good value.</b> Realistic, or Harsh when you want to judge the strategy. Harsher is safer for judging the strategy: if it still makes money when trades go badly, the result is more believable. Optimistic can make a weak strategy look good.</p>
            <p><b>How to set it.</b> Pick one from the list. It is saved and applies from the next pretend trade, with no restart.</p>
          </Info>
        </div>
        <select id="fill-preset" value={cur} onChange={(e) => e.target.value !== "custom" && pick(e.target.value as PresetName)}>
          <option value="optimistic">Optimistic</option>
          <option value="realistic">Realistic (default)</option>
          <option value="harsh">Harsh</option>
          {cur === "custom" && <option value="custom">Custom (set in config.json)</option>}
        </select>
        {msg && <div className={`result ${msg.ok ? "ok" : "bad"}`}>{msg.text}</div>}
      </div>
      {engine.skipped && engine.skipped.checks.length > 0 && (
        <div className="skipped" style={{ border: "none", paddingTop: 0 }}>
          <h4>Skipped in paper mode</h4>
          <p>Paper cannot copy everything about real trading. These parts are skipped, not hidden.</p>
          <ul>
            {engine.skipped.checks.map((c) => <li key={c.name}><b>{c.name}.</b> {c.why}</li>)}
          </ul>
        </div>
      )}
    </div>
  );
}

// ------------------------------------------------------------------ Connections

function Connections({ engine }: { engine: EngineState }) {
  const [status, setStatus] = useState<KeyStatus | null>(null);
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState("");
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    api.heliusStatus().then(setStatus).catch(() => setStatus({ saved: false, hint: "No key saved", storage: "", envOverride: false, unavailable: true }));
  }, []);

  const err = (e: unknown) => String(e instanceof Error ? e.message : e);
  const save = async () => {
    setBusy("save");
    setResult(null);
    try {
      const s = await api.heliusSave(key);
      setKey(""); // the key is never kept in the page after saving
      setStatus(s);
      setResult({ ok: true, text: "Saved. The app is already using it, no restart needed." });
    } catch (e) {
      setResult({ ok: false, text: err(e) });
    }
    setBusy("");
  };
  const test = async () => {
    setBusy("test");
    setResult(null);
    try {
      const r: ConnTest = await api.heliusTest(key);
      setResult({ ok: r.ok, text: r.message });
    } catch (e) {
      setResult({ ok: false, text: err(e) });
    }
    setBusy("");
  };
  const remove = async () => {
    setBusy("remove");
    setResult(null);
    try {
      setStatus(await api.heliusRemove());
      setResult({ ok: true, text: "Key removed. The app is back on the public connection." });
    } catch (e) {
      setResult({ ok: false, text: err(e) });
    }
    setBusy("");
  };

  return (
    <div className="acc-pad">
      <div className="field">
        <div className="lblrow">
          <label htmlFor="helius-key">Helius API key</label>
          <Info label="Helius API key">
            <p><b>What it does.</b> Helius is a free service that gives the app a faster, steadier line to the Solana network. Without a key the app uses a shared public line that drops often and misses trades.</p>
            <p><b>A good value.</b> A free Helius key is enough. You never need to pay for this.</p>
            <p><b>How to set it.</b> Get a free key at helius.dev, then Dashboard &gt; API Keys. Paste it here and press Save. It is kept in your computer's secure storage and is never shown again, only its last four characters.</p>
          </Info>
        </div>
        <input id="helius-key" type="password" autoComplete="off" spellCheck={false} placeholder={status?.saved ? "Paste a new key to replace the saved one" : "Paste your key here"} value={key} onChange={(e) => setKey(e.target.value)} />
        <div className="note">Get a free key at <a className="srclink" style={{ marginLeft: 0 }} href="https://www.helius.dev" rel="noreferrer">helius.dev</a>, then Dashboard &gt; API Keys.</div>
      </div>
      <div className="btns">
        <button className="btn" onClick={save} disabled={busy !== "" || key.trim() === ""}>Save</button>
        <button className="btn ghost" onClick={test} disabled={busy !== "" || (key.trim() === "" && !status?.saved)}>Test connection</button>
        <button className="btn danger" onClick={remove} disabled={busy !== "" || !status?.saved}>Remove key</button>
      </div>
      <div className="kvrows" data-testid="helius-status">
        <div><span className="k">Helius key</span><b>{status ? status.hint : "Checking"}</b></div>
        {status?.saved && status.storage && <div><span className="k">Kept in</span>{status.storage}</div>}
        {status?.envOverride && <div className="note">A HELIUS_API_KEY setting on this computer overrides the saved key.</div>}
        {status?.unavailable && <div className="note">{"Saving a key only works in the desktop app. This page is the browser preview."}</div>}
      </div>
      {result && <div className={`result ${result.ok ? "ok" : "bad"}`} role="status">{result.text}</div>}
      <div className="field" style={{ marginTop: 14 }}>
        <div className="lblrow">
          <b>Buy/sell data health</b>
          <Info label="Buy/sell data health">
            <p><b>What it does.</b> A coin's score comes from the buys and sells the app sees while it watches the coin for 60 seconds. This panel shows whether those buys and sells are really arriving.</p>
            <p><b>A good value.</b> "Trades read, last minute" above 0 while coins are being watched. "Confirmed by the node" close to "coins being watched".</p>
            <p><b>How to set it.</b> If it stays at 0, save a free Helius key above and press Test connection. If you already did, the line "The data connection says" tells you what the server answered.</p>
          </Info>
        </div>
        <FeedHealthLine feed={engine.feed} />
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ Startup and browser

function Toggle({ id, label, checked, onChange, disabled, children }: { id: string; label: string; checked: boolean; onChange: (v: boolean) => void; disabled?: boolean; children?: ReactNode }) {
  return (
    <div className="field">
      <div className="lblrow">
        <label className="chk" htmlFor={id}>
          <input id={id} type="checkbox" checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />
          <span>{label}</span>
        </label>
        {children && <Info label={label}>{children}</Info>}
      </div>
    </div>
  );
}

function StartupBrowser({ engine }: { engine: EngineState }) {
  const [st, setSt] = useState<StartupView | null>(null);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [code, setCode] = useState<string | null>(null);
  const [showGuide, setShowGuide] = useState(false);
  const err = (e: unknown) => String(e instanceof Error ? e.message : e);
  useEffect(() => {
    api.getStartup().then(setSt).catch((e) => setMsg({ ok: false, text: err(e) }));
  }, []);
  const ext = engine.extension;

  const change = async (patch: Partial<StartupView>) => {
    if (!st) return;
    const next = { ...st, ...patch, pages: { ...st.pages, ...(patch.pages ?? {}) } };
    setSt(next);
    try {
      await api.setStartup({ openChrome: next.openChrome, pages: next.pages, closeTabOnClose: next.closeTabOnClose, keepAwake: next.keepAwake });
      setMsg(null);
    } catch (e) {
      setSt(st);
      setMsg({ ok: false, text: err(e) });
    }
  };
  const autostart = async (on: boolean) => {
    if (!st) return;
    try {
      const v = await api.setAutostart(on);
      setSt({ ...st, autostart: v });
      setMsg(null);
    } catch (e) {
      setMsg({ ok: false, text: err(e) });
    }
  };
  const pair = async () => {
    try {
      setCode(await api.extPairStart());
      setMsg(null);
    } catch (e) {
      setMsg({ ok: false, text: err(e) });
    }
  };
  const unpair = async () => {
    try {
      await api.extUnpair();
      setCode(null);
      setMsg({ ok: true, text: "Unpaired. The extension can no longer connect until you pair it again." });
    } catch (e) {
      setMsg({ ok: false, text: err(e) });
    }
  };
  const openNow = async () => {
    try {
      setMsg({ ok: true, text: await api.openChromeNow() });
    } catch (e) {
      setMsg({ ok: false, text: err(e) });
    }
  };
  const folder = async () => {
    try {
      await api.openExtensionFolder();
    } catch (e) {
      setMsg({ ok: false, text: err(e) });
    }
  };
  // the code disappears once the extension has used it
  useEffect(() => {
    if (ext?.paired) setCode(null);
  }, [ext?.paired]);

  const link = !ext ? "Checking" : ext.connected ? "Connected" : ext.paired ? "Paired, Chrome is not connected right now" : "Not paired yet";
  const pagesOff = !st?.openChrome;
  return (
    <div className="acc-pad" data-testid="startup-browser">
      <div className="kvrows" data-testid="ext-status">
        <div><span className="k">Chrome extension</span><b className={ext?.connected ? "pos" : ""}>{link}</b></div>
        {st && !st.chromeFound && <div className="note">Chrome was not found on this computer. The app will open your default browser instead, but the extension needs Chrome.</div>}
      </div>
      <div className="btns">
        {!ext?.paired && <button className="btn" onClick={pair}>Get pairing code</button>}
        {ext?.paired && <button className="btn ghost" onClick={pair}>Pair again</button>}
        {ext?.paired && <button className="btn danger" onClick={unpair}>Unpair</button>}
        <button className="btn ghost" onClick={openNow}>Open Chrome now</button>
      </div>
      {code && (
        <div className="paircode" data-testid="pair-code">
          <div className="note">Paste this code into the Claude Trader side panel in Chrome. It works once and ends in 10 minutes.</div>
          <div className="code">{code}</div>
          {api.kind === "demo" && <div className="note">DEMO: this code is not real.</div>}
        </div>
      )}
      {st && (
        <>
          <Toggle id="su-chrome" label="Open Chrome when Claude Trader starts" checked={st.openChrome} onChange={(v) => change({ openChrome: v })}>
            <p><b>What it does.</b> Starts Chrome for you so the extension can connect to the app. If Chrome is already open, nothing happens.</p>
            <p><b>A good value.</b> On.</p>
            <p><b>How to set it.</b> Tick or untick the box. It takes effect the next time the app starts.</p>
          </Toggle>
          <div className="field">
            <div className="lblrow"><label>Pages to open at launch</label></div>
            <div className="subtoggles">
              <Toggle id="su-pump" label="pump.fun home page" checked={st.pages.pumpFunHome} disabled={pagesOff} onChange={(v) => change({ pages: { ...st.pages, pumpFunHome: v } })}>
                <p><b>What it does.</b> Opens pump.fun, where new coins appear. If you already have a pump.fun tab, the extension reuses it.</p>
                <p><b>A good value.</b> On.</p>
              </Toggle>
              <Toggle id="su-dex" label="Dexscreener chart of each coin the bot buys" checked={st.pages.dexscreenerHeld} disabled={pagesOff} onChange={(v) => change({ pages: { ...st.pages, dexscreenerHeld: v } })}>
                <p><b>What it does.</b> When the bot buys a coin, the extension opens that coin's Dexscreener chart in a background tab so you can watch it. One tab per coin, never duplicated.</p>
                <p><b>A good value.</b> On.</p>
              </Toggle>
            </div>
          </div>
          <Toggle id="su-close" label="Close a coin's tab when its trade closes" checked={st.closeTabOnClose} onChange={(v) => change({ closeTabOnClose: v })}>
            <p><b>What it does.</b> When the bot sells a coin for good, the extension closes the chart tab it opened for that coin. It never closes tabs you opened yourself.</p>
            <p><b>A good value.</b> Off if you like to look at the chart after a trade, on to keep Chrome tidy.</p>
          </Toggle>
          <Toggle id="su-auto" label="Start Claude Trader when Windows starts" checked={st.autostart} onChange={autostart}>
            <p><b>What it does.</b> Opens the app by itself when you sign in to Windows, so the bot is running without you remembering.</p>
            <p><b>A good value.</b> Your choice. Leave it off until you trust the bot.</p>
          </Toggle>
          <Toggle id="su-awake" label="Keep this PC awake while the bot runs" checked={st.keepAwake} onChange={(v) => change({ keepAwake: v })}>
            <p><b>What it does.</b> Stops Windows from going to sleep while Claude Trader is open. A sleeping PC cannot watch coins or sell. The screen can still turn off. It is released when you close the app.</p>
            <p><b>A good value.</b> On.</p>
          </Toggle>
        </>
      )}
      {msg && <div className={`result ${msg.ok ? "ok" : "bad"}`} role="status">{msg.text}</div>}

      <div className="field">
        <button className="btn ghost" style={{ width: "fit-content" }} aria-expanded={showGuide} onClick={() => setShowGuide(!showGuide)}>
          {showGuide ? "Hide" : "Show"}: Install the Chrome extension
        </button>
        {showGuide && (
          <div className="guide" data-testid="ext-guide">
            <p className="note">Do this once. Chrome does not let apps install extensions for you, so these steps are by hand.</p>
            <ol>
              <li>Press <b>Show the extension folder</b> below. A window opens with the extension files.</li>
              <li>In Chrome, type <b>chrome://extensions</b> in the address bar and press Enter.</li>
              <li>Turn on <b>Developer mode</b> (switch at the top right).</li>
              <li>Press <b>Load unpacked</b> and pick the folder from step 1 (the one that holds manifest.json).</li>
              <li>Click the puzzle icon in Chrome and pin <b>Claude Trader</b>. Click its icon to open the side panel.</li>
              <li>Here in the app, press <b>Get pairing code</b>. Paste the code into the side panel and press Connect.</li>
            </ol>
            <p className="note">The status above turns to Connected. After that it connects by itself every time. Chrome may show a "Disable developer mode extensions" notice at start. Choose Keep. The extension never gets your Helius key or any secret. It cannot buy or sell, only look, run a scam check, and press the kill switch.</p>
            <div className="btns">
              <button className="btn" onClick={folder}>Show the extension folder</button>
            </div>
            {st && <div className="note" data-testid="ext-folder">Folder: {st.extensionFolder}</div>}
          </div>
        )}
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ About and updates

function About() {
  const [info, setInfo] = useState<BuildInfo | null>(null);
  const [up, setUp] = useState<UpdateCheck | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  useEffect(() => {
    api.buildInfo().then(setInfo).catch(() => {});
  }, []);
  const check = async () => {
    setBusy(true);
    setErr("");
    try {
      setUp(await api.checkUpdates());
    } catch (e) {
      setErr(String(e instanceof Error ? e.message : e));
    }
    setBusy(false);
  };
  return (
    <div className="acc-pad">
      <div className="kvrows" data-testid="about">
        <div><span className="k">App version</span><b>{info?.version ?? "..."}</b></div>
        <div><span className="k">Build</span>{info ? `${info.shortSha}${info.branch ? ` on ${info.branch}` : ""}` : "..."}</div>
        <div><span className="k">Built on</span>{info?.builtAtText ?? "..."}</div>
      </div>
      <div className="btns">
        <button className="btn" onClick={check} disabled={busy}>{busy ? "Checking" : "Check for updates"}</button>
      </div>
      {err && <div className="result bad" role="status">{err}</div>}
      {up && (
        <div className={`result ${up.verdict === "update-available" ? "" : up.verdict === "up-to-date" ? "ok" : ""}`} role="status">
          {up.verdict === "update-available" && <b>Update available. </b>}
          {up.message.replace(/^Update available\.\s*/, "")} {up.demo && <span className="demotag">DEMO</span>}
          {up.verdict === "update-available" && up.latest && (
            <>
              <div className="btns" style={{ marginTop: 8 }}>
                <button className="btn" onClick={() => api.openLink(up.latest!.url)}>Open download page</button>
              </div>
              <ol className="steps">
                {up.steps.map((s) => <li key={s}>{s}</li>)}
              </ol>
            </>
          )}
          {up.verdict !== "update-available" && up.latest && <div className="note">Newest build: {up.latest.shortSha || "unknown"}, <SourceLink url={up.latest.url} prefix="see" /></div>}
        </div>
      )}
      <div className="note">The check only reads public information from GitHub. It sends nothing about you and never installs anything by itself.</div>
    </div>
  );
}

function Appearance() {
  const [pref, setPref] = useState<ThemePref>(readThemePref);
  const opts: [ThemePref, string][] = [["light", "Light"], ["dark", "Dark"], ["system", "System"]];
  return (
    <div className="acc-pad">
      <p className="note">Choose how the app looks. This only changes colors on this device and does not affect trading.</p>
      <div className="seg" role="radiogroup" aria-label="Appearance" style={{ width: "fit-content" }}>
        {opts.map(([v, label]) => (
          <button key={v} role="radio" aria-checked={pref === v} onClick={() => { setPref(v); saveThemePref(v); }}>{label}</button>
        ))}
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ page

export default function SettingsPage({ config, engine, onConfig }: { config: Config; engine: EngineState; onConfig: (c: Config) => void }) {
  const rows = flatten(config as unknown as Record<string, unknown>);
  const [open, setOpen] = useState<string[]>(readOpen);
  const toggle = (id: string) => {
    const next = open.includes(id) ? open.filter((x) => x !== id) : [...open, id];
    setOpen(next);
    try {
      localStorage.setItem(OPEN_KEY, JSON.stringify(next));
    } catch {
      /* storage can be blocked: the sections still work, they just do not remember */
    }
  };
  const isOpen = (id: string) => open.includes(id);
  return (
    <div>
      <h1 className="set-title">Settings</h1>
      <p className="set-sub">
        Every setting the app uses, in plain words. Open a section, then press the i next to any line to see what it does, a good value, and how to set it. Most values are read only and come from config.json.
      </p>
      {GROUPS.map((g) => {
        const secs = SECTIONS.filter((s) => g.keys.includes(s.key));
        const count = rows.filter((r) => g.keys.includes(r.section)).length;
        return (
          <Section key={g.id} id={g.id} title={g.title} summary={`${count} settings`} open={isOpen(g.id)} toggle={() => toggle(g.id)}>
            <p className="acc-blurb">{g.blurb}</p>
            {g.id === "fill" && <FillPreset config={config} onApplied={onConfig} engine={engine} />}
            {secs.map((s) => {
              const sr = rows.filter((r) => r.section === s.key);
              if (!sr.length) return null;
              return (
                <div key={s.key}>
                  {secs.length > 1 && <p className="acc-blurb"><b>{s.title}.</b> {s.blurb}</p>}
                  <div className="set-list">
                    {sr.map((r) => <SettingRow key={r.display} row={r} />)}
                  </div>
                </div>
              );
            })}
          </Section>
        );
      })}
      <Section id="connections" title="Connections" summary="Helius key and data health" open={isOpen("connections")} toggle={() => toggle("connections")}>
        <p className="acc-blurb">Outside services the app talks to. The key is kept in your computer's secure storage, never in config.json, logs or git.</p>
        <Connections engine={engine} />
      </Section>
      <Section id="startup" title="Startup and browser" summary="Chrome, extension, Windows start" open={isOpen("startup")} toggle={() => toggle("startup")}>
        <p className="acc-blurb">How the app starts and how it works with Chrome. The Chrome extension adds a side panel and a coin check to the pages you browse. It is optional.</p>
        <StartupBrowser engine={engine} />
      </Section>
      <Section id="appearance" title="Appearance" summary="Light, dark or system" open={isOpen("appearance")} toggle={() => toggle("appearance")}>
        <Appearance />
      </Section>
      <Section id="about" title="About and updates" summary="Version and update check" open={isOpen("about")} toggle={() => toggle("about")}>
        <About />
      </Section>
    </div>
  );
}
