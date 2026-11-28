import type { Config, RiskVerdict, WatchRow } from "../types";
import type { EngineState } from "../useEngine";
import { FeesCard, OpenPositions, PerformanceCard, PositionChart } from "./Trading";
import { LearningCard } from "./Learning";
import { SourceLink } from "./links";
import { reasonLabel } from "../reasons";
import { FunnelCard } from "./Health";
import { XCommunityCard, ThesisCard } from "./XMonitor";

const solscan = (mint: string) => `https://solscan.io/token/${mint}`;
const shortMint = (m: string) => `${m.slice(0, 4)}..${m.slice(-4)}`;

function Empty({ children }: { children: string }) {
  return <div className="empty">{children}</div>;
}

function Rows({ rows }: { rows: [string, string][] }) {
  return (
    <div className="rows">
      {rows.map(([l, r]) => (
        <div className="row" key={l}>
          <span className="l">{l}</span>
          <span className="r">{r}</span>
        </div>
      ))}
    </div>
  );
}

function VerdictBadge({ v }: { v: RiskVerdict | null }) {
  if (!v) return null;
  return <span className={`vbadge ${v}`}>{v.replace("_", " ")}</span>;
}

const STATUS_TEXT: Record<WatchRow["status"], string> = { checking: "Checking", blocked: "Blocked", observing: "Watching", scored: "Scored", picked: "Picked" };

function Watchlist({ rows, links }: { rows: WatchRow[]; links: EngineState["links"] }) {
  return (
    <section className="card span2">
      <h3>Watchlist <span className="hint">{rows.length} coins, newest first</span></h3>
      {rows.length === 0 ? (
        <Empty>No coins yet. Every coin the app looks at appears here, including the ones it skips and the reason why.</Empty>
      ) : (
        <div className="watch">
          {rows.slice(0, 60).map((r) => (
            <div className="wrow" key={r.mint}>
              <span className="wts">{new Date(r.ts).toLocaleTimeString([], { hour12: false })}</span>
              <a className="wcoin" href={solscan(r.mint)} target="_blank" rel="noreferrer" title={r.mint}>
                <b>{r.symbol || "?"}</b> <span>{shortMint(r.mint)}</span>
              </a>
              <span className={`chip ${r.status}`}>{STATUS_TEXT[r.status]}</span>
              <span className="wreason" title={`${r.status === "blocked" ? reasonLabel(r.code) + ": " : ""}${r.reason}`}>
                {r.reason}
              </span>
              <span className="wsrc">
                {(() => {
                  const l = links[r.mint];
                  return l ? (
                    <>
                      {l.website && <SourceLink url={l.website} prefix="" />}
                      {l.twitter && <SourceLink url={l.twitter} prefix="" />}
                      {l.telegram && <SourceLink url={l.telegram} prefix="" />}
                    </>
                  ) : null;
                })()}
              </span>
              <span className="wscore">{r.score !== null ? r.score.toFixed(0) : ""}</span>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

function ShieldCard({ engine }: { engine: EngineState }) {
  const s = engine.shield;
  const latest = engine.verdicts[0];
  const max = Math.max(1, ...(s?.byReason.map((r) => r.count) ?? [1]));
  return (
    <section className="card">
      <h3>Scam Shield <span className="hint">today</span></h3>
      <div className="row"><span className="l">Coins checked</span><span className="r">{s ? s.checked : 0}</span></div>
      <div className="row"><span className="l">Coins blocked</span><span className="r">{s ? s.blocked : 0}</span></div>
      <div className="row"><span className="l">Passed to Token Picker</span><span className="r">{s ? s.passed : 0}</span></div>
      <div className="row">
        <span className="l">Latest verdict</span>
        {latest ? <VerdictBadge v={latest.verdict} /> : <span className="verdict">No coins checked yet</span>}
      </div>
      {!s || s.byReason.length === 0 ? (
        <Empty>Reasons for blocks will be listed here, such as live mint authority or bundled wallets.</Empty>
      ) : (
        <div className="reasons">
          {s.byReason.slice(0, 8).map((r) => (
            <div className="rbar" key={r.reason}>
              <span className="rl">{reasonLabel(r.reason)}</span>
              <span className="rt"><i style={{ width: `${(r.count / max) * 100}%` }} /></span>
              <span className="rc">{r.count}</span>
            </div>
          ))}
        </div>
      )}
      {engine.skipped && engine.skipped.checks.length > 0 && (
        <div className="skipped">
          <h4>Skipped in {engine.skipped.executor} mode</h4>
          <p>These checks cannot run here. They are not hidden: live mode will do them.</p>
          <ul>
            {engine.skipped.checks.map((c) => (
              <li key={c.name} title={c.why}><b>{c.name}.</b> {c.why}</li>
            ))}
          </ul>
        </div>
      )}
      {engine.verdicts.length > 0 && (
        <div className="evid">
          <h4>Latest checks and evidence</h4>
          {engine.verdicts.slice(0, 5).map((v, i) => (
            <div className="erow" key={i}>
              <VerdictBadge v={v.verdict} />
              <span className="et">{v.symbol} {v.outcome === "block" ? `blocked: ${reasonLabel(v.reason)}` : `passed ${v.stage}`}</span>
              <span className="el">
                {v.evidence.slice(0, 2).map((e) => (
                  <a key={e.url} href={e.url} target="_blank" rel="noreferrer">{e.label}</a>
                ))}
              </span>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

export default function Widgets({ config, openSettings, engine }: { config: Config; openSettings: () => void; engine: EngineState }) {
  return (
    <div className="grid">
      <FunnelCard engine={engine} />

      <XCommunityCard engine={engine} />

      <ThesisCard engine={engine} />

      <Watchlist rows={engine.watchlist} links={engine.links} />

      <OpenPositions engine={engine} config={config} />

      <ShieldCard engine={engine} />

      <PositionChart engine={engine} />

      <LearningCard engine={engine} />

      <FeesCard lane={engine.stats?.lanes[0]} />

      <PerformanceCard engine={engine} />

      <section className="card">
        <h3>Settings <span className="hint">key values</span></h3>
        <Rows
          rows={[
            ["Mode", config.mode],
            ["Bankroll", `${config.paper.bankrollSol} SOL`],
            ["Trade size", `${config.risk.minTradeSol} to ${config.risk.maxTradeSol} SOL`],
            ["Hard stop", `${config.exits.hardStopPct}%`],
            ["Max hold", `${config.exits.maxHoldSeconds / 60} minutes`],
            ["Daily loss limit", `${config.risk.dailyLossLimitPct}%`],
          ]}
        />
        <div style={{ marginTop: 12 }}>
          <button className="btn" onClick={openSettings}>Open all settings</button>
        </div>
      </section>
    </div>
  );
}
