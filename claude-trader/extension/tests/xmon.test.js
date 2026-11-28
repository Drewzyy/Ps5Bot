// X Monitor tests: GraphQL parsers on SYNTHETIC fixtures (see make-fixtures.mjs), pacing, jobs, and the runner with a fake browser.
import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";
import { Pacer, gapMs, cleanPace, BACKOFF_MS, DEFAULT_PACE } from "../lib/xpace.js";
import { jobUrlOk, newCollector, addPiece, addPageState, isComplete, buildResult } from "../lib/xjob.js";
import { createXRunner, JOB_TIMEOUT_MS, DOM_FALLBACK_AT_MS } from "../xrunner.js";

const ctx = {};
vm.runInNewContext(fs.readFileSync(new URL("../lib/xparse.js", import.meta.url), "utf8"), ctx);
const P = ctx.CTXParse;
const J = (x) => JSON.parse(JSON.stringify(x)); // objects made inside vm have another prototype
const fx = (n) => JSON.parse(fs.readFileSync(new URL(`./fixtures/${n}.json`, import.meta.url), "utf8"));
const src = (f) => fs.readFileSync(new URL(`../${f}`, import.meta.url), "utf8");

// ------------------------------------------------------------------ fixtures are labelled as synthetic

test("every fixture says it is synthetic", () => {
  for (const f of fs.readdirSync(new URL("./fixtures/", import.meta.url))) {
    const j = fx(f.replace(/\.json$/, ""));
    assert.match(String(j.__synthetic__), /SYNTHETIC/, f);
  }
});

// ------------------------------------------------------------------ operations

test("operation names come from the URL path and unknown ones are ignored", () => {
  assert.equal(P.opName("https://x.com/i/api/graphql/AbC-123_x/UserByScreenName?variables=%7B%7D"), "UserByScreenName");
  assert.equal(P.opName("https://x.com/i/api/1.1/jot/client_event.json"), null);
  assert.equal(P.classify("UserByScreenName"), "user");
  assert.equal(P.classify("UserTweets"), "posts");
  assert.equal(P.classify("TweetDetail"), "detail");
  assert.equal(P.classify("CommunityByRestId"), "community");
  assert.equal(P.classify("CommunityTweetsTimeline"), "community");
  assert.equal(P.classify("FollowersYouFollow"), "smart");
  assert.equal(P.classify("HomeTimeline"), null);
  assert.equal(P.classify("CreateTweet"), null, "a write operation is never read");
  assert.equal(P.classify("FavoriteTweet"), null);
});

// ------------------------------------------------------------------ users

test("UserByScreenName: new core/privacy layout", () => {
  const r = P.parseUser(fx("user_by_screen_name"));
  assert.equal(r.state, "ok");
  assert.equal(r.user.restId, "1845500112233445566");
  assert.equal(r.user.screenName, "moonbearsol");
  assert.equal(r.user.name, "Moon Bear");
  assert.equal(r.user.followers, 8431);
  assert.equal(r.user.following, 212);
  assert.equal(r.user.statuses, 346);
  assert.equal(r.user.verified, true);
  assert.equal(r.user.protected, false);
  assert.equal(r.user.createdAtMs, Date.parse("2026-10-14T09:12:44Z"));
  assert.match(r.user.description, /\$MOONB/);
});

test("UserByScreenName: older legacy layout gives the same facts", () => {
  const a = P.parseUser(fx("user_by_screen_name")).user;
  const b = P.parseUser(fx("user_legacy_shape")).user;
  for (const k of ["restId", "screenName", "name", "followers", "following", "statuses", "createdAtMs"]) assert.equal(b[k], a[k], k);
});

test("suspended, missing and protected accounts are told apart", () => {
  assert.equal(P.parseUser(fx("user_suspended")).state, "suspended");
  assert.equal(P.parseUser(fx("user_not_found")).state, "not_found");
  const p = P.parseUser(fx("user_protected"));
  assert.equal(p.state, "protected");
  assert.equal(p.user.screenName, "privatebear");
});

test("an unknown shape is drift (null), never a made-up account", () => {
  assert.equal(P.parseUser(fx("user_drift")), null);
  assert.equal(P.parseUser(null), null);
  assert.equal(P.parseUser("nope"), null);
  assert.deepEqual(J(P.parseResponse("UserByScreenName", fx("user_drift"))), { kind: "user", drift: true });
});

test("rate limit and locked-account errors are recognised inside a normal-looking answer", () => {
  assert.equal(P.parseUser(fx("error_rate_limit")).state, "limited");
  assert.equal(P.parseUser(fx("error_locked")).state, "locked");
  assert.equal(P.errorState({ errors: [{ code: 32, message: "Could not authenticate you." }] }), "login");
  assert.equal(P.errorState({ data: {} }), null);
});

// ------------------------------------------------------------------ posts

test("UserTweets: own posts only, with wrapper, note tweet and pinned entry, without the retweet and the tombstone", () => {
  const posts = P.parsePosts(fx("user_tweets"), "1845500112233445566");
  const ids = posts.map((t) => t.id);
  assert.ok(ids.includes("1861000000000000001"), "pinned post");
  assert.ok(ids.includes("1861000000000000012"), "TweetWithVisibilityResults wrapper is unwrapped");
  assert.ok(!ids.includes("1861000000000000014"), "retweets are left out");
  assert.ok(!ids.includes("1861000000000000015"), "tombstone is left out");
  assert.equal(posts.length, 5);
  const a = posts.find((t) => t.id === "1861000000000000010");
  assert.equal(a.views, 9800);
  assert.equal(a.likes, 190);
  assert.equal(a.replies, 22);
  assert.equal(a.reposts, 31);
  assert.equal(a.author.screenName, "moonbearsol");
  const long = posts.find((t) => t.id === "1861000000000000013");
  assert.match(long.text, /^A longer note about the roadmap/, "note tweet text wins over the short text");
});

