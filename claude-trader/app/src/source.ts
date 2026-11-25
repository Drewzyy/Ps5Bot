import type { Config, EngineEvent } from "./types";
import defaultConfig from "../../config.json";

export interface DataSource {
  kind: "tauri" | "mock";
  subscribe(cb: (e: EngineEvent) => void): () => void;
  getConfig(): Promise<Config>;
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
  };
}

export const source: DataSource = isTauri() ? tauriSource() : mockSource();
