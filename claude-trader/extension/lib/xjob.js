// X Monitor job logic. Pure: turns the pieces the page's own responses delivered into one structured result.
// The background worker feeds it; the tests feed it fixtures.

const URL_RE = {
  profile: /^https:\/\/x\.com\/[A-Za-z0-9_]{1,15}$/,
  smart: /^https:\/\/x\.com\/[A-Za-z0-9_]{1,15}\/followers_you_follow$/,
  tweet: /^https:\/\/x\.com\/(?:[A-Za-z0-9_]{1,15}|i\/web)\/status\/\d{5,25}$/,
  community: /^https:\/\/x\.com\/i\/communities\/\d{5,25}$/,
};

/** First path segments on x.com that are pages, not accounts. A job may never open them. */
export const RESERVED = new Set(["home", "i", "compose", "intent", "search", "explore", "settings", "notifications", "messages", "login", "logout", "signup", "share", "hashtag", "tos", "privacy", "about", "account", "download", "jobs", "lists", "bookmarks", "premium", "verified", "communities", "support", "help"]);

/** The app tells us what to open. We only ever open an x.com address of exactly these four shapes. */
export function jobUrlOk(job) {
  if (!job || typeof job.url !== "string" || !URL_RE[job.kind] || !URL_RE[job.kind].test(job.url)) return false;
  const first = job.url.slice("https://x.com/".length).split("/")[0].toLowerCase();
  if (job.kind === "community") return true;
  if (job.kind === "tweet" && first === "i") return true;
  return !RESERVED.has(first);
}

/** Terminal page states: we stop waiting as soon as one shows up. */
export const TERMINAL = new Set(["not_found", "suspended", "protected", "limited", "locked", "login"]);
/** States that mean the account (not the target) has a problem: pause everything. */
export const ACCOUNT_PROBLEM = new Set(["limited", "locked"]);

export function newCollector(job, now) {
  return { job, startedAt: now, user: null, userAt: 0, state: "", posts: [], tweet: null, replies: [], community: null, communityAt: 0, smart: null, drift: false, http: [], source: "graphql", pieces: 0 };
}

/** Add one parsed piece (the output of CTXParse.parseResponse) to the collector. */
export function addPiece(c, piece, now) {
  if (!piece) return c;
  c.pieces += 1;
  if (piece.drift) c.drift = true;
  if (piece.state && TERMINAL.has(piece.state)) c.state = c.state || piece.state;
  if (piece.kind === "user") {
    if (piece.state === "ok" || piece.state === "protected") {
      c.user = piece.user;
      c.userAt = now;
      if (piece.state === "protected") c.state = c.state || "protected";
    } else if (piece.state && piece.state !== "unavailable") c.state = c.state || piece.state;
  } else if (piece.kind === "posts") {
    if (Array.isArray(piece.posts)) c.posts = piece.posts;
  } else if (piece.kind === "detail") {
    if (piece.tweet) {
      c.tweet = piece.tweet;
      c.replies = piece.replies || [];
    }
  } else if (piece.kind === "community") {
    if (piece.community) {
      const prev = c.community || {};
      const n = piece.community;
      c.community = { id: n.id || prev.id || "", name: n.name || prev.name || "", members: n.members ?? prev.members ?? null, createdAtMs: n.createdAtMs || prev.createdAtMs || 0, description: n.description || prev.description || "", postsSeen: Math.max(n.postsSeen || 0, prev.postsSeen || 0), latestPostMs: Math.max(n.latestPostMs || 0, prev.latestPostMs || 0) };
      c.communityAt = now;
    }
  } else if (piece.kind === "smart") {
    if (piece.smart) c.smart = piece.smart;
  }
  return c;
}

/** Page-level state from the DOM check (login wall, rate limit page, suspended account ...). */
export function addPageState(c, state) {
  if (state && state !== "ok" && TERMINAL.has(state)) c.state = c.state || state;
  return c;
}

/** Do we have what this kind of job needs? Posts and community posts may trail the main response by a moment (grace). */
export function isComplete(c, now, graceMs = 4000) {
  if (c.state) return true;
  switch (c.job.kind) {
    case "profile":
      return !!c.user && (c.posts.length > 0 || now - c.userAt >= graceMs);
    case "tweet":
      return !!c.tweet;
    case "community":
      return !!c.community && c.community.members != null && (c.community.postsSeen > 0 || now - c.communityAt >= graceMs);
    case "smart":
      return !!c.smart;
    default:
      return false;
  }
}

/** The structured result sent to the app. `ok` is true when we learned something about the target (even "it does not exist"). */
export function buildResult(c, now) {
  const base = { t: "xresult", jobId: c.job.id, kind: c.job.kind, key: c.job.key, source: c.source, ms: Math.max(0, now - c.startedAt), drift: c.drift };
  if (c.state === "limited" || c.state === "locked" || c.state === "login") return { ...base, ok: false, state: c.state, error: stateText(c.state) };
  if (c.state === "not_found" || c.state === "suspended") return { ...base, ok: true, state: c.state, error: "" };
  const have = c.user || c.tweet || c.community || c.smart;
  if (!have && c.state !== "protected") return { ...base, ok: false, state: c.drift ? "drift" : "timeout", error: c.drift ? "X changed its page format and the parser could not read it." : "No data arrived in time." };
  const out = { ...base, ok: true, state: c.state === "protected" ? "protected" : "ok", error: "" };
  if (c.user) out.user = c.user;
  if (c.posts.length) out.posts = c.posts;
  if (c.tweet) {
    out.tweet = c.tweet;
    out.replies = c.replies;
  }
  if (c.community) out.community = c.community;
  if (c.smart) out.smart = c.smart;
  return out;
}

export function stateText(s) {
  return { limited: "X says this account is rate limited.", locked: "X locked this account or asked it to verify.", login: "x.com is not logged in.", not_found: "That account or post does not exist.", suspended: "That account is suspended.", protected: "That account is protected." }[s] || "";
}