// ------------------------------------------------------------------ tweet detail

test("TweetDetail: the focal post, its author and the direct replies", () => {
  const d = P.parseDetail(fx("tweet_detail"), fx("tweet_detail_focal_id").focalId);
  assert.equal(d.tweet.id, "1861500000000000001");
  assert.match(d.tweet.text, /\$MOONB/);
  assert.equal(d.tweet.views, 215000);
  assert.equal(d.tweet.likes, 3100);
  assert.equal(d.tweet.author.screenName, "alphacalls");
  assert.equal(d.tweet.author.followers, 182000);
  assert.equal(d.replies.length, 10);
  assert.ok(d.replies.every((r) => r.id !== d.tweet.id));
  const bot = d.replies.find((r) => r.authorScreenName === "bot_user48271905");
  assert.equal(bot.defaultAvatar, true);
  assert.equal(bot.text, "LFG 🚀🚀🚀");
});

test("TweetDetail: a wrong focal id is drift, not a different post", () => {
  assert.equal(P.parseDetail(fx("tweet_detail"), "999"), null);
  assert.deepEqual(J(P.parseResponse("TweetDetail", fx("tweet_detail"), { focalId: "999" })), { kind: "detail", drift: true });
});

// ------------------------------------------------------------------ community and smart followers

test("community: members from the community object, posts from its timeline", () => {
  const meta = P.parseCommunity(fx("community_by_rest_id"));
  assert.equal(meta.id, "1861700000000000007");
  assert.equal(meta.name, "MoonBear Den");
  assert.equal(meta.members, 412);
  assert.equal(meta.createdAtMs, 1795000000000);
  const posts = P.parseCommunity(fx("community_tweets"));
  assert.equal(posts.postsSeen, 5);
  assert.ok(posts.latestPostMs > 0);
  assert.equal(P.parseCommunity(fx("community_drift")), null);
});

test("followers_you_follow: handles, count and the biggest follower", () => {
  const s = P.parseSmart(fx("followers_you_follow"));
  assert.equal(s.count, 7);
  assert.equal(s.handles[0].handle, "cobie_fan_acc");
  assert.equal(s.topFollowers, 410000);
  assert.equal(s.hasMore, true);
  assert.equal(P.parseSmart({ data: { user: {} } }), null, "no list at all is drift");
});

// ------------------------------------------------------------------ hostile and odd input

test("parsers never throw on odd input", () => {
  const odd = [null, undefined, 1, "x", [], {}, { data: null }, { data: [] }, { data: { user: { result: { legacy: null } } } }, { data: { a: { b: { c: [{ tweet_results: { result: { rest_id: 5 } } }] } } } }];
  for (const o of odd) {
    for (const op of ["UserByScreenName", "UserTweets", "TweetDetail", "CommunityByRestId", "FollowersYouFollow"]) {
      assert.doesNotThrow(() => P.parseResponse(op, o, {}), `${op} ${JSON.stringify(o)}`);
    }
  }
  // deep nesting does not blow the stack
  let deep = { x: 1 };
  for (let i = 0; i < 5000; i++) deep = { n: deep };
  assert.doesNotThrow(() => P.parseCommunity({ data: deep }));
});

// ------------------------------------------------------------------ DOM fallback helpers

test("DOM text helpers: counts, join date, metrics label, profile", () => {
  assert.equal(P.parseCount("1.2K Followers"), 1200);
  assert.equal(P.parseCount("3.4M"), 3400000);
  assert.equal(P.parseCount("1,234 Followers"), 1234);
  assert.equal(P.parseCount("2B"), 2000000000);
  assert.equal(P.parseCount("no number"), 0);
  assert.equal(P.parseJoined("Joined June 2009"), Date.parse("2009-06-01T00:00:00Z"));
  assert.equal(P.parseJoined("hello"), 0);
  assert.deepEqual(J(P.parseMetricsLabel("12 replies, 34 reposts, 56 likes, 7 bookmarks, 1,234 views")), { replies: 12, reposts: 34, likes: 56, bookmarks: 7, views: 1234 });
  assert.equal(P.parseMetricsLabel("1 reply, 2.5K likes").likes, 2500);
  const u = P.profileFromDom({ name: "Moon Bear", handle: "@moonbearsol", followersText: "8.4K Followers", followingText: "212 Following", joinedText: "Joined October 2026", bio: "hi" });
  assert.equal(u.screenName, "moonbearsol");
  assert.equal(u.followers, 8400);
  assert.equal(P.profileFromDom({ name: "x" }), null);
});

test("page detection: login wall, lock, rate limit, suspended, protected, missing", () => {
  const d = (text, url = "https://x.com/someone") => P.detectPage({ url, text });
  assert.equal(d("Home / X"), "ok");
  assert.equal(d("", "https://x.com/i/flow/login"), "login");
  assert.equal(d("Sign in to X\nPhone, email or username"), "login");
  assert.equal(d("Your account has been temporarily locked"), "locked");
  assert.equal(d("We detected unusual login activity on your account"), "locked");
  assert.equal(d("anything", "https://x.com/account/access"), "locked");
  assert.equal(d("Rate limit exceeded"), "limited");
  assert.equal(d("Account suspended\nX suspends accounts which violate the X Rules"), "suspended");
  assert.equal(d("These posts are protected\nOnly approved followers can see"), "protected");
  assert.equal(d("This account doesn’t exist\nTry searching for another."), "not_found");
  assert.equal(d("Hmm...this page doesn’t exist. Try searching for something else."), "not_found");
});

