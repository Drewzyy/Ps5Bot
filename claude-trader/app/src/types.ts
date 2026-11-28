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
  | ({ type: "extension"; ts: number } & ExtensionStatus)
  | ({ type: "feed-health" } & FeedHealth)
  | ({ type: "funnel" } & Funnel)
  | ({ type: "x-monitor" } & XMonitorView);

/** Mirrors engine/src/xmon/mod.rs XMonitorView: X Monitor and Thesis Finder. */
export interface XMonitorView {
  ts: number;
  enabled: boolean;
  paused: boolean;
  mode: string;
  /** working | waiting | paused | not-logged-in | off */
  state: string;
  statusLine: string;
  thesisLine: string;
  extAlive: boolean;
  loggedIn: boolean | null;
  pausedReason: string;
  pausedUntilMs: number;
  loadsLastHour: number;
  maxPerHour: number;
  jobs: { queued: number; running: number; done: number; failed: number; stale: number };
  checkedHour: number;
  rising: XRising[];
  latest: XCheckRow[];
  stories: XStory[];
  narratives: XNarrative[];
  /** True for seeded screenshot data. */
  demo?: boolean;
}

export interface XRising {
  handle: string;
  name: string;
  url: string;
  followers: number;
  gain: number;
  gainPct: number;
  minutes: number;
  why: string;
  symbol: string;
}

export interface XCheckRow {
  ts: number;
  mint: string;
  symbol: string;
  kind: string;
  handle: string;
  url: string;
  linkLabel: string;
  /** ready | unknown | blocked | nolink | off */
  status: string;
  score: number;
  points: number;
  reason: string;
  flags: string[];
  blocks: string[];
  rank: number;
  clones: number;
}

export interface XStory {
  mint: string;
  symbol: string;
  name: string;
  ts: number;
  kind: string;
  url: string;
  handle: string;
  author: string;
  text: string;
  reach: number;
  views: number;
  tweetAgeMin: number | null;
  rank: number;
  clones: number;
  cluster: string;
}

export interface XNarrative {
  key: string;
  label: string;
  kind: string;
  url: string;
  count: number;
  symbols: string[];
  firstMs: number;
  lastMs: number;
  text: string;
}

/** Mirrors engine/src/xmon/mod.rs XSettings (the user's X Monitor choices). */
export interface XSettings {
  enabled: boolean;
  paused: boolean;
  minGapSecs: number;
  maxGapSecs: number;
  maxLoadsPerHour: number;
  cacheMinutes: number;
}

/** Mirrors engine/src/events.rs FeedHealthView: is buy/sell data reaching the Token Picker? */
export interface FeedHealth {
  ts: number;
  coinsConnected: boolean;
  tradesConnected: boolean;
  watched: number;
  subsActive: number;
  tradesLastMin: number;
  tradesTotal: number;
  lastTradeMs: number;
  lastError: string;
  /** Plain warning, empty when all is well. */
  warn: string;
  rugRequests: number;
  rugFailures: number;
}

/** Mirrors engine/src/events.rs FunnelView: where coins dropped off today. */
export interface Funnel {
  ts: number;
  seen: number;
  blocked: number;
  blockedReasons: { reason: string; count: number }[];
  inProgress: number;
  scored: number;
  scoredNoTrades: number;
  bestScore: number;
  bestSymbol: string;
  minScore: number;
  bestBlockedBy: { reason: string; count: number }[];
  picked: number;
  bought: number;
  rugFallbackPassed: number;
  rugFallbackBlocked: number;
  /** True for seeded screenshot data. */
  demo?: boolean;
}

/** One line of the "pages the bot needs" checklist (engine/src/events.rs PageStatus). */
export interface PageStatus {
  id: string;
  label: string;
  state: "open" | "missing" | "loggedout" | "off";
  url: string;
}

/** Chrome extension link (EngineEvent::Extension without the type and time). */
export interface ExtensionStatus {
  paired: boolean;
  connected: boolean;
  lastSeenMs: number;
  /** Version the extension reported. */
  version: string;
  /** Version of the extension files in the stable folder. */
  diskVersion: string;
  /** This app's version. */
  appVersion: string;
  /** Last time the extension said anything (ms). 0 = not since the app started. */
  heartbeatMs: number;
  /** "ok", "reloading" (the app asked it to reload) or "manual" (load it from the stable folder once). */
  link: "ok" | "reloading" | "manual";
  /** Set for 30 minutes after the extension came back with a newer version. */
  updatedTo: string;
  updatedAt: number;
  pages: PageStatus[];
  pagesTs: number;
}

/** Answer of "Test connection". */
export interface ExtTest {
  ok: boolean;
  latencyMs?: number;
  extVersion?: string;
  error?: string;
}

/** Mirrors engine/src/startup.rs plus what the app adds. */
export interface StartupView {
  openChrome: boolean;
  pages: { pumpFunHome: boolean; dexscreenerHeld: boolean };
  closeTabOnClose: boolean;
  keepAwake: boolean;
  autostart: boolean;
  chromeFound: boolean;
  extensionId: string;
  /** The stable folder to load in Chrome (in the app data, it never moves). */
  extensionFolder: string;
  /** Version of the files in that folder. */
  extensionVersion: string;
  /** The copy inside the install folder. Do not load this one. */
  extensionBundledFolder: string;
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

export interface UpdaterInfo {
  available: boolean;
  version: string;
  notes: string;
}

export interface VaultSettings {
  path: string;
  trades: boolean;
  coins: boolean;
  days: boolean;
  lessons: boolean;
  situations: boolean;
}

export interface VaultStatus {
  settings: VaultSettings;
  folderExists: boolean;
  looksLikeObsidian: boolean;
  notesFolder: string;
  lastError: string;
}

export interface VaultTest {
  ok: boolean;
  message: string;
}

export interface BackfillResult {
  trades: number;
  days: number;
  coins: number;
  lessons: number;
  situations: number;
  keptYours: number;
}

export interface NotifyEvents {
  buy: boolean;
  sell: boolean;
  dailySummary: boolean;
  learningChange: boolean;
  killSwitch: boolean;
  lossLimit: boolean;
  health: boolean;
  updateInstalled: boolean;
}

export type ConnName = "discord" | "webhook";

/** What a connector card may know. Secrets never come back: only masked hints. */
export interface ConnStatus {
  conn: ConnName;
  configured: boolean;
  hints: Record<string, string>;
  events: NotifyEvents;
  lastError: string;
  lastOkMs: number;
}
