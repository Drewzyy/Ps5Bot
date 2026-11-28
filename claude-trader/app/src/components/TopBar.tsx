import { useEffect, useState } from "react";
import Logo from "../Logo";
import type { Page } from "../App";
import type { ExtensionStatus, KillState, StatsState } from "../types";
import { extChip } from "../extStatus";
import { openSection } from "../nav";
import { UpdateButton } from "./UpdateUi";
import { useUpdater } from "../updater";

// Demo SOL price used only until the live price feed exists.
const DEMO_SOL_USD = 150;

const usd = (n: number) => n.toLocaleString("en-US", { style: "currency", currency: "USD" });
const sol = (n: number, sign = false) => `${sign && n > 0 ? "+" : ""}${Math.abs(n) < 1 && n !== 0 ? n.toFixed(4) : n.toFixed(2)} SOL`;

export default function TopBar({ page, go, mode, bankroll, stats, kill, extension, askKill, askResume }: { page: Page; go: (p: Page) => void; mode: string; bankroll: number; stats: StatsState | null; kill: KillState | null; extension: ExtensionStatus | null; askKill: () => void; askResume: () => void }) {
  const [now, setNow] = useState(new Date());
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(t);
  }, []);

  const upd = useUpdater();
  const chip = extChip(extension);
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
          <Logo size={30} />
        </span>
        Claude Trader
      </div>
      <div className="stat clock">
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
      {chip && (
        <button className={`ext-chip ${chip.tone}`} onClick={() => openSection("startup")} title={chip.hint} aria-label={`${chip.text}. ${chip.hint}`} data-testid="ext-chip">
          <span className="ed" aria-hidden="true" />
          {chip.text}
          {chip.attention && <b className="bang" aria-hidden="true">!</b>}
        </button>
      )}
      <nav className="nav">
        <button className={page === "dashboard" ? "active" : ""} onClick={() => go("dashboard")}>Dashboard</button>
        <button className={page === "settings" ? "active" : ""} onClick={() => go("settings")}>Settings{upd.state.phase === "ready" && <span className="set-badge" data-testid="update-badge">Update ready</span>}</button>
      </nav>
      <UpdateButton />
      {kill?.killed ? (
        <button className="kill-btn resume" onClick={askResume} title="Kill switch is on. Click to allow trading again.">Resume trading</button>
      ) : (
        <button className="kill-btn" onClick={askKill} title="Stop all trading and sell every open position now (Ctrl+Shift+K)">Kill switch</button>
      )}
      <span className={`badge ${mode === "live" ? "live" : ""}`}>{mode === "live" ? "LIVE" : "PAPER"}</span>
    </header>
  );
}
