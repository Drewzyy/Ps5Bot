import { useEffect, useState } from "react";
import { api } from "../source";
import Notifiers from "./Notifiers";
import type { BackfillResult, VaultSettings, VaultStatus } from "../types";

const TYPES: { key: keyof Omit<VaultSettings, "path">; label: string; hint: string }[] = [
  { key: "trades", label: "Trades", hint: "one note per closed trade" },
  { key: "coins", label: "Coins", hint: "one note per coin, with its verdicts" },
  { key: "days", label: "Days", hint: "daily summary and funnel" },
  { key: "lessons", label: "Lessons", hint: "learning reviews, changes, efficiency" },
  { key: "situations", label: "Situations", hint: "kill switch, loss limit, feed warnings, RugCheck outages, missed coins" },
];

export default function Connectors() {
  const [st, setSt] = useState<VaultStatus | null>(null);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState("");
  useEffect(() => {
    api.vaultStatus().then(setSt).catch(() => {});
  }, []);
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
  const connected = !!st?.settings.path;
  const choose = () =>
    run("choose", async () => {
      const p = await api.pickFolder();
      if (p) setSt(await api.vaultConnect(p));
    });
  const test = () => run("test", async () => setMsg(await api.vaultTest().then((t) => ({ ok: t.ok, text: t.message }))));
  const disconnect = () =>
    run("disc", async () => {
      setSt(await api.vaultDisconnect());
      setMsg({ ok: true, text: "Disconnected. Nothing more is written. The notes already in your vault stay." });
    });
  const backfill = () =>
    run("back", async () => {
      const r: BackfillResult = await api.vaultBackfill();
      setMsg({ ok: true, text: `Backfill done: ${r.trades} trade notes, ${r.days} days, ${r.coins} coins, ${r.lessons} lesson notes, ${r.situations} missed-coin notes.${r.keptYours ? ` ${r.keptYours} notes of yours with the same name were left alone.` : ""}` });
    });
  const toggle = (k: keyof Omit<VaultSettings, "path">) =>
    run("tog", async () => {
      if (!st) return;
      setSt(await api.vaultSetToggles({ ...st.settings, [k]: !st.settings[k] }));
    });
  return (
    <>
    <div className="conn-card" data-testid="obsidian-card">
      <h3>Obsidian vault {api.kind === "demo" && <span className="demotag">DEMO</span>}</h3>
      <p className="note" style={{ margin: 0 }}>Gives the bot a readable memory: markdown notes with [[links]] in a Claude Trader folder inside your vault. Local files only, no Obsidian plugin needed.</p>
      <div>
        {connected ? (
          <>
            <span className="k">Vault folder </span>
            <span className="vault-path">{st!.settings.path}</span>
            <div className="note">{st!.looksLikeObsidian ? "Obsidian vault found. Notes go into the Claude Trader folder inside it." : "No .obsidian folder here yet. Notes are still written. Open this folder once in Obsidian (Open folder as vault)."}</div>
          </>
        ) : (
          <span className="note">Not connected. Nothing is written.</span>
        )}
      </div>
      <div className="btns">
        <button className="btn" onClick={choose} disabled={!!busy}>{connected ? "Change vault folder" : "Choose vault folder"}</button>
        <button className="btn ghost" onClick={test} disabled={!connected || !!busy}>Test</button>
        <button className="btn ghost" onClick={backfill} disabled={!connected || !!busy}>{busy === "back" ? "Writing" : "Backfill"}</button>
        <button className="btn ghost" onClick={disconnect} disabled={!connected || !!busy}>Disconnect</button>
      </div>
      <div className="toggles" role="group" aria-label="Note types">
        {TYPES.map((t) => (
          <label key={t.key} title={t.hint}>
            <input type="checkbox" checked={st?.settings[t.key] ?? true} onChange={() => toggle(t.key)} disabled={!st || !!busy} /> {t.label}
          </label>
        ))}
      </div>
      {msg && <div className={`result ${msg.ok ? "ok" : "bad"}`} role="status">{msg.text}</div>}
      {st?.lastError && <div className="result bad" role="alert">{st.lastError}</div>}
      <ol className="steps">
        <li>Install Obsidian from obsidian.md (free).</li>
        <li>Create a new vault, or open one you already have.</li>
        <li>Press Choose vault folder.</li>
        <li>Pick the vault's folder (the one that holds your notes).</li>
        <li>Press Test. It writes and reads back one note.</li>
        <li>In Obsidian, look for the Claude Trader folder in the file list. Open Index.</li>
      </ol>
      <div className="note">Backfill writes notes for trades and days that closed before you connected. The app never deletes your notes and never overwrites a note you wrote yourself.</div>
    </div>
      <Notifiers />
    </>
  );
}
