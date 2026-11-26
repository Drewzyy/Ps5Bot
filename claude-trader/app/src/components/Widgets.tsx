import type { Config, RiskVerdict, WatchRow } from "../types";
import type { EngineState } from "../useEngine";

export const REASON_LABEL: Record<string, string> = {
  MINT_AUTHORITY: "Mint authority still set",
  FREEZE_AUTHORITY: "Freeze authority still set",
  TRANSFER_HOOK: "Token-2022 transfer hook",
  PERMANENT_DELEGATE: "Permanent delegate",
  TRANSFER_FEE: "Transfer fee on the token",
  CLOSE_AUTHORITY: "Mint close authority",
  CREATOR_HOLDING: "Creator holds too much",
  SINGLE_HOLDER: "One wallet holds too much",
  TOP_HOLDERS: "Top wallets hold too much",
  INSIDERS: "Insider wallets hold too much",
  BUNDLE_BURST: "Bundled launch (many buys in one block)",
  BUNDLE_PAYERS: "Bundled wallets (few wallets, many buys)",
  BUNDLE_SUPPLY: "3 wallets hold most of the supply",
  DEV_SNIPE: "Sniped in the first block",
  WASH_VOLUME: "Wash or thin volume",
  COPYCAT: "Copycat of a trending coin",
  CREATOR_BLACKLISTED: "Creator on blacklist",
  RUGCHECK_RUGGED: "RugCheck says rugged",
  RUGCHECK_DANGER: "RugCheck danger flag",
  METADATA_MISSING: "Name, symbol or link missing",
  METADATA_UNRESOLVED: "Metadata link does not open",
  NO_IMAGE: "No image",
  NO_SOCIALS: "No social links",
  CHASE_RISK: "Price already spiked 3x",
  BACKLOG: "Skipped: too busy",
  LOW_SCORE: "Score too low",
  GATE: "Entry rules not met",
};

export const reasonLabel = (code: string) => REASON_LABEL[code] ?? (code.startsWith("UNDECIDED_") ? `Could not verify (${code.slice(10).toLowerCase().replace(/_/g, " ")})` : code);

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

function Watchlist({ rows }: { rows: WatchRow[] }) {
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
      <Watchlist rows={engine.watchlist} />

      <section className="card">
        <h3>Open positions <span className="hint">0 of {config.risk.maxOpenPositions}</span></h3>
        <Empty>No open trades. Each position closes within 5 minutes.</Empty>
      </section>

      <ShieldCard engine={engine} />

      <section className="card span2">
        <h3>Position chart <span className="hint">no position selected</span></h3>
        <div className="chartbox">Pick an open position to see its price, your entry, your stop and your profit targets.</div>
        <div className="legend">
          <span><i className="g" />Entry</span>
          <span><i className="r" />Exit</span>
          <span><i className="r dash" />Stop loss</span>
          <span><i className="g dash" />Take profit</span>
          <span><i className="k" />Live price</span>
        </div>
      </section>

      <section className="card">
        <h3>Fees paid</h3>
        <Rows
          rows={[
            ["Today", "0.0000 SOL"],
            ["All time", "0.0000 SOL"],
            ["PumpPortal", "0.0000 SOL"],
            ["Network fee", "0.0000 SOL"],
            ["Priority fee", "0.0000 SOL"],
            ["Jito tip", "0.0000 SOL"],
            ["Average per trade", "none yet"],
          ]}
        />
      </section>

      <section className="card span2">
        <h3>Performance <span className="hint">all closed trades</span></h3>
        <div className="stats4">
          {[
            ["Win rate", "none yet"],
            ["Average win", "none yet"],
            ["Average loss", "none yet"],
            ["Profit factor", "none yet"],
            ["Max drawdown", "none yet"],
            ["Best trade", "none yet"],
            ["Worst trade", "none yet"],
            ["Trades per day", "none yet"],
          ].map(([k, v]) => (
            <div key={k}><b>{v}</b><span>{k}</span></div>
          ))}
        </div>
        <div className="chartbox" style={{ height: 90 }}>The equity curve appears after the first closed trade.</div>
      </section>

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
