// X Monitor pacing. Pure logic, no Chrome APIs, so it can be tested with a fake clock.
// Goal: look like a person reading a few pages, not a crawler. One navigation every 8 to 15 s (with jitter),
// at most 120 page loads an hour, 800 a day, and a long pause the moment X says the account is limited.

export const DEFAULT_PACE = { minGapSecs: 8, maxGapSecs: 15, maxLoadsPerHour: 120, maxLoadsPerDay: 800, cacheMinutes: 30 };

const HOUR = 3600_000;
const DAY = 24 * HOUR;
/** Back-off after X limits or locks the account: 15 minutes, then 1 hour, then the rest of the day. */
export const BACKOFF_MS = [15 * 60_000, HOUR, DAY];

const clampNum = (v, lo, hi, d) => {
  const n = Number(v);
  return Number.isFinite(n) ? Math.min(hi, Math.max(lo, n)) : d;
};

/** Settings from the app, forced into safe ranges (a bad value can never make the extension faster than the hard floor). */
export function cleanPace(s) {
  const o = s || {};
  const min = clampNum(o.minGapSecs, 5, 120, DEFAULT_PACE.minGapSecs);
  const max = Math.max(min, clampNum(o.maxGapSecs, 5, 300, DEFAULT_PACE.maxGapSecs));
  return {
    minGapSecs: min,
    maxGapSecs: max,
    maxLoadsPerHour: Math.round(clampNum(o.maxLoadsPerHour, 1, 200, DEFAULT_PACE.maxLoadsPerHour)),
    maxLoadsPerDay: Math.round(clampNum(o.maxLoadsPerDay, 1, 1000, DEFAULT_PACE.maxLoadsPerDay)),
    cacheMinutes: Math.round(clampNum(o.cacheMinutes, 1, 240, DEFAULT_PACE.cacheMinutes)),
  };
}

/** Wait between two navigations, in ms: uniform between min and max seconds. `rand` returns [0,1). */
export function gapMs(pace, rand = Math.random) {
  const p = cleanPace(pace);
  return Math.round((p.minGapSecs + rand() * (p.maxGapSecs - p.minGapSecs)) * 1000);
}

export class Pacer {
  constructor(pace, saved) {
    this.pace = cleanPace(pace);
    this.loads = Array.isArray(saved?.loads) ? saved.loads.filter((t) => Number.isFinite(t)) : [];
    this.strikes = saved?.strikes || 0;
    this.lastStrike = saved?.lastStrike || 0;
    this.pausedUntil = saved?.pausedUntil || 0;
    this.pausedReason = saved?.pausedReason || "";
    this.lastLoad = saved?.lastLoad || 0;
    this.cache = new Map(Array.isArray(saved?.cache) ? saved.cache : []);
  }

  setPace(pace) {
    this.pace = cleanPace(pace);
  }

  toJSON() {
    return { loads: this.loads, strikes: this.strikes, lastStrike: this.lastStrike, pausedUntil: this.pausedUntil, pausedReason: this.pausedReason, lastLoad: this.lastLoad, cache: [...this.cache.entries()].slice(-300) };
  }

  loadsIn(now, ms) {
    return this.loads.filter((t) => now - t < ms).length;
  }

  /** Can one more page be opened right now? If not: how long to wait and why. */
  canLoad(now) {
    this.loads = this.loads.filter((t) => now - t < DAY);
    if (now < this.pausedUntil) return { ok: false, waitMs: this.pausedUntil - now, reason: "paused" };
    const gap = this.lastLoad ? this.lastLoad + this.pace.minGapSecs * 1000 - now : 0;
    if (gap > 0) return { ok: false, waitMs: gap, reason: "gap" };
    const hour = this.loads.filter((t) => now - t < HOUR);
    if (hour.length >= this.pace.maxLoadsPerHour) return { ok: false, waitMs: Math.max(1000, hour[0] + HOUR - now), reason: "hour" };
    if (this.loads.length >= this.pace.maxLoadsPerDay) return { ok: false, waitMs: Math.max(1000, this.loads[0] + DAY - now), reason: "day" };
    return { ok: true, waitMs: 0, reason: "" };
  }

  recordLoad(now) {
    this.loads.push(now);
    this.lastLoad = now;
  }

  /** X limited or locked the account. Returns when the pause ends. Each new incident within 6 hours doubles the wait. */
  onLimited(now, reason) {
    if (now - this.lastStrike > 6 * HOUR) this.strikes = 0;
    const wait = BACKOFF_MS[Math.min(this.strikes, BACKOFF_MS.length - 1)];
    this.strikes += 1;
    this.lastStrike = now;
    this.pausedUntil = now + wait;
    this.pausedReason = reason || "limited";
    return this.pausedUntil;
  }

  paused(now) {
    return now < this.pausedUntil;
  }

  status(now) {
    return { loadsLastHour: this.loadsIn(now, HOUR), loadsToday: this.loadsIn(now, DAY), paused: this.paused(now), pausedUntil: this.paused(now) ? this.pausedUntil : 0, pausedReason: this.paused(now) ? this.pausedReason : "", strikes: this.strikes };
  }

  // cache: one entry per handle / tweet / community, 30 minutes by default. A cached page is never loaded again inside that time.
  cacheGet(key, now) {
    const e = this.cache.get(key);
    if (!e) return null;
    if (now - e.at > this.pace.cacheMinutes * 60_000) {
      this.cache.delete(key);
      return null;
    }
    return e.value;
  }

  cachePut(key, value, now) {
    this.cache.set(key, { at: now, value });
    if (this.cache.size > 300) this.cache.delete(this.cache.keys().next().value);
  }
}
