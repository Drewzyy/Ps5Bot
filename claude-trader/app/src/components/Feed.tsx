import type { ActivityItem } from "../types";
import { SourceLink } from "./links";

export default function Feed({ items }: { items: ActivityItem[] }) {
  return (
    <section className="card">
      <h3>Activity feed <span className="hint">newest first</span></h3>
      {items.length === 0 ? (
        <div className="empty">Nothing yet. When the app finds, checks, buys or sells a coin, it shows up here with the time.</div>
      ) : (
        <div className="feed">
          {items.map((it, i) => (
            <div key={i} className="plain">
              <span className="ts">{new Date(it.ts).toLocaleTimeString([], { hour12: false })}</span>
              {it.mint ? (
                <a className="fmain" href={`https://solscan.io/token/${it.mint}`} target="_blank" rel="noreferrer" title="Open this coin on Solscan">{it.text}</a>
              ) : (
                <span className="fmain">{it.text}</span>
              )}
              <SourceLink url={it.sourceUrl} />
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
