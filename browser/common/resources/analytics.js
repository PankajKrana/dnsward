// Stand-in for Google Analytics analytics.js: the `ga()` command queue runs
// callbacks and hit callbacks but sends nothing.
(function () {
  'use strict';
  var w = window;
  var name = w.GoogleAnalyticsObject || 'ga';
  var previous = w[name];
  var queued = (previous && previous.q) || [];
  var noop = function () {};
  var tracker = {
    get: function () { return undefined; },
    set: noop,
    send: noop,
  };
  function run(args) {
    if (typeof args[0] === 'function') {
      try { args[0](tracker); } catch (e) { /* the page's own error */ }
      return;
    }
    for (var i = 0; i < args.length; i++) {
      var a = args[i];
      if (a && typeof a === 'object' && typeof a.hitCallback === 'function') {
        setTimeout(a.hitCallback, 0);
      }
    }
  }
  function ga() { run(Array.prototype.slice.call(arguments)); }
  ga.loaded = true;
  ga.create = function () { return tracker; };
  ga.getByName = function () { return tracker; };
  ga.getAll = function () { return []; };
  ga.remove = noop;
  w[name] = ga;
  for (var i = 0; i < queued.length; i++) run(Array.prototype.slice.call(queued[i]));
})();
