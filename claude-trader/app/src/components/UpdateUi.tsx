import { useEffect } from "react";
import { api } from "../source";
import { useUpdater } from "../updater";
import type { UpdPhase } from "../updater";
import { openSection } from "../nav";

export const ACTIVE: UpdPhase[] = ["closing", "downloading", "installing"];
export const FALLBACK_URL = "https://github.com/Drewzyy/Ps5Bot/actions";

/** Round "circular arrows" button in the top bar. A dot shows when the background check found an update. */
export function UpdateButton() {
  const u = useUpdater();
  const p = u.state.phase;
  const spinning = p === "checking" || ACTIVE.includes(p);
  const label = p === "ready" ? `Update to v${u.state.version}` : "Check for update";
  return (
    <button className={`upd-btn ${spinning ? "spin" : ""}`} onClick={u.click} disabled={spinning || p === "confirm"} aria-label={label} data-tip={label} title={label} data-testid="update-btn">
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d="M20 11a8 8 0 0 0-14.3-4.6L4 8" />
        <path d="M4 4v4h4" />
        <path d="M4 13a8 8 0 0 0 14.3 4.6L20 16" />
        <path d="M20 20v-4h-4" />
      </svg>
      {p === "ready" && <span className="upd-dot" aria-hidden="true" />}
    </button>
  );
}

export function ProgressBar({ value }: { value: number | null }) {
  return (
    <div className={`progress ${value === null ? "indet" : ""}`} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={value === null ? undefined : Math.round(value * 100)}>
      <div style={{ width: value === null ? "40%" : `${Math.round(value * 100)}%` }} />
    </div>
  );
}

export function stageText(phase: UpdPhase, version: string, open: number, progress: number | null): string {
  if (phase === "closing") return `Closing ${open} open paper position${open === 1 ? "" : "s"} first, no new buys...`;
  if (phase === "downloading") return `Updating to v${version}... downloading${progress === null ? "" : ` ${Math.round(progress * 100)}%`}`;
  if (phase === "installing") return `Updating to v${version}... installing, the app restarts in a moment`;
  return "";
}

/** Toasts and the open-position confirmation, shown over every page. */
export default function UpdateOverlay() {
  const u = useUpdater();
  const { phase, version, progress, message, current, fallback, kind } = u.state;
  useEffect(() => {
    if (phase !== "confirm") return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && u.cancel();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [phase, u]);
  if (phase === "confirm") {
    const n = u.openPositions;
    return (
      <div className="kill-overlay" role="dialog" aria-modal="true" aria-label="Install update">
        <div className="kill-dialog upd-dialog">
          <h2>Update to v{version}</h2>
          <p>{n} open paper position{n === 1 ? "" : "s"} will be closed first. Continue?</p>
          <p className="small">They are sold the normal way, with the normal fees. No new buys start meanwhile. This is not the kill switch, so trading carries on by itself after the restart. Your settings and trade history stay.</p>
          <div className="kill-actions">
            <button className="btn" onClick={u.confirm} autoFocus>Close {n === 1 ? "it" : "them"} and update</button>
            <button className="kill-cancel" onClick={u.cancel}>Cancel</button>
          </div>
        </div>
      </div>
    );
  }
  if (ACTIVE.includes(phase)) {
    return (
      <div className="toast upd-toast" role="status" data-testid="update-progress">
        <b>{stageText(phase, version, u.openPositions, progress)}</b>
        <ProgressBar value={phase === "closing" ? null : progress} />
      </div>
    );
  }
  if (phase === "uptodate") {
    return <div className="toast" role="status">{message}</div>;
  }
  if (phase === "error") {
    return (
      <div className="toast bad" role="alert">
        <span>{message || `Could not update from v${current}.`}</span>
        <span className="toast-actions">
          {kind === "not-set-up" && <button className="btn ghost" onClick={() => { u.dismiss(); openSection("about"); }}>Show steps</button>}
          {fallback && kind !== "not-set-up" && <button className="btn ghost" onClick={() => api.openLink(FALLBACK_URL)}>Open download page</button>}
          <button className="kill-cancel" onClick={u.dismiss}>Close</button>
        </span>
      </div>
    );
  }
  return null;
}