// ------------------------------------------------------------------ jobs

const job = (kind, key, url, extra = {}) => ({ id: 1, kind, key, url, ...extra });

test("only the four x.com address shapes can be opened", () => {
  assert.ok(jobUrlOk(job("profile", "a", "https://x.com/moonbearsol")));
  assert.ok(jobUrlOk(job("smart", "a", "https://x.com/moonbearsol/followers_you_follow")));
  assert.ok(jobUrlOk(job("tweet", "1", "https://x.com/alphacalls/status/1861500000000000001")));
  assert.ok(jobUrlOk(job("tweet", "1", "https://x.com/i/web/status/1861500000000000001")));
  assert.ok(jobUrlOk(job("community", "1", "https://x.com/i/communities/1861700000000000007")));
  for (const bad of ["https://evil.com/moonbearsol", "http://x.com/moonbearsol", "https://x.com/moonbearsol/", "https://x.com/home", "https://x.com/compose/post", "https://x.com/intent/like?tweet_id=1", "javascript:alert(1)", "https://x.com.evil.com/a", "https://x.com/a/b/c"]) {
    assert.ok(!jobUrlOk(job("profile", "a", bad)), bad);
  }
  assert.ok(!jobUrlOk(job("like", "a", "https://x.com/moonbearsol")), "there is no job kind that writes");
  assert.ok(!jobUrlOk(null));
});

test("a profile job completes when the user and the posts arrived, or after the grace time", () => {
  const j = job("profile", "moonbearsol", "https://x.com/moonbearsol");
  const c = newCollector(j, 1000);
  assert.equal(isComplete(c, 1000), false);
  addPiece(c, P.parseResponse("UserByScreenName", fx("user_by_screen_name")), 2000);
  assert.equal(isComplete(c, 2100), false, "waits a moment for the posts");
  assert.equal(isComplete(c, 6100), true, "grace over");
  addPiece(c, P.parseResponse("UserTweets", fx("user_tweets"), { ownerRestId: "1845500112233445566" }), 2500);
  assert.equal(isComplete(c, 2600), true);
  const r = buildResult(c, 3000);
  assert.equal(r.ok, true);
  assert.equal(r.state, "ok");
  assert.equal(r.user.screenName, "moonbearsol");
  assert.equal(r.posts.length, 5);
});

test("a suspended or missing account ends the job at once and counts as an answer", () => {
  for (const [name, state] of [["user_suspended", "suspended"], ["user_not_found", "not_found"]]) {
    const c = newCollector(job("profile", "x", "https://x.com/x"), 0);
    addPiece(c, P.parseResponse("UserByScreenName", fx(name)), 1);
    assert.equal(isComplete(c, 2), true);
    const r = buildResult(c, 3);
    assert.equal(r.ok, true);
    assert.equal(r.state, state);
  }
  const c = newCollector(job("profile", "x", "https://x.com/x"), 0);
  addPiece(c, P.parseResponse("UserByScreenName", fx("user_protected")), 1);
  assert.equal(buildResult(c, 3).state, "protected");
});

test("limited, locked and login are failures, never answers", () => {
  for (const s of ["limited", "locked", "login"]) {
    const c = newCollector(job("profile", "x", "https://x.com/x"), 0);
    addPageState(c, s);
    const r = buildResult(c, 1);
    assert.equal(r.ok, false);
    assert.equal(r.state, s);
  }
});

test("nothing arrived: timeout; unreadable shape: drift. Both are failures", () => {
  const c = newCollector(job("tweet", "1", "https://x.com/i/web/status/1861500000000000001"), 0);
  assert.equal(buildResult(c, 30000).state, "timeout");
  addPiece(c, P.parseResponse("TweetDetail", fx("tweet_detail"), { focalId: "wrong" }), 1);
  const r = buildResult(c, 2);
  assert.equal(r.ok, false);
  assert.equal(r.state, "drift");
});

test("tweet, community and smart jobs each finish on their own data", () => {
  const t = newCollector(job("tweet", "1", "https://x.com/alphacalls/status/1861500000000000001", { tweetId: "1861500000000000001" }), 0);
  addPiece(t, P.parseResponse("TweetDetail", fx("tweet_detail"), { focalId: "1861500000000000001" }), 1);
  assert.equal(isComplete(t, 2), true);
  const r = buildResult(t, 3);
  assert.equal(r.tweet.author.followers, 182000);
  assert.equal(r.replies.length, 10);

  const c = newCollector(job("community", "1861700000000000007", "https://x.com/i/communities/1861700000000000007"), 0);
  addPiece(c, P.parseResponse("CommunityByRestId", fx("community_by_rest_id")), 1);
  assert.equal(isComplete(c, 2), false, "waits briefly for the posts");
  addPiece(c, P.parseResponse("CommunityTweetsTimeline", fx("community_tweets")), 3);
  assert.equal(isComplete(c, 4), true);
  const cr = buildResult(c, 5);
  assert.equal(cr.community.members, 412);
  assert.equal(cr.community.postsSeen, 5);

  const s = newCollector(job("smart", "moonbearsol", "https://x.com/moonbearsol/followers_you_follow"), 0);
  addPiece(s, P.parseResponse("FollowersYouFollow", fx("followers_you_follow")), 1);
  assert.equal(buildResult(s, 2).smart.count, 7);
});

