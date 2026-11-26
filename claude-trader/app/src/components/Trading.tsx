import { useMemo, useState } from "react";
import type { Config, FeeBreakdown, LaneStats, PositionView } from "../types";
import type { EngineState } from "../useEngine";

const f4 = (n: number) => `${n.toFixed(4)} SOL`;
const sgn = (n: number, d = 4) => `${n > 0 ? "+" : ""}${n.toFixed(d)}`;
const price = (p: number) => (p >= 0.001 ? p.toFixed(6) : p.toPrecision(4));
const dur = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
const LANE_NAME: Record<string, string> = { momentum: "Momentum", "dip-40pct": "Dip variant" };
const laneName = (l: string) => LANE_NAME[l] ?? l;

function Empty({ children }: { children: string }) {
  return <div className="empty">{children}</div>;
}

export function ReplayTag({ on }: { on: boolean }) {
  return on ? <span className="replaytag" title="This data comes from a scripted replay, not the live market">REPLAY</span> : null;
}

const isOpen = (p: PositionView) => p.state !== "CLOSED";

export function OpenPositions({ engine, config }: { engine: EngineState; config: Config }) {
  const open = engine.positions.filter(isOpen);
  return (
    <section className="card">
      <h3>Open positions <span className="hint">{open.length} of {config.risk.maxOpenPositions}</span></h3>
      {open.length === 0 ? (
        <Empty>No open trades. Each position closes within 5 minutes.</Empty>
      ) : (
        <div className="poslist">
          {open.map((p) => (
            <div className="pos" key={p.id}>
              <div className="ph">
                <b>{p.symbol}</b> <ReplayTag on={p.replay} />
                <span className="chip lane">{laneName(p.lane)}</span>
                <span className={`pl ${p.pnlSol < 0 ? "neg" : "pos"}`}>{sgn(p.pnlSol)} SOL ({sgn(p.pnlPct, 1)}%)</span>
              </div>
              <div className="pg">
                <span>Entry <b>{price(p.entryPrice)}</b></span>
                <span>Now <b>{price(p.currentPrice)}</b> ({sgn(p.pricePct, 1)}%)</span>
                <span>Size <b>{p.sizeSol} SOL</b></span>
                <span>Held <b>{dur(p.holdSeconds)}</b> of 5:00</span>
              </div>
              <div className="pn">Next: {p.nextAction}</div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

interface Candle { t: number; o: number; h: number; l: number; c: number }

function candles(ticks: [number, number][], step: number): Candle[] {
  const m = new Map<number, Candle>();
  for (const [ts, pr] of ticks) {
    const k = Math.floor(ts / step) * step;
    const c = m.get(k);
    if (!c) m.set(k, { t: k, o: pr, h: pr, l: pr, c: pr });
    else {
      c.h = Math.max(c.h, pr);
      c.l = Math.min(c.l, pr);
      c.c = pr;
    }
  }
  return [...m.values()].sort((a, b) => a.t - b.t);
}

export function PositionChart({ engine }: { engine: EngineState }) {
  const [pick, setPick] = useState<number | null>(null);
  const list = engine.positions;
  const sel = list.find((p) => p.id === pick) ?? list.find(isOpen) ?? list[0];
  const body = useMemo(() => (sel ? <Chart p={sel} /> : null), [sel]);
  return (
    <section className="card span2">
      <h3>
        <span>Position chart {sel && <ReplayTag on={sel.replay} />}</span>
        {list.length > 1 ? (
          <select value={sel?.id ?? ""} onChange={(e) => setPick(Number(e.target.value))} aria-label="Choose position">
            {list.map((p) => (
              <option key={p.id} value={p.id}>{p.symbol} ({laneName(p.lane)}, {p.state.toLowerCase()})</option>
            ))}
          </select>
        ) : (
          <span className="hint">{sel ? `${sel.symbol}, ${sel.state.toLowerCase()}` : "no position yet"}</span>
        )}
      </h3>
      {sel ? body : <div className="chartbox">When a trade opens, its price, your entry, your stop and your profit targets show here.</div>}
      <div className="legend">
        <span><i className="g" />Entry</span>
        <span><i className="r" />Exit</span>
        <span><i className="r dash" />Stop loss</span>
        <span><i className="g dash" />Take profit</span>
        <span><i className="k" />Live price</span>
      </div>
    </section>
  );
}

function Chart({ p }: { p: PositionView }) {
  const W = 900, H = 290, L = 8, R = 92, T = 12, B = 24;
  const ticks = p.ticks;
  const last = ticks.length ? ticks[ticks.length - 1][0] : p.entryTs;
  const endMs = p.state === "CLOSED" ? Math.max(last, ...p.exits.map((e) => e.ts)) + 8000 : Math.max(last, p.entryTs) + 5000;
  const startMs = Math.min(p.entryTs - 45000, ticks.length ? ticks[0][0] : p.entryTs);
  const vis = ticks.filter(([t]) => t >= startMs);
  const step = Math.max(5000, Math.ceil((endMs - startMs) / 40 / 1000) * 1000);
  const cs = candles(vis, step);
  const levels = [p.entryPrice, p.hardStop, p.softStop, ...p.takeProfits, p.currentPrice, ...vis.map((v) => v[1])].filter((x) => x > 0);
  let lo = Math.min(...levels), hi = Math.max(...levels);
  const pad = (hi - lo) * 0.06 || hi * 0.05;
  lo -= pad; hi += pad;
  const x = (t: number) => L + ((t - startMs) / (endMs - startMs)) * (W - L - R);
  const y = (v: number) => T + (1 - (v - lo) / (hi - lo)) * (H - T - B);
  const cw = Math.max(4, ((W - L - R) / Math.max(1, (endMs - startMs) / step)) * 0.7);
  const hline = (v: number, cls: string, label: string) => (
    <g key={cls + label}>
      <line x1={L} x2={W - R} y1={y(v)} y2={y(v)} className={cls} />
      <text x={W - R + 6} y={y(v) + 4} className={`lbl ${cls}`}>{label}</text>
    </g>
  );
  const done = p.tranchesDone;
  const ys = Array.from({ length: 4 }, (_, i) => lo + ((hi - lo) * (i + 0.5)) / 4);
  return (
    <div className="chartwrap">
      <svg viewBox={`0 0 ${W} ${H}`} className="chart" role="img" aria-label={`Price chart for ${p.symbol}`}>
        {ys.map((v, i) => (
          <g key={i}>
            <line x1={L} x2={W - R} y1={y(v)} y2={y(v)} className="grid" />
          </g>
        ))}
        {[0, 1, 2, 3, 4].map((i) => {
          const t = startMs + ((endMs - startMs) * i) / 4;
          return <text key={i} x={x(t)} y={H - 6} className="tick" textAnchor="middle">{new Date(t).toLocaleTimeString([], { hour12: false, minute: "2-digit", second: "2-digit" })}</text>;
        })}
        {cs.map((c) => {
          const up = c.c >= c.o;
          const cx = x(c.t + step / 2);
          return (
            <g key={c.t} className={up ? "cu" : "cd"}>
              <line x1={cx} x2={cx} y1={y(c.h)} y2={y(c.l)} />
              <rect x={cx - cw / 2} y={Math.min(y(c.o), y(c.c))} width={cw} height={Math.max(1.5, Math.abs(y(c.o) - y(c.c)))} />
            </g>
          );
        })}
        {hline(p.hardStop, "sl", `Stop ${price(p.hardStop)}`)}
        <line x1={L} x2={W - R} y1={y(p.softStop)} y2={y(p.softStop)} className="sl soft" />
        {p.takeProfits.map((v, i) => hline(v, `tp${i < done ? " hit" : ""}`, p.state !== "CLOSED" && Math.abs(y(v) - y(p.currentPrice)) < 12 ? "" : `TP${i + 1} ${price(v)}`))}
        {hline(p.entryPrice, "entryline", `Entry ${price(p.entryPrice)}`)}
        {p.state !== "CLOSED" && hline(p.currentPrice, "live", `Now ${price(p.currentPrice)}`)}
        <g className="entrymark">
          <polygon points={`${x(p.entryTs)},${y(p.entryPrice) - 2} ${x(p.entryTs) - 7},${y(p.entryPrice) + 12} ${x(p.entryTs) + 7},${y(p.entryPrice) + 12}`} />
          <text x={x(p.entryTs)} y={y(p.entryPrice) + 25} textAnchor="middle">BUY</text>
        </g>
        {p.exits.map((e, i) => (
          <g key={i} className="exitmark">
            <polygon points={`${x(e.ts)},${y(e.price) + 2} ${x(e.ts) - 7},${y(e.price) - 12} ${x(e.ts) + 7},${y(e.price) - 12}`} />
            <text x={x(e.ts)} y={y(e.price) - 16} textAnchor="middle">{e.reason.replace("take-profit-", "TP")}</text>
          </g>
        ))}
      </svg>
      <div className="chartnote">
        {p.state === "CLOSED" ? `Closed: ${p.exitReason}. Net ${sgn(p.pnlSol)} SOL (${sgn(p.pnlPct, 1)}%).` : `Candles of ${step / 1000} s built from the coin's own trades. Net ${sgn(p.pnlSol)} SOL (${sgn(p.pnlPct, 1)}%) if sold now, after all fees.`}
      </div>
    </div>
  );
}

const feeRows = (f: FeeBreakdown): [string, number][] => [
  ["pump.fun fee", f.pumpFun],
  ["Creator fee", f.creator],
  ["PumpPortal fee", f.pumpPortal],
  ["Network fee", f.tx],
  ["Priority fee", f.priority],
  ["Jito tip", f.jito],
];
const sumFees = (f: FeeBreakdown) => f.pumpFun + f.creator + f.pumpPortal + f.tx + f.priority + f.jito;

export function FeesCard({ lane }: { lane: LaneStats | undefined }) {
  const z: FeeBreakdown = { pumpFun: 0, creator: 0, pumpPortal: 0, tx: 0, priority: 0, jito: 0 };
  const all = lane?.feesAll ?? z;
  const today = lane?.feesToday ?? z;
  const max = Math.max(1e-9, ...feeRows(all).map((r) => r[1]));
  return (
    <section className="card">
      <h3>Fees paid <span className="hint">{lane ? laneName(lane.lane) : ""}</span></h3>
      <div className="row"><span className="l">Today</span><span className="r">{f4(sumFees(today))}</span></div>
      <div className="row"><span className="l">All time</span><span className="r">{f4(sumFees(all))}</span></div>
      <div className="reasons" style={{ marginTop: 8 }}>
        {feeRows(all).map(([k, v]) => (
          <div className="rbar feebar" key={k}>
            <span className="rl">{k}</span>
            <span className="rt"><i style={{ width: `${(v / max) * 100}%` }} /></span>
            <span className="rc fee">{v.toFixed(5)}</span>
          </div>
        ))}
      </div>
      <div className="row"><span className="l">Rent locked now (returned on close)</span><span className="r">{(lane?.rentLockedSol ?? 0).toFixed(5)}</span></div>
      <div className="row"><span className="l">Average per trade</span><span className="r">{lane && lane.trades > 0 ? f4(lane.feePerTrade) : "none yet"}</span></div>
      {lane && lane.failedBuys > 0 && <div className="row"><span className="l">Buys that failed</span><span className="r">{lane.failedBuys}</span></div>}
    </section>
  );
}

function Equity({ pts, base }: { pts: [number, number][]; base: number }) {
  if (pts.length < 1) return <div className="chartbox" style={{ height: 90 }}>The equity curve appears after the first closed trade.</div>;
  const W = 600, H = 100;
  const data: [number, number][] = [[pts[0][0] - 1, base], ...pts];
  const xs = data.map((d) => d[0]), ys = data.map((d) => d[1]);
  const x0 = Math.min(...xs), x1 = Math.max(...xs) || 1;
  let lo = Math.min(...ys, base), hi = Math.max(...ys, base);
  if (hi - lo < 1e-9) { hi += 0.01; lo -= 0.01; }
  const X = (t: number) => 6 + ((t - x0) / (x1 - x0 || 1)) * (W - 12);
  const Y = (v: number) => 8 + (1 - (v - lo) / (hi - lo)) * (H - 16);
  const d = data.map((p, i) => `${i ? "L" : "M"}${X(p[0]).toFixed(1)},${Y(p[1]).toFixed(1)}`).join(" ");
  const up = ys[ys.length - 1] >= base;
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="equity" role="img" aria-label="Equity curve">
      <line x1="6" x2={W - 6} y1={Y(base)} y2={Y(base)} className="grid" />
      <path d={d} className={up ? "eqline" : "eqline down"} />
      {data.map((p, i) => i > 0 && <circle key={i} cx={X(p[0])} cy={Y(p[1])} r="3" className={p[1] >= data[i - 1][1] ? "eqdot" : "eqdot down"} />)}
      <text x="8" y="12" className="tick">{hi.toFixed(3)}</text>
      <text x="8" y={H - 3} className="tick">{lo.toFixed(3)}</text>
    </svg>
  );
}

export function PerformanceCard({ engine }: { engine: EngineState }) {
  const lanes = engine.stats?.lanes ?? [];
  const m = lanes[0];
  const v = lanes[1];
  const none = "none yet";
  const has = !!m && m.trades > 0;
  const cells: [string, string][] = [
    ["Win rate", has ? `${m!.winRatePct.toFixed(0)}% (${m!.wins}/${m!.trades})` : none],
    ["Average win", has && m!.wins ? `${sgn(m!.avgWinSol)} SOL` : none],
    ["Average loss", has && m!.losses ? `${sgn(m!.avgLossSol)} SOL` : none],
    ["Profit factor", has ? (m!.profitFactor === null ? "no losses yet" : m!.profitFactor.toFixed(2)) : none],
    ["Max drawdown", has ? `${m!.maxDrawdownSol.toFixed(4)} SOL` : none],
    ["Best trade", has ? `${sgn(m!.bestSol)} SOL` : none],
    ["Worst trade", has ? `${sgn(m!.worstSol)} SOL` : none],
    ["Trades per day", has ? m!.tradesPerDay.toFixed(1) : none],
  ];
  return (
    <section className="card span2">
      <h3>Performance <span className="hint">{m ? laneName(m.lane) : "all closed trades"}, net of all fees {engine.stats?.replay && <ReplayTag on />}</span></h3>
      {m?.halted && <div className="haltbar">Trading halted: {m.halted}. It needs a manual restart.</div>}
      {m?.paused && !m.halted && <div className="haltbar">Paused: the last 20 trades have a profit factor under 0.8.</div>}
      <div className="stats4">
        {cells.map(([k, val]) => (
          <div key={k}><b>{val}</b><span>{k}</span></div>
        ))}
      </div>
      <Equity pts={m?.equity ?? []} base={engine.stats?.bankrollSol ?? 0} />
      {v && (
        <div className="variant">
          Paper variant <b>{laneName(v.lane)}</b> (tracked separately, same rules): {v.trades} trades, {v.trades ? `${v.winRatePct.toFixed(0)}% wins, ` : ""}P/L {sgn(v.totalPnlSol)} SOL
          {v.profitFactor !== null && v.trades ? `, profit factor ${v.profitFactor.toFixed(2)}` : ""}. Main lane P/L {m ? sgn(m.totalPnlSol) : "0"} SOL.
        </div>
      )}
    </section>
  );
}
