// Builds the fixtures in tests/fixtures/*.json. Run: node tests/make-fixtures.mjs
//
// SYNTHETIC. These files were written by hand-coding the nesting that x.com's GraphQL answers are known to use
// (UserByScreenName, UserTweets, TweetDetail, community and followers_you_follow operations, the older "legacy" layout and
// the newer "core"/"privacy" layout, TweetWithVisibilityResults wrappers, note tweets, tombstones and error bodies).
// They are NOT recordings from x.com (this sandbox cannot reach it), and every person, handle and number in them is made up.
// Each file carries "__synthetic__" at the top so nobody mistakes it for real data. Real shapes rotate; the parsers are
// defensive and the first real run on a logged-in browser is the real test.
import fs from "node:fs";

const OUT = new URL("./fixtures/", import.meta.url);
const NOTE = "SYNTHETIC test fixture, not a recording from x.com. All names and numbers are invented.";
const b64 = (s) => Buffer.from(s).toString("base64");

const user = (o) => {
  const base = { __typename: "User", id: b64(`User:${o.rest}`), rest_id: o.rest, is_blue_verified: !!o.blue, profile_image_shape: "Circle" };
  if (o.legacyShape) {
    return { ...base, legacy: { created_at: o.created, description: o.bio || "", default_profile: true, default_profile_image: !!o.defaultAvatar, followers_count: o.followers, friends_count: o.following ?? 100, listed_count: o.listed ?? 3, name: o.name, protected: !!o.prot, screen_name: o.handle, statuses_count: o.statuses ?? 50, verified: false, entities: { description: { urls: [] } } } };
  }
  return {
    ...base,
    avatar: { image_url: o.defaultAvatar ? "https://abs.twimg.com/sticky/default_profile_images/default_profile_normal.png" : "https://pbs.twimg.com/profile_images/0/x_normal.jpg" },
    core: { created_at: o.created, name: o.name, screen_name: o.handle },
    privacy: { protected: !!o.prot },
    verification: { verified: false },
    profile_bio: { description: o.bio || "" },
    legacy: { default_profile_image: !!o.defaultAvatar, description: o.bio || "", followers_count: o.followers, friends_count: o.following ?? 100, listed_count: o.listed ?? 3, statuses_count: o.statuses ?? 50 },
  };
};

const tweet = (o) => {
  const t = {
    __typename: "Tweet",
    rest_id: o.id,
    core: { user_results: { result: user(o.author) } },
    views: o.views == null ? { state: "Enabled" } : { count: String(o.views), state: "EnabledWithCount" },
    legacy: {
      created_at: o.created,
      full_text: o.text,
      favorite_count: o.likes ?? 0,
      reply_count: o.replies ?? 0,
      retweet_count: o.reposts ?? 0,
      quote_count: o.quotes ?? 0,
      bookmark_count: o.bookmarks ?? 0,
      conversation_id_str: o.convo || o.id,
      lang: "en",
      ...(o.inReplyTo ? { in_reply_to_status_id_str: o.inReplyTo } : {}),
      ...(o.retweetOf ? { retweeted_status_result: { result: tweet(o.retweetOf) } } : {}),
    },
  };
  if (o.note) t.note_tweet = { is_expandable: true, note_tweet_results: { result: { id: b64(`NoteTweet:${o.id}`), text: o.note } } };
  if (o.community) t.community_results = { result: { __typename: "Community", rest_id: o.community } };
  return o.wrap ? { __typename: "TweetWithVisibilityResults", tweet: t, tweetInterstitial: null } : t;
};

const item = (t) => ({ entryId: `tweet-${t.rest_id || t.tweet?.rest_id}`, sortIndex: "1", content: { entryType: "TimelineTimelineItem", __typename: "TimelineTimelineItem", itemContent: { itemType: "TimelineTweet", __typename: "TimelineTweet", tweet_results: { result: t }, tweetDisplayType: "Tweet" } } });
const cursor = (k) => ({ entryId: `cursor-${k}-1`, sortIndex: "0", content: { entryType: "TimelineTimelineCursor", __typename: "TimelineTimelineCursor", value: "DAABCgABG", cursorType: k === "bottom" ? "Bottom" : "Top" } });

const write = (name, json) => {
  fs.mkdirSync(OUT, { recursive: true });
  fs.writeFileSync(new URL(name, OUT), JSON.stringify({ __synthetic__: NOTE, ...json }, null, 1) + "\n");
};

