import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { api } from "./source";

export type UpdPhase = "idle" | "checking" | "uptodate" | "ready" | "confirm" | "closing" | "downloading" | "installing" | "error";

export interface UpdState {
  phase: UpdPhase;
  /** Version running now. */
  current: string;
  /** Version that is ready (or being installed). */
  version: string;
  /** 0..1, or null when the download size is unknown. */
  progress: number | null;
  message: string;
  /** The updater could not work: show the old GitHub download page as a fallback. */
  fallback: boolean;
}

export interface Updater {
  state: UpdState;
  /** The Update icon: check, and when something is ready go on to install (after the open-position warning). */
  click: () => void;
  /** Settings button "Check for updates": only checks. */
  check: () => void;
  /** "Update and restart". */
  start: () => void;
  confirm: () => void;
  cancel: () => void;
  dismiss: () => void;
  openPositions: number;
}

const SIX_HOURS = 6 * 60 * 60 * 1000;

const clean = (e: unknown) => String(e instanceof Error ? e.message : e);

const Ctx = createContext<Updater | null>(null);
export const useUpdater = () => useContext(Ctx)!;

export function UpdaterProvider({ openPositions, children }: { openPositions: number; children: ReactNode }) {
  const [state, setState] = useState<UpdState>({ phase: "idle", current: "", version: "", progress: null, message: "", fallback: false });
  const busy = useRef(false);
  const posRef = useRef(openPositions);
  posRef.current = openPositions;
  const set = useCallback((p: Partial<UpdState>) => setState((s) => ({ ...s, ...p })), []);

  const currentVersion = useCallback(async () => {
    try {
      return (await api.buildInfo()).version;
    } catch {
      return "";
    }
  }, []);

  /** Look for an update. `quiet` (startup and every 6 hours) never shows a spinner or an error. Returns true when one is ready. */
  const doCheck = useCallback(
    async (quiet: boolean): Promise<boolean> => {
      if (busy.current) return false;
      busy.current = true;
      if (!quiet) set({ phase: "checking", message: "", fallback: false });
      try {
        const cur = await currentVersion();
        const r = await api.updaterCheck();
        if (r.available) {
          setState((s) => (["downloading", "installing", "closing", "confirm"].includes(s.phase) ? s : { ...s, phase: "ready", current: cur, version: r.version, message: "", fallback: false }));
          return true;
        }
        if (!quiet) set({ phase: "uptodate", current: cur, version: "", message: `Up to date (v${cur})` });
        else setState((s) => (s.phase === "ready" ? { ...s, phase: "idle" } : s));
        return false;
      } catch (e) {
        if (!quiet) {
          const raw = clean(e);
          const notSet = /404|valid release|not found|signature|pubkey|endpoint/i.test(raw);
          set({
            phase: "error",
            current: await currentVersion(),
            message: notSet ? "Automatic updates are not switched on yet (no signed release is published). Use the download page instead." : `Could not check for updates: ${raw}`,
            fallback: true,
          });
        }
        return false;
      } finally {
        busy.current = false;
      }
    },
    [currentVersion, set],
  );

  const run = useCallback(async () => {
    const open = posRef.current;
    set({ phase: open > 0 ? "closing" : "downloading", progress: null, message: "", fallback: false });
    try {
      await api.updaterInstall((stage, progress) => set({ phase: stage, progress }));
    } catch (e) {
      set({ phase: "error", message: `The update did not install: ${clean(e)}. Trading carries on. You can use the download page instead.`, fallback: true, progress: null });
    }
  }, [set]);

  const start = useCallback(() => {
    if (posRef.current > 0) set({ phase: "confirm" });
    else void run();
  }, [run, set]);

  const click = useCallback(() => {
    setState((s) => {
      if (["checking", "closing", "downloading", "installing", "confirm"].includes(s.phase)) return s;
      if (s.phase === "ready") {
        queueMicrotask(start);
        return s;
      }
      void doCheck(false).then((found) => {
        if (found) start();
      });
      return s;
    });
  }, [doCheck, start]);

  const cancel = useCallback(() => {
    void api.cancelUpdate().catch(() => {});
    set({ phase: "ready" });
  }, [set]);

  // quiet check at startup and every 6 hours; the person still has to click to install
  useEffect(() => {
    const t0 = setTimeout(() => void doCheck(true), 4000);
    const t = setInterval(() => void doCheck(true), SIX_HOURS);
    return () => {
      clearTimeout(t0);
      clearInterval(t);
    };
  }, [doCheck]);

  // "Up to date" is a brief toast
  useEffect(() => {
    if (state.phase !== "uptodate") return;
    const t = setTimeout(() => setState((s) => (s.phase === "uptodate" ? { ...s, phase: "idle" } : s)), 4000);
    return () => clearTimeout(t);
  }, [state.phase]);

  const value: Updater = {
    state,
    click,
    check: () => void doCheck(false),
    start,
    confirm: () => void run(),
    cancel,
    dismiss: () => set({ phase: "idle", message: "" }),
    openPositions,
  };
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
