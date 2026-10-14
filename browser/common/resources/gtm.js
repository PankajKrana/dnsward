// Stand-in for Google Tag Manager: keeps `dataLayer` usable and fires event
// callbacks, but loads no tags.
(function () {
  'use strict';
  var w = window;
  var layer = (w.dataLayer = w.dataLayer || []);
  function fire(item) {
    if (item && typeof item === 'object' && typeof item.eventCallback === 'function') {
      setTimeout(item.eventCallback, 0);
    }
  }
  var push = layer.push;
  layer.push = function () {
    for (var i = 0; i < arguments.length; i++) fire(arguments[i]);
    return push.apply(layer, arguments);
  };
  w.google_tag_manager = w.google_tag_manager || {};
})();
