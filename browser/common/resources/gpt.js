// Stand-in for Google Publisher Tag (googletag): every call succeeds and no
// ad slot is ever filled.
(function () {
  'use strict';
  var w = window;
  var empty = function () { return []; };
  var chain = (function () {
    var self = function () { return proxy; };
    var proxy = new Proxy(self, {
      get: function (_, key) {
        if (key === 'then') return undefined;
        if (key === Symbol.toPrimitive) return function () { return ''; };
        if (key === 'getSlots' || key === 'getTargetingKeys' || key === 'getTargeting') return empty;
        return self;
      },
    });
    return proxy;
  })();
  var googletag = w.googletag || {};
  var pending = Array.isArray(googletag.cmd) ? googletag.cmd : [];
  googletag.apiReady = true;
  googletag.pubadsReady = true;
  googletag.cmd = {
    push: function () {
      for (var i = 0; i < arguments.length; i++) {
        if (typeof arguments[i] === 'function') {
          try { arguments[i].call(googletag); } catch (e) { /* the page's own error */ }
        }
      }
      return 0;
    },
  };
  googletag.pubads = function () { return chain; };
  googletag.companionAds = function () { return chain; };
  googletag.content = function () { return chain; };
  ['defineSlot', 'defineOutOfPageSlot', 'defineUnit', 'enableServices', 'display',
    'destroySlots', 'disablePublisherConsole', 'setAdIframeTitle', 'openConsole']
    .forEach(function (n) { googletag[n] = function () { return chain; }; });
  googletag.sizeMapping = function () { return chain; };
  googletag.getVersion = function () { return ''; };
  w.googletag = googletag;
  googletag.cmd.push.apply(googletag.cmd, pending);
})();
