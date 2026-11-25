import type { Config } from "../types";

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

export default function Widgets({ config, openSettings }: { config: Config; openSettings: () => void }) {
  return (
    <div className="grid">
      <section className="card">
        <h3>Watchlist <span className="hint">0 coins</span></h3>
        <Empty>No coins yet. Every coin the app looks at appears here, including the ones it skips and the reason why.</Empty>
      </section>

      <section className="card">
        <h3>Open positions <span className="hint">0 of {config.risk.maxOpenPositions}</span></h3>
        <Empty>No open trades. Each position closes within 5 minutes.</Empty>
      </section>

      <section className="card">
        <h3>Scam Shield <span className="hint">today</span></h3>
        <div className="row"><span className="l">Coins blocked</span><span className="r">0</span></div>
        <div className="row"><span className="l">Verdict</span><span className="verdict">No coins checked yet</span></div>
        <Empty>Reasons for blocks will be listed here, such as live mint authority or bundled wallets.</Empty>
      </section>

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
