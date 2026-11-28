import type { ConnName, ConnStatus, NotifyEvents, BackfillResult, BuildInfo, Config, ConnTest, EngineEvent, KeyStatus, StartupView, ExtTest, XSettings, UpdateCheck, UpdaterInfo, VaultSettings, VaultStatus, VaultTest } from "./types";
import { demoEvents, demoUpdate } from "./demo";
import defaultConfig from "../../config.json";

export interface DataSource {
  kind: "tauri" | "mock" | "ws";
  subscribe(cb: (e: EngineEvent) => void): () => void;
  getConfig(): Promise<Config>;
  /** Control command to the engine: kill switch on or trading resumed. Returns a short answer. */
  command(cmd: string): Promise<string>;
}

export const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Open the page with ?demo=1 to see the screens with clearly labelled DEMO data (screenshots, no engine needed). */
export const isDemo = () => typeof window !== "undefined" && new URLSearchParams(window.location.search).get("demo") === "1";

const AGENTS = [
  "scam-shield",
  "token-picker",
  "executor",
  "hold-timer",
  "thesis-finder",
  "memecoin-monitor",
  "x-monitor",
  "top-trader-monitor",
  "top-gainers",
] as const;

/** Browser-only stand-in for the engine. Idle statuses plus an occasional pulse. */
function mockSource(): DataSource {
  return {
    kind: "mock",
    subscribe(cb) {
      if (isDemo()) for (const e of demoEvents()) cb(e);
      for (const a of AGENTS) if (!(isDemo() && (a === "memecoin-monitor" || a === "x-monitor" || a === "thesis-finder"))) cb({ type: "status", agent: a, color: "grey", line: "Idle (demo)" });
      const routes: [typeof AGENTS[number], typeof AGENTS[number], string][] = [
        ["memecoin-monitor", "scam-shield", "New launch handed to Scam Shield"],
        ["scam-shield", "token-picker", "Clean token handed to Token Picker"],
        ["token-picker", "executor", "Pick handed to Executor"],
      ];
      let i = 0;
      const fire = () => {
        const [from, to, text] = routes[i++ % routes.length];
        cb({ type: "pulse", from, to, text });
      };
      const first = setTimeout(fire, 9000);
      const timer = setInterval(fire, 14000);
      return () => {
        clearTimeout(first);
        clearInterval(timer);
      };
    },
    getConfig: async () => defaultConfig as Config,
    command: async () => "demo mode: no engine is running, nothing to stop",
  };
}

function tauriSource(): DataSource {
  return {
    kind: "tauri",
    subscribe(cb) {
      let un: (() => void) | undefined;
      let dead = false;
      import("@tauri-apps/api/event").then(({ listen }) =>
        listen<EngineEvent>("engine-event", (ev) => cb(ev.payload)).then((u) => {
          if (dead) u();
          else un = u;
        })
      );
      return () => {
        dead = true;
        un?.();
      };
    },
    async getConfig() {
      const { invoke } = await import("@tauri-apps/api/core");
      return (await invoke("get_config")) as Config;
    },
    async command(cmd) {
      const { invoke } = await import("@tauri-apps/api/core");
      if (cmd === "kill") return (await invoke("kill_switch")) as string;
      if (cmd === "resume") return (await invoke("resume_trading")) as string;
      return (await invoke("learning_command", { cmd })) as string;
    },
  };
}

/** Browser mode against a running engine-cli: open the page with ?engine=8765 (or ?engine=ws://127.0.0.1:8765). */
function engineWsUrl(): string | null {
  if (typeof window === "undefined") return null;
  const q = new URLSearchParams(window.location.search).get("engine");
  if (!q) return null;
  return /^wss?:\/\//.test(q) ? q : `ws://127.0.0.1:${q}`;
}