// ------------------------------------------------------------------ pacing

test("gap between pages is 8 to 15 seconds with jitter, and settings cannot go below the floor", () => {
  assert.equal(gapMs(DEFAULT_PACE, () => 0), 8000);
  assert.equal(gapMs(DEFAULT_PACE, () => 0.999999), 15000);
  const seen = new Set();
  for (let i = 0; i < 200; i++) {
    const g = gapMs(DEFAULT_PACE);
    assert.ok(g >= 8000 && g <= 15000);
    seen.add(Math.round(g / 500));
  }
  assert.ok(seen.size > 5, "jitter varies");
  assert.equal(cleanPace({ minGapSecs: 0, maxGapSecs: 0 }).minGapSecs, 5, "never faster than 5 s");
  assert.equal(cleanPace({ maxLoadsPerHour: 99999 }).maxLoadsPerHour, 200);
  assert.deepEqual(cleanPace(null), DEFAULT_PACE);
  assert.ok(cleanPace({ minGapSecs: 30, maxGapSecs: 10 }).maxGapSecs >= 30, "max is never below min");
});

test("at most 120 loads an hour: the 121st waits until the oldest one is an hour old", () => {
  const p = new Pacer(DEFAULT_PACE);
  let t = 0;
  for (let i = 0; i < 120; i++) {
    t += 9000; // 9 s apart, so only the hourly cap can stop us
    assert.equal(p.canLoad(t).ok, true, `load ${i + 1}`);
    p.recordLoad(t);
  }
  const c = p.canLoad(t + 9000);
  assert.equal(c.ok, false);
  assert.equal(c.reason, "hour");
  assert.ok(c.waitMs > 0);
  assert.equal(p.canLoad(9000 + 3600_000 + 9000).ok, true, "an hour after the first load there is room again");
});

test("the minimum gap is enforced between two loads", () => {
  const p = new Pacer(DEFAULT_PACE);
  p.recordLoad(100_000);
  const c = p.canLoad(103_000);
  assert.equal(c.ok, false);
  assert.equal(c.reason, "gap");
  assert.equal(c.waitMs, 5000);
  assert.equal(p.canLoad(108_000).ok, true);
});

test("daily cap of 800", () => {
  const p = new Pacer({ ...DEFAULT_PACE, maxLoadsPerHour: 200, maxLoadsPerDay: 5 });
  let t = 0;
  for (let i = 0; i < 5; i++) {
    t += 9000;
    p.recordLoad(t);
  }
  assert.equal(p.canLoad(t + 9000).reason, "day");
});

test("a rate-limit or lock backs off 15 min, then 1 h, then the rest of the day, and recovers after 6 quiet hours", () => {
  const p = new Pacer(DEFAULT_PACE);
  let t = 1_000_000;
  assert.equal(p.onLimited(t, "limited") - t, BACKOFF_MS[0]);
  assert.equal(p.canLoad(t + 1000).reason, "paused");
  assert.equal(p.status(t + 1000).paused, true);
  assert.equal(p.status(t + 1000).pausedReason, "limited");
  t += BACKOFF_MS[0] + 1000;
  assert.equal(p.status(t).paused, false);
  assert.equal(p.onLimited(t, "locked") - t, BACKOFF_MS[1]);
  t += BACKOFF_MS[1] + 1000;
  assert.equal(p.onLimited(t, "limited") - t, BACKOFF_MS[2], "third time: stop for the day");
  t += 7 * 3600_000 + BACKOFF_MS[2];
  assert.equal(p.onLimited(t, "limited") - t, BACKOFF_MS[0], "after a long quiet time it starts over");
});

test("cache: the same handle or tweet is not loaded again for 30 minutes", () => {
  const p = new Pacer(DEFAULT_PACE);
  p.cachePut("profile:moonbearsol", { ok: true }, 0);
  assert.deepEqual(p.cacheGet("profile:moonbearsol", 29 * 60_000), { ok: true });
  assert.equal(p.cacheGet("profile:moonbearsol", 31 * 60_000), null);
  assert.equal(p.cacheGet("profile:other", 1), null);
  // survives a worker restart
  const q = new Pacer(DEFAULT_PACE, JSON.parse(JSON.stringify(p.toJSON())));
  p.cachePut("tweet:1", { ok: 1 }, 100);
  assert.deepEqual(new Pacer(DEFAULT_PACE, JSON.parse(JSON.stringify(p.toJSON()))).cacheGet("tweet:1", 200), { ok: 1 });
  assert.equal(q.loads.length, 0);
});

// ------------------------------------------------------------------ the runner with a fake browser and a fake clock

function rig(opts = {}) {
  let clock = 1_000_000;
  const log = { navigations: [], created: 0, sent: [], dom: 0 };
  let tabAlive = false;
  const settings = { enabled: true, paused: false, pace: { ...DEFAULT_PACE } };
  const answers = [...(opts.jobs || [])];
  const store = new Map();
  const chrome = {
    tabs: {
      get: async (id) => {
        if (!tabAlive) throw new Error("no tab");
        return { id };
      },
      create: async ({ url, active }) => {
        assert.equal(active, false, "the tab is never focused");
        tabAlive = true;
        log.created++;
        log.navigations.push(url);
        opts.onNavigate?.(url, 77);
        return { id: 77 };
      },
      update: async (id, { url, active }) => {
        assert.equal(id, 77);
        assert.equal(active, false, "the tab is never focused");
        log.navigations.push(url);
        opts.onNavigate?.(url, 77);
      },
      sendMessage: async () => {
        log.dom++;
        return opts.dom || {};
      },
    },
  };
  let runner;
  const deps = {
    chrome,
    ask: async (m) => {
      log.sent.push(m);
      if (m.t === "xnext") return { t: "xjob", job: answers.shift() || null };
      return { t: "ok" };
    },
    connected: () => true,
    now: () => clock,
    sleep: async (ms) => {
      clock += ms;
      opts.onSleep?.(runner, clock);
      await new Promise((r) => setImmediate(r)); // let the "page" answer
    },
    rand: () => 0.5,
    getSettings: () => settings,
    store: { get: async (k) => store.get(k), set: async (k, v) => void store.set(k, JSON.parse(JSON.stringify(v))) },
  };
  runner = createXRunner(deps);
  return { runner, log, settings, now: () => clock, store, advance: (ms) => (clock += ms) };
}

