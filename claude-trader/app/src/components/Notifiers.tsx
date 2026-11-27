import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { api } from "../source";
import type { ConnName, ConnStatus, NotifyEvents } from "../types";

interface Field {
  key: string;
  label: string;
  placeholder: string;
  /** The page never shows a saved value: a masked hint is shown instead. */
  secret?: boolean;
  optional?: boolean;
}

const EVENT_LABELS: { key: keyof NotifyEvents; label: string }[] = [
  { key: "buy", label: "Buy" },
  { key: "sell", label: "Sell (with P&L)" },
  { key: "dailySummary", label: "Daily summary" },
  { key: "learningChange", label: "Learning change applied" },
  { key: "killSwitch", label: "Kill switch" },
  { key: "lossLimit", label: "Daily loss limit" },
  { key: "health", label: "Feed / RugCheck health warnings" },
  { key: "updateInstalled", label: "Update installed" },
];

const PAYLOAD = `POST <your link>
Content-Type: application/json
X-Claude-Trader-Secret: <optional secret you set>

{
  "schema": "claude-trader/event/v1",
  "event": "sell",            // buy | sell | dailySummary | learningChange | killSwitch | lossLimit | health | updateInstalled | test
  "mode": "paper",            // "paper" or "live"
  "paper": true,
  "sent_at": "2026-11-27T14:03:11+00:00",
  "title": "Win CAT",
  "data": {                   // the recorded numbers of the event
    "event": "sell", "ts": 1795000000000, "symbol": "CAT", "mint": "...",
    "lane": "momentum", "pnl_sol": 0.018, "pnl_pct": 18.0,
    "exit_reason": "trailing-stop", "hold_seconds": 95, "fees_sol": 0.0012
  },
  "links": { "dexscreener": "https://dexscreener.com/solana/...", "solscan": "https://solscan.io/token/..." }
}`;

function Card({ conn, title, intro, fields, steps, extra, saveLabel = "Save" }: { conn: ConnName; title: string; intro: ReactNode; fields: Field[]; steps: ReactNode[]; extra?: (st: ConnStatus) => ReactNode; saveLabel?: string }) {
  const [st, setSt] = useState<ConnStatus | null>(null);
  const [vals, setVals] = useState<Record<string, string>>({});
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState("");
  useEffect(() => {
    api.notifyStatus().then((all) => setSt(all.find((c) => c.conn === conn) ?? null)).catch(() => {});
  }, [conn]);
  const run = async (what: string, f: () => Promise<void>) => {
    setBusy(what);
    setMsg(null);
    try {
      await f();
    } catch (e) {
      setMsg({ ok: false, text: String(e instanceof Error ? e.message : e) });
    }
    setBusy("");
  };
  const save = () =>
    run("save", async () => {
      setSt(await api.notifySave(conn, vals));
      setVals({});
      setMsg({ ok: true, text: "Saved in your computer's secure storage." });
    });
  const test = () => run("test", async () => setMsg({ ok: true, text: await api.notifyTest(conn) }));
  const remove = () =>
    run("rm", async () => {
      setSt(await api.notifyRemove(conn));
      setVals({});
      setMsg({ ok: true, text: "Disconnected. The saved secrets were removed from this computer." });
    });
  const toggle = (k: keyof NotifyEvents) =>
    run("ev", async () => {
      if (st) setSt(await api.notifySetEvents(conn, { ...st.events, [k]: !st.events[k] }));
    });
  const anyTyped = Object.values(vals).some((v) => v.trim());
  return (
    <div className="conn-card" data-testid={`conn-${conn}`}>
      <h3>{title} {st?.configured && <span className="okpill">connected</span>} {api.kind === "demo" && <span className="demotag">DEMO</span>}</h3>
      <div className="note" style={{ margin: 0 }}>{intro}</div>
      <div className="conn-fields">
        {fields.map((f) => (
          <label key={f.key} className="conn-field">
            <span className="k">{f.label}{f.optional ? " (optional)" : ""}</span>
            <input type={f.secret ? "password" : "text"} autoComplete="off" spellCheck={false} placeholder={st?.hints[f.key] ?? f.placeholder} value={vals[f.key] ?? ""} onChange={(e) => setVals({ ...vals, [f.key]: e.target.value })} />
            {st?.hints[f.key] && <span className="hint-saved">{st.hints[f.key]}</span>}
          </label>
        ))}
      </div>
      <div className="btns">
        <button className="btn" onClick={save} disabled={!anyTyped || !!busy}>{saveLabel}</button>
        <button className="btn ghost" onClick={test} disabled={!st?.configured || !!busy}>{busy === "test" ? "Sending" : "Send test message"}</button>
        <button className="btn ghost" onClick={remove} disabled={!st?.configured && !Object.keys(st?.hints ?? {}).length} >Disconnect</button>
      </div>
      {st && extra?.(st)}
      <div className="toggles" role="group" aria-label={`${title} events`}>
        {EVENT_LABELS.map((e) => (
          <label key={e.key}>
            <input type="checkbox" checked={st?.events[e.key] ?? false} onChange={() => toggle(e.key)} disabled={!st || !!busy} /> {e.label}
          </label>
        ))}
      </div>
      {msg && <div className={`result ${msg.ok ? "ok" : "bad"}`} role="status">{msg.text}</div>}
      {st?.lastError && <div className="result bad" role="alert">Last send failed: {st.lastError}</div>}
      <ol className="steps">{steps.map((s, i) => <li key={i}>{s}</li>)}</ol>
      <div className="note">Messages come only from what the app really recorded. A failed send never stops trading. Secrets are masked and never written to logs or config.json.</div>
    </div>
  );
}

export default function Notifiers() {
  return (
    <>
      <Card
        conn="discord"
        title="Discord webhook"
        intro="Posts a coloured card (green for a win, red for a loss) with the numbers and a Dexscreener link."
        fields={[{ key: "discord-url", label: "Webhook URL", placeholder: "https://discord.com/api/webhooks/...", secret: true }]}
        steps={["In Discord open the server, then Server Settings > Integrations > Webhooks.", "Press New Webhook, pick the channel, then Copy Webhook URL.", "Paste it above and press Save. It must start with https://discord.com/api/webhooks/ (or discordapp.com).", "Press Send test message. Switch off the events you do not want."]}
      />
      <Card
        conn="webhook"
        title="Generic webhook"
        intro="Sends a JSON POST to any link: Zapier, Make, n8n or your own server."
        fields={[
          { key: "webhook-url", label: "Webhook URL", placeholder: "https://hooks.zapier.com/...", secret: true },
          { key: "webhook-secret", label: "Secret header value", placeholder: "sent as X-Claude-Trader-Secret", secret: true, optional: true },
        ]}
        steps={["In Zapier, Make or n8n create a flow that starts with a Webhook (catch hook) and copy its URL.", "Paste it above. It must start with https:// (http://localhost is allowed for testing).", "Optional: add a secret. It is sent as the header X-Claude-Trader-Secret so your flow can check it.", "Press Save, then Send test message. The flow receives the JSON below."]}
        extra={() => (
          <details className="payload-doc">
            <summary>Payload schema (claude-trader/event/v1)</summary>
            <pre>{PAYLOAD}</pre>
          </details>
        )}
      />
    </>
  );
}
