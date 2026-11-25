import { useEffect, useRef, useState } from "react";
import { source } from "./source";
import type { ActivityItem, AgentId, AgentState, EngineEvent, Pulse } from "./types";

export interface EngineState {
  agents: Record<string, AgentState>;
  activity: ActivityItem[];
  decisions: Record<string, { ts: number; text: string }[]>;
  pulses: Pulse[];
}

const initial: EngineState = { agents: {}, activity: [], decisions: {}, pulses: [] };
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
        };
        for (const e of batch) {
          if (e.type === "status") next.agents[e.agent] = { color: e.color, line: e.line };
          else if (e.type === "activity")
            next.activity = [{ ts: e.ts, agent: e.agent, text: e.text, mint: e.mint }, ...next.activity].slice(0, 200);
          else if (e.type === "decision") {
            const list = next.decisions[e.agent] ?? [];
            next.decisions[e.agent] = [{ ts: e.ts, text: e.text }, ...list].slice(0, 10);
          } else if (e.type === "pulse") {
            next.pulses = [...next.pulses, { id: ++pulseId.current, from: e.from, to: e.to, text: e.text }].slice(-3);
          }
        }
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
