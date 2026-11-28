// X Monitor, MAIN world, document_start on x.com.
// It only LISTENS. It wraps fetch and XMLHttpRequest so that, when x.com's own page asks for its own GraphQL data,
// a copy of the answer is handed (by postMessage) to our isolated script. It never changes a request, never sends
// one of its own, and never clicks, posts, likes or follows anything. Any error in here is swallowed so the page works as normal.
(function () {
  "use strict";
  if (window.__ctXHook) return;
  window.__ctXHook = true;
  var OP = /\/i\/api\/graphql\/[^/?#]+\/([A-Za-z0-9_]+)/;
  var WANT = /^(UserByScreenName|UserByRestId|UserTweets|UserTweetsAndReplies|TweetDetail|TweetResultByRestId|Communit[A-Za-z]*|[A-Za-z]*FollowersYouFollow[A-Za-z]*)$/;
  var MAX = 6 * 1024 * 1024;

  function opOf(url) {
    var m = OP.exec(String(url || ""));
    return m && WANT.test(m[1]) ? m[1] : null;
  }

  function relay(op, status, text, remaining, reset) {
    try {
      if (typeof text !== "string" || text.length > MAX) text = null;
      window.postMessage({ ct: "x-resp", op: op, status: status, body: text, rate: { remaining: remaining, reset: reset } }, location.origin);
    } catch (e) {
      /* never break the page */
    }
  }

  var origFetch = window.fetch;
  if (typeof origFetch === "function") {
    window.fetch = function () {
      var p = origFetch.apply(this, arguments);
      try {
        var a0 = arguments[0];
        var op = opOf(typeof a0 === "string" ? a0 : a0 && a0.url);
        if (op) {
          p.then(function (r) {
            try {
              var rem = r.headers.get("x-rate-limit-remaining");
              var rst = r.headers.get("x-rate-limit-reset");
              r.clone().text().then(function (t) { relay(op, r.status, t, rem, rst); }, function () {});
            } catch (e) { /* ignore */ }
          }, function () {});
        }
      } catch (e) { /* ignore */ }
      return p;
    };
  }

  var XHR = window.XMLHttpRequest && window.XMLHttpRequest.prototype;
  if (XHR) {
    var origOpen = XHR.open;
    var origSend = XHR.send;
    XHR.open = function (method, url) {
      try { this.__ctOp = opOf(url); } catch (e) { /* ignore */ }
      return origOpen.apply(this, arguments);
    };
    XHR.send = function () {
      try {
        var self = this;
        if (self.__ctOp) {
          self.addEventListener("load", function () {
            try {
              var text = self.responseType === "" || self.responseType === "text" ? self.responseText : null;
              relay(self.__ctOp, self.status, text, self.getResponseHeader("x-rate-limit-remaining"), self.getResponseHeader("x-rate-limit-reset"));
            } catch (e) { /* ignore */ }
          });
        }
      } catch (e) { /* ignore */ }
      return origSend.apply(this, arguments);
    };
  }
})();
