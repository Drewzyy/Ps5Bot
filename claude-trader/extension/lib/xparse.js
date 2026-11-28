// X Monitor: pure parsers for the JSON that x.com's own pages fetch (GraphQL), plus text helpers for the DOM fallback.
// Classic script (no import/export) so a content script can load it; it sets globalThis.CTXParse. Tests load it with node:vm.
//
// Rules:
// - X rotates query ids, wrappers and nesting every few weeks. Every function is defensive: a missing field gives a
//   default, never a throw, and a response whose shape we do not recognise returns null (the caller reports "drift").
// - Nothing here makes a request. The parsers only read JSON that the page already fetched.
(function (root) {
  "use strict";

  const MAX_NODES = 250000; // walk budget, so a huge or hostile response cannot freeze the page
  const num = (v) => {
    const n = typeof v === "string" ? Number(v.replace(/,/g, "")) : Number(v);
    return Number.isFinite(n) && n >= 0 ? Math.round(n) : 0;
  };
  const str = (v, max) => (typeof v === "string" ? v.slice(0, max || 2000) : "");
  const obj = (v) => (v && typeof v === "object" && !Array.isArray(v) ? v : null);
  const first = (...vs) => {
    for (const v of vs) if (v !== undefined && v !== null) return v;
    return undefined;
  };

  /** "Tue Jun 02 20:12:29 +0000 2009", an ISO string, or a number (seconds or ms) to ms since 1970. 0 when unknown. */
  function toMs(v) {
    if (typeof v === "number" && Number.isFinite(v)) return v > 1e11 ? Math.round(v) : Math.round(v * 1000);
    if (typeof v === "string" && v) {
      if (/^\d{9,13}$/.test(v)) return toMs(Number(v));
      const t = Date.parse(v);
      return Number.isFinite(t) ? t : 0;
    }
    return 0;
  }

  // ------------------------------------------------------------------ which response is this

  const OP_URL = /\/i\/api\/graphql\/[^/?#]+\/([A-Za-z0-9_]+)/;
  // X rotates the id in the URL, never (so far) the operation name. We match by NAME only, and only read these. The names come from
  // twscrape's api.py (a maintained X client) and from our own earlier guesses: AboutAccountQuery, the community operations
  // (CommunityQuery, CommunityTweetsTimeline, membersSliceTimeline_Query, moderatorsSliceTimeline_Query) and the user and post ones.
  // xhook.js carries the same list (it runs in the page, so it cannot import this file); a test keeps the two in step.
  const OPS = {
    AboutAccountQuery: "about",
    UserByScreenName: "user",
    UserByRestId: "user",
    UserTweets: "posts",
    UserTweetsAndReplies: "posts",
    TweetDetail: "detail",
    TweetResultByRestId: "detail",
    CommunityQuery: "community",
    CommunityByRestId: "community",
    CommunityTweetsTimeline: "community",
    membersSliceTimeline_Query: "community",
    moderatorsSliceTimeline_Query: "community",
  };
  const WANTED = /^(Communit[A-Za-z]*|[A-Za-z]*FollowersYouFollow[A-Za-z]*)$/;

  function opName(url) {
    const m = OP_URL.exec(String(url || ""));
    return m ? m[1] : null;
  }

  /** user | about | posts | detail | community | smart, or null for an operation we do not read. */
  function classify(op) {
    if (!op) return null;
    if (Object.prototype.hasOwnProperty.call(OPS, op)) return OPS[op];
    if (!WANTED.test(op)) return null;
    if (/^Communit/.test(op)) return "community";
    if (/FollowersYouFollow/.test(op)) return "smart";
    return null;
  }

  // ------------------------------------------------------------------ errors inside a 200 response

  /** GraphQL errors that mean the account is limited or locked, not that the data is missing. */
  function errorState(json) {
    const errs = Array.isArray(json && json.errors) ? json.errors : [];
    for (const e of errs) {
      const code = Number(e && e.code);
      const msg = String((e && e.message) || "");
      if (code === 88 || /rate limit/i.test(msg)) return "limited";
      if (code === 326 || /temporarily locked|account is locked|suspicious activity/i.test(msg)) return "locked";
      if (code === 32 || code === 215 || /could not authenticate|bad authentication/i.test(msg)) return "login";
    }
    return null;
  }

  // ------------------------------------------------------------------ users

  function idFromGlobal(id) {
    // "VXNlcjo0NDE5NjM5Nw==" decodes to "User:44196397"
    try {
      const s = typeof atob === "function" ? atob(String(id)) : Buffer.from(String(id), "base64").toString("utf8");
      const m = /^User:(\d+)$/.exec(s);
      return m ? m[1] : "";
    } catch {
      return "";
    }
  }

  /** One user result (any of the layouts seen so far). Returns null when it has no handle. */
  function userFromResult(r) {
    const u = obj(r);
    if (!u) return null;
    const legacy = obj(u.legacy) || {};
    const core = obj(u.core) || {};
    const privacy = obj(u.privacy) || {};
    const screenName = str(first(legacy.screen_name, core.screen_name, u.screen_name), 40);
    if (!screenName) return null;
    const avatarUrl = str((obj(u.avatar) || {}).image_url || legacy.profile_image_url_https, 300);
    return {
      restId: str(first(u.rest_id, legacy.id_str, u.id_str) || idFromGlobal(u.id), 30),
      screenName,
      name: str(first(legacy.name, core.name, u.name), 80),
      createdAtMs: toMs(first(legacy.created_at, core.created_at, u.created_at)),
      followers: num(first(legacy.followers_count, u.followers_count, (obj(u.public_metrics) || {}).followers_count)),
      following: num(first(legacy.friends_count, u.friends_count, legacy.following_count, (obj(u.public_metrics) || {}).following_count)),
      statuses: num(first(legacy.statuses_count, u.statuses_count)),
      listed: num(first(legacy.listed_count, u.listed_count)),
      verified: !!first(u.is_blue_verified, legacy.verified, (obj(u.verification) || {}).verified, false),
      protected: !!first(legacy.protected, privacy.protected, u.protected, false),
      defaultAvatar: !!first(legacy.default_profile_image, /default_profile/.test(avatarUrl), false),
      description: str(first(legacy.description, (obj(u.profile_bio) || {}).description, u.description), 400),
    };
  }

  /** UserByScreenName / UserByRestId. { state: ok | not_found | suspended | protected | unavailable | limited | locked | login, user } or null if unrecognised. */
  function parseUser(json) {
    const j = obj(json);
    if (!j) return null;
    const es = errorState(j);
    const data = obj(j.data);
    if (!data) return es ? { state: es, user: null } : null;
    const holder = obj(data.user) || obj(data.user_result) || obj(data.user_results);
    if (!holder || !obj(holder.result)) {
      // data: {} or user: {} means the account does not exist; any other shape is drift
      if (es) return { state: es, user: null };
      return Object.keys(data).length === 0 || holder ? { state: "not_found", user: null } : null;
    }
    const r = holder.result;
    if (r.__typename === "UserUnavailable" || (!r.legacy && !r.core && r.reason)) {
      const why = String(r.reason || "").toLowerCase();
      return { state: /suspend/.test(why) ? "suspended" : /not.?found|deactivat/.test(why) ? "not_found" : "unavailable", user: null };
    }
    const user = userFromResult(r);
    if (!user) return null;
    return { state: user.protected ? "protected" : "ok", user };
  }

  // ------------------------------------------------------------------ tweets

  function tweetFromResult(r0) {
    let r = obj(r0);
    if (!r) return null;
    if (r.__typename === "TweetTombstone") return null;
    if (r.__typename === "TweetWithVisibilityResults" || (obj(r.tweet) && !r.legacy)) r = obj(r.tweet);
    if (!r) return null;
    const legacy = obj(r.legacy) || {};
    const id = str(first(r.rest_id, legacy.id_str), 30);
    if (!id) return null;
    const note = obj(obj(obj(r.note_tweet) && r.note_tweet.note_tweet_results) && r.note_tweet.note_tweet_results.result);
    const author = userFromResult(obj(obj(r.core) && r.core.user_results) && r.core.user_results.result);
    const inner = obj(obj(legacy.retweeted_status_result) && legacy.retweeted_status_result.result);
    const comm = obj(obj(r.community_results) && r.community_results.result);
    return {
      id,
      text: str(first(note && note.text, legacy.full_text, legacy.text), 1500),
      createdAtMs: toMs(legacy.created_at),
      views: num(obj(r.views) && r.views.count),
      likes: num(legacy.favorite_count),
      replies: num(legacy.reply_count),
      reposts: num(legacy.retweet_count),
      quotes: num(legacy.quote_count),
      bookmarks: num(legacy.bookmark_count),
      isRetweet: !!inner || /^RT @/.test(str(legacy.full_text, 8)),
      inReplyToId: str(legacy.in_reply_to_status_id_str, 30),
      communityId: str(first(comm && comm.rest_id, r.community && r.community.rest_id), 30),
      author,
    };
  }

  /** Every `tweet_results.result` in the response, in document order, without repeats. */
  function walkTweets(json) {
    const out = [];
    const seen = new Set();
    let budget = MAX_NODES;
    (function walk(n, depth) {
      if (!n || typeof n !== "object" || budget-- <= 0 || depth > 40) return;
      if (Array.isArray(n)) {
        for (const x of n) walk(x, depth + 1);
        return;
      }
      if (n.tweet_results && obj(n.tweet_results.result)) {
        const t = tweetFromResult(n.tweet_results.result);
        if (t && !seen.has(t.id)) {
          seen.add(t.id);
          out.push(t);
        }
      }
      // community timelines list their posts as `items_results: [{ result: <tweet> }]` (with slice_info.next_cursor), not as entries
      if (Array.isArray(n.items_results)) {
        for (const it of n.items_results) {
          const r = obj(it) && obj(it.result);
          if (r && (r.__typename === "Tweet" || r.__typename === "TweetWithVisibilityResults" || (obj(r.legacy) && r.legacy.full_text !== undefined))) {
            const t = tweetFromResult(r);
            if (t && !seen.has(t.id)) {
              seen.add(t.id);
              out.push(t);
            }
          }
        }
      }
      for (const k of Object.keys(n)) if (k !== "tweet_results") walk(n[k], depth + 1);
    })(json, 0);
    return out;
  }

  /** UserTweets: the account's own posts, newest first as X sends them (retweets left out). */
  function parsePosts(json, ownerRestId) {
    const j = obj(json);
    if (!j || !obj(j.data)) return null;
    const all = walkTweets(j.data);
    const own = all.filter((t) => !t.isRetweet && (!ownerRestId || (t.author && t.author.restId === ownerRestId)));
    return own.slice(0, 20);
  }

  /** TweetDetail / TweetResultByRestId: the focal post and its direct replies. */
  function parseDetail(json, focalId) {
    const j = obj(json);
    if (!j || !obj(j.data)) return null;
    const all = walkTweets(j.data);
    let tweet = null;
    if (focalId) tweet = all.find((t) => t.id === String(focalId)) || null;
    else tweet = all[0] || null;
    if (!tweet) return null;
    const ownAuthor = tweet.author && tweet.author.restId;
    const replies = all
      .filter((t) => t.id !== tweet.id && t.inReplyToId === tweet.id && !(ownAuthor && t.author && t.author.restId === ownAuthor))
      .slice(0, 60)
      .map((t) => ({
        id: t.id,
        text: t.text,
        likes: t.likes,
        authorRestId: t.author ? t.author.restId : "",
        authorScreenName: t.author ? t.author.screenName : "",
        authorCreatedAtMs: t.author ? t.author.createdAtMs : 0,
        authorFollowers: t.author ? t.author.followers : 0,
        defaultAvatar: t.author ? t.author.defaultAvatar : false,
      }));
    return { tweet, replies };
  }

  // ------------------------------------------------------------------ communities and smart followers

  /** Community pages: the operation names are not documented, so find the community object by its member count. */
  function parseCommunity(json) {
    const j = obj(json);
    if (!j || !obj(j.data)) return null;
    let found = null;
    let budget = MAX_NODES;
    (function walk(n, depth) {
      if (found || !n || typeof n !== "object" || budget-- <= 0 || depth > 40) return;
      if (Array.isArray(n)) {
        for (const x of n) walk(x, depth + 1);
        return;
      }
      if (first(n.member_count, n.members_count, n.membersCount) !== undefined && (n.name || n.rest_id || n.id_str)) {
        found = n;
        return;
      }
      for (const k of Object.keys(n)) walk(n[k], depth + 1);
    })(j.data, 0);
    const posts = walkTweets(j.data);
    const latest = posts.reduce((m, t) => Math.max(m, t.createdAtMs || 0), 0);
    if (!found && !posts.length) return null;
    const c = found || {};
    return {
      id: str(first(c.rest_id, c.id_str, c.id), 30),
      name: str(c.name, 100),
      members: found ? num(first(c.member_count, c.members_count, c.membersCount)) : null,
      createdAtMs: toMs(first(c.created_at, c.created_at_ms)),
      description: str(c.description, 300),
      postsSeen: posts.length,
      latestPostMs: latest,
    };
  }

  /**
   * AboutAccountQuery (the "About this account" page): `about_profile` with `username_changes {count, last_changed_at_msec}`,
   * `account_based_in`, `location_accurate`, `affiliate_username` and `source`. X leaves `username_changes` out when there are
   * none, so a missing field is "unknown" (changesKnown false), never "0 changes". Returns null when no about_profile is found.
   */
  function parseAbout(json) {
    const j = obj(json);
    if (!j || !obj(j.data)) return null;
    let found = null;
    let budget = MAX_NODES;
    (function walk(n, depth) {
      if (found || !n || typeof n !== "object" || budget-- <= 0 || depth > 40) return;
      if (Array.isArray(n)) {
        for (const x of n) walk(x, depth + 1);
        return;
      }
      if (obj(n.about_profile)) {
        found = n.about_profile;
        return;
      }
      for (const k of Object.keys(n)) walk(n[k], depth + 1);
    })(j.data, 0);
    if (!found) return null;
    const uc = obj(found.username_changes);
    const known = !!uc && uc.count !== undefined && uc.count !== null;
    const acc = found.location_accurate;
    return {
      usernameChanges: known ? num(uc.count) : 0,
      changesKnown: known,
      lastChangedMs: uc ? toMs(first(uc.last_changed_at_msec, uc.last_changed_at)) : 0,
      country: str(found.account_based_in, 60),
      locationAccurate: typeof acc === "boolean" ? acc : acc === "true" ? true : acc === "false" ? false : null,
      affiliate: str(found.affiliate_username, 15).replace(/^@/, ""),
      source: str(found.source, 60),
    };
  }

  /** membersSliceTimeline_Query / moderatorsSliceTimeline_Query: `items_results` of users with `slice_info.next_cursor`. */
  function parseCommunitySlice(json, op) {
    const j = obj(json);
    if (!j || !obj(j.data)) return null;
    const users = [];
    const seen = new Set();
    let sawSlice = false;
    let budget = MAX_NODES;
    (function walk(n, depth) {
      if (!n || typeof n !== "object" || budget-- <= 0 || depth > 40) return;
      if (Array.isArray(n)) {
        for (const x of n) walk(x, depth + 1);
        return;
      }
      if (Array.isArray(n.items_results) || obj(n.slice_info)) sawSlice = true;
      if (Array.isArray(n.items_results)) {
        for (const it of n.items_results) {
          const u = obj(it) && obj(it.result) ? userFromResult(it.result) : null;
          if (u && !seen.has(u.screenName.toLowerCase())) {
            seen.add(u.screenName.toLowerCase());
            users.push(u);
          }
        }
      }
      for (const k of Object.keys(n)) walk(n[k], depth + 1);
    })(j.data, 0);
    if (!sawSlice) return null;
    const handles = users.map((u) => u.screenName);
    return /^moderators/i.test(String(op)) ? { moderators: handles.slice(0, 20), membersSample: 0 } : { moderators: [], membersSample: Math.min(handles.length, 1000) };
  }

  /** followers_you_follow: accounts the logged-in spare account follows that also follow the target. */
  function parseSmart(json) {
    const j = obj(json);
    if (!j || !obj(j.data)) return null;
    const users = [];
    const seen = new Set();
    let budget = MAX_NODES;
    let sawList = false;
    (function walk(n, depth) {
      if (!n || typeof n !== "object" || budget-- <= 0 || depth > 40) return;
      if (Array.isArray(n)) {
        for (const x of n) walk(x, depth + 1);
        return;
      }
      if (n.user_results && obj(n.user_results.result)) {
        sawList = true;
        const u = userFromResult(n.user_results.result);
        if (u && !seen.has(u.screenName.toLowerCase())) {
          seen.add(u.screenName.toLowerCase());
          users.push({ handle: u.screenName, followers: u.followers, restId: u.restId });
        }
      }
      if (Array.isArray(n.instructions) || n.timeline) sawList = true;
      for (const k of Object.keys(n)) if (k !== "user_results") walk(n[k], depth + 1);
    })(j.data, 0);
    if (!sawList) return null;
    const hasMore = /"cursor-bottom"|Bottom"/.test(safeSlice(j.data));
    return { count: users.length, handles: users.slice(0, 40), topFollowers: users.reduce((m, u) => Math.max(m, u.followers || 0), 0), hasMore };
  }

  function safeSlice(d) {
    try {
      return JSON.stringify(d).slice(0, 400000);
    } catch {
      return "";
    }
  }

  /** One response from the page. Returns { kind, ... } or null when it is not something we read or the shape drifted. */
  function parseResponse(op, json, ctx) {
    const kind = classify(op);
    if (!kind) return null;
    const c = ctx || {};
    const es = errorState(json);
    if (kind === "user") {
      const r = parseUser(json);
      return r ? { kind, state: r.state, user: r.user } : es ? { kind, state: es } : { kind, drift: true };
    }
    if (kind === "posts") {
      const p = parsePosts(json, c.ownerRestId);
      return p ? { kind, posts: p } : es ? { kind, state: es } : { kind, drift: true };
    }
    if (kind === "detail") {
      const d = parseDetail(json, c.focalId);
      return d ? { kind, tweet: d.tweet, replies: d.replies } : es ? { kind, state: es } : { kind, drift: true };
    }
    if (kind === "about") {
      const a = parseAbout(json);
      return a ? { kind, about: a } : es ? { kind, state: es } : { kind, drift: true };
    }
    if (kind === "community") {
      if (/^(members|moderators)SliceTimeline/i.test(String(op))) {
        const s = parseCommunitySlice(json, op);
        return s ? { kind, community: { id: "", name: "", members: null, createdAtMs: 0, description: "", postsSeen: 0, latestPostMs: 0, moderators: s.moderators, membersSample: s.membersSample } } : es ? { kind, state: es } : { kind, drift: true };
      }
      const d = parseCommunity(json);
      return d ? { kind, community: d } : es ? { kind, state: es } : { kind, drift: true };
    }
    const s = parseSmart(json);
    return s ? { kind, smart: s } : es ? { kind, state: es } : { kind, drift: true };
  }

  // ------------------------------------------------------------------ DOM fallback helpers (text only, pure)

  /** "1,234", "1.2K", "3.4M", "2B" to a number. 0 when it is not a count. */
  function parseCount(text) {
    const m = /([\d][\d.,]*)\s*([KMB])?/i.exec(String(text || ""));
    if (!m) return 0;
    let n = m[1];
    const suffix = (m[2] || "").toUpperCase();
    if (suffix) n = n.replace(/,/g, ".");
    else n = n.replace(/,/g, "");
    const v = Number(n);
    if (!Number.isFinite(v)) return 0;
    const mult = suffix === "K" ? 1e3 : suffix === "M" ? 1e6 : suffix === "B" ? 1e9 : 1;
    return Math.round(v * mult);
  }

  /** "Joined June 2009" to the first of that month (ms). 0 when unknown. */
  function parseJoined(text) {
    const m = /Joined\s+([A-Za-z]+)\s+(\d{4})/.exec(String(text || ""));
    if (!m) return 0;
    const t = Date.parse(`${m[1]} 1, ${m[2]} 00:00:00 UTC`);
    return Number.isFinite(t) ? t : 0;
  }

  /** The aria-label on a post's action row: "12 replies, 34 reposts, 56 likes, 7 bookmarks, 1,234 views". */
  function parseMetricsLabel(label) {
    const out = { replies: 0, reposts: 0, likes: 0, bookmarks: 0, views: 0 };
    const re = /([\d.,]+\s*[KMB]?)\s+(repl(?:y|ies)|reposts?|likes?|bookmarks?|views?)/gi;
    let m;
    while ((m = re.exec(String(label || "")))) {
      const k = m[2].toLowerCase();
      const v = parseCount(m[1]);
      if (k.startsWith("repl")) out.replies = v;
      else if (k.startsWith("repost")) out.reposts = v;
      else if (k.startsWith("like")) out.likes = v;
      else if (k.startsWith("bookmark")) out.bookmarks = v;
      else out.views = v;
    }
    return out;
  }

  /** Profile fields read from the page text. Only used when the network data did not arrive. */
  function profileFromDom(f) {
    const handle = str(f.handle, 40).replace(/^@/, "");
    if (!handle) return null;
    return {
      restId: "",
      screenName: handle,
      name: str(f.name, 80),
      createdAtMs: parseJoined(f.joinedText),
      followers: parseCount(f.followersText),
      following: parseCount(f.followingText),
      statuses: 0,
      listed: 0,
      verified: false,
      protected: false,
      defaultAvatar: false,
      description: str(f.bio, 400),
    };
  }

  // ------------------------------------------------------------------ what page are we on

  /**
   * Read what the page itself says. `text` is the visible text (first few thousand characters), `url` the address.
   * Returns ok | login | locked | limited | suspended | not_found | protected.
   */
  function detectPage(p) {
    const url = String((p && p.url) || "");
    const text = String((p && p.text) || "").slice(0, 8000);
    if (/\/i\/flow\/login|\/login(\?|$)/.test(url) || /^(Sign in to X|Log in to X)/i.test(text.trim())) return "login";
    if (/\/account\/access|\/i\/flow\/(consent|verify)/.test(url) || /(account|it) (has been|is|was) (temporarily )?locked|unusual login activity|confirm you.?re not a robot|verify your identity/i.test(text)) return "locked";
    if (/rate limit exceeded|you are rate limited|too many requests/i.test(text)) return "limited";
    if (/account suspended|this account has been suspended/i.test(text)) return "suspended";
    if (/(posts|tweets) are protected|only approved followers can see/i.test(text)) return "protected";
    if (/this account doesn.?t exist|hmm\.\.\.\s*this page doesn.?t exist|this post is from an account that no longer exists|this post was deleted/i.test(text)) return "not_found";
    return "ok";
  }

  const api = { opName, classify, errorState, parseUser, parsePosts, parseDetail, parseCommunity, parseSmart, parseAbout, parseCommunitySlice, parseResponse, parseCount, parseJoined, parseMetricsLabel, profileFromDom, detectPage, userFromResult, tweetFromResult, toMs };
  root.CTXParse = api;
})(globalThis);
