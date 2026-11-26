import { useEffect, useState } from "react";
import { source } from "./source";
import { useEngine } from "./useEngine";
import type { Config } from "./types";
import defaultConfig from "../../config.json";
import TopBar from "./components/TopBar";
import Graph from "./components/Graph";
import Feed from "./components/Feed";
import Widgets from "./components/Widgets";
import SettingsPage from "./components/SettingsPage";
import { TipsPage, Walkthrough, markWalkthroughSeen, walkthroughSeen } from "./tips/Tips";

export type Page = "dashboard" | "settings" | "tips";

const pageFromHash = (): Page => (window.location.hash === "#/settings" ? "settings" : window.location.hash === "#/tips" ? "tips" : "dashboard");

export default function App() {
  const [page, setPage] = useState<Page>(pageFromHash());
  const [config, setConfig] = useState<Config>(defaultConfig as Config);
  const engine = useEngine();
  const [tour, setTour] = useState(() => !walkthroughSeen());
  const closeTour = () => {
    markWalkthroughSeen();
    setTour(false);
  };

  useEffect(() => {
    source.getConfig().then(setConfig).catch(() => {});
    const onHash = () => setPage(pageFromHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  const go = (p: Page) => {
    window.location.hash = p === "settings" ? "#/settings" : p === "tips" ? "#/tips" : "#/";
    setPage(p);
  };

  return (
    <>
      <TopBar page={page} go={go} mode={config.mode} bankroll={config.paper.bankrollSol} stats={engine.stats} />
      <main className="page">
        {page === "dashboard" ? (
          <>
            <Graph engine={engine} config={config} openSettings={() => go("settings")} />
            <div className="section-title">Activity</div>
            <Feed items={engine.activity} />
            <div className="section-title">Widgets</div>
            <Widgets config={config} openSettings={() => go("settings")} engine={engine} />
          </>
        ) : page === "tips" ? (
          <TipsPage config={config} replay={() => { go("dashboard"); setTour(true); }} />
        ) : (
          <SettingsPage config={config} />
        )}
      </main>
      {tour && page === "dashboard" && <Walkthrough config={config} onClose={closeTour} />}
    </>
  );
}