const sender = (tabId = 77) => ({ tab: { id: tabId } });

test("runner: opens one background tab, collects the page data, returns a structured result, and only ever reads", async () => {
  const j = { id: 5, kind: "profile", key: "moonbearsol", url: "https://x.com/moonbearsol" };
  let r;
  r = rig({
    onNavigate: () => {
      // the page answers while the runner waits: the content script sends parsed pieces
      setImmediate(() => {
        r.runner.onMessage({ t: "xpiece", jobId: 5, piece: P.parseResponse("UserByScreenName", fx("user_by_screen_name")) }, sender());
        r.runner.onMessage({ t: "xpiece", jobId: 5, piece: P.parseResponse("UserTweets", fx("user_tweets"), { ownerRestId: "1845500112233445566" }) }, sender());
      });
    },
  });
  await r.runner.runJob(j);
  assert.deepEqual(r.log.navigations, ["https://x.com/moonbearsol"]);
  assert.equal(r.log.created, 1);
  const res = r.log.sent.find((m) => m.t === "xresult");
  assert.equal(res.ok, true);
  assert.equal(res.jobId, 5);
  assert.equal(res.user.followers, 8431);
  assert.equal(res.posts.length, 5);
});

test("runner: a second job on the same rig reuses the tab", async () => {
  const jobs = [
    { id: 1, kind: "tweet", key: "1861500000000000001", tweetId: "1861500000000000001", url: "https://x.com/alphacalls/status/1861500000000000001" },
    { id: 2, kind: "smart", key: "alphacalls", url: "https://x.com/alphacalls/followers_you_follow" },
  ];
  let r;
  r = rig({
    onNavigate: (url) => {
      setImmediate(() => {
        const id = url.includes("status") ? 1 : 2;
        const piece = id === 1 ? P.parseResponse("TweetDetail", fx("tweet_detail"), { focalId: "1861500000000000001" }) : P.parseResponse("FollowersYouFollow", fx("followers_you_follow"));
        r.runner.onMessage({ t: "xpiece", jobId: id, piece }, sender());
      });
    },
  });
  await r.runner.runJob(jobs[0]);
  await r.runner.runJob(jobs[1]);
  assert.equal(r.log.created, 1, "one tab only");
  assert.equal(r.log.navigations.length, 2);
  const results = r.log.sent.filter((m) => m.t === "xresult");
  assert.equal(results[0].tweet.id, "1861500000000000001");
  assert.equal(results[1].smart.count, 7);
});

test("runner: messages from any tab other than the job tab are ignored", async () => {
  const r = rig({ onNavigate: () => {} });
  const j = { id: 9, kind: "profile", key: "x", url: "https://x.com/x" };
  const p = r.runner.runJob(j);
  await new Promise((res) => setImmediate(res));
  assert.deepEqual(r.runner.onMessage({ t: "xhello" }, sender(5)), { active: false }, "another tab is told to stay silent");
  assert.equal(r.runner.onMessage({ t: "xpiece", jobId: 9, piece: { kind: "user", state: "ok", user: { screenName: "evil" } } }, sender(5)), undefined);
  assert.equal(r.runner.onMessage({ t: "xpiece", jobId: 4, piece: { kind: "user", state: "ok", user: { screenName: "stale" } } }, sender(77)), undefined, "an old job id is ignored");
  const hello = r.runner.onMessage({ t: "xhello" }, sender(77));
  assert.equal(hello.active, true);
  assert.equal(hello.job.id, 9);
  await p; // times out (nothing arrived), which proves the stray pieces were not used
  const res = r.log.sent.find((m) => m.t === "xresult");
  assert.equal(res.ok, false);
  assert.equal(res.state, "timeout");
});

test("runner: after 14 s with no data it asks the page for a DOM read, and still fails cleanly if that has nothing", async () => {
  const r = rig({});
  await r.runner.runJob({ id: 3, kind: "profile", key: "x", url: "https://x.com/x" });
  assert.equal(r.log.dom, 1);
  const res = r.log.sent.find((m) => m.t === "xresult");
  assert.equal(res.ok, false);
  assert.ok(r.now() - 1_000_000 >= JOB_TIMEOUT_MS);
  assert.ok(DOM_FALLBACK_AT_MS < JOB_TIMEOUT_MS);
});

test("runner: DOM fallback data is accepted and marked as such", async () => {
  const dom = { piece: { kind: "user", state: "ok", user: P.profileFromDom({ name: "Moon Bear", handle: "@moonbearsol", followersText: "8.4K Followers", followingText: "212 Following" }) } };
  const r = rig({ dom });
  await r.runner.runJob({ id: 3, kind: "profile", key: "moonbearsol", url: "https://x.com/moonbearsol" });
  const res = r.log.sent.find((m) => m.t === "xresult");
  assert.equal(res.ok, true);
  assert.equal(res.source, "dom");
  assert.equal(res.user.followers, 8400);
});

