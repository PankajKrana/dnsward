/*
 * FuckAds content script (isolated world, every frame, document_start).
 *
 * Collects the class and id names present in the page and asks the
 * background script for matching cosmetic filters. Only these tokens and
 * the frame's hostname are sent, and only to the local FuckAds service.
 *
 * The first reply may also carry procedural filters for this site
 * (:has-text() and friends); procedural.js evaluates them against the page.
 */
(() => {
  'use strict';
  const api = globalThis.browser ?? globalThis.chrome;
  if (!/^https?:$/.test(location.protocol)) return;

  const seenClasses = new Set();
  const seenIds = new Set();
  const MAX_TOKENS = 20000;

  // Procedural filters: evaluate once the service has sent them, then again
  // (debounced, time-boxed) as the page changes.
  let proceduralStarted = false;
  const startProcedural = (rules) => {
    if (proceduralStarted || typeof fuckAdsProcedural === 'undefined') return;
    proceduralStarted = true;
    const runner = fuckAdsProcedural.createRunner(rules, fuckAdsProcedural.domEnv(document, window));
    let timer = null;
    const slice = () => {
      timer = null;
      if (!runner.run().complete) timer = setTimeout(slice, 50);
    };
    const schedule = () => {
      if (timer === null) timer = setTimeout(slice, 250);
    };
    // Attribute changes are not observed: hiding an element changes its style
    // attribute, which would retrigger this forever.
    new MutationObserver(schedule).observe(document, { childList: true, subtree: true, characterData: true });
    slice();
  };

  const send = (classes, ids, complex) => {
    try {
      const reply = api.runtime.sendMessage({ type: 'cosmetic', classes, ids, complex });
      if (complex && reply && typeof reply.then === 'function') {
        reply
          .then((r) => {
            if (r && Array.isArray(r.procedural) && r.procedural.length > 0) startProcedural(r.procedural);
          })
          .catch(() => {});
      } else {
        reply?.catch?.(() => {});
      }
    } catch (e) {
      /* extension reloaded */
    }
  };

  // Site-specific and complex generic selectors: once, as early as possible.
  send([], [], true);

  const collect = (root) => {
    const newClasses = [];
    const newIds = [];
    if (seenClasses.size + seenIds.size > MAX_TOKENS) return [newClasses, newIds];
    const visit = (el) => {
      if (el.id && !seenIds.has(el.id) && el.id.length <= 128) {
        seenIds.add(el.id);
        newIds.push(el.id);
      }
      const cl = el.classList;
      if (cl) {
        for (const c of cl) {
          if (!seenClasses.has(c) && c.length <= 128) {
            seenClasses.add(c);
            newClasses.push(c);
          }
        }
      }
    };
    if (root.nodeType === 1) visit(root);
    if (root.querySelectorAll) for (const el of root.querySelectorAll('[id],[class]')) visit(el);
    return [newClasses, newIds];
  };

  let pendingNodes = [];
  let scheduled = false;
  const flush = () => {
    scheduled = false;
    const nodes = pendingNodes;
    pendingNodes = [];
    const classes = [];
    const ids = [];
    for (const n of nodes) {
      const [c, i] = collect(n);
      classes.push(...c);
      ids.push(...i);
    }
    if (classes.length || ids.length) send(classes, ids, false);
  };
  const schedule = (node) => {
    pendingNodes.push(node);
    if (!scheduled) {
      scheduled = true;
      setTimeout(flush, 200);
    }
  };

  const start = () => {
    schedule(document.documentElement);
    new MutationObserver((mutations) => {
      for (const m of mutations) {
        if (m.type === 'attributes') schedule(m.target);
        else for (const n of m.addedNodes) if (n.nodeType === 1) schedule(n);
      }
    }).observe(document.documentElement, { childList: true, subtree: true, attributes: true, attributeFilter: ['class', 'id'] });
  };
  if (document.documentElement) start();
  else document.addEventListener('DOMContentLoaded', start, { once: true });
})();
