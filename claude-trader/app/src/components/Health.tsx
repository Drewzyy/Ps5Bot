import type { Funnel, FeedHealth } from "../types";
import type { EngineState } from "../useEngine";
import { reasonLabel } from "../reasons";

function agoText(now: number, ms: number): string {
  if (!ms) return "none yet";
  const s = Math.max(0, Math.round((now - ms) / 1000));
  return s < 90 ? `${s} s ago` : `${Math.round(s / 60)} min ago`;
}

/** The buy/sell data health in plain words. Shown in the Memecoin Monitor panel and in Settings > Connections. */
export function FeedHealthLine({ feed }: { feed: FeedHealth | null }) {
  if (!feed) return <div className="empty" data-testid="feed-health">Waiting for the first health report from the engine.</div>;
  const now = feed.ts;
  return (
    <div className="fhealth" data-testid="feed-health">
      {feed.warn && (
        <div className="fwarn" role="alert">
          <b>{feed.warn}</b>
          {feed.lastError && <span> The data connection says: {feed.lastError}</span>}
        </div>
      )}
      <div className="kvrows">
        <div><span className="k">New-coin feed</span><b className={feed.coinsConnected ? "okc" : "badc"}>{feed.coinsConnected ? "connected" : "not connected"}</b></div>
        <div><span className="k">Buy/sell data</span><b className={feed.tradesConnected ? "okc" : "badc"}>{feed.tradesConnected ? "connected" : "not connected"}</b></div>
        <div><span className="k">Trades read, last minute</span><b>{feed.tradesLastMin}</b></div>
        <div><span className="k">Coins being watched</span><b>{feed.watched} ({feed.subsActive} confirmed by the node)</b></div>
        <div><span className="k">Last trade seen</span><b>{agoText(now, feed.lastTradeMs)}</b></div>
        <div><span className="k">RugCheck</span><b>{feed.rugRequests === 0 ? "not asked yet" : `${feed.rugRequests - feed.rugFailures} of ${feed.rugRequests} worked`}</b></div>
        {!feed.warn && feed.lastError && <div className="note">Last message from the data connection: {feed.lastError}</div>}
      </div>
    </div>
  );
}

/** A red bar at the top of the dashboard when no buy/sell data is arriving. Renders nothing otherwise. */
export function FeedWarningBar({ feed }: { feed: FeedHealth | null }) {
  if (!feed?.warn) return null;
  return (
    <div className="fwarn bar" role="alert" data-testid="feed-warning">
      <b>{feed.warn}</b> Open Settings, Connections to see why. A free Helius key usually fixes it.
    </div>
  );
}

function headline(f: Funnel): string {
  if (f.seen === 0) return "No coins seen yet today. The app is waiting for the first new coin.";
  if (f.bought > 0) return `${f.bought} trade${f.bought === 1 ? "" : "s"} made today.`;
  if (f.scored === 0) {
    return f.blocked >= f.seen - f.inProgress && f.blocked > 0
      ? `Every coin so far was stopped by Scam Shield (${f.blocked} of ${f.seen}). None reached scoring.`
      : "Coins are still being checked. None has been scored yet.";
  }
  if (f.scoredNoTrades > 0 && f.scoredNoTrades >= f.scored) return "Coins were watched, but not one buy or sell was seen. Check the buy/sell data in Settings, Connections.";
  if (f.picked === 0) return `${f.scored} coins were scored but none cleared every entry rule${f.bestBlockedBy[0] ? `. The rule that stopped the best ones most: ${f.bestBlockedBy[0].reason}` : ""}.`;
  return `${f.picked} coin${f.picked === 1 ? "" : "s"} picked, none bought yet. The risk limits or a failed paper buy stopped it.`;
}

function Step({ n, title, value, children, tone }: { n: number; title: string; value: string | number; children?: React.ReactNode; tone?: "red" | "green" }) {
  return (
    <div className="fstep">
      <span className="fn">{n}</span>
      <div className="fbody">
        <div className="fhead"><span>{title}</span><b className={tone ?? ""}>{value}</b></div>
        {children}
      </div>
    </div>
  );
}

function Bars({ rows, label }: { rows: { reason: string; count: number }[]; label: (r: string) => string }) {
  const max = Math.max(1, ...rows.map((r) => r.count));
  return (
    <div className="reasons">
      {rows.map((r) => (
        <div className="rbar" key={r.reason}>
          <span className="rl" title={label(r.reason)}>{label(r.reason)}</span>
          <span className="rt"><i style={{ width: `${(r.count / max) * 100}%` }} /></span>
          <span className="rc">{r.count}</span>
        </div>
      ))}
    </div>
  );
}

/** "Why no trades?" Built from real counts only (decisions and scores stored by the engine). */
export function FunnelCard({ engine }: { engine: EngineState }) {
  const f = engine.funnel;
  return (
    <section className="card span2" data-testid="funnel">
      <h3>Why no trades? <span className="hint">today{f?.demo ? " (DEMO data)" : ""}</span></h3>
      {!f ? (
        <div className="empty">Counts appear here a few seconds after the engine starts. They come from real coins, never made up.</div>
      ) : (
        <>
          <p className="fhl">{headline(f)}</p>
          <div className="fsteps">
            <Step n={1} title="New coins seen" value={f.seen}>
              <p className="fx">Every coin pump.fun launched while the app was running.</p>
            </Step>
            <Step n={2} title="Blocked by Scam Shield" value={f.blocked} tone={f.blocked ? "red" : undefined}>
              <p className="fx">These looked unsafe or could not be checked, so the app never buys them. Top reasons:</p>
              {f.blockedReasons.length ? <Bars rows={f.blockedReasons.slice(0, 6)} label={reasonLabel} /> : <p className="fx">None yet.</p>}
              {(f.rugFallbackPassed > 0 || f.rugFallbackBlocked > 0) && (
                <p className="fx">RugCheck (a free safety website) had no answer for {f.rugFallbackPassed + f.rugFallbackBlocked} coins. The app checked those on the blockchain itself: {f.rugFallbackPassed} passed (bought at half size if picked), {f.rugFallbackBlocked} blocked.</p>
              )}
            </Step>
            <Step n={3} title="Watched for 60 seconds and scored" value={f.scored}>
              <p className="fx">
                The coins that passed Scam Shield. {f.inProgress > 0 ? `${f.inProgress} more are being checked or watched right now. ` : ""}
                {f.scoredNoTrades > 0 ? `${f.scoredNoTrades} of the ${f.scored} had no buy or sell at all in that minute (most new coins are dead on arrival).` : ""}
              </p>
            </Step>
            <Step n={4} title="Best score today" value={f.scored ? `${f.bestScore.toFixed(0)}${f.bestSymbol ? ` (${f.bestSymbol})` : ""}` : "none"}>
              <p className="fx">A coin needs {f.minScore} to be considered. {f.scored && f.bestScore < f.minScore ? "Nothing reached it, so every coin simply looked too quiet." : ""}</p>
              {f.bestBlockedBy.length > 0 && (
                <>
                  <p className="fx">What stopped the 10 best-scoring coins that were not picked:</p>
                  <Bars rows={f.bestBlockedBy.slice(0, 6)} label={(r) => r} />
                </>
              )}
            </Step>
            <Step n={5} title="Picked, then bought" value={`${f.picked}, ${f.bought}`} tone={f.bought ? "green" : undefined}>
              <p className="fx">Picked means every rule passed. Bought means the paper buy also cleared the risk limits.</p>
            </Step>
          </div>
        </>
      )}
    </section>
  );
}
