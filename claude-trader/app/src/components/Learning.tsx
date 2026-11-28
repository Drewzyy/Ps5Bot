import { useState } from "react";
import { api } from "../source";
import type { ChangeRow, JournalRow, LearningSnapshot } from "../types";
import type { EngineState } from "../useEngine";
import { SourceLink } from "./links";

const sgn = (n: number, d = 3) => `${n > 0 ? "+" : ""}${n.toFixed(d)}`;
const when = (ts: number) => new Date(ts).toLocaleTimeString([], { hour12: false, hour: "2-digit", minute: "2-digit" });
const num = (n: number) => String(Math.round(n * 1000) / 1000);

const STATUS: Record<ChangeRow["status"], [string, string]> = {
  shadow: ["Testing", "chip observing"],
  applied: ["Applied", "chip picked"],
  rejected: ["Not used", "chip"],
  undone: ["Undone", "chip"],
  "awaiting-approval": ["Needs your OK", "chip scored"],
};

function Entry({ r }: { r: JournalRow }) {
  const [more, setMore] = useState(false);
  const shown = more ? r.observations : r.observations.slice(0, 2);
  return (
    <div className="jentry">
      <div className="jhead">
        <span className="jts">{when(r.ts)}</span>
        <b className={r.pnlSol >= 0 ? "jpos" : "jneg"}>{r.headline}</b>
        <span className="jsym">{r.symbol}</span>
        {r.lane !== "momentum" && <span className="chip lane">{r.lane === "dip-40pct" ? "Dip variant" : r.lane === "deep-sprint" ? "Deep sprint" : r.lane}</span>}
        <span className="jmeta">{r.exitReason.replace(/-/g, " ")}, held {Math.round(r.holdSeconds)} s</span>
      </div>
      <div className="jfeat">
        Score {r.score.toFixed(1)}, {r.buyers} buyers, buy/sell {r.ratio.toFixed(1)}, curve {r.curveSol.toFixed(1)} SOL, age {Math.round(r.ageSeconds)} s
      </div>
      {r.observations.length > 0 && (
        <ul className="jobs">
          <li className="jlab">Observed:</li>
          {shown.map((o, i) => <li key={i}>{o}</li>)}
        </ul>
      )}
      <div className="jfoot">
        {r.observations.length > 2 && (
          <button className="linkbtn" onClick={() => setMore(!more)}>{more ? "Show less" : `Show ${r.observations.length - 2} more`}</button>
        )}
        <SourceLink url={r.sourceUrl} />
      </div>
    </div>
  );
}

function Change({ c, demo, live }: { c: ChangeRow; demo: boolean; live: boolean }) {
  const [msg, setMsg] = useState("");
  const [name, tone] = STATUS[c.status];
  const act = async (verb: "undo-change" | "approve-change") => {
    try {
      setMsg(await api.learning(`${verb}:${c.id}`));
    } catch (e) {
      setMsg(String(e instanceof Error ? e.message : e));
    }
  };
  return (
    <div className="jchange">
      <div className="jhead">
        <span className={tone}>{name}</span>
        <b>{c.label}: {num(c.before)} to {num(c.after)}</b>
        {demo && <span className="demotag">DEMO</span>}
      </div>
      {c.evidence.map((e, i) => <div className="jfeat" key={i}>Why: {e}</div>)}
      {c.result && <div className="jfeat">Result: {c.result}</div>}
      <div className="jfoot">
        {c.status === "applied" && <button className="btn ghost small" onClick={() => act("undo-change")}>Undo</button>}
        {c.status === "awaiting-approval" && (
          <>
            <button className="btn small" onClick={() => act("approve-change")}>Approve</button>
            <button className="btn ghost small" onClick={() => act("undo-change")}>Drop it</button>
          </>
        )}
        {c.status === "shadow" && <button className="btn ghost small" onClick={() => act("undo-change")}>Stop the test</button>}
        {c.status === "applied" && live && <span className="jmeta">Live mode: changes need your approval.</span>}
        {msg && <span className="jmeta">{msg}</span>}
      </div>
    </div>
  );
}

