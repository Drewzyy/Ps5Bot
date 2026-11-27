import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";
import { applyTheme, readThemePref } from "./theme";
import { api, isTauri } from "./source";

applyTheme(readThemePref());
window.matchMedia?.("(prefers-color-scheme: dark)").addEventListener?.("change", () => {
  if (readThemePref() === "system") applyTheme("system");
});

// In the desktop app a plain link would stay inside the window, so every web link opens in the real browser instead.
if (isTauri()) {
  document.addEventListener("click", (ev) => {
    const a = (ev.target as HTMLElement | null)?.closest?.("a[href]") as HTMLAnchorElement | null;
    if (a && /^https?:\/\//i.test(a.href)) {
      ev.preventDefault();
      api.openLink(a.href);
    }
  });
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
