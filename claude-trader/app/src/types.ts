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
  | { type: "decision"; ts: number; agent: AgentId; text: string };

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
