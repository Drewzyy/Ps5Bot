import type { ReactElement } from "react";
import type { AgentId } from "./types";

export interface AgentDef {
  id: AgentId;
  name: string;
  role: string;
  x: number; // stage coordinates (1000 x 520)
  y: number;
  icon: ReactElement;
  settingsPrefix: string[]; // config sections shown in the panel
  note?: string;
}

const svg = (d: ReactElement) => (
  <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    {d}
  </svg>
);

const ICONS = {
  shield: svg(<><path d="M12 3l7 3v5c0 4.5-3 8-7 10-4-2-7-5.5-7-10V6z" /><path d="M9 12l2 2 4-4" /></>),
  target: svg(<><circle cx="12" cy="12" r="8" /><circle cx="12" cy="12" r="3" /><path d="M12 2v3M12 19v3M2 12h3M19 12h3" /></>),
  bolt: svg(<path d="M13 2L4 14h7l-1 8 9-12h-7z" />),
  timer: svg(<><circle cx="12" cy="13" r="8" /><path d="M12 9v4l3 2M9 2h6" /></>),
  bulb: svg(<><path d="M9 18h6M10 21h4" /><path d="M12 3a6 6 0 00-4 10.5c.7.7 1 1.5 1 2.5h6c0-1 .3-1.8 1-2.5A6 6 0 0012 3z" /></>),
  radar: svg(<><circle cx="12" cy="12" r="9" /><circle cx="12" cy="12" r="5" /><path d="M12 12l6-6" /></>),
  x: svg(<path d="M5 4l14 16M19 4L5 20" />),
  user: svg(<><circle cx="12" cy="8" r="4" /><path d="M4 21c1-4 4-6 8-6s7 2 8 6" /></>),
  trend: svg(<><path d="M3 17l6-6 4 4 8-8" /><path d="M15 7h6v6" /></>),
};

export const CX = 500;
export const CY = 260;

const ring = (i: number) => {
  const a = ((-90 + i * 40) * Math.PI) / 180;
  return { x: Math.round(CX + 370 * Math.cos(a)), y: Math.round(CY + 200 * Math.sin(a)) };
};

export const AGENTS: AgentDef[] = [
  { id: "scam-shield", name: "Scam Shield", role: "Checks every new coin for scams before anything is bought. It can block a coin, and nobody can override it.", icon: ICONS.shield, settingsPrefix: ["filters"], ...ring(0) },
  { id: "token-picker", name: "Token Picker", role: "Watches coins that passed Scam Shield and scores them. Only high scores move on.", icon: ICONS.target, settingsPrefix: ["observation", "entry"], ...ring(1) },
  { id: "executor", name: "Executor", role: "Places the buys and sells. In paper mode these are pretend trades with realistic fees and failures.", icon: ICONS.bolt, settingsPrefix: ["risk", "fees", "paperFill"], ...ring(2) },
  { id: "hold-timer", name: "Hold Timer", role: "Watches open positions every second and sells on profit steps, stops and the 5 minute limit.", icon: ICONS.timer, settingsPrefix: ["exits"], ...ring(3) },
  { id: "thesis-finder", name: "Thesis Finder", role: "Writes a plain reason for each pick, so every trade can be explained afterwards.", icon: ICONS.bulb, settingsPrefix: [], note: "Starts working in a later version.", ...ring(4) },
  { id: "memecoin-monitor", name: "Memecoin Monitor", role: "Listens to the live feed of newly launched coins and hands each one on.", icon: ICONS.radar, settingsPrefix: ["feed"], ...ring(5) },
  { id: "x-monitor", name: "X Monitor", role: "Looks for buzz about a coin on X. Free tools only.", icon: ICONS.x, settingsPrefix: [], note: "Starts working in a later version.", ...ring(6) },
  { id: "top-trader-monitor", name: "Top Trader Monitor", role: "Would follow proven wallets. Off for now because no free, trustworthy source exists. The app never copies trades.", icon: ICONS.user, settingsPrefix: [], note: "Off in version 1.", ...ring(7) },
  { id: "top-gainers", name: "Top Gainers", role: "Tracks which coins are rising fastest right now, to learn what winners look like.", icon: ICONS.trend, settingsPrefix: [], note: "Starts working in a later version.", ...ring(8) },
];

export const BOT_ICON = (
  <svg viewBox="0 0 48 48" width="44" height="44" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <rect x="9" y="15" width="30" height="22" rx="7" />
    <path d="M24 15V9" />
    <circle cx="24" cy="7" r="2.2" />
    <circle cx="18" cy="26" r="2.6" fill="currentColor" stroke="none" />
    <circle cx="30" cy="26" r="2.6" fill="currentColor" stroke="none" />
    <path d="M19 32h10" />
    <path d="M5 24v6M43 24v6" />
  </svg>
);
