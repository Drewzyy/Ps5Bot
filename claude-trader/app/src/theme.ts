// Appearance preference: UI only, never part of the trading config.
export type ThemePref = "light" | "dark" | "system";
export const THEME_KEY = "claude-trader.theme";

export function readThemePref(): ThemePref {
  try {
    const v = localStorage.getItem(THEME_KEY);
    if (v === "light" || v === "dark" || v === "system") return v;
  } catch { /* storage unavailable: fall back to light */ }
  return "light";
}

export function resolveTheme(pref: ThemePref): "light" | "dark" {
  if (pref === "system") {
    try { return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light"; } catch { return "light"; }
  }
  return pref;
}

export function applyTheme(pref: ThemePref) {
  document.documentElement.setAttribute("data-theme", resolveTheme(pref));
}

export function saveThemePref(pref: ThemePref) {
  try { localStorage.setItem(THEME_KEY, pref); } catch { /* ignore */ }
  applyTheme(pref);
}
