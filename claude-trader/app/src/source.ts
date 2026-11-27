import type { BuildInfo, Config, ConnTest, EngineEvent, KeyStatus, UpdateCheck } from "./types";
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
      for (const a of AGENTS) cb({ type: "status", agent: a, color: "grey", line: "Idle (demo)" });
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
  setPaperFill(values: Record<string, unknown>): Promise<string>;
  learning(cmd: string): Promise<string>;
}

const BROWSER_MSG = "Saving a key only works in the desktop app. This page is the browser preview.";

function openInTab(url: string) {
  if (/^https?:\/\//i.test(url)) window.open(url, "_blank", "noopener,noreferrer");
}

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
    setPaperFill: (v) => call("set_paper_fill", { values: JSON.stringify(v) }),
    learning: (cmd) => call("learning_command", { cmd }),
  };
}

/** Demo and browser preview: nothing is stored anywhere. Demo pretends so the screens can be shown. */
function localApi(demo: boolean): Api {
  let saved = "";
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
    buildInfo: async () => ({ version: "0.1.0", sha: "unknown", shortSha: demo ? "a1b2c3d" : "unknown", branch: "", builtAt: 0, builtAtText: demo ? "2026-11-27 (DEMO)" : "unknown" }),
    checkUpdates: async () => {
      if (demo) return demoUpdate();
      throw new Error("Update checks work in the desktop app.");
    },
    openLink: openInTab,
    setPaperFill: async () => {
      if (!demo) throw new Error("Saving settings only works in the desktop app.");
      return "saved";
    },
    learning: async (cmd) => {
      if (!demo) throw new Error("This only works in the desktop app.");
      return `DEMO: ${cmd} (nothing changed)`;
    },
  };
}

function wsApi(): Api {
  const base = localApi(false);
  return {
    ...base,
    setPaperFill: async (v) => (await source.command(`set-paper-fill:${JSON.stringify(v)}`)),
    learning: async (cmd) => (await source.command(cmd)),
  };
}

export const api: Api = isTauri() ? tauriApi() : isDemo() ? localApi(true) : wsUrl ? wsApi() : localApi(false);