function wsSource(url: string): DataSource {
  let current: WebSocket | null = null;
  return {
    kind: "ws",
    subscribe(cb) {
      let ws: WebSocket | null = null;
      let dead = false;
      let retry: ReturnType<typeof setTimeout> | undefined;
      const open = () => {
        ws = new WebSocket(url);
        current = ws;
        ws.onmessage = (m) => {
          try {
            cb(JSON.parse(m.data as string) as EngineEvent);
          } catch {
            /* ignore malformed frame */
          }
        };
        ws.onclose = () => {
          if (!dead) retry = setTimeout(open, 2000);
        };
      };
      open();
      return () => {
        dead = true;
        clearTimeout(retry);
        ws?.close();
      };
    },
    getConfig: async () => defaultConfig as Config,
    async command(cmd) {
      if (!current || current.readyState !== WebSocket.OPEN) return "not connected to the engine";
      current.send(JSON.stringify({ cmd }));
      return "sent";
    },
  };
}

const wsUrl = engineWsUrl();
export const source: DataSource = isTauri() ? tauriSource() : wsUrl ? wsSource(wsUrl) : mockSource();


// ------------------------------------------------------------------ calls that are not events

/** Everything the Settings screens ask the app to do. In the desktop app these run in Rust; the key never comes back to the page. */
export interface Api {
  kind: "tauri" | "demo" | "browser";
  heliusStatus(): Promise<KeyStatus>;
  heliusSave(key: string): Promise<KeyStatus>;
  heliusTest(key: string): Promise<ConnTest>;
  heliusRemove(): Promise<KeyStatus>;
  buildInfo(): Promise<BuildInfo>;
  checkUpdates(): Promise<UpdateCheck>;
  openLink(url: string): void;
  /** In-app updater: look for a signed newer version. */
  updaterCheck(): Promise<UpdaterInfo>;
  /** Close open paper positions the normal way, download, install, restart. Progress is 0..1 or null when the size is unknown. */
  updaterInstall(onStage: (stage: "closing" | "downloading" | "installing", progress: number | null) => void): Promise<void>;
  cancelUpdate(): Promise<void>;
  pickFolder(): Promise<string | null>;
  notifyStatus(): Promise<ConnStatus[]>;
  notifySave(conn: ConnName, fields: Record<string, string>): Promise<ConnStatus>;
  notifyRemove(conn: ConnName): Promise<ConnStatus>;
  notifySetEvents(conn: ConnName, events: NotifyEvents): Promise<ConnStatus>;
  notifyTest(conn: ConnName): Promise<string>;
  vaultStatus(): Promise<VaultStatus>;
  vaultConnect(path: string): Promise<VaultStatus>;
  vaultDisconnect(): Promise<VaultStatus>;
  vaultSetToggles(s: VaultSettings): Promise<VaultStatus>;
  vaultTest(): Promise<VaultTest>;
  vaultBackfill(): Promise<BackfillResult>;
  setPaperFill(values: Record<string, unknown>): Promise<string>;
  recorderStatus(): Promise<RecorderStatus>;
  setRecorder(values: { enabled?: boolean; retentionDays?: number; maxMegabytes?: number }): Promise<string>;
  learning(cmd: string): Promise<string>;
  extPairStart(): Promise<string>;
  extUnpair(): Promise<void>;
  getStartup(): Promise<StartupView>;
  setStartup(s: Omit<StartupView, "autostart" | "chromeFound" | "extensionId" | "extensionFolder" | "extensionVersion" | "extensionBundledFolder">): Promise<void>;
  setAutostart(on: boolean): Promise<boolean>;
  openChromeNow(): Promise<string>;
  openExtensionFolder(): Promise<void>;
  /** "Test connection": a ping that goes to the extension and back. */
  extTest(): Promise<ExtTest>;
  /** Ask the extension to open the pages the bot needs. */
  extOpenPages(): Promise<string>;
  getXmonSettings(): Promise<XSettings>;
  setXmonSettings(s: XSettings): Promise<void>;
  // ---- Storage (the one data folder)
  storageGet(): Promise<StorageView>;
  /** Folder picker for the data folder. */
  pickDataFolder(): Promise<string | null>;
  storageCheck(dest: string): Promise<MoveCheck>;
  /** Pauses trading, copies, verifies, switches the pointer and restarts the app. */
  storageMove(dest: string): Promise<MoveReport>;
  /** First launch: null keeps the default folder, a path chooses another (the app restarts). */
  storageFirstRun(dest: string | null): Promise<void>;
  storageDeleteOld(): Promise<{ removed: string[]; kept: string[] }>;
  storageKeepOld(): Promise<void>;
  storageShowFolder(): Promise<void>;
  storageAckExtension(): Promise<void>;
}