export function LearningCard({ engine }: { engine: EngineState }) {
  const L: LearningSnapshot | null = engine.learning;
  const empty = !L || (L.journal.length === 0 && L.changes.length === 0);
  const applied = L?.changes.filter((c) => c.status === "applied").length ?? 0;
  const sp = L?.shadowProgress;
  const eff = L?.efficiency;
  return (
    <section className="card span2" id="learning-card">
      <h3>
        <span>What it learned {L?.demo && <span className="demotag">DEMO DATA</span>}</span>
        <span className="hint">only from real recorded trades</span>
      </h3>
      {!L || !L.enabled ? (
        <div className="empty">{L && !L.enabled ? "Learning is switched off in Settings." : "Waiting for the engine. Nothing has been learned yet."}</div>
      ) : (
        <>
          <div className="lstats">
            <div><b>{L.closedTrades}</b><span>trades closed</span></div>
            <div><b>{L.nextReviewIn}</b><span>trades until the next review</span></div>
            <div><b>{applied}</b><span>changes applied</span></div>
          </div>
          {empty && <div className="empty">No trade has closed yet, so there is nothing to learn from. After the first trade closes, a note about it shows up here with the real numbers.</div>}
          {L.journal.length > 0 && (
            <>
              <h4 className="lh">Latest entries</h4>
              <div className="jlist">{L.journal.slice(0, 5).map((r) => <Entry key={`${r.lane}-${r.id}`} r={r} />)}</div>
            </>
          )}
          {sp && L.shadow && (
            <div className="jshadow">
              <b>Testing now:</b> {L.shadow.label}, {num(L.shadow.before)} to {num(L.shadow.after)}, in a separate pretend lane. Main lane {sp.mainDone} of {sp.target} trades.
              Test lane: {sp.shadow.trades} trades, net {sgn(sp.shadow.netSol)} SOL. Main lane: {sp.main.trades} trades, net {sgn(sp.main.netSol)} SOL.
              It is used only if the test trades average a profit AND the low end of the 95% range is above zero, with at least 20 trades. Fewer trades or a lucky-looking result are not enough.
            </div>
          )}
          <h4 className="lh">Changes applied</h4>
          {L.changes.length === 0 ? (
            <div className="empty">No changes yet. The first review happens after {L.reviewEvery} closed trades, and a change is made only if its test trades show a real average profit, not luck.</div>
          ) : (
            <div className="jlist">{L.changes.map((c) => <Change key={c.id} c={c} demo={L.demo} live={L.liveMode} />)}</div>
          )}
          {L.lastReview && (
            <div className="jreview">
              <b>Last review ({L.lastReview.tradesReviewed} trades):</b>{" "}
              {L.lastReview.result.findings.length ? `${L.lastReview.result.findings.length} pattern(s) found. ${L.lastReview.result.findings[0].text}` : L.lastReview.result.notes.join(" ")}
            </div>
          )}
          {eff && (
            <>
              <h4 className="lh">Speed and costs</h4>
              <div className="rows">
                <div className="row"><span className="l">Time from pick to fill</span><span className="r">{eff.fills ? `${eff.avgPickToFillS.toFixed(1)} s on average (${eff.fills} fills)` : "no fills yet"}</span></div>
                <div className="row"><span className="l">Cost of failed transactions</span><span className="r">{eff.failedTxCostSol.toFixed(4)} SOL ({eff.failedBuys} buys never filled)</span></div>
                <div className="row"><span className="l">Fees as a share of gross profit (winners)</span><span className="r">{eff.feeSharePct === null ? "no winning trades yet" : `${eff.feeSharePct.toFixed(0)}%`}</span></div>
                <div className="row"><span className="l">Missed coins (scored 80 or more, blocked)</span><span className="r">{eff.missed}</span></div>
              </div>
              {eff.notes.length > 0 && (
                <ul className="jobs">
                  {eff.notes.slice(0, 3).map((n) => (
                    <li key={n.id}>{n.text} <SourceLink url={n.sourceUrl} /></li>
                  ))}
                </ul>
              )}
            </>
          )}
        </>
      )}
    </section>
  );
}
