import { useEffect, useMemo, useState } from "react";
import type { Config } from "../types";
import { GLOSSARY, buildSteps, buildTopics } from "./content";

const KEY = "claudeTrader.walkthroughSeen.v1";

export function walkthroughSeen(): boolean {
  try {
    return window.localStorage.getItem(KEY) === "1";
  } catch {
    return false;
  }
}

export function markWalkthroughSeen() {
  try {
    window.localStorage.setItem(KEY, "1");
  } catch {
    /* storage can fail (private mode, blocked). The app works without it. */
  }
}

function findTarget(selector: string | null, card?: string): HTMLElement | null {
  if (!selector) return null;
  try {
    if (card) {
      const hs = Array.from(document.querySelectorAll<HTMLElement>(".card h3"));
      const h = hs.find((x) => (x.textContent ?? "").trim().toLowerCase().startsWith(card.toLowerCase()));
      return (h?.closest(".card") as HTMLElement | null) ?? null;
    }
    return document.querySelector<HTMLElement>(selector);
  } catch {
    return null;
  }
}

export function Walkthrough({ config, onClose }: { config: Config; onClose: () => void }) {
  const steps = useMemo(() => buildSteps(config), [config]);
  const [i, setI] = useState(0);
  const s = steps[i];
  const last = i === steps.length - 1;
  const [side, setSide] = useState<"left" | "right">("right");

  useEffect(() => {
    const el = findTarget(s.target, s.card);
    if (!el) return;
    el.classList.add("tour-hl");
    try {
      el.scrollIntoView({ block: "center", behavior: "smooth" });
      // Put the tour card on the side opposite the highlighted area so it never covers it.
      const r = el.getBoundingClientRect();
      const wide = r.width > window.innerWidth * 0.6;
      setSide(!wide && r.left + r.width / 2 > window.innerWidth / 2 ? "left" : "right");
    } catch {
      /* ignore */
    }
    return () => el.classList.remove("tour-hl");
  }, [s]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className={`tour ${s.target ? "docked" : "centered"} ${side}`} role="dialog" aria-modal="false" aria-label="Welcome walkthrough">
      <div className="tour-card">
        <div className="tour-top">
          <span className="tour-count">Step {i + 1} of {steps.length}</span>
          <button className="linkbtn" onClick={onClose}>Skip tour</button>
        </div>
        <h2>{s.title}</h2>
        {s.what.map((p, k) => <p key={k}>{p}</p>)}
        <div className="tour-gb">
          <p><b className="g">Good looks like.</b> {s.good}</p>
          <p><b className="r">Bad looks like.</b> {s.bad}</p>
          <p><b>What you do.</b> {s.todo}</p>
        </div>
        <div className="tour-nav">
          <div className="dots" aria-hidden="true">{steps.map((_, k) => <i key={k} className={k === i ? "on" : ""} />)}</div>
          <span className="spacer" />
          <button className="btn ghost" disabled={i === 0} onClick={() => setI(i - 1)}>Back</button>
          <button className="btn" onClick={() => (last ? onClose() : setI(i + 1))}>{last ? "Done" : "Next"}</button>
        </div>
      </div>
    </div>
  );
}

export function TipsPage({ config, replay }: { config: Config; replay: () => void }) {
  const topics = useMemo(() => buildTopics(config), [config]);
  const [q, setQ] = useState("");
  const needle = q.trim().toLowerCase();
  const terms = GLOSSARY.filter((t) => !needle || (t.term + " " + t.def).toLowerCase().includes(needle));
  return (
    <div className="tips">
      <div className="tips-head">
        <div>
          <div className="set-title">Tips</div>
          <p className="set-sub">Short, plain explanations of what the app is doing. Most memecoins lose money, and paper profit does not promise live profit. Numbers below come from your current settings.</p>
        </div>
        <button className="btn" onClick={replay}>Replay walkthrough</button>
      </div>
      <input className="tips-search" placeholder="Search tips and glossary" aria-label="Search tips" value={q} onChange={(e) => setQ(e.target.value)} />
      <nav className="tips-toc" aria-label="Topics">
        {topics.map((t) => <a key={t.id} href={`#tips-${t.id}`} onClick={(e) => { e.preventDefault(); document.getElementById(`tips-${t.id}`)?.scrollIntoView({ behavior: "smooth", block: "start" }); }}>{t.title}</a>)}
        <a href="#tips-glossary" onClick={(e) => { e.preventDefault(); document.getElementById("tips-glossary")?.scrollIntoView({ behavior: "smooth", block: "start" }); }}>Glossary</a>
      </nav>
      <div className="tips-grid">
        {topics.map((t) => {
          const entries = t.entries.filter((e) => !needle || (e.q + " " + e.a.join(" ")).toLowerCase().includes(needle));
          if (entries.length === 0) return null;
          return (
            <section className="card" id={`tips-${t.id}`} key={t.id}>
              <h3>{t.title} <span className="hint">{t.intro}</span></h3>
              {entries.map((e) => (
                <div className="tip-entry" key={e.q}>
                  <h4>{e.q}</h4>
                  {e.a.map((p, k) => <p key={k}>{p}</p>)}
                </div>
              ))}
            </section>
          );
        })}
        <section className="card span-all" id="tips-glossary">
          <h3>Glossary <span className="hint">{terms.length} terms</span></h3>
          <dl className="gloss">
            {terms.map((t) => (<div key={t.term}><dt>{t.term}</dt><dd>{t.def}</dd></div>))}
          </dl>
        </section>
      </div>
    </div>
  );
}