export interface StorageItem {
  name: string;
  path: string;
  bytes: number;
}
export interface StorageView {
  root: string;
  defaultRoot: string;
  isDefault: boolean;
  firstRun: boolean;
  warning: string | null;
  previousRoot: string | null;
  totalBytes: number;
  freeBytes: number | null;
  items: StorageItem[];
  pointerFile: string;
  extensionFolder: string;
  extensionNotice: boolean;
}
export interface MoveCheck {
  dest: string;
  kind: "empty" | "existingData" | "notEmpty";
  count?: number;
  ok: boolean;
  message: string;
  needBytes: number;
  freeBytes: number | null;
}
export interface MoveReport {
  oldRoot: string;
  newRoot: string;
  files: number;
  bytes: number;
  mode: "copied" | "existing";
}

/** Raw feed recording: settings and what is on disk. */
export interface RecorderStatus {
  enabled: boolean;
  retentionDays: number;
  maxMegabytes: number;
  files: number;
  bytes: number;
  newest: string;
  dir: string;
  off?: boolean;
}

const EXT_ID = "jihnmdnpkeglfhoombfjhdfbmiopnljj";
const DEFAULT_STARTUP: StartupView = { openChrome: true, pages: { pumpFunHome: true, dexscreenerHeld: true }, closeTabOnClose: false, keepAwake: true, autostart: false, chromeFound: true, extensionId: EXT_ID, extensionFolder: "C:\\Users\\You\\AppData\\Local\\Claude Trader\\chrome-extension", extensionVersion: "0.1.42", extensionBundledFolder: "C:\\Program Files\\Claude Trader\\extension" };

const BROWSER_MSG = "Saving a key only works in the desktop app. This page is the browser preview.";

