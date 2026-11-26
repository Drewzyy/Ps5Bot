import { useState } from "react";
import { readThemePref, saveThemePref } from "../theme";
import type { ThemePref } from "../theme";
import type { Config } from "../types";
import { SECTIONS, flatten } from "../settingsMeta";
import type { Row } from "../settingsMeta";

function show(v: unknown): string {
  if (typeof v === "boolean") return v ? "On" : "Off";
  if (Array.isArray(v)) return v.join(", ");
  return String(v);
}

function SettingRow({ row }: { row: Row }) {
  const [open, setOpen] = useState(false);
  const id = `exp-${row.display}`;
  return (
    <div className="set-row" data-setting={row.display}>
      <div className="set-main">
        <span className="lab">
          {row.meta?.label ?? row.display}
          <span className="key">{row.display}</span>
        </span>
        <span className="val">{show(row.value)}</span>
        <button className="info" aria-expanded={open} aria-controls={id} aria-label={`Explain ${row.meta?.label ?? row.display}`} onClick={() => setOpen(!open)}>
          i
        </button>
      </div>
      {open && (
        <div className="explain" id={id}>
          {row.meta ? (
            <>
              <p><b>What it does.</b> {row.meta.what}</p>
              <p><b>A good value.</b> {row.meta.good}</p>
              <p><b>How to set it.</b> {row.meta.how}</p>
            </>
          ) : (
            <p>No explanation written yet for this setting.</p>
          )}
        </div>
      )}
    </div>
  );
}

function Appearance() {
  const [pref, setPref] = useState<ThemePref>(readThemePref);
  const opts: [ThemePref, string][] = [["light", "Light"], ["dark", "Dark"], ["system", "System"]];
  return (
    <section className="set-sec">
      <h2>Appearance</h2>
      <p>Choose how the app looks. This only changes colors on this device and does not affect trading.</p>
      <div className="seg" role="radiogroup" aria-label="Appearance">
        {opts.map(([v, label]) => (
          <button key={v} role="radio" aria-checked={pref === v} onClick={() => { setPref(v); saveThemePref(v); }}>{label}</button>
        ))}
      </div>
    </section>
  );
}

export default function SettingsPage({ config }: { config: Config }) {
  const rows = flatten(config as unknown as Record<string, unknown>);
  return (
    <div>
      <h1 className="set-title">Settings</h1>
      <p className="set-sub">
        Every setting the app uses, in plain words. Press the i next to any line to see what it does, a good value, and how to set it. For now these values are read only and come from config.json.
      </p>
      <Appearance />
      {SECTIONS.map((s) => {
        const sr = rows.filter((r) => r.section === s.key);
        if (!sr.length) return null;
        return (
          <section className="set-sec" key={s.key}>
            <h2>{s.title}</h2>
            <p>{s.blurb}</p>
            <div className="set-list">
              {sr.map((r) => (
                <SettingRow key={r.display} row={r} />
              ))}
            </div>
          </section>
        );
      })}
    </div>
  );
}
