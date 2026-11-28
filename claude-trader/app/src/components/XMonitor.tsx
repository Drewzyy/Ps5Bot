import { useEffect, useState } from "react";
import type { MouseEvent, ReactNode } from "react";
import { api } from "../source";
import type { EngineState } from "../useEngine";
import type { XCheckRow, XMonitorView, XSettings } from "../types";
import { linkLabel } from "./links";

const ago = (ts: number) => {
  const s = Math.max(0, Math.round((Date.now() - ts) / 1000));
  return s < 90 ? `${s} s ago` : `${Math.round(s / 60)} min ago`;
};
const num = (n: number) => n.toLocaleString("en-US");

/** A link to the exact X post, profile or community. Opens in the browser. */
export function XLink({ url, label, children }: { url: string; label?: string; children?: ReactNode }) {
  const go = (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    api.openLink(url);
  };
  return (
    <a className="srclink" href={url} onClick={go} rel="noreferrer" title={url}>
      {children ?? label ?? linkLabel(url)}
    </a>
  );
}

const STATUS_WORDS: Record<string, string> = { ready: "Scored", blocked: "Blocked", unknown: "Unknown", nolink: "No X link", off: "Off" };

function CheckRow({ r }: { r: XCheckRow }) {
  const note = r.status === "blocked" ? r.blocks[0] ?? r.reason : r.status === "ready" ? r.flags[0] ?? "" : r.reason;
  return (
    <div className="xrow" data-testid="x-check">
      <span className="xt">{ago(r.ts)}</span>
      <b className="xs">{r.symbol || "?"}</b>
      <span className={`chip xchip ${r.status}`}>{STATUS_WORDS[r.status] ?? r.status}</span>
      <span className="xv">{r.status === "ready" ? `${r.score.toFixed(0)} of 100, +${r.points} pts` : ""}</span>
      <span className="xn" title={note}>{note}</span>
      <XLink url={r.url} label={r.linkLabel || undefined} />
    </div>
  );
}

/** Dashboard card: rising accounts and the latest checks, each with a link to the exact post or profile. */
export function XCommunityCard({ engine }: { engine: EngineState }) {
  const x = engine.xmon;
  return (
    <section className="card span2" data-testid="x-community">
      <h3>X community <span className="hint">{x?.demo ? "DEMO data" : x ? x.statusLine : ""}</span></h3>
      {!x ? (
        <div className="empty">X Monitor starts after the engine does. It needs the Chrome extension and a spare X account (see Settings, X Monitor).</div>
      ) : (
        <>
          <p className="fhl">{x.statusLine}{x.state === "working" ? ` Pages opened this hour: ${x.loadsLastHour} of ${x.maxPerHour}.` : ""}</p>
          <h4 className="xh">Rising accounts</h4>
          {x.rising.length === 0 ? (
            <div className="empty">None yet. An account counts as rising when its followers grow 20% or more between two looks, or it is under 2 weeks old with about 1,000 followers a day.</div>
          ) : (
            <div className="xlist">
              {x.rising.map((r) => (
                <div className="xrow" key={r.handle} data-testid="x-rising">
                  <b className="xs">@{r.handle}</b>
                  <span className="xv">{num(r.followers)} followers</span>
                  <span className="xn">{r.why}{r.symbol ? ` (coin ${r.symbol})` : ""}</span>
                  <XLink url={r.url} label="X profile" />
                </div>
              ))}
            </div>
          )}
          <h4 className="xh">Latest checks</h4>
          {x.latest.length === 0 ? <div className="empty">No coin with an X link has been seen yet.</div> : <div className="xlist">{x.latest.map((r) => <CheckRow key={r.mint} r={r} />)}</div>}
          <p className="fx">Only a hard block stops a buy. A good X score adds at most 15 points and can never buy a coin by itself. Unknown means the page was not read in time, and it counts as nothing.</p>
        </>
      )}
    </section>
  );
}