function openInTab(url: string) {
  if (/^https?:\/\//i.test(url)) window.open(url, "_blank", "noopener,noreferrer");
}

let pendingUpdate: import("@tauri-apps/plugin-updater").Update | null = null;

function tauriApi(): Api {
  const call = async <T,>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
    const { invoke } = await import("@tauri-apps/api/core");
    return (await invoke(cmd, args)) as T;
  };
  return {
    kind: "tauri",
    heliusStatus: () => call("helius_status"),
    heliusSave: (key) => call("save_helius_key", { key }),
    heliusTest: (key) => call("test_helius_key", { key: key || null }),
    heliusRemove: () => call("remove_helius_key"),
    buildInfo: () => call("build_info"),
    checkUpdates: () => call("check_for_updates"),
    openLink: (url) => {
      call("open_link", { url }).catch(() => openInTab(url));
    },
    updaterCheck: async () => {
      const { check } = await import("@tauri-apps/plugin-updater");
      const u = await check();
      pendingUpdate = u;
      return u ? { available: true, version: u.version, notes: u.body ?? "" } : { available: false, version: "", notes: "" };
    },
    updaterInstall: async (onStage) => {
      const u = pendingUpdate;
      if (!u) throw new Error("No update is waiting. Check for updates first.");
      try {
        onStage("closing", null);
        await call("prepare_update");
        let total = 0;
        let done = 0;
        onStage("downloading", null);
        await u.downloadAndInstall((e) => {
          if (e.event === "Started") total = e.data.contentLength ?? 0;
          else if (e.event === "Progress") {
            done += e.data.chunkLength;
            onStage("downloading", total > 0 ? Math.min(1, done / total) : null);
          } else if (e.event === "Finished") onStage("installing", 1);
        });
        onStage("installing", 1);
        const { relaunch } = await import("@tauri-apps/plugin-process");
        await relaunch();
      } catch (e) {
        await call("cancel_update").catch(() => {});
        throw e;
      }
    },
    cancelUpdate: async () => {
      await call("cancel_update");
    },
    pickFolder: async () => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const r = await open({ directory: true, multiple: false, title: "Choose your Obsidian vault folder" });
      return typeof r === "string" ? r : null;
    },
    storageGet: () => call("get_storage"),
    pickDataFolder: async () => {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const r = await open({ directory: true, multiple: false, title: "Choose the folder for Claude Trader's data" });
      return typeof r === "string" ? r : null;
    },
    storageCheck: (dest) => call("storage_check", { dest }),
    storageMove: (dest) => call("storage_move", { dest }),
    storageFirstRun: (dest) => call("storage_first_run", { dest }),
    storageDeleteOld: () => call("storage_delete_old"),
    storageKeepOld: () => call("storage_keep_old"),
    storageShowFolder: () => call("storage_show_folder"),
    storageAckExtension: () => call("storage_ack_extension"),
    notifyStatus: () => call("notify_status"),
    notifySave: (conn, fields) => call("notify_save", { conn, fields }),
    notifyRemove: (conn) => call("notify_remove", { conn }),
    notifySetEvents: (conn, events) => call("notify_set_events", { conn, events }),
    notifyTest: (conn) => call("notify_test", { conn }),
    vaultStatus: () => call("vault_status"),
    vaultConnect: (path) => call("vault_connect", { path }),
    vaultDisconnect: () => call("vault_disconnect"),
    vaultSetToggles: (settings) => call("vault_set_toggles", { settings }),
    vaultTest: () => call("vault_test"),
    vaultBackfill: () => call("vault_backfill"),
    setPaperFill: (v) => call("set_paper_fill", { values: JSON.stringify(v) }),
    recorderStatus: async () => JSON.parse(await call<string>("recorder_status")) as RecorderStatus,
    setRecorder: (v) => call("set_recorder", { values: JSON.stringify(v) }),
    learning: (cmd) => call("learning_command", { cmd }),
    extPairStart: () => call("ext_pair_start"),
    extUnpair: () => call("ext_unpair"),
    getStartup: () => call("get_startup"),
    setStartup: (settings) => call("set_startup", { settings }),
    setAutostart: (on) => call("set_autostart", { on }),
    openChromeNow: () => call("open_chrome_now"),
    openExtensionFolder: () => call("open_extension_folder"),
    extTest: async () => JSON.parse(await call<string>("ext_test")) as ExtTest,
    extOpenPages: () => call("ext_open_pages"),
    getXmonSettings: () => call("get_xmon_settings"),
    setXmonSettings: (settings) => call("set_xmon_settings", { settings }),
  };
}

/** Demo and browser preview: nothing is stored anywhere. Demo pretends so the screens can be shown. */
let demoVault: VaultStatus = { settings: { path: "", trades: true, coins: true, days: true, lessons: true, situations: true }, folderExists: false, looksLikeObsidian: false, notesFolder: "", lastError: "" };

const ALL_ON: NotifyEvents = { buy: true, sell: true, dailySummary: true, learningChange: true, killSwitch: true, lossLimit: true, health: true, updateInstalled: true };
const demoConns: ConnStatus[] = (["discord", "webhook"] as ConnName[]).map((conn) => ({ conn, configured: false, hints: {}, events: { ...ALL_ON }, lastError: "", lastOkMs: 0 }));

