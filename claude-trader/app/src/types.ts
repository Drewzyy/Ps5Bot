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
  | { type: "activity"; ts: number; agent: AgentId; text: string; mint: string | null; sourceUrl?: string | null }
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
  | { type: "stats"; ts: number; balanceSol: number; bankrollSol: number; solUsd: number; replay: boolean; lanes: LaneStats[] }
  | ({ type: "kill" } & KillState)
  | ({ type: "learning" } & LearningSnapshot)
  | { type: "links"; mint: string; website: string; twitter: string; telegram: string }
  | { type: "skipped"; executor: string; checks: { name: string; why: string }[] }
  | { type: "extension"; ts: number; paired: boolean; connected: boolean; lastSeenMs: number };

/** Mirrors engine/src/startup.rs plus what the app adds. */
export interface StartupView {
  openChrome: boolean;
  pages: { pumpFunHome: boolean; dexscreenerHeld: boolean };
  closeTabOnClose: boolean;
  keepAwake: boolean;
  autostart: boolean;
  chromeFound: boolean;
  extensionId: string;
  extensionFolder: string;
}

/** Mirrors engine/src/learning.rs */
export interface JournalRow {
  id: number;
  ts: number;
  positionId: number;
  lane: string;
  tradeNo: number;
  symbol: string;
  mint: string;
  pnlSol: number;
  pnlPct: number;
  feesSol: number;
  grossSol: number;
  holdSeconds: number;
  exitReason: string;
  score: number;
  buyers: number;
  ratio: number;
  curveSol: number;
  ageSeconds: number;
  peakPct: number;
  exitPct: number;
  fillSeconds: number;
  observations: string[];
  sourceUrl: string;
  headline: string;
}

export interface ChangeRow {
  id: number;
  ts: number;
  key: string;
  label: string;
  before: number;
  after: number;
  status: "shadow" | "applied" | "rejected" | "undone" | "awaiting-approval";
  why: string;
  evidence: string[];
  result: string;
  startMs: number;
  mainStart: number;
  target: number;
  doneMs: number;
}

export interface ArmStat {
  trades: number;
  netSol: number;
  wins: number;
  profitFactor: number | null;
}

export interface NoteRow {
  id: number;
  ts: number;
  kind: string;
  text: string;
  sourceUrl: string;
}

export interface LearningSnapshot {
  ts: number;
  enabled: boolean;
  liveMode: boolean;
  demo: boolean;
  closedTrades: number;
  reviewEvery: number;
  nextReviewIn: number;
  journal: JournalRow[];
  changes: ChangeRow[];
  shadow: ChangeRow | null;
  shadowProgress: { mainDone: number; target: number; shadow: ArmStat; main: ArmStat } | null;
  lastReview: { ts: number; tradesReviewed: number; result: { tradesReviewed: number; findings: { text: string }[]; exitLines: string[]; proposal: { label: string; before: number; after: number; why: string; evidence: string[] } | null; notes: string[] } } | null;
  efficiency: {
    fills: number;
    avgPickToFillS: number;
    failedBuys: number;
    failedTxCostSol: number;
    feesSol: number;
    grossProfitSol: number;
    feeSharePct: number | null;
    missed: number;
    notes: NoteRow[];
  };
}

export interface SourceLinks {
  website: string;
  twitter: string;
  telegram: string;
}

export interface KeyStatus {
  saved: boolean;
  hint: string;
  storage: string;
  envOverride: boolean;
  /** True in a plain browser tab: key storage only works in the desktop app. */
  unavailable?: boolean;
}

export interface ConnTest {
  ok: boolean;
  latencyMs: number | null;
  message: string;
}

export interface BuildInfo {
  version: string;
  sha: string;
  shortSha: string;
  branch: string;
  builtAt: number;
  builtAtText: string;
}

export interface UpdateCheck {
  verdict: "up-to-date" | "update-available" | "unknown";
  message: string;
  current: BuildInfo;
  latest: { sha: string; shortSha: string; url: string; kind: string; title: string; createdAt: string; createdUnix: number } | null;
  steps: string[];
  demo?: boolean;
}

export interface KillItem {
  symbol: string;
  lane: string;
  status: string;
}

export interface KillState {
  ts: number;
  killed: boolean;
  killedAtMs: number;
  closing: number;
  closed: number;
  items: KillItem[];
  text: string;
}

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
  creator: number;
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
  rentLockedSol: number;
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
  /** Coin's own website, X and Telegram from its metadata (empty until known). */
  links?: SourceLinks;
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
  /** Where to read about it (a coin's website, X or Telegram; later an X post or thesis link). */
  sourceUrl?: string | null;
}

export interface Pulse {
  id: number;
  from: AgentId;
  to: AgentId;
  text: string;
}

export type Config = typeof import("../../config.json");
