export type AgentId =
  | "claude-bot"
  | "scam-shield"
  | "token-picker"
  | "executor"
  | "hold-timer"
  | "thesis-finder"
  | "memecoin-monitor"
  | "x-monitor"
  | "top-trader-monitor"
  | "top-gainers";

export type StatusColor = "green" | "amber" | "grey" | "red";

/** Mirrors engine/src/events.rs */
export type EngineEvent =
  | { type: "status"; agent: AgentId; color: StatusColor; line: string }
  | { type: "pulse"; from: AgentId; to: AgentId; text: string }
  | { type: "activity"; ts: number; agent: AgentId; text: string; mint: string | null }
  | { type: "decision"; ts: number; agent: AgentId; text: string }
  | { type: "token"; ts: number; mint: string; symbol: string; name: string; creator: string; mcapSol: number }
  | {
      type: "verdict";
      ts: number;
      mint: string;
      symbol: string;
      stage: string;
      outcome: "pass" | "block";
      reason: string;
      detail: string;
      verdict: RiskVerdict;
      evidence: Evidence[];
    }
  | {
      type: "score";
      ts: number;
      mint: string;
      symbol: string;
      score: number;
      minScore: number;
      picked: boolean;
      buyers: number;
      buys: number;
      sells: number;
      ratio: number;
      priceChangePct: number;
      curveSol: number;
      mcapSol: number;
      gates: string[];
    }
  | { type: "shield-summary"; ts: number; checked: number; blocked: number; passed: number; byReason: { reason: string; count: number }[] };

export type RiskVerdict = "LOW_SIGNAL" | "ELEVATED" | "HIGH";

export interface Evidence {
  label: string;
  url: string;
}

export type WatchStatus = "checking" | "blocked" | "observing" | "scored" | "picked";

export interface WatchRow {
  mint: string;
  symbol: string;
  name: string;
  ts: number;
  status: WatchStatus;
  /** Plain-language reason or detail for the current status. */
  reason: string;
  code: string;
  verdict: RiskVerdict | null;
  score: number | null;
  evidence: Evidence[];
}

export interface ShieldSummary {
  checked: number;
  blocked: number;
  passed: number;
  byReason: { reason: string; count: number }[];
}

export interface LatestVerdict {
  mint: string;
  symbol: string;
  stage: string;
  outcome: "pass" | "block";
  reason: string;
  verdict: RiskVerdict;
  evidence: Evidence[];
  ts: number;
}

export interface AgentState {
  color: StatusColor;
  line: string;
}

export interface ActivityItem {
  ts: number;
  agent: AgentId;
  text: string;
  mint: string | null;
}

export interface Pulse {
  id: number;
  from: AgentId;
  to: AgentId;
  text: string;
}

export type Config = typeof import("../../config.json");
