import { useEffect, useState } from "react";
import { source, isDemo } from "./source";
import { useEngine } from "./useEngine";
import type { Config } from "./types";
import defaultConfig from "../../config.json";
import TopBar from "./components/TopBar";
import Graph from "./components/Graph";
import Feed from "./components/Feed";
import Widgets from "./components/Widgets";
import SettingsPage from "./components/SettingsPage";
import KillUi, { KillBanner } from "./components/Kill";
import { FeedWarningBar } from "./components/Health";
import { Walkthrough, markWalkthroughSeen, walkthroughSeen } from "./tips/Tips";
import { UpdaterProvider } from "./updater";
import UpdateOverlay from "./components/UpdateUi";

export type Page = "dashboard" | "settings";

// the old #/tips address now opens Settings (Tips live there)
const pageFromHash = (): Page => (window.location.hash === "#/settings" || window.location.hash === "#/tips" ? "settings" : "dashboard");

export default function App() {
  const [page, setPage] = useState<Page>(pageFromHash());
  const [config, setConfig] = useState<Config>(defaultConfig as Config);
  const engine = useEngine();
  const [ask, setAsk] = useState<"kill" | "resume" | null>(null);
  const [tour, setTour] = useState(() => !walkthroughSeen());
  const closeTour = () => {
    markWalkthroughSeen();
    setTour(false);
  };

  useEffect(() => {
    source.getConfig().then(setConfig).catch(() => {});
    const onHash = () => setPage(pageFromHash());
    window.addEventListener("hashchange", onHash);
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setAsk("kill");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("hashchange", onHash);
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  const go = (p: Page) => {
    window.location.hash = p === "settings" ? "#/settings" : "#/";
    setPage(p);
  };

  // ?demo=1&upd=confirm pretends one position is open, to show the warning dialog
  const demoOpen = isDemo() && new URLSearchParams(window.location.search).get("upd") === "confirm" ? 1 : 0;
  const openPositions = demoOpen || engine.positions.filter((p) => p.state !== "CLOSED" && !p.replay).length;

  return (
    <UpdaterProvider openPositions={openPositions}>
      <TopBar page={page} go={go} mode={config.mode} bankroll={config.paper.bankrollSol} stats={engine.stats} kill={engine.kill} extension={engine.extension} askKill={() => setAsk("kill")} askResume={() => setAsk("resume")} />
      <KillBanner kill={engine.kill} />
      <FeedWarningBar feed={engine.feed} />
      <KillUi ask={ask} close={() => setAsk(null)} killed={!!engine.kill?.killed} />
      <main className="page">
        {page === "dashboard" ? (
          <>
            <Graph engine={engine} config={config} openSettings={() => go("settings")} />
            <div className="section-title">Activity</div>
            <Feed items={engine.activity} />
            <div className="section-title">Widgets</div>
            <Widgets config={config} openSettings={() => go("settings")} engine={engine} />
          </>
        ) : (
          <SettingsPage config={config} engine={engine} onConfig={setConfig} replay={() => { go("dashboard"); setTour(true); }} />
        )}
      </main>
      {tour && page === "dashboard" && <Walkthrough config={config} onClose={closeTour} />}
      <UpdateOverlay />
    </UpdaterProvider>
  );
}