// ------------------------------------------------------------ users
const MOONB = { rest: "1845500112233445566", handle: "moonbearsol", name: "Moon Bear", created: "Mon Oct 14 09:12:44 +0000 2026", followers: 8431, following: 212, statuses: 346, listed: 31, blue: true, bio: "The bear that went to the moon. $MOONB" };
write("user_by_screen_name.json", { data: { user: { result: user(MOONB) } } });
write("user_legacy_shape.json", { data: { user: { result: user({ ...MOONB, legacyShape: true }) } } });
write("user_suspended.json", { data: { user: { result: { __typename: "UserUnavailable", reason: "Suspended", message: "User is suspended" } } } });
write("user_not_found.json", { data: {} });
write("user_protected.json", { data: { user: { result: user({ ...MOONB, handle: "privatebear", prot: true }) } } });
write("user_drift.json", { data: { account_lookup: { node: { id: "x", fields: [] } } } });
write("error_rate_limit.json", { errors: [{ message: "Rate limit exceeded", code: 88, kind: "Operational", name: "RateLimitExceededError" }] });
write("error_locked.json", { errors: [{ message: "Authorization: Denied by access control: Your account is temporarily locked.", code: 326, kind: "Permissions" }] });

// ------------------------------------------------------------ the profile's own posts (UserTweets)
const T0 = "Fri Nov 27 08:00:00 +0000 2026";
const own = (id, text, views, likes, replies, reposts, extra = {}) => tweet({ id, text, views, likes, replies, reposts, created: T0, author: MOONB, ...extra });
const instructions = [
  { type: "TimelineClearCache" },
  { type: "TimelinePinEntry", entry: item(own("1861000000000000001", "Pinned: Moon Bear is live on pump.fun. $MOONB", 41000, 640, 88, 120)) },
  {
    type: "TimelineAddEntries",
    entries: [
      item(own("1861000000000000010", "gm bears. building in public, no promises. $MOONB", 9800, 190, 22, 31)),
      item(own("1861000000000000011", "Community call at 18:00 UTC. Come ask anything.", 7400, 150, 41, 12)),
      item(own("1861000000000000012", "New art drop for the den.", 5200, 98, 9, 14, { wrap: true })),
      item(own("1861000000000000013", "short", 0, 0, 0, 0, { note: "A longer note about the roadmap. ".repeat(12) })),
      item(tweet({ id: "1861000000000000014", text: "RT @alphacalls: look at this", created: T0, author: MOONB, retweetOf: { id: "1860999999999999999", text: "look at this", created: T0, author: { rest: "99887766", handle: "alphacalls", name: "Alpha Calls", created: "Sat Mar 01 10:00:00 +0000 2020", followers: 182000 } } })),
      { entryId: "tweet-1861000000000000015", sortIndex: "1", content: { entryType: "TimelineTimelineItem", itemContent: { itemType: "TimelineTweet", tweet_results: { result: { __typename: "TweetTombstone", tombstone: { text: { text: "This post was deleted by the post author." } } } } } } },
      cursor("bottom"),
    ],
  },
];
write("user_tweets.json", { data: { user: { result: { __typename: "User", timeline: { timeline: { instructions } } } } } });

// ------------------------------------------------------------ TweetDetail: a big account's post and its replies
const ALPHA = { rest: "99887766", handle: "alphacalls", name: "Alpha Calls", created: "Sat Mar 01 10:00:00 +0000 2020", followers: 182000, following: 900, statuses: 12000, blue: true, bio: "Daily alpha. Not financial advice." };
const FOCAL = "1861500000000000001";
const reply = (id, handle, text, o = {}) =>
  tweet({ id, text, created: "Fri Nov 27 08:10:00 +0000 2026", likes: o.likes ?? 0, inReplyTo: FOCAL, convo: FOCAL, author: { rest: String(7000000 + id.length * 1000 + Number(id.slice(-3))), handle, name: o.name || handle, created: o.created || "Sun Nov 22 12:00:00 +0000 2026", followers: o.followers ?? 12, defaultAvatar: !!o.defaultAvatar } });
