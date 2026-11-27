import { useEffect, useRef, useState } from "react";
import { source } from "./source";
import type { LearningSnapshot, SourceLinks, KillState, ActivityItem, AgentId, AgentState, EngineEvent, LatestVerdict, PositionView, Pulse, ShieldSummary, StatsState, WatchRow } from "./types";

export interface EngineState {
  agents: Record<string, AgentState>;
  activity: ActivityItem[];
  decisions: Record<string, { ts: number; text: string }[]>;
  pulses: Pulse[];
  /** Newest first. */
  watchlist: WatchRow[];
  shield: ShieldSummary | null;
  /** Newest first, last 12 stage results with evidence links. */
  verdicts: LatestVerdict[];
  /** Open positions plus the last closed ones (for the chart). */
  positions: PositionView[];
  stats: StatsState | null;
  kill: KillState | null;
  /** Latest learning journal snapshot. */
  learning: LearningSnapshot | null;
  /** Coin website, X and Telegram links by mint. */
  links: Record<string, SourceLinks>;
  /** What the current executor cannot run ("skipped in paper"). */
  skipped: { executor: string; checks: { name: string; why: string }[] } | null;
  /** Chrome extension link status. */
  extension: { paired: boolean; connected: boolean; lastSeenMs: number } | null;
  /** True once real engine events (not the browser mock) have arrived. */
  live: boolean;
}

const initial: EngineState = { agents: {}, activity: [], decisions: {}, pulses: [], watchlist: [], shield: null, verdicts: [], positions: [], stats: null, kill: null, learning: null, links: {}, skipped: null, extension: null, live: false };
const WATCH_MAX = 300;
const FLUSH_MS = 300; // UI updates throttled to about 3 per second

export function useEngine(): EngineState {
  const [state, setState] = useState<EngineState>(initial);
  const buf = useRef<EngineEvent[]>([]);
  const pulseId = useRef(0);

  useEffect(() => {
    const unsub = source.subscribe((e) => buf.current.push(e));
    const timer = setInterval(() => {
      if (!buf.current.length) return;
      const batch = buf.current;
      buf.current = [];
      setState((prev) => {
        const next: EngineState = {
          agents: { ...prev.agents },
          activity: prev.activity,
          decisions: { ...prev.decisions },
          pulses: prev.pulses,
          watchlist: prev.watchlist,
          shield: prev.shield,
          verdicts: prev.verdicts,
          positions: prev.positions,
          stats: prev.stats,
          kill: prev.kill,
          learning: prev.learning,
          links: prev.links,
          skipped: prev.skipped,
          extension: prev.extension,
          live: prev.live,
        };
        let watch: Map<string, WatchRow> | null = null;
        const w = () => (watch ??= new Map(prev.watchlist.map((r) => [r.mint, r])));
        for (const e of batch) {
          if (e.type === "status") next.agents[e.agent] = { color: e.color, line: e.line };
          else if (e.type === "activity")
            next.activity = [{ ts: e.ts, agent: e.agent, text: e.text, mint: e.mint, sourceUrl: e.sourceUrl ?? null }, ...next.activity].slice(0, 200);
          else if (e.type === "decision") {
            const list = next.decisions[e.agent] ?? [];
            next.decisions[e.agent] = [{ ts: e.ts, text: e.text }, ...list].slice(0, 10);
          } else if (e.type === "token") {
            next.live = true;
            w().set(e.mint, { mint: e.mint, symbol: e.symbol, name: e.name, ts: e.ts, status: "checking", reason: "Being checked by Scam Shield", code: "", verdict: null, score: null, evidence: [] });
          } else if (e.type === "verdict") {
            const row = w().get(e.mint);
            if (row) {
              if (e.outcome === "block") {
                w().set(e.mint, { ...row, status: "blocked", reason: e.detail, code: e.reason, verdict: e.verdict, evidence: e.evidence });
              } else if (e.stage === "chain") {
                w().set(e.mint, { ...row, status: "observing", reason: "Passed Scam Shield. Token Picker is watching it for 60 s.", code: "", verdict: e.verdict, evidence: e.evidence });
              }
            }
            // cheap-prefilter passes are only the first gate, not a verdict worth showing as evidence
            if (!(e.outcome === "pass" && e.stage === "cheap-prefilter")) next.verdicts = [{ mint: e.mint, symbol: e.symbol, stage: e.stage, outcome: e.outcome, reason: e.reason, verdict: e.verdict, evidence: e.evidence, ts: e.ts }, ...next.verdicts].slice(0, 12);
          } else if (e.type === "score") {
            const row = w().get(e.mint);
            const keepBlocked = row?.status === "blocked";
            if (row && !keepBlocked) {
              w().set(e.mint, {
                ...row,
                status: e.picked ? "picked" : "scored",
                score: e.score,
                code: e.picked ? "" : "SCORE",
                reason: e.picked
                  ? `Picked. Score ${e.score} (needs ${e.minScore}). ${e.buyers} buyers, buy/sell ${e.ratio.toFixed(1)}.`
                  : `Score ${e.score} (needs ${e.minScore}). ${e.gates.length ? e.gates.join("; ") : "Below the minimum score."}`,
              });
            } else if (row && keepBlocked) {
              w().set(e.mint, { ...row, score: e.score });
            }
          } else if (e.type === "positions") {
            next.positions = e.positions;
          } else if (e.type === "stats") {
            next.stats = { balanceSol: e.balanceSol, bankrollSol: e.bankrollSol, solUsd: e.solUsd, replay: e.replay, lanes: e.lanes };
          } else if (e.type === "kill") {
            next.kill = e;
          } else if (e.type === "learning") {
            const { type: _t, ...snap } = e;
            next.learning = snap as LearningSnapshot;
          } else if (e.type === "links") {
            const keys = Object.keys(next.links);
            next.links = { ...(keys.length > 600 ? {} : next.links), [e.mint]: { website: e.website, twitter: e.twitter, telegram: e.telegram } };
          } else if (e.type === "skipped") {
            next.skipped = { executor: e.executor, checks: e.checks };
          } else if (e.type === "extension") {
            next.extension = { paired: e.paired, connected: e.connected, lastSeenMs: e.lastSeenMs };
          } else if (e.type === "shield-summary") {
            next.shield = { checked: e.checked, blocked: e.blocked, passed: e.passed, byReason: e.byReason };
          } else if (e.type === "pulse") {
            next.pulses = [...next.pulses, { id: ++pulseId.current, from: e.from, to: e.to, text: e.text }].slice(-3);
          }
        }
        if (watch) next.watchlist = [...(watch as Map<string, WatchRow>).values()].sort((a, b) => b.ts - a.ts).slice(0, WATCH_MAX);
        return next;
      });
    }, FLUSH_MS);
    return () => {
      unsub();
      clearInterval(timer);
    };
  }, []);

  return state;
}

export type { AgentId };
