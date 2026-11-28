import { useCallback, useEffect, useState } from "react";
import { api } from "../source";
import type { MoveCheck, StorageView } from "../source";
import { openSection } from "../nav";
import { ProgressBar } from "./UpdateUi";

const err = (e: unknown) => String(e instanceof Error ? e.message : e);

export function bytesText(n: number | null | undefined): string {
  if (n === null || n === undefined) return "unknown";
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)} MB`;
  if (n >= 1e3) return `${Math.round(n / 1e3)} KB`;
  return `${n} bytes`;
}

export const EXT_MOVED_STEPS = [
  "Open chrome://extensions in Chrome.",
  "Remove the old Claude Trader extension.",
  "Press Load unpacked and pick the folder shown here (press Show folder to open it).",
  "Open the extension's side panel and pair it again (Settings, Startup and browser, Pair).",
];

/** The "Chrome extension moved" notice with its steps. Shown in Storage and as a bar on every page. */
export function ExtensionMoved({ folder, onDone }: { folder: string; onDone: () => void }) {
  const [msg, setMsg] = useState("");
  return (
    <div className="result" role="status" data-testid="ext-moved">
      <b>Chrome extension moved. Reload it once from the new folder.</b>
      <ol className="steps">
        {EXT_MOVED_STEPS.map((s) => <li key={s}>{s}</li>)}
      </ol>
      <div className="kvrows" style={{ marginTop: 6 }}><div><span className="k">New folder</span><code className="path">{folder}</code></div></div>
      <div className="btns" style={{ marginTop: 8 }}>
        <button className="btn" onClick={() => api.openExtensionFolder().catch((e) => setMsg(err(e)))}>Show folder</button>
        <button className="btn ghost" onClick={() => api.storageAckExtension().then(onDone).catch((e) => setMsg(err(e)))}>Done, I reloaded it</button>
      </div>
      {msg && <div className="note" role="alert">{msg}</div>}
    </div>
  );
}

/** A slim bar under the top bar until the person has reloaded the extension from the new folder. */
export function ExtensionMovedBar() {
  const [v, setV] = useState<StorageView | null>(null);
  const load = useCallback(() => {
    api.storageGet().then(setV).catch(() => {});
  }, []);
  useEffect(() => {
    load();
    window.addEventListener("ct-storage-changed", load);
    return () => window.removeEventListener("ct-storage-changed", load);
  }, [load]);
  if (!v?.extensionNotice) return null;
  return (
    <div className="fwarn bar" role="alert" data-testid="ext-moved-bar">
      <b>Chrome extension moved. Reload it once from the new folder.</b>{" "}
      <button className="btn ghost" onClick={() => openSection("storage")}>Show steps</button>
    </div>
  );
}

// ------------------------------------------------------------------ move dialog

type Phase = "choose" | "working" | "restarting";

function MoveDialog({ view, openPositions, onClose }: { view: StorageView; openPositions: number; onClose: () => void }) {
  const [dest, setDest] = useState("");
  const [chk, setChk] = useState<MoveCheck | null>(null);
  const [phase, setPhase] = useState<Phase>("choose");
  const [error, setError] = useState("");
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && phase === "choose" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [phase, onClose]);

  const check = async (d: string) => {
    setDest(d);
    setError("");
    setChk(null);
    if (!d.trim()) return;
    try {
      setChk(await api.storageCheck(d.trim()));
    } catch (e) {
      setError(err(e));
    }
  };
  const choose = async () => {
    try {
      const p = await api.pickDataFolder();
      if (p) await check(p);
    } catch (e) {
      setError(err(e));
    }
  };
  const go = async () => {
    if (!chk?.ok) return;
    setPhase("working");
    setError("");
    try {
      await api.storageMove(chk.dest);
      setPhase("restarting");
    } catch (e) {
      setError(err(e));
      setPhase("choose");
    }
  };
  const existing = chk?.kind === "existingData";
  return (
    <div className="kill-overlay" role="dialog" aria-modal="true" aria-label="Move the data folder" data-testid="move-dialog">
      <div className="kill-dialog upd-dialog" style={{ maxWidth: 560, width: "100%" }}>
        <h2>Move the data folder</h2>
        {phase === "choose" && (
          <>
            <div className="kvrows">
              <div><span className="k">Now</span><code className="path">{view.root}</code></div>
            </div>
            <div className="field" style={{ marginTop: 10 }}>
              <label htmlFor="mv-dest">New folder</label>
              <div className="btns">
                <input id="mv-dest" style={{ flex: 1, minWidth: 220 }} value={dest} placeholder="D:\ClaudeTraderData" onChange={(e) => setDest(e.target.value)} onBlur={() => dest.trim() && !chk && check(dest)} spellCheck={false} />
                <button className="btn" onClick={choose}>Choose folder...</button>
              </div>
            </div>
            {chk && <div className={`result ${chk.ok ? "ok" : "bad"}`} role={chk.ok ? "status" : "alert"} data-testid="move-check">{chk.message}{chk.ok && chk.kind === "empty" && ` Space needed ${bytesText(chk.needBytes)}, free ${bytesText(chk.freeBytes)}.`}</div>}
            {error && <div className="result bad" role="alert">{error}</div>}
            {chk?.ok && openPositions > 0 && <p className="small" style={{ marginTop: 8 }}>{openPositions} open paper position{openPositions === 1 ? "" : "s"} will be closed first. They are sold the normal way, with the normal fees. No new buys start meanwhile.</p>}
            <p className="small" style={{ marginTop: 8 }}>Steps: pause trading, copy everything, check the copy, switch, restart. Your old folder is not touched. After the restart you can delete it.</p>
            <div className="kill-actions">
              <button className="btn" onClick={go} disabled={!chk?.ok}>{existing ? "Switch to it and restart" : openPositions > 0 ? "Close positions, move and restart" : "Move and restart"}</button>
              <button className="kill-cancel" onClick={onClose}>Cancel</button>
            </div>
          </>
        )}
        {phase === "working" && (
          <div role="status">
            <p>{openPositions > 0 ? "Closing paper positions, then copying. " : "Copying and checking. "}Please keep the app open.</p>
            <ProgressBar value={null} />
          </div>
        )}
        {phase === "restarting" && <p role="status">Moved and checked. The app restarts now.</p>}
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ Settings > Storage

export function StorageSection({ openPositions }: { openPositions: number }) {
  const [v, setV] = useState<StorageView | null>(null);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [dialog, setDialog] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const load = useCallback(() => api.storageGet().then(setV).catch((e) => setMsg({ ok: false, text: err(e) })), []);
  useEffect(() => {
    load();
  }, [load]);
  const changed = () => {
    load();
    window.dispatchEvent(new Event("ct-storage-changed"));
  };
  if (!v) return <div className="acc-pad">{msg ? <div className="result bad" role="alert">{msg.text}</div> : "Loading."}</div>;
  return (
    <div className="acc-pad" data-testid="storage">
      <p className="hint">Everything the app writes is kept in one folder, so it can live on any drive. Move it to D: and the C: drive only keeps a tiny pointer file.</p>
      <div className="kvrows">
        <div><span className="k">Data folder</span><code className="path" data-testid="data-root">{v.root}</code></div>
        <div><span className="k">Size</span>{bytesText(v.totalBytes)}, {bytesText(v.freeBytes)} free on that drive</div>
        {!v.isDefault && <div><span className="k">Default was</span><code className="path">{v.defaultRoot}</code></div>}
      </div>
      {v.warning && <div className="result bad" role="alert">{v.warning}</div>}
      <div className="btns" style={{ marginTop: 8 }}>
        <button className="btn" onClick={() => { setMsg(null); setDialog(true); }}>Move to another folder...</button>
        <button className="btn ghost" onClick={() => api.storageShowFolder().catch((e) => setMsg({ ok: false, text: err(e) }))}>Show folder</button>
      </div>
      {msg && <div className={`result ${msg.ok ? "ok" : "bad"}`} role={msg.ok ? "status" : "alert"}>{msg.text}</div>}

      {v.previousRoot && (
        <div className="result" role="status" data-testid="old-copy" style={{ marginTop: 10 }}>
          <b>Your data was moved. The old copy is still at</b> <code className="path">{v.previousRoot}</code>
          <div className="note">The new folder was checked before the switch. Delete the old copy to free the space. Only the app's own files are removed there.</div>
          {!confirmDelete ? (
            <div className="btns" style={{ marginTop: 6 }}>
              <button className="btn" onClick={() => setConfirmDelete(true)}>Delete old copy...</button>
              <button className="btn ghost" onClick={() => api.storageKeepOld().then(changed).catch((e) => setMsg({ ok: false, text: err(e) }))}>Keep it</button>
            </div>
          ) : (
            <div className="btns" style={{ marginTop: 6 }}>
              <span>Delete {v.previousRoot}? This cannot be undone.</span>
              <button className="btn" onClick={() => api.storageDeleteOld().then((r) => { setConfirmDelete(false); setMsg({ ok: true, text: `Old copy deleted (${r.removed.length} item${r.removed.length === 1 ? "" : "s"}).${r.kept.length ? ` Left alone, not ours: ${r.kept.join(", ")}.` : ""}` }); changed(); }).catch((e) => { setConfirmDelete(false); setMsg({ ok: false, text: err(e) }); })}>Yes, delete it</button>
              <button className="kill-cancel" onClick={() => setConfirmDelete(false)}>Cancel</button>
            </div>
          )}
        </div>
      )}

      {v.extensionNotice && <div style={{ marginTop: 10 }}><ExtensionMoved folder={v.extensionFolder} onDone={changed} /></div>}

      <p className="acc-blurb" style={{ marginTop: 12 }}><b>What is stored where</b></p>
      <table className="plain" data-testid="storage-items">
        <tbody>
          {v.items.map((i) => (
            <tr key={i.name}><td>{i.name}</td><td className="num">{bytesText(i.bytes)}</td><td><code className="path">{i.path}</code></td></tr>
          ))}
        </tbody>
      </table>
      <div className="note" style={{ marginTop: 8 }}>
        Not in this folder: the pointer file <code className="path">{v.pointerFile}</code> (a few bytes, it says where the folder is), your Helius key and webhook secrets in Windows Credential Manager (Windows controls that), the app itself in its install folder, and Chrome's own downloads. Chrome saves downloads where chrome://settings/downloads says. The app cannot change that. To change it, open that page in Chrome and press Change.
      </div>
      {dialog && <MoveDialog view={v} openPositions={openPositions} onClose={() => { setDialog(false); load(); }} />}
    </div>
  );
}

// ------------------------------------------------------------------ first run

/** On the very first launch: where should the data live? Nothing is created before the answer. */
export function FirstRunPrompt() {
  const [v, setV] = useState<StorageView | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);
  useEffect(() => {
    api.storageGet().then(setV).catch(() => {});
  }, []);
  if (!v?.firstRun || done) return null;
  const choose = async () => {
    setError("");
    try {
      const p = await api.pickDataFolder();
      if (!p) return;
      setBusy(true);
      const c = await api.storageCheck(p);
      if (!c.ok && c.kind === "notEmpty") throw new Error(c.message);
      await api.storageFirstRun(p);
      setDone(true);
    } catch (e) {
      setError(err(e));
      setBusy(false);
    }
  };
  const useDefault = async () => {
    setError("");
    setBusy(true);
    try {
      await api.storageFirstRun(null);
      window.location.reload();
    } catch (e) {
      setError(err(e));
      setBusy(false);
    }
  };
  return (
    <div className="kill-overlay" role="dialog" aria-modal="true" aria-label="Where to keep the data" data-testid="first-run">
      <div className="kill-dialog upd-dialog" style={{ maxWidth: 520 }}>
        <h2>Where should Claude Trader keep its data?</h2>
        <p>Trade history, settings, logs and recordings go in one folder. The default is:</p>
        <div className="result"><code className="path" data-testid="default-root">{v.defaultRoot}</code></div>
        <p className="small" style={{ marginTop: 8 }}>You can pick another drive, for example D:\ClaudeTraderData. You can move it later in Settings, Storage.</p>
        {error && <div className="result bad" role="alert">{error}</div>}
        <div className="kill-actions">
          <button className="btn" onClick={choose} disabled={busy} autoFocus>Choose folder...</button>
          <button className="kill-cancel" onClick={useDefault} disabled={busy}>Use the default</button>
        </div>
        {done && <p role="status">Saved. The app restarts now.</p>}
      </div>
    </div>
  );
}