function localApi(demo: boolean): Api {
  let saved = "";
  let startup: StartupView = { ...DEFAULT_STARTUP, pages: { ...DEFAULT_STARTUP.pages } };
  let xset: XSettings = { enabled: true, paused: false, minGapSecs: 8, maxGapSecs: 15, maxLoadsPerHour: 120, cacheMinutes: 30 };
  const needApp = () => {
    if (!demo) throw new Error("This only works in the desktop app.");
  };
  const status = (): KeyStatus =>
    saved
      ? { saved: true, hint: `saved, ends in ...${saved.slice(-4)}`, storage: "Windows Credential Manager (demo)", envOverride: false }
      : { saved: false, hint: "No key saved", storage: "", envOverride: false, unavailable: !demo };
  return {
    kind: demo ? "demo" : "browser",
    heliusStatus: async () => status(),
    heliusSave: async (key) => {
      if (!demo) throw new Error(BROWSER_MSG);
      if (key.trim().length < 12) throw new Error("That does not look like a Helius key. It is usually about 36 characters.");
      saved = key.trim();
      return status();
    },
    heliusTest: async (key) => {
      if (!demo) return { ok: false, latencyMs: null, message: BROWSER_MSG };
      return key.trim() || saved ? { ok: true, latencyMs: 142, message: "OK. Helius answered in 142 ms. (DEMO)" } : { ok: false, latencyMs: null, message: "Paste your Helius API key first." };
    },
    heliusRemove: async () => {
      saved = "";
      return status();
    },
    buildInfo: async () => ({ version: demo ? "0.1.42" : "0.1.0", sha: "unknown", shortSha: demo ? "a1b2c3d" : "unknown", branch: "", builtAt: 0, builtAtText: demo ? "2026-11-27 (DEMO)" : "unknown" }),
    checkUpdates: async () => {
      if (demo) return demoUpdate();
      throw new Error("Update checks work in the desktop app.");
    },
    openLink: openInTab,
    // Demo: ?upd=none (up to date), ?upd=error, ?upd=downloading (frozen at 42%), ?upd=closing, otherwise an update is ready.
    updaterCheck: async () => {
      if (!demo) throw new Error("Updates work in the desktop app.");
      const m = new URLSearchParams(window.location.search).get("upd");
      if (m === "none") return { available: false, version: "", notes: "" };
      if (m === "error") throw new Error("Could not fetch a valid release JSON from the remote (HTTP 404)");
      return { available: true, version: "0.1.43", notes: "DEMO release notes" };
    },
    updaterInstall: async (onStage) => {
      const m = new URLSearchParams(window.location.search).get("upd");
      if (m === "closing") {
        onStage("closing", null);
        return new Promise<void>(() => {});
      }
      onStage("downloading", 0);
      if (m === "downloading") {
        onStage("downloading", 0.42);
        return new Promise<void>(() => {});
      }
      for (let i = 1; i <= 20; i++) {
        await new Promise((r) => setTimeout(r, 200));
        onStage("downloading", i / 20);
      }
      onStage("installing", 1);
      throw new Error("DEMO: nothing was installed and the app did not restart.");
    },
    cancelUpdate: async () => {},
    storageGet: async () => {
      if (!demo) throw new Error("Storage is shown in the desktop app.");
      return demoStorage();
    },
    pickDataFolder: async () => (demo ? "D:\\ClaudeTraderData" : null),
    storageCheck: async (dest) => demoMoveCheck(dest),
    storageMove: async (dest) => {
      needApp();
      const c = demoMoveCheck(dest);
      if (!c.ok) throw new Error(c.message);
      await new Promise((r) => setTimeout(r, 900));
      throw new Error("DEMO: nothing was moved.");
    },
    storageFirstRun: async () => {
      needApp();
    },
    storageDeleteOld: async () => {
      needApp();
      return { removed: ["claude-trader.db", "paper-logs", "recordings"], kept: [] };
    },
    storageKeepOld: async () => {},
    storageShowFolder: async () => {},
    storageAckExtension: async () => {},
    pickFolder: async () => (demo ? "C:\\Users\\You\\Documents\\My Vault" : null),
    notifyStatus: async () => demoConns,
    notifySave: async (conn, fields) => {
      if (!demo) throw new Error("Connectors work in the desktop app.");
      const c = demoConns.find((x) => x.conn === conn)!;
      for (const [k, v] of Object.entries(fields)) if (v.trim()) c.hints[k] = k.endsWith("url") ? "saved, discord.com/...a1b2" : "saved, ends in ...a1b2";
      c.configured = Object.keys(c.hints).some((k) => k.endsWith("url"));
      return { ...c };
    },
    notifyRemove: async (conn) => {
      const c = demoConns.find((x) => x.conn === conn)!;
      c.hints = {};
      c.configured = false;
      return { ...c };
    },
    notifySetEvents: async (conn, events) => {
      const c = demoConns.find((x) => x.conn === conn)!;
      c.events = events;
      return { ...c };
    },
    notifyTest: async (conn) => {
      if (!demo || !demoConns.find((x) => x.conn === conn)?.configured) throw new Error("Save the connection details first.");
      return "Test message sent. Check the channel. (DEMO: nothing was really sent)";
    },
    vaultStatus: async () => demoVault,
    vaultConnect: async (path) => {
      if (!demo) throw new Error("Connectors work in the desktop app.");
      demoVault = { ...demoVault, settings: { ...demoVault.settings, path }, folderExists: true, looksLikeObsidian: true, notesFolder: `${path}\\Claude Trader` };
      return demoVault;
    },
    vaultDisconnect: async () => {
      demoVault = { ...demoVault, settings: { ...demoVault.settings, path: "" }, folderExists: false, looksLikeObsidian: false, notesFolder: "" };
      return demoVault;
    },
    vaultSetToggles: async (settings) => {
      demoVault = { ...demoVault, settings: { ...settings, path: demoVault.settings.path } };
      return demoVault;
    },
    vaultTest: async () => (demo && demoVault.settings.path ? { ok: true, message: "OK. Wrote and read back Claude Trader/Test note.md. Open Obsidian and look for the Claude Trader folder. (DEMO)" } : { ok: false, message: "Choose your vault folder first." }),
    vaultBackfill: async () => {
      if (!demo || !demoVault.settings.path) throw new Error("Choose your vault folder first.");
      return { trades: 14, days: 3, coins: 12, lessons: 4, situations: 2, keptYours: 0 };
    },
    setPaperFill: async () => {
      if (!demo) throw new Error("Saving settings only works in the desktop app.");
      return "saved";
    },
    recorderStatus: async () => {
      needApp();
      return { enabled: true, retentionDays: 14, maxMegabytes: 2000, files: 3, bytes: 41_000_000, newest: "2026-11-28", dir: "C:\\Users\\you\\AppData\\Roaming\\claude-trader\\recordings" };
    },
    setRecorder: async () => {
      needApp();
      return "saved";
    },
    learning: async (cmd) => {
      if (!demo) throw new Error("This only works in the desktop app.");
      return `DEMO: ${cmd} (nothing changed)`;
    },
    extPairStart: async () => {
      needApp();
      return "4821-9037";
    },
    extUnpair: async () => needApp(),
    getStartup: async () => startup,
    setStartup: async (v) => {
      needApp();
      startup = { ...startup, ...v };
    },
    setAutostart: async (on) => {
      needApp();
      startup = { ...startup, autostart: on };
      return on;
    },
    openChromeNow: async () => {
      needApp();
      return "DEMO: Chrome is starting.";
    },
    openExtensionFolder: async () => needApp(),
    // DEMO: ?ext=offline answers with the error, anything else with a made-up latency
    extTest: async () => {
      needApp();
      await new Promise((r) => setTimeout(r, 450));
      return new URLSearchParams(window.location.search).get("ext") === "offline" ? { ok: false, error: "The extension is not connected." } : { ok: true, latencyMs: 14, extVersion: "0.1.42" };
    },
    extOpenPages: async () => {
      needApp();
      return "DEMO: asked the extension to open the missing pages.";
    },
    getXmonSettings: async () => xset,
    setXmonSettings: async (s) => {
      needApp();
      if (s.minGapSecs < 5 || s.maxGapSecs < s.minGapSecs || s.maxLoadsPerHour < 1 || s.maxLoadsPerHour > 200) throw new Error("Those pacing numbers are outside the safe range.");
      xset = { ...s };
    },
  };
}

