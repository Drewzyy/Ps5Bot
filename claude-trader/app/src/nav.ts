// Open a Settings section from anywhere (the top-bar chip, the update message). The open sections are remembered in localStorage;
// this adds one, goes to Settings, and the Settings page scrolls to it.
export const OPEN_KEY = "claudeTrader.settingsOpen.v1";
export const SECTION_EVENT = "ct-open-section";

export function openSection(id: string): void {
  try {
    const v = JSON.parse(localStorage.getItem(OPEN_KEY) ?? "[]");
    const list: string[] = Array.isArray(v) ? v.filter((x) => typeof x === "string") : [];
    if (!list.includes(id)) localStorage.setItem(OPEN_KEY, JSON.stringify([...list, id]));
  } catch {
    /* storage blocked: the section just does not stay open */
  }
  if (window.location.hash !== "#/settings") window.location.hash = "#/settings";
  window.dispatchEvent(new CustomEvent(SECTION_EVENT, { detail: id }));
  // the Settings page may still be mounting: scroll after it has drawn
  setTimeout(() => document.querySelector(`[data-section="${id}"]`)?.scrollIntoView({ behavior: "smooth", block: "start" }), 120);
}