const replies = [
  reply("1861500000000000101", "real_trader_ana", "The chart plus the Moon Bear thread is why I looked. Holder spread is better than the last three launches.", { created: "Thu Jun 12 08:00:00 +0000 2025", followers: 1900, likes: 5 }),
  reply("1861500000000000102", "cryptokev_77", "Curve is at 11 SOL and the dev wallet has not sold. Watching the next 5 minutes.", { created: "Tue Feb 04 08:00:00 +0000 2025", followers: 640, likes: 3 }),
  reply("1861500000000000103", "bot_user48271905", "LFG 🚀🚀🚀", { defaultAvatar: true }),
  reply("1861500000000000104", "bot_user48271906", "LFG 🚀🚀🚀", { defaultAvatar: true }),
  reply("1861500000000000105", "gm_fren_33", "gm", { followers: 3 }),
  reply("1861500000000000106", "gm_fren_34", "gm", { followers: 2 }),
  reply("1861500000000000107", "moonboi20260101", "to the moon", { created: "Fri Nov 20 12:00:00 +0000 2026" }),
  reply("1861500000000000108", "sol_sage", "Not touching it. The tweet never mentions the contract and I cannot find the team.", { created: "Mon Aug 11 08:00:00 +0000 2025", followers: 5100, likes: 9 }),
  reply("1861500000000000109", "dex_dan", "Bought a small bag at 9 SOL on the curve, will sell into the first push.", { created: "Sat Sep 20 08:00:00 +0000 2025", followers: 430 }),
  reply("1861500000000000110", "bullish_bro", "🔥🔥🔥", { followers: 8, created: "Sat Nov 21 12:00:00 +0000 2026" }),
];
const focal = tweet({ id: FOCAL, text: "New one to watch: $MOONB, the bear that went to the moon. CA below.", created: "Fri Nov 27 08:05:00 +0000 2026", views: 215000, likes: 3100, replies: 410, reposts: 640, quotes: 55, bookmarks: 120, author: ALPHA });
const detailEntries = [
  item(focal),
  { entryId: `conversationthread-${FOCAL}-1`, sortIndex: "2", content: { entryType: "TimelineTimelineModule", __typename: "TimelineTimelineModule", displayType: "VerticalConversation", items: replies.map((r, i) => ({ entryId: `conversationthread-${FOCAL}-tweet-${i}`, item: { itemContent: { itemType: "TimelineTweet", tweet_results: { result: r } } } })) } },
  cursor("bottom"),
];
write("tweet_detail.json", { data: { threaded_conversation_with_injections_v2: { instructions: [{ type: "TimelineAddEntries", entries: detailEntries }] } } });
write("tweet_detail_focal_id.json", { focalId: FOCAL, note: "the id to pass as focalId when parsing tweet_detail.json" });

// ------------------------------------------------------------ community
const COMM = "1861700000000000007";
write("community_by_rest_id.json", { data: { communityResults: { result: { __typename: "Community", rest_id: COMM, id_str: COMM, name: "MoonBear Den", description: { text: "Home of the bear." }, member_count: 412, created_at: 1795000000000, role: "NonMember", is_nsfw: false, join_policy: "Open", admin_results: { result: user({ ...MOONB, rest: "1845500112233445566" }) } } } } });
write("community_tweets.json", { data: { communityResults: { result: { __typename: "Community", rest_id: COMM, ranked_community_timeline: { timeline: { instructions: [{ type: "TimelineAddEntries", entries: [1, 2, 3, 4, 5].map((n) => item(tweet({ id: `186170000000000010${n}`, text: `Den post ${n}`, created: "Fri Nov 27 07:5" + n + ":00 +0000 2026", views: 300 * n, likes: 10 * n, author: { rest: "5550" + n, handle: `den_member_${n}`, name: `Den Member ${n}`, created: "Thu Jan 09 08:00:00 +0000 2025", followers: 100 * n } }))) }] } } } } } });
write("community_drift.json", { data: { community_new_shape: { items: [] } } });

// ------------------------------------------------------------ smart followers (accounts the spare account follows that follow the target)
const kols = [
  ["cobie_fan_acc", 410000], ["solana_whale_x", 220000], ["degen_daily", 85000], ["mevtrader_eth", 64000], ["alpha_hunter", 33000], ["onchain_ana", 21000], ["tinykol", 6200],
];
write("followers_you_follow.json", { data: { user: { result: { __typename: "User", timeline: { timeline: { instructions: [{ type: "TimelineAddEntries", entries: [...kols.map(([h, f], i) => ({ entryId: `user-${9000 + i}`, sortIndex: "1", content: { entryType: "TimelineTimelineItem", itemContent: { itemType: "TimelineUser", user_results: { result: user({ rest: String(9000 + i), handle: h, name: h.replace(/_/g, " "), created: "Wed Jan 15 08:00:00 +0000 2020", followers: f }) } } } })), cursor("bottom")] }] } } } } } });

console.log("fixtures written to", OUT.pathname);
