import { useEffect, useState } from "react";
import type { Page } from "../App";
import type { StatsState } from "../types";

// Demo SOL price used only until the live price feed exists.
const DEMO_SOL_USD = 150;

const usd = (n: number) => n.toLocaleString("en-US", { style: "currency", currency: "USD" });
const sol = (n: number, sign = false) => `${sign && n > 0 ? "+" : ""}${Math.abs(n) < 1 && n !== 0 ? n.toFixed(4) : n.toFixed(2)} SOL`;

export default function TopBar({ page, go, mode, bankroll, stats }: { page: Page; go: (p: Page) => void; mode: string; bankroll: number; stats: StatsState | null }) {
  const [now, setNow] = useState(new Date());
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(t);
  }, []);

  const main = stats?.lanes[0];
  const buys = main?.buysToday ?? 0;
  const sells = main?.sellsToday ?? 0;
  const pnlToday = main?.pnlToday ?? 0;
  const pnlTotal = main?.totalPnlSol ?? 0;
  const balance = stats?.balanceSol ?? bankroll;
  const solUsd = stats?.solUsd ?? DEMO_SOL_USD;
  const cls = (n: number) => (n < 0 ? "neg" : "pos");

  return (
    <header className="topbar">
      <div className="brand">
        <span className="logo">
          <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><rect x="4" y="8" width="16" height="12" rx="4" /><path d="M12 8V4" /><circle cx="9" cy="14" r="1" fill="currentColor" /><circle cx="15" cy="14" r="1" fill="currentColor" /></svg>
        </span>
        Claude Trader
      </div>
      <div className="stat">
        <span className="k">Time</span>
        <span className="v">{now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}</span>
      </div>
      <div className="stat hover" tabIndex={0}>
        <span className="k">Trades today</span>
        <span className="v">{main?.tradesToday ?? 0}</span>
        <span className="tip" role="tooltip">
          <span className="b">Buys {buys}</span>
          <span className="s">Sells {sells}</span>
        </span>
      </div>
      <div className="stat hover" tabIndex={0}>
        <span className="k">Balance</span>
        <span className="v">{sol(balance)}<small>{usd(balance * solUsd)}</small></span>
        <span className="tip" role="tooltip">{stats?.replay ? "REPLAYED market, pretend money. " : "Pretend money. "}USD uses {usd(solUsd)} per SOL.</span>
      </div>
      <div className="stat">
        <span className="k">P/L today</span>
        <span className={`v ${cls(pnlToday)}`}>{sol(pnlToday, true)}<small>{usd(pnlToday * solUsd)}</small></span>
      </div>
      <div className="stat">
        <span className="k">P/L total</span>
        <span className={`v ${cls(pnlTotal)}`}>{sol(pnlTotal, true)}<small>{usd(pnlTotal * solUsd)}</small></span>
      </div>
      <div className="spacer" />
      <nav className="nav">
        <button className={page === "dashboard" ? "active" : ""} onClick={() => go("dashboard")}>Dashboard</button>
        <button className={page === "settings" ? "active" : ""} onClick={() => go("settings")}>Settings</button>
      </nav>
      <span className={`badge ${mode === "live" ? "live" : ""}`}>{mode === "live" ? "LIVE" : "PAPER"}</span>
    </header>
  );
}