test("runner: a rate-limit page pauses the whole monitor (15 min) and reports it", async () => {
  let r;
  r = rig({
    onNavigate: () => setImmediate(() => r.runner.onMessage({ t: "xhttp", jobId: 1, op: "UserByScreenName", status: 429 }, sender())),
  });
  await r.runner.runJob({ id: 1, kind: "profile", key: "x", url: "https://x.com/x" });
  const res = r.log.sent.find((m) => m.t === "xresult");
  assert.equal(res.ok, false);
  assert.equal(res.state, "limited");
  const st = r.runner.status();
  assert.equal(st.paused, true);
  assert.equal(st.pausedReason, "limited");
  assert.ok(st.pausedUntil - r.now() > 14 * 60_000);
  assert.equal(r.runner.pacer.canLoad(r.now()).ok, false);
});

test("runner: a login page marks X as logged out", async () => {
  let r;
  r = rig({ onNavigate: () => setImmediate(() => r.runner.onMessage({ t: "xpage", jobId: 1, state: "login" }, sender())) });
  await r.runner.runJob({ id: 1, kind: "profile", key: "x", url: "https://x.com/x" });
  assert.equal(r.log.sent.find((m) => m.t === "xresult").state, "login");
  assert.equal(r.runner.status().loggedIn, false);
});

test("runner: the cache answers a repeat job without opening a page", async () => {
  let r;
  r = rig({
    onNavigate: () => setImmediate(() => r.runner.onMessage({ t: "xpiece", jobId: 1, piece: P.parseResponse("UserByScreenName", fx("user_suspended")) }, sender())),
  });
  await r.runner.runJob({ id: 1, kind: "profile", key: "x", url: "https://x.com/x" });
  assert.equal(r.log.navigations.length, 1);
  await r.runner.runJob({ id: 2, kind: "profile", key: "x", url: "https://x.com/x" });
  assert.equal(r.log.navigations.length, 1, "no second page load inside 30 minutes");
  const second = r.log.sent.filter((m) => m.t === "xresult")[1];
  assert.equal(second.jobId, 2);
  assert.equal(second.cached, true);
  assert.equal(second.state, "suspended");
});

test("runner: an address that is not an x.com page is refused without opening anything", async () => {
  const r = rig({});
  await r.runner.runJob({ id: 1, kind: "profile", key: "x", url: "https://evil.example/x" });
  assert.equal(r.log.navigations.length, 0);
  assert.equal(r.log.sent[0].ok, false);
  assert.equal(r.log.sent[0].state, "bad_job");
});

test("runner loop: waits 8 to 15 s between pages and does nothing while paused or off", async () => {
  const jobs = [1, 2, 3].map((n) => ({ id: n, kind: "smart", key: `h${n}`, url: `https://x.com/h${n}/followers_you_follow` }));
  const times = [];
  let r;
  let sleeps = 0;
  r = rig({
    jobs,
    onNavigate: (url) => {
      times.push(r.now());
      const id = Number(/h(\d)/.exec(url)[1]);
      setImmediate(() => r.runner.onMessage({ t: "xpiece", jobId: id, piece: P.parseResponse("FollowersYouFollow", fx("followers_you_follow")) }, sender()));
    },
    onSleep: (run) => {
      if (++sleeps > 80) run.stop();
    },
  });
  r.runner.start();
  for (let i = 0; i < 400 && sleeps <= 80; i++) await new Promise((res) => setImmediate(res));
  assert.equal(times.length, 3);
  for (let i = 1; i < times.length; i++) {
    const gap = times[i] - times[i - 1];
    assert.ok(gap >= 8000 && gap <= 16000, `gap ${gap}`);
  }
  r.runner.stop();

  // paused or off: it never even asks for work
  const q = rig({ jobs: [jobs[0]], onSleep: () => {} });
  q.settings.paused = true;
  let n = 0;
  q.runner.start();
  for (let i = 0; i < 30; i++) await new Promise((res) => setImmediate(res));
  q.runner.stop();
  n = q.log.sent.filter((m) => m.t === "xnext").length;
  assert.equal(n, 0, "paused: no job requests");
});

// ------------------------------------------------------------------ AboutAccountQuery and the community operations (matched by NAME)

const OPS_SEEN = ["AboutAccountQuery", "UserByScreenName", "UserByRestId", "UserTweets", "UserTweetsAndReplies", "TweetDetail", "TweetResultByRestId", "CommunityQuery", "CommunityByRestId", "CommunityTweetsTimeline", "membersSliceTimeline_Query", "moderatorsSliceTimeline_Query", "UserFollowersYouFollow"];
const OPS_IGNORED = ["HomeTimeline", "CreateTweet", "FavoriteTweet", "SearchTimeline", "Followers", "BlueVerifiedFollowers", "Retweeters", "ListLatestTweetsTimeline", "aboutAccountQuery", "membersSliceTimeline", "UserByScreenNameExtra"];

/** Run the real xhook.js in a fake page and report which GraphQL operations it copied out. */
async function hookRelays(urls) {
  const posted = [];
  const sandbox = { location: { origin: "https://x.com" }, console };
  sandbox.window = sandbox;
  sandbox.postMessage = (m) => posted.push(m);
  sandbox.fetch = (u) => Promise.resolve({ status: 200, headers: { get: () => null }, clone: () => ({ text: () => Promise.resolve('{"data":{}}') }) });
  vm.runInNewContext(src("xhook.js"), sandbox);
  for (const u of urls) {
    const r = sandbox.fetch(u);
    await r;
    await new Promise((res) => setImmediate(res));
  }
  return posted.map((m) => m.op);
}

