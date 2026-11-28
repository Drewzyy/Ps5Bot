// What the top-bar chip and Settings say about the Chrome extension. Pure functions, tested in extStatus.test.ts.
import type { ExtensionStatus, PageStatus } from "./types";

export type ChipTone = "on" | "gray" | "off";

export interface Chip {
  tone: ChipTone;
  text: string;
  /** Longer line for the tooltip. */
  hint: string;
  /** A small "!" next to the text: connected, but it needs a look. */
  attention: boolean;
}

/** null until the first status arrives (the chip is hidden then). */
export function extChip(ext: ExtensionStatus | null): Chip | null {
  if (!ext) return null;
  if (!ext.paired) return { tone: "gray", text: "Extension not paired", hint: "Open Settings, Startup and browser to pair the Chrome extension.", attention: false };
  if (!ext.connected) return { tone: "off", text: "Extension offline", hint: "Chrome is closed or the extension is off. Click for details.", attention: false };
  if (ext.link === "manual") return { tone: "on", text: "Extension connected", hint: "It runs old files. Reload it from the new folder once (Settings, Startup and browser).", attention: true };
  return { tone: "on", text: "Extension connected", hint: ext.version ? `Extension v${ext.version}` : "The Chrome extension is connected.", attention: false };
}

/** "3 s ago", "2 min ago", "never". */
export function ago(thenMs: number, nowMs: number): string {
  if (!thenMs) return "never";
  const s = Math.max(0, Math.round((nowMs - thenMs) / 1000));
  if (s < 60) return `${s} s ago`;
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  return `${Math.floor(s / 3600)} h ago`;
}

/** Plain line about the versions. */
export function versionLine(ext: ExtensionStatus): string {
  const v = ext.version ? `v${ext.version}` : "not reported yet";
  const d = ext.diskVersion ? `v${ext.diskVersion}` : "none yet";
  const a = ext.appVersion ? `v${ext.appVersion}` : "unknown";
  return `Extension ${v}, files on disk ${d}, app ${a}`;
}

/** The checklist counts, for a short heading. Pages that are off do not count. */
export function pagesSummary(pages: PageStatus[]): { need: number; ok: number; missing: number; loggedOut: number } {
  const need = pages.filter((p) => p.state !== "off");
  return { need: need.length, ok: need.filter((p) => p.state === "open").length, missing: need.filter((p) => p.state === "missing").length, loggedOut: need.filter((p) => p.state === "loggedout").length };
}

export const PAGE_STATE_TEXT: Record<PageStatus["state"], string> = { open: "Open", missing: "Missing", loggedout: "Logged out", off: "Off" };

/** Is "Extension updated to vX" still worth showing? */
export function updatedNotice(ext: ExtensionStatus): string {
  return ext.updatedTo ? `Extension updated to v${ext.updatedTo}` : "";
}