const demoQ = (k: string) => new URLSearchParams(window.location.search).get(k);
const DEMO_ROOT = "C:\\Users\\You\\AppData\\Roaming\\app.claudetrader.desktop";

function demoStorage(): StorageView {
  const moved = demoQ("moved") === "1";
  const root = moved ? "D:\\ClaudeTraderData" : DEMO_ROOT;
  const items: StorageItem[] = [
    { name: "Database", path: `${root}\\claude-trader.db`, bytes: 18_400_000 },
    { name: "Paper trade logs", path: `${root}\\paper-logs`, bytes: 1_250_000 },
    { name: "Raw feed recordings", path: `${root}\\recordings`, bytes: 410_000_000 },
    { name: "Encrypted key file", path: `${root}\\helius.key.enc`, bytes: 0 },
    { name: "Chrome extension", path: `${root}\\chrome-extension`, bytes: 310_000 },
    { name: "Window cache", path: `${root}\\webview`, bytes: 42_000_000 },
  ];
  return {
    root,
    defaultRoot: DEMO_ROOT,
    isDefault: !moved,
    firstRun: demoQ("first") === "1",
    warning: null,
    previousRoot: moved ? DEMO_ROOT : null,
    totalBytes: items.reduce((a, b) => a + b.bytes, 0),
    freeBytes: 612_000_000_000,
    items,
    pointerFile: `${DEMO_ROOT}\\data-location.json`,
    extensionFolder: `${root}\\chrome-extension`,
    extensionNotice: moved,
  };
}

