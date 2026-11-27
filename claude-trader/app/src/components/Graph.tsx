import type { CSSProperties } from "react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { AGENTS, BOT_ICON, CX, CY, HUES } from "../agents";
import type { AgentDef } from "../agents";
import type { EngineState } from "../useEngine";
import type { AgentId, Config, Pulse } from "../types";
import { flatten } from "../settingsMeta";
import { FeedHealthLine } from "./Health";

const W = 1000;
const H = 520;
const BOT: AgentId = "claude-bot";

function pos(id: AgentId) {
  if (id === BOT) return { x: CX, y: CY };
  const a = AGENTS.find((x) => x.id === id)!;
  return { x: a.x, y: a.y };
}

/** Stops a spoke line short of the node box and the central orb so lines do not run under the cards. */
function trim(from: { x: number; y: number }, to: { x: number; y: number }) {
  return { x1: from.x, y1: from.y, x2: to.x, y2: to.y };
}

function PulseLines({ pulse }: { pulse: Pulse }) {
  // from node -> center (0 to 0.7s), then center -> to node (0.7 to 1.4s)
  const a = pos(pulse.from);
  const c = pos(BOT);
  const b = pos(pulse.to);
  const leg = (p: { x: number; y: number }, q: { x: number; y: number }, begin: number) => (
    <g key={`${p.x}-${q.x}-${begin}`}>
      <line className="spoke-pulse" {...trim(p, q)} style={{ animationDelay: `${begin}s` }} />
      <circle className="dot" r="5" opacity="0">
        <animateMotion dur="0.7s" begin={`${begin}s`} fill="freeze" path={`M ${p.x} ${p.y} L ${q.x} ${q.y}`} />
        <animate attributeName="opacity" values="0;1;1;0" keyTimes="0;0.1;0.9;1" dur="0.7s" begin={`${begin}s`} fill="freeze" />
      </circle>
    </g>
  );
  return (
    <>
      {leg(a, c, 0)}
      {leg(c, b, 0.7)}
    </>
  );
}

function Node({ def, state, selected, glow, onClick }: { def: AgentDef; state?: { color: string; line: string }; selected: boolean; glow: boolean; onClick: () => void }) {
  const color = state?.color ?? "grey";
  return (
    <button className={`node ${selected ? "selected" : ""} ${glow ? "glow" : ""}`} style={{ left: def.x, top: def.y, "--ac": `var(--c-${HUES[def.id]})` } as CSSProperties} onClick={onClick} aria-label={`${def.name}: open details`}>
      <span className="ico">{def.icon}</span>
      <span className="txt">
        <span className="nm"><span className={`sdot ${color}`} />{def.name}</span>
        <span className="st" style={{ display: "block" }}>{state?.line ?? "Idle"}</span>
      </span>
    </button>
  );
}

export default function Graph({ engine, config, openSettings }: { engine: EngineState; config: Config; openSettings: () => void }) {
  const wrap = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(1);
  const [selected, setSelected] = useState<AgentId | null>(null);
  const [glowing, setGlowing] = useState<Set<AgentId>>(new Set());

  useLayoutEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const update = () => setScale(el.clientWidth / W);
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const latest = engine.pulses[engine.pulses.length - 1];
  useEffect(() => {
    if (!latest) return;
    setGlowing(new Set([latest.from, BOT]));
    const t1 = setTimeout(() => setGlowing(new Set([BOT, latest.to])), 700);
    const t2 = setTimeout(() => setGlowing(new Set()), 1500);
    return () => {
      clearTimeout(t1);
      clearTimeout(t2);
    };
  }, [latest?.id]);

  const def = selected && selected !== BOT ? AGENTS.find((a) => a.id === selected) : undefined;

  return (
    <div className="graph-wrap" ref={wrap} style={{ height: H * scale }}>
      <div className="stage" style={{ transform: `scale(${scale})` }}>
        <svg className="lines" viewBox={`0 0 ${W} ${H}`}>
          {AGENTS.map((a) => (
            <line key={a.id} className="spoke" x1={CX} y1={CY} x2={a.x} y2={a.y} />
          ))}
          {latest && <PulseLines key={latest.id} pulse={latest} />}
        </svg>
        <button className={`center ${glowing.has(BOT) ? "glow" : ""} ${selected === BOT ? "selected" : ""}`} onClick={() => setSelected(selected === BOT ? null : BOT)} aria-label="Claude Trader: open details">
          <span className="orb">{BOT_ICON}</span>
          <span className="cap" style={{ display: "block" }}>Claude Trader</span>
        </button>
        {AGENTS.map((a) => (
          <Node key={a.id} def={a} state={engine.agents[a.id]} selected={selected === a.id} glow={glowing.has(a.id)} onClick={() => setSelected(selected === a.id ? null : a.id)} />
        ))}
      </div>
      {selected && (
        <aside className="panel" style={{ transformOrigin: "top right" }}>
          <div className="hd">
            <h3>{selected === BOT ? "Claude Trader" : def?.name}</h3>
            <button className="close" onClick={() => setSelected(null)} aria-label="Close panel">x</button>
          </div>
          {selected === BOT ? (
            <p className="role">The hub. Every handoff between agents passes through here. It runs in paper mode: nothing real is bought or sold.</p>
          ) : def ? (
            <PanelBody def={def} engine={engine} config={config} openSettings={openSettings} />
          ) : null}
        </aside>
      )}
    </div>
  );
}

function PanelBody({ def, engine, config, openSettings }: { def: AgentDef; engine: EngineState; config: Config; openSettings: () => void }) {
  const st = engine.agents[def.id];
  const decisions = engine.decisions[def.id] ?? [];
  const rows = flatten(config as unknown as Record<string, unknown>).filter((r) => def.settingsPrefix.includes(r.section));
  return (
    <>
      <p className="role">{def.role}</p>
      <h4>Current task</h4>
      <div className="task">{st?.line ?? "Idle"}</div>
      {def.id === "memecoin-monitor" && (
        <>
          <h4>Buy/sell data health</h4>
          <FeedHealthLine feed={engine.feed} />
        </>
      )}
      <h4>Last 10 decisions</h4>
      {decisions.length === 0 ? (
        <div className="empty">No decisions yet.</div>
      ) : (
        decisions.map((d, i) => (
          <div className="kv" key={i}>
            <span>{new Date(d.ts).toLocaleTimeString()}</span>
            <span>{d.text}</span>
          </div>
        ))
      )}
      <h4>Settings</h4>
      {def.note && <p className="role" style={{ marginBottom: 6 }}>{def.note}</p>}
      {rows.length === 0 ? (
        <div className="empty">No settings for this agent yet.</div>
      ) : (
        rows.slice(0, 8).map((r) => (
          <div className="kv" key={r.display}>
            <span>{r.meta?.label ?? r.display}</span>
            <span>{typeof r.value === "boolean" ? (r.value ? "On" : "Off") : String(r.value)}</span>
          </div>
        ))
      )}
      {rows.length > 8 && <p className="role">and {rows.length - 8} more.</p>}
      <button className="linkbtn" onClick={openSettings}>Open all settings</button>
    </>
  );
}
