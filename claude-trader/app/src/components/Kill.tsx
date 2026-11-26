import { useEffect, useState } from "react";
import { source } from "../source";
import type { KillState } from "../types";

/** Red strip under the top bar while the kill switch is on: how many positions are still closing. */
export function KillBanner({ kill }: { kill: KillState | null }) {
  if (!kill?.killed) return null;
  return (
    <div className="kill-banner" role="status">
      <strong>{kill.text}</strong>
      {kill.items.length > 0 && (
        <ul>
          {kill.items.map((i, n) => (
            <li key={n}>{i.symbol} ({i.lane}): {i.status}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** Confirm dialogs for the kill switch and for resuming. Nothing happens until the person confirms. */
export default function KillUi({ ask, close, killed }: { ask: "kill" | "resume" | null; close: () => void; killed: boolean }) {
  const [msg, setMsg] = useState("");
  useEffect(() => {
    if (!ask) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [ask, close]);
  if (!ask) return null;
  // Ctrl+Shift+K while already killed: nothing to confirm
  if (ask === "kill" && killed) return null;
  const isKill = ask === "kill";
  const go = async () => {
    setMsg(await source.command(isKill ? "kill" : "resume"));
    close();
  };
  return (
    <div className="kill-overlay" role="dialog" aria-modal="true" aria-label={isKill ? "Kill switch" : "Resume trading"}>
      <div className="kill-dialog">
        <h2>{isKill ? "Kill switch" : "Resume trading"}</h2>
        <p>{isKill ? "Stop all trading and sell every open position now?" : "Allow the bot to open new trades again?"}</p>
        {isKill && <p className="small">No new buys will be made until you press Resume trading. This stays in force even if you restart the app.</p>}
        <div className="kill-actions">
          <button className="kill-btn" onClick={go} autoFocus>{isKill ? "Yes, stop and sell everything" : "Yes, resume trading"}</button>
          <button className="kill-cancel" onClick={close}>Cancel</button>
        </div>
        {msg && <p className="small">{msg}</p>}
      </div>
    </div>
  );
}
