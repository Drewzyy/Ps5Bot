import type { ActivityItem } from "../types";

export default function Feed({ items }: { items: ActivityItem[] }) {
  return (
    <section className="card">
      <h3>Activity feed <span className="hint">newest first</span></h3>
      {items.length === 0 ? (
        <div className="empty">Nothing yet. When the app finds, checks, buys or sells a coin, it shows up here with the time.</div>
      ) : (
        <div className="feed">
          {items.map((it, i) => (
            <button key={i} title="Token details come in a later version">
              <span className="ts">{new Date(it.ts).toLocaleTimeString()}</span>
              <span>{it.text}</span>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