const gql = (id, op) => `https://x.com/i/api/graphql/${id}/${op}?variables=%7B%7D`;

test("the page hook copies exactly the listed operations, matched by name, whatever the query id", async () => {
  const urls = [...OPS_SEEN.map((o, i) => gql(`Id${i}-AbC_x`, o)), ...OPS_IGNORED.map((o, i) => gql(`Zz${i}`, o))];
  const relayed = await hookRelays(urls);
  assert.deepEqual(relayed.sort(), [...OPS_SEEN].sort());
  // the id rotates: the same name with another id is still copied
  assert.deepEqual(await hookRelays([gql("TzOG2twZEfhr9KmClvVVqA", "AboutAccountQuery"), gql("brandNewId", "AboutAccountQuery")]), ["AboutAccountQuery", "AboutAccountQuery"]);
  // a request that is not GraphQL, and a name that only contains a wanted one, are not copied
  assert.deepEqual(await hookRelays(["https://x.com/i/api/1.1/jot/client_event.json", "https://x.com/i/api/graphql/abc/NotUserByScreenName"]), []);
});

test("the hook's list and the parser's list agree on every operation name", async () => {
  const all = [...OPS_SEEN, ...OPS_IGNORED];
  const relayed = new Set(await hookRelays(all.map((o, i) => gql(`i${i}`, o))));
  for (const op of all) assert.equal(relayed.has(op), P.classify(op) !== null, op);
});

test("operations: AboutAccountQuery and the four community operations are classified by name", () => {
  assert.equal(P.classify("AboutAccountQuery"), "about");
  for (const op of ["CommunityQuery", "CommunityByRestId", "CommunityTweetsTimeline", "membersSliceTimeline_Query", "moderatorsSliceTimeline_Query"]) assert.equal(P.classify(op), "community", op);
  assert.equal(P.classify("aboutaccountquery"), null, "names are exact");
  assert.equal(P.opName("https://x.com/i/api/graphql/TzOG2twZEfhr9KmClvVVqA/AboutAccountQuery?variables=%7B%22screenName%22%3A%22moonbearsol%22%7D"), "AboutAccountQuery");
  assert.equal(P.opName("https://x.com/i/api/graphql/abc/membersSliceTimeline_Query"), "membersSliceTimeline_Query");
});

test("AboutAccountQuery: username changes (count, last change), country, location flag and affiliate", () => {
  const a = J(P.parseAbout(fx("about_account")));
  assert.deepEqual(a, { usernameChanges: 2, changesKnown: true, lastChangedMs: 1793000000000, country: "Iceland", locationAccurate: false, affiliate: "pumpfun", source: "Web" });
  const r = J(P.parseResponse("AboutAccountQuery", fx("about_account")));
  assert.equal(r.kind, "about");
  assert.equal(r.about.usernameChanges, 2);
});

test("AboutAccountQuery: a missing username_changes is unknown, not zero changes; a count of 0 is known", () => {
  const none = J(P.parseAbout(fx("about_account_no_changes")));
  assert.equal(none.changesKnown, false);
  assert.equal(none.country, "Canada");
  assert.equal(none.locationAccurate, true);
  assert.equal(none.lastChangedMs, 0);
  const zero = J(P.parseAbout(fx("about_account_clean")));
  assert.deepEqual([zero.changesKnown, zero.usernameChanges], [true, 0]);
});

test("AboutAccountQuery: an unknown shape is drift, an error body is its state, and odd input never throws", () => {
  assert.equal(P.parseAbout(fx("about_drift")), null);
  assert.equal(J(P.parseResponse("AboutAccountQuery", fx("about_drift"))).drift, true);
  assert.equal(P.parseResponse("AboutAccountQuery", fx("error_rate_limit")).state, "limited");
  for (const bad of [null, 1, "x", [], {}, { data: null }, { data: { about_profile: 5 } }, { data: { a: { about_profile: { username_changes: { count: "many" } } } } }]) {
    assert.doesNotThrow(() => P.parseAbout(bad));
  }
  assert.equal(J(P.parseAbout({ data: { a: { about_profile: { username_changes: { count: -3 } } } } })).usernameChanges, 0, "a negative count is not believed");
});

test("community slices: the members list and the moderators list (items_results with slice_info)", () => {
  const m = J(P.parseResponse("membersSliceTimeline_Query", fx("community_members_slice")));
  assert.equal(m.kind, "community");
  assert.equal(m.community.membersSample, 4);
  assert.deepEqual(m.community.moderators, []);
  assert.equal(m.community.members, null, "the member count comes from the community object, not from a slice");
  const mod = J(P.parseResponse("moderatorsSliceTimeline_Query", fx("community_moderators_slice")));
  assert.deepEqual(mod.community.moderators, ["den_member_1", "den_member_2"]);
  assert.equal(J(P.parseResponse("membersSliceTimeline_Query", fx("community_drift"))).drift, true);
});

test("community timelines that list posts as items_results (not entries) are read", () => {
  const c = J(P.parseCommunity(fx("community_timeline_items")));
  assert.equal(c.members, 412);
  assert.equal(c.postsSeen, 3);
  assert.ok(c.latestPostMs > 0);
  const via = J(P.parseResponse("CommunityTweetsTimeline", fx("community_timeline_items")));
  assert.equal(via.community.postsSeen, 3);
  // the old fixture shapes still parse
  assert.equal(J(P.parseCommunity(fx("community_tweets"))).postsSeen, 5);
});

