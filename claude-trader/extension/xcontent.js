// X Monitor, isolated world, x.com. Reads only.
// It listens for the copies of x.com's own GraphQL answers that xhook.js relays, parses them with CTXParse, and hands
// the result to the background worker. It does nothing unless the background worker says THIS tab is the bot's job tab,
// so your own x.com browsing in other tabs is never read. It never clicks, types, posts, likes or follows.
(function () {
  "use strict";
  if (window.__ctX) return;
  window.__ctX = true;
  var P = globalThis.CTXParse;
  if (!P) return;
  var active = false;
  var job = null; // { id, kind, key, focalId, ownerRestId }
  var queue = [];
  var probeTimer = null;

  function send(msg) {
    try {
      chrome.runtime.sendMessage(msg, function (r) {
        void chrome.runtime.lastError;
        if (r && r.done) stopProbe();
      });
    } catch (e) {
      /* the extension was reloaded: this page is stale */
    }
  }

  function handle(m) {
    if (!job) return;
    var piece = null;
    if (m.status >= 400) {
      send({ t: "xhttp", jobId: job.id, op: m.op, status: m.status, rate: m.rate });
      return;
    }
    if (m.rate && m.rate.remaining !== null && m.rate.remaining !== undefined) send({ t: "xrate", jobId: job.id, rate: m.rate });
    try {
      var json = JSON.parse(m.body);
      piece = P.parseResponse(m.op, json, { focalId: job.focalId, ownerRestId: job.ownerRestId });
    } catch (e) {
      piece = { kind: P.classify(m.op), drift: true };
    }
    if (piece) {
      if (piece.kind === "user" && piece.user) job.ownerRestId = piece.user.restId;
      send({ t: "xpiece", jobId: job.id, piece: piece });
    }
  }

  window.addEventListener("message", function (ev) {
    if (ev.source !== window || ev.origin !== location.origin) return;
    var d = ev.data;
    if (!d || d.ct !== "x-resp" || typeof d.op !== "string") return;
    if (!active) {
      if (queue.length < 30) queue.push(d);
      return;
    }
    handle(d);
  });

  function pageText() {
    return document.body ? String(document.body.innerText || "").slice(0, 8000) : "";
  }

  function probe() {
    send({ t: "xpage", jobId: job && job.id, state: P.detectPage({ url: location.href, text: pageText() }), url: location.href });
  }
  function stopProbe() {
    clearInterval(probeTimer);
    probeTimer = null;
  }

  function start(info) {
    active = true;
    job = info;
    var q = queue;
    queue = [];
    q.forEach(handle);
    if (!probeTimer) probeTimer = setInterval(probe, 2500);
    setTimeout(probe, 1200);
  }

  // DOM fallback: read what is on screen. Asked for by the background worker when no data arrived in time.
  function domFallback(kind) {
    var out = { kind: kind };
    function t(sel) {
      var el = document.querySelector(sel);
      return el ? String(el.innerText || el.textContent || "") : "";
    }
    if (kind === "profile" || kind === "smart") {
      var nameBlock = t('[data-testid="UserName"]').split("\n").map(function (s) { return s.trim(); }).filter(Boolean);
      var handleText = nameBlock.filter(function (s) { return s[0] === "@"; })[0] || "";
      var fol = document.querySelector('a[href$="/verified_followers"], a[href$="/followers"]');
      var folg = document.querySelector('a[href$="/following"]');
      var user = P.profileFromDom({ name: nameBlock[0] || "", handle: handleText, bio: t('[data-testid="UserDescription"]'), followersText: fol ? fol.innerText : "", followingText: folg ? folg.innerText : "", joinedText: t('[data-testid="UserJoinDate"]') });
      if (user && user.followers) out.piece = { kind: "user", state: "ok", user: user };
    } else if (kind === "tweet") {
      var art = document.querySelector('article[data-testid="tweet"]');
      if (art) {
        var group = art.querySelector('[role="group"][aria-label]');
        var m = P.parseMetricsLabel(group ? group.getAttribute("aria-label") : "");
        var who = (art.querySelector('[data-testid="User-Name"]') || {}).innerText || "";
        var handle = (/@([A-Za-z0-9_]{1,15})/.exec(who) || [])[1] || "";
        var tt = art.querySelector('[data-testid="tweetText"]');
        var idm = /\/status\/(\d+)/.exec(location.pathname);
        if (handle && idm) {
          out.piece = { kind: "detail", tweet: { id: idm[1], text: tt ? String(tt.innerText || "").slice(0, 1500) : "", createdAtMs: 0, views: m.views, likes: m.likes, replies: m.replies, reposts: m.reposts, quotes: 0, bookmarks: m.bookmarks, isRetweet: false, inReplyToId: "", communityId: "", author: { restId: "", screenName: handle, name: who.split("\n")[0] || "", createdAtMs: 0, followers: 0, following: 0, statuses: 0, listed: 0, verified: false, protected: false, defaultAvatar: false, description: "" } }, replies: [] };
        }
      }
    } else if (kind === "community") {
      var mm = /([\d.,]+\s*[KMB]?)\s+Members/i.exec(pageText());
      if (mm) out.piece = { kind: "community", community: { id: "", name: "", members: P.parseCount(mm[1]), createdAtMs: 0, description: "", postsSeen: 0, latestPostMs: 0 } };
    }
    return out;
  }

  chrome.runtime.onMessage.addListener(function (msg, sender, reply) {
    if (!msg || sender.id !== chrome.runtime.id) return;
    if (msg.t === "xdom" && active && job) {
      reply(domFallback(msg.kind));
    } else if (msg.t === "xstart") {
      start(msg.job);
      reply({ ok: true });
    }
  });

  // Ask whether this tab is the bot's job tab. If not, stay silent.
  try {
    chrome.runtime.sendMessage({ t: "xhello", url: location.href }, function (r) {
      void chrome.runtime.lastError;
      if (r && r.active && r.job) start(r.job);
      else queue = [];
    });
  } catch (e) {
    /* ignore */
  }
})();
