// Stand-in for the legacy Google Analytics ga.js: accepts every call and
// reports nothing.
(function () {
  'use strict';
  var w = window;
  var noop = function () {};
  var tracker = new Proxy(noop, {
    get: function (_, key) {
      if (key === 'then') return undefined;
      if (key === Symbol.toPrimitive) return function () { return ''; };
      return function () { return tracker; };
    },
    apply: function () { return tracker; },
  });
  var queue = w._gaq;
  var gaq = {
    push: function () {
      for (var i = 0; i < arguments.length; i++) {
        if (typeof arguments[i] === 'function') {
          try { arguments[i](); } catch (e) { /* the page's own error */ }
        }
      }
      return 0;
    },
  };
  w._gaq = gaq;
  if (Array.isArray(queue)) gaq.push.apply(gaq, queue);
  w._gat = {
    _getTracker: function () { return tracker; },
    _createTracker: function () { return tracker; },
    _anonymizeIp: noop,
    _forceSSL: noop,
  };
})();
