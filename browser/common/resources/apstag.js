// Stand-in for Amazon Publisher Services (apstag): bid requests complete at
// once with no bids.
(function () {
  'use strict';
  var noop = function () {};
  window.apstag = {
    init: noop,
    fetchBids: function (_config, callback) {
      if (typeof callback === 'function') setTimeout(function () { callback([]); }, 0);
    },
    setDisplayBids: noop,
    targetingKeys: function () { return []; },
    renderImp: noop,
    updateReg: noop,
    _Q: [],
  };
})();
