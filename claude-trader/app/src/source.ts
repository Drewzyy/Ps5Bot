import type { Config, EngineEvent } from "./types";
import defaultConfig from "../../config.json";

export interface DataSource {
  kind: "tauri" | "mock" | "ws";
  subscribe(cb: (e: EngineEvent) => void): () => void;
  getConfig(): Promise<Config>;
  /** Control command to the engine: kill switch on or trading resumed. Returns a short answer. */
  command(cmd: "kill" | "resume"): Promise<string>;
}

export const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

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
      return (await invoke(cmd === "kill" ? "kill_switch" : "resume_trading")) as string;
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
