// Stand-in for the comScore / Scorecard Research beacon.
(function () {
  'use strict';
  var noop = function () {};
  window.COMSCORE = window.COMSCORE || {};
  window.COMSCORE.beacon = noop;
  window.COMSCORE.purge = noop;
  window._comscore = window._comscore || [];
})();
