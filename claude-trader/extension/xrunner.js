// X Monitor runner (background worker side). It takes one job at a time from the app, opens the page in ONE reused
// background tab (never focused), waits for the page's own data, sends the structured result back, and waits
// 8 to 15 seconds before the next page. It only reads. It never clicks, types, posts, likes or follows.
//
// Everything Chrome-specific is passed in (`deps`), so the same code runs in the tests with a fake browser and a fake clock.
import { Pacer, gapMs } from "./lib/xpace.js";
import { jobUrlOk, newCollector, addPiece, addPageState, isComplete, buildResult, ACCOUNT_PROBLEM } from "./lib/xjob.js";

export const JOB_TIMEOUT_MS = 28_000;
export const DOM_FALLBACK_AT_MS = 14_000;
const POLL_MS = 250;
const IDLE_POLL_MS = 3000;
const LOGIN_RETRY_MS = 5 * 60_000;

export function createXRunner(deps) {
  const { chrome, ask, connected, now, sleep, rand, getSettings, store, log = () => {} } = deps;
  let pacer = new Pacer(getSettings().pace);
  let running = false;
  let current = null; // { job, collector, tabId }
  let loggedIn = null;
  let last = { at: 0, kind: "", state: "", error: "" };

  async function persist() {
    try {
      await store.set("xpace", pacer.toJSON());
    } catch {
      /* storage can fail: pacing then restarts from zero, which only makes it slower */
    }
  }

  async function restore() {
    try {
      const saved = await store.get("xpace");
      if (saved) pacer = new Pacer(getSettings().pace, saved);
    } catch {
      /* ignore */
    }
  }

  function status() {
    const s = pacer.status(now());
    return { loggedIn, paused: s.paused, pausedReason: s.pausedReason, pausedUntil: s.pausedUntil, loadsLastHour: s.loadsLastHour, loadsToday: s.loadsToday, lastError: last.error };
  }

  async function tabFor(url) {
    const id = await store.get("xTab");
    if (id != null) {
      try {
        await chrome.tabs.get(id);
        await chrome.tabs.update(id, { url, active: false });
        return id;
      } catch {
        /* the tab was closed: open a new one */
      }
    }
    const t = await chrome.tabs.create({ url, active: false });
    await store.set("xTab", t.id);
    return t.id;
  }

  /** Messages from the content script on x.com. Only the job tab is answered; everything else gets nothing. */
  function onMessage(msg, sender) {
    const tabId = sender && sender.tab && sender.tab.id;
    if (!msg || typeof msg.t !== "string") return undefined;
    if (msg.t === "xhello") {
      if (current && tabId === current.tabId) {
        const j = current.job;
        return { active: true, job: { id: j.id, kind: j.kind, key: j.key, focalId: j.tweetId || "", ownerRestId: "" } };
      }
      return { active: false };
    }
    if (!current || tabId !== current.tabId || msg.jobId !== current.job.id) return undefined;
    const c = current.collector;
    if (msg.t === "xpiece") {
      addPiece(c, msg.piece, now());
    } else if (msg.t === "xpage") {
      addPageState(c, msg.state);
      return { done: isComplete(c, now()) };
    } else if (msg.t === "xhttp") {
      c.http.push({ op: msg.op, status: msg.status });
      if (msg.status === 429) c.state = c.state || "limited";
      else if (msg.status === 401) c.state = c.state || "login";
      else if (msg.status === 403) c.state = c.state || "locked";
    } else if (msg.t === "xrate") {
      if (msg.rate && String(msg.rate.remaining) === "0") c.rateExhausted = true;
    }
    return { done: isComplete(c, now()) };
  }

  async function waitFor(c, tabId) {
    const t0 = now();
    let triedDom = false;
    for (;;) {
      if (isComplete(c, now())) return;
      const age = now() - t0;
      if (age >= JOB_TIMEOUT_MS) return;
      if (!triedDom && age >= DOM_FALLBACK_AT_MS && c.pieces === 0 && !c.state) {
        triedDom = true;
        try {
          const r = await chrome.tabs.sendMessage(tabId, { t: "xdom", kind: c.job.kind });
          if (r && r.piece) {
            c.source = "dom";
            addPiece(c, r.piece, now());
          }
        } catch {
          /* no content script in the tab: nothing to read */
        }
      }
      await sleep(POLL_MS);
    }
  }

  async function runJob(job) {
    const t0 = now();
    if (!jobUrlOk(job)) {
      await ask({ t: "xresult", id: job && job.id, ok: false, state: "bad_job", error: "The app sent an address that is not an x.com page this extension may open." });
      return;
    }
    const cached = pacer.cacheGet(`${job.kind}:${job.key}`, t0);
    if (cached) {
      await ask({ ...cached, id: job.id, cached: true });
      return;
    }
    const c = newCollector(job, t0);
    let tabId;
    try {
      pacer.recordLoad(t0);
      current = { job, collector: c, tabId: -1 };
      tabId = await tabFor(job.url);
      current.tabId = tabId;
    } catch (e) {
      current = null;
      await ask({ t: "xresult", id: job.id, ok: false, state: "tab", error: "Could not open a background tab in Chrome." });
      await persist();
      return;
    }
    try {
      await waitFor(c, tabId);
    } finally {
      // keep `current` until the result is built, then drop it so stray pages are ignored
    }
    const result = buildResult(c, now());
    if (c.rateExhausted && result.ok && !c.state) result.rateLow = true;
    current = null;
    if (result.ok) {
      loggedIn = true;
      pacer.cachePut(`${job.kind}:${job.key}`, result, now());
    }
    if (c.state === "login") loggedIn = false;
    if (ACCOUNT_PROBLEM.has(c.state)) {
      const until = pacer.onLimited(now(), c.state);
      log(`X paused until ${new Date(until).toISOString()} (${c.state})`);
    }
    last = { at: now(), kind: job.kind, state: result.state, error: result.ok ? "" : result.error };
    await ask(result);
    await persist();
  }

  async function loop() {
    await restore();
    while (running) {
      try {
        const settings = getSettings();
        pacer.setPace(settings.pace);
        if (!settings.enabled || settings.paused || !connected()) {
          await sleep(IDLE_POLL_MS);
          continue;
        }
        if (loggedIn === false && now() - last.at < LOGIN_RETRY_MS) {
          // x.com showed a login page: do not keep loading pages. Look again in a few minutes.
          await ask({ t: "xstatus", ...status() });
          await sleep(30_000);
          continue;
        }
        const can = pacer.canLoad(now());
        if (!can.ok) {
          // keep the app informed (it shows "X paused: account limited") but do not ask for work
          await ask({ t: "xstatus", ...status() });
          await sleep(Math.min(can.waitMs, can.reason === "paused" ? 30_000 : 10_000));
          continue;
        }
        const r = await ask({ t: "xnext", ...status() });
        if (!r || !r.job) {
          await sleep(IDLE_POLL_MS);
          continue;
        }
        await runJob(r.job);
        await sleep(gapMs(settings.pace, rand));
      } catch (e) {
        log(`x runner: ${e && e.message}`);
        await sleep(IDLE_POLL_MS);
      }
    }
  }

  return {
    start() {
      if (running) return;
      running = true;
      loop();
    },
    stop() {
      running = false;
    },
    onMessage,
    status,
    runJob,
    get pacer() {
      return pacer;
    },
    get current() {
      return current;
    },
  };
}