function demoMoveCheck(dest: string): MoveCheck {
  const base = { dest, needBytes: 520_000_000, freeBytes: 612_000_000_000 as number | null };
  if (dest.toLowerCase().includes("photos")) return { ...base, kind: "notEmpty", count: 3, ok: false, message: `${dest} is not empty (3 items). Pick an empty folder, or make a new one such as D:\\ClaudeTraderData. To use data from an earlier install, pick the folder that holds claude-trader.db.` };
  if (dest.toLowerCase().includes("older")) return { ...base, kind: "existingData", ok: true, message: `${dest} already holds Claude Trader data. The app will switch to it. Your current data stays where it is until you delete it.` };
  return { ...base, kind: "empty", ok: true, message: `472.0 MB of data will be copied to ${dest}.` };
}

function wsApi(): Api {
  const base = localApi(false);
  return {
    ...base,
    setPaperFill: async (v) => (await source.command(`set-paper-fill:${JSON.stringify(v)}`)),
    // this browser link only sends commands (no answer comes back), so the result is in the engine log; the desktop app shows it
    extTest: async () => {
      await source.command("ext-test");
      return { ok: false, error: "The ping was sent. Its result is in the engine log here, and on screen in the desktop app." };
    },
    extOpenPages: async () => await source.command("ext-open-pages"),
    recorderStatus: async () => JSON.parse(await source.command("recorder-status")) as RecorderStatus,
    setRecorder: async (v) => (await source.command(`set-recorder:${JSON.stringify(v)}`)),
    learning: async (cmd) => (await source.command(cmd)),
  };
}

export const api: Api = isTauri() ? tauriApi() : isDemo() ? localApi(true) : wsUrl ? wsApi() : localApi(false);
