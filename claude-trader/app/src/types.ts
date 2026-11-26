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
  | { type: "shield-summary"; ts: number; checked: number; blocked: number; passed: number; byReason: { reason: string; count: number }[] }
  | { type: "positions"; ts: number; positions: PositionView[] }
  | { type: "stats"; ts: number; balanceSol: number; bankrollSol: number; solUsd: number; replay: boolean; lanes: LaneStats[] };

export interface ExitMark {
  ts: number;
  price: number;
  reason: string;
}

/** Mirrors engine/src/events.rs PositionView */
export interface PositionView {
  id: number;
  lane: string;
  mint: string;
  symbol: string;
  state: "OPEN" | "SELLING" | "CLOSED";
  entryTs: number;
  entryPrice: number;
  currentPrice: number;
  sizeSol: number;
  tokensRemaining: number;
  pnlSol: number;
  pnlPct: number;
  pricePct: number;
  holdSeconds: number;
  nextAction: string;
  softStop: number;
  hardStop: number;
  takeProfits: number[];
  tranchesDone: number;
  exits: ExitMark[];
  exitReason: string;
  ticks: [number, number][];
  replay: boolean;
}

export interface FeeBreakdown {
  pumpFun: number;
  pumpPortal: number;
  tx: number;
  priority: number;
  jito: number;
}

export interface LaneStats {
  lane: string;
  trades: number;
  wins: number;
  losses: number;
  winRatePct: number;
  avgWinSol: number;
  avgLossSol: number;
  profitFactor: number | null;
  maxDrawdownSol: number;
  totalPnlSol: number;
  bestSol: number;
  worstSol: number;
  tradesPerDay: number;
  equity: [number, number][];
  feesToday: FeeBreakdown;
  feesAll: FeeBreakdown;
  feePerTrade: number;
  pnlToday: number;
  tradesToday: number;
  buysToday: number;
  sellsToday: number;
  failedBuys: number;
  open: number;
  halted: string | null;
  paused: boolean;
}

export interface StatsState {
  balanceSol: number;
  bankrollSol: number;
  solUsd: number;
  replay: boolean;
  lanes: LaneStats[];
}

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