test("the collector keeps the About data and the community slices for a profile and a community job", () => {
  const job = { id: 1, kind: "profile", key: "moonbearsol", url: "https://x.com/moonbearsol" };
  const c = newCollector(job, 0);
  addPiece(c, P.parseResponse("UserByScreenName", fx("user_by_screen_name")), 10);
  addPiece(c, P.parseResponse("AboutAccountQuery", fx("about_account")), 20);
  addPiece(c, P.parseResponse("UserTweets", fx("user_tweets"), { ownerRestId: "1845500112233445566" }), 30);
  const res = J(buildResult(c, 40));
  assert.equal(res.ok, true);
  assert.equal(res.about.usernameChanges, 2);
  assert.equal(res.about.country, "Iceland");
  // without the page ever fetching About, the result simply has none
  const c2 = newCollector(job, 0);
  addPiece(c2, P.parseResponse("UserByScreenName", fx("user_by_screen_name")), 10);
  assert.equal("about" in buildResult(c2, 20), false);
  // a community job merges the community object, the timeline and both slices
  const cj = newCollector({ id: 2, kind: "community", key: "1861700000000000007", url: "https://x.com/i/communities/1861700000000000007" }, 0);
  addPiece(cj, P.parseResponse("CommunityQuery", fx("community_by_rest_id")), 5);
  addPiece(cj, P.parseResponse("membersSliceTimeline_Query", fx("community_members_slice")), 6);
  addPiece(cj, P.parseResponse("moderatorsSliceTimeline_Query", fx("community_moderators_slice")), 7);
  addPiece(cj, P.parseResponse("CommunityTweetsTimeline", fx("community_timeline_items")), 8);
  const out = J(buildResult(cj, 9));
  assert.equal(out.community.members, 412);
  assert.deepEqual([out.community.membersSample, out.community.moderators.length, out.community.postsSeen], [4, 2, 3]);
});

test("runner: the About answer that the page fetched during a profile job is in the result sent to the app", async () => {
  const j = { id: 8, kind: "profile", key: "moonbearsol", url: "https://x.com/moonbearsol" };
  let r;
  r = rig({
    onNavigate: () => {
      setImmediate(() => {
        r.runner.onMessage({ t: "xpiece", jobId: 8, piece: P.parseResponse("UserByScreenName", fx("user_by_screen_name")) }, sender());
        r.runner.onMessage({ t: "xpiece", jobId: 8, piece: P.parseResponse("AboutAccountQuery", fx("about_account")) }, sender());
        r.runner.onMessage({ t: "xpiece", jobId: 8, piece: P.parseResponse("UserTweets", fx("user_tweets"), { ownerRestId: "1845500112233445566" }) }, sender());
      });
    },
  });
  await r.runner.runJob(j);
  const res = r.log.sent.find((m) => m.t === "xresult");
  assert.equal(res.about.usernameChanges, 2);
  assert.equal(res.about.lastChangedMs, 1793000000000);
  assert.equal(r.log.navigations.length, 1, "no extra page was opened to get it");
});

test("the extension never opens an About page of its own: only the four address shapes stay allowed", () => {
  assert.equal(jobUrlOk({ kind: "profile", url: "https://x.com/moonbearsol/about" }), false);
  assert.equal(jobUrlOk({ kind: "profile", url: "https://x.com/moonbearsol" }), true);
});


// ------------------------------------------------------------------ read only: no write path exists

test("the X code never clicks, posts, likes or follows", () => {
  const files = ["xhook.js", "xcontent.js", "xrunner.js", "lib/xparse.js", "lib/xjob.js", "lib/xpace.js"].map(src).join("\n");
  assert.ok(!/\.click\s*\(/.test(files), "no click()");
  assert.ok(!/dispatchEvent\s*\(/.test(files), "no synthetic events");
  assert.ok(!/\.submit\s*\(|\.focus\s*\(|execCommand|\.value\s*=|insertText/.test(files), "no typing or form submits");
  assert.ok(!/method\s*:\s*["'](POST|PUT|DELETE|PATCH)/i.test(files), "no write requests of our own");
  assert.ok(!/CreateTweet|FavoriteTweet|CreateRetweet|CreateFriendship|DeleteTweet|FollowUser|createFriendship/.test(files), "no write operation names");
  assert.ok(!/fetch\s*\(/.test(src("xcontent.js")) && !/fetch\s*\(/.test(src("xrunner.js")), "the content script and the runner make no requests");
  // the hook passes every request through unchanged
  const hook = src("xhook.js");
  assert.ok(/origFetch\.apply\(this, arguments\)/.test(hook) && /origSend\.apply\(this, arguments\)/.test(hook));
});

test("manifest: the X scripts run on x.com only, the hook is MAIN world at document_start", () => {
  const m = JSON.parse(src("manifest.json"));
  const x = m.content_scripts.filter((c) => c.matches.some((u) => u.includes("x.com")));
  assert.equal(x.length, 2);
  const hook = x.find((c) => c.js.includes("xhook.js"));
  assert.equal(hook.world, "MAIN");
  assert.equal(hook.run_at, "document_start");
  assert.deepEqual(hook.matches, ["https://x.com/*"]);
  const iso = x.find((c) => c.js.includes("xcontent.js"));
  assert.deepEqual(iso.js, ["lib/xparse.js", "xcontent.js"]);
  assert.ok(!iso.world);
  assert.ok(m.host_permissions.includes("https://x.com/*"));
  for (const c of m.content_scripts) for (const u of c.matches) assert.ok(!/\*:\/\/|<all_urls>|https:\/\/\*/.test(u), u);
});