/** Dashboard card: the story of each coin and the narratives that are trending (Thesis Finder). */
export function ThesisCard({ engine }: { engine: EngineState }) {
  const x = engine.xmon;
  return (
    <section className="card span2" data-testid="thesis">
      <h3>Thesis Finder <span className="hint">{x?.demo ? "DEMO data" : "stories from the last 3 hours"}</span></h3>
      {!x ? (
        <div className="empty">Stories appear when coins with an X link show up.</div>
      ) : (
        <>
          <p className="fhl">{x.thesisLine}</p>
          {x.narratives.map((n) => (
            <div className="xnarr" key={n.key} data-testid="x-narrative">
              <b>{n.label}: {n.count} launches</b>
              <span> {n.symbols.slice(0, 6).join(", ")}. {n.text}</span> <XLink url={n.url} />
            </div>
          ))}
          <h4 className="xh">Story of each coin</h4>
          {x.stories.length === 0 ? <div className="empty">No stories yet.</div> : (
            <div className="xlist">
              {x.stories.map((s) => (
                <div className="xstory" key={s.mint} data-testid="x-story">
                  <div className="xsh"><b>{s.symbol}</b>{s.rank > 0 && <span className="chip">copy {s.rank} of {s.clones}</span>}<XLink url={s.url} /></div>
                  <div className="xtxt">{s.text || "No text read yet."}</div>
                  <div className="xmeta">
                    {s.author && <span>by @{s.author}</span>}
                    {s.reach > 0 && <span>reach {num(s.reach)}{s.kind === "community" ? " members" : ""}</span>}
                    {s.views > 0 && <span>{num(s.views)} views</span>}
                    {s.tweetAgeMin !== null && <span>post was {s.tweetAgeMin} min older than the coin</span>}
                  </div>
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </section>
  );
}

/** The node panel for X Monitor and Thesis Finder. */
export function XPanel({ x, which }: { x: XMonitorView | null; which: "x-monitor" | "thesis-finder" }) {
  if (!x) return <div className="empty">No data yet.</div>;
  if (which === "thesis-finder") {
    return (
      <>
        <h4>Right now</h4>
        <div className="task">{x.thesisLine}</div>
        {x.narratives.slice(0, 3).map((n) => <div className="kv" key={n.key}><span>{n.label}</span><span>{n.count} coins <XLink url={n.url} /></span></div>)}
      </>
    );
  }
  const j = x.jobs;
  return (
    <>
      <h4>Pages</h4>
      <div className="kv"><span>Waiting / running</span><span>{j.queued} / {j.running}</span></div>
      <div className="kv"><span>Done / failed / stale</span><span>{j.done} / {j.failed} / {j.stale}</span></div>
      <div className="kv"><span>Opened this hour</span><span>{x.loadsLastHour} of {x.maxPerHour}</span></div>
      <div className="kv"><span>x.com login</span><span>{x.loggedIn === null ? "not known yet" : x.loggedIn ? "logged in" : "not logged in"}</span></div>
    </>
  );
}

// ------------------------------------------------------------------ Settings section

function Num({ label, value, min, max, onChange, hint }: { label: string; value: number; min: number; max: number; onChange: (n: number) => void; hint: string }) {
  return (
    <label className="xnum">
      <span>{label}</span>
      <input type="number" min={min} max={max} value={value} onChange={(e) => onChange(Number(e.target.value))} aria-label={label} />
      <small>{hint}</small>
    </label>
  );
}

/** Settings > X Monitor: directions, the on/off switch, the pause button and the pacing. */
export function XMonitorSettings({ engine }: { engine: EngineState }) {
  const [s, setS] = useState<XSettings | null>(null);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const err = (e: unknown) => String(e instanceof Error ? e.message : e);
  useEffect(() => {
    api.getXmonSettings().then(setS).catch((e) => setMsg({ ok: false, text: err(e) }));
  }, []);
  const save = async (next: XSettings) => {
    const prev = s;
    setS(next);
    try {
      await api.setXmonSettings(next);
      setMsg({ ok: true, text: "Saved." });
    } catch (e) {
      setS(prev);
      setMsg({ ok: false, text: err(e) });
    }
  };
  const x = engine.xmon;
  const ext = engine.extension;
  return (
    <div className="acc-pad" data-testid="x-monitor-settings">
      <div className="kvrows" data-testid="x-status">
        <div><span className="k">Status</span><b className={x?.state === "working" ? "pos" : x?.state === "not-logged-in" || (x?.state === "paused" && x.mode === "account-limited") ? "neg" : ""}>{x?.statusLine ?? "Waiting for the engine"}{x?.demo ? " (DEMO)" : ""}</b></div>
        <div><span className="k">Chrome extension</span><b className={ext?.connected ? "pos" : ""}>{ext?.connected ? "Connected" : ext?.paired ? "Paired, Chrome is not connected right now" : "Not paired yet"}</b></div>
        <div><span className="k">Pages this hour</span><b>{x ? `${x.loadsLastHour} of ${x.maxPerHour}` : "0"}</b></div>
      </div>

      <div className="xdir">
        <h4>Set it up (once)</h4>
        <ol>
          <li>Make a spare X account. <b>Do not use your main account.</b> X bans automated reading, and this account could get locked.</li>
          <li>Log into x.com in the Chrome profile the bot uses. The extension reads pages in one background tab.</li>
          <li>Optional, but it makes the score much better: with that account, follow a few hundred crypto KOLs (well-known crypto accounts). The score counts how many of them follow a coin's account.</li>
          <li>Keep the Chrome extension paired (Settings, Startup and browser).</li>
        </ol>
        <p className="note">The extension only reads. It never posts, likes, follows or clicks. It opens at most one page every 8 to 15 seconds and never more than 120 pages an hour.</p>
      </div>

      {s && (
        <>
          <div className="btns">
            <button className={`btn ${s.paused ? "" : "ghost"}`} onClick={() => save({ ...s, paused: !s.paused })} data-testid="x-pause">{s.paused ? "Resume X Monitor" : "Pause X Monitor"}</button>
          </div>
          <div className="field">
            <div className="lblrow">
              <label className="chk" htmlFor="x-on">
                <input id="x-on" type="checkbox" checked={s.enabled} onChange={(e) => save({ ...s, enabled: e.target.checked })} />
                <span>Use X Monitor</span>
              </label>
              <XInfo label="Use X Monitor">
                <p><b>What it does.</b> Reads the X post, profile or community that a coin links to and adds up to 15 points to the coin's score. It can also block a coin (reused account, copy of a post, bot replies).</p>
                <p><b>A good value.</b> On. It starts working once the extension is paired and x.com is logged in. Until then every coin is scored without X.</p>
                <p><b>How to set it.</b> Untick to switch it off. X alone can never trigger a buy.</p>
              </XInfo>
            </div>
          </div>
          <div className="xgrid">
            <Num label="Shortest wait between pages (seconds)" value={s.minGapSecs} min={5} max={120} onChange={(n) => setS({ ...s, minGapSecs: n })} hint="8. Never below 5." />
            <Num label="Longest wait between pages (seconds)" value={s.maxGapSecs} min={5} max={300} onChange={(n) => setS({ ...s, maxGapSecs: n })} hint="15. The wait is random between the two." />
            <Num label="Most pages per hour" value={s.maxLoadsPerHour} min={1} max={200} onChange={(n) => setS({ ...s, maxLoadsPerHour: n })} hint="120. Never above 200." />
            <Num label="Keep a page's data for (minutes)" value={s.cacheMinutes} min={1} max={240} onChange={(n) => setS({ ...s, cacheMinutes: n })} hint="30. The same account or post is not read again before this." />
          </div>
          <div className="btns">
            <button className="btn ghost" onClick={() => save(s)}>Save pacing</button>
            <XInfo label="Pacing">
              <p><b>What it does.</b> Slower reading looks more like a person and is less likely to lock the spare account. If X says the account is limited, the extension stops for 15 minutes, then 1 hour, then the rest of the day, and the app shows "X paused: account limited".</p>
              <p><b>A good value.</b> The defaults. Go slower if the account ever gets locked.</p>
              <p><b>How to set it.</b> Type the numbers and press Save pacing. Values outside the safe range are refused.</p>
            </XInfo>
          </div>
        </>
      )}
      {msg && <div className={`note ${msg.ok ? "" : "neg"}`} role="status">{msg.text}</div>}
    </div>
  );
}

function XInfo({ label, children }: { label: string; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button className="info inline-i" aria-expanded={open} aria-label={`Explain ${label}`} onClick={() => setOpen(!open)}>i</button>
      {open && <div className="explain" style={{ borderRadius: 10, border: "1px solid var(--line)", marginTop: 6 }}>{children}</div>}
    </>
  );
}
