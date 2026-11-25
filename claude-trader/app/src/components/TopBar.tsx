import { useEffect, useState } from "react";
import type { Page } from "../App";

// Demo SOL price used only until the live price feed exists.
const DEMO_SOL_USD = 150;

const usd = (n: number) => n.toLocaleString("en-US", { style: "currency", currency: "USD" });
const sol = (n: number, sign = false) => `${sign && n > 0 ? "+" : ""}${n.toFixed(2)} SOL`;

export default function TopBar({ page, go, mode, bankroll }: { page: Page; go: (p: Page) => void; mode: string; bankroll: number }) {
  const [now, setNow] = useState(new Date());
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(t);
  }, []);

  const buys = 0, sells = 0;
  const pnlToday = 0, pnlTotal = 0;
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
        <span className="v">{buys + sells}</span>
        <span className="tip" role="tooltip">
          <span className="b">Buys {buys}</span>
          <span className="s">Sells {sells}</span>
        </span>
      </div>
      <div className="stat hover" tabIndex={0}>
        <span className="k">Balance</span>
        <span className="v">{sol(bankroll)}<small>{usd(bankroll * DEMO_SOL_USD)}</small></span>
        <span className="tip" role="tooltip">Pretend money. USD uses a demo price of {usd(DEMO_SOL_USD)} per SOL.</span>
      </div>
      <div className="stat">
        <span className="k">P/L today</span>
        <span className={`v ${cls(pnlToday)}`}>{sol(pnlToday, true)}<small>{usd(pnlToday * DEMO_SOL_USD)}</small></span>
      </div>
      <div className="stat">
        <span className="k">P/L total</span>
        <span className={`v ${cls(pnlTotal)}`}>{sol(pnlTotal, true)}<small>{usd(pnlTotal * DEMO_SOL_USD)}</small></span>
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
