/*
 * Procedural cosmetic filters (:has-text(), :matches-css(), :xpath(),
 * :upward(), :remove()).
 *
 * The FuckAds service parses and validates these filters before they get
 * here: operators are a fixed set and every argument is typed data (plain
 * text, a bounded regular expression, an XPath expression, a number, or a
 * native CSS selector). Nothing from a filter list is ever evaluated as code.
 * This file only interprets that data against the page.
 *
 * All DOM access goes through an `env` object so the logic can be tested
 * without a browser (browser/tests/procedural.test.js); `domEnv()` builds the
 * real one.
 */
'use strict';

const fuckAdsProcedural = (() => {
  const MAX_TEXT = 8192; // characters of an element's text that are examined
  const MAX_ELEMENTS = 2000; // elements carried between steps

  /**
   * Builds a predicate from a `{regex, pattern, flags}` matcher, or null if
   * it cannot be used. `exact` compares whole values (CSS), otherwise plain
   * text means "contains".
   */
  function makeMatcher(m, exact) {
    if (!m || typeof m.pattern !== 'string') return null;
    if (!m.regex) return exact ? (s) => s === m.pattern : (s) => s.includes(m.pattern);
    let re;
    try {
      re = new RegExp(m.pattern, typeof m.flags === 'string' ? m.flags.replace(/[^imsu]/g, '') : '');
    } catch (e) {
      return null;
    }
    return (s) => re.test(s);
  }

  function unique(list) {
    const out = [];
    const seen = new Set();
    for (const el of list) {
      if (el && !seen.has(el)) {
        seen.add(el);
        out.push(el);
        if (out.length >= MAX_ELEMENTS) break;
      }
    }
    return out;
  }

  /** The elements a rule selects on the current page. Never throws. */
  function evaluate(rule, env) {
    if (!rule || typeof rule !== 'object') return [];
    // null means "the whole document" (a rule that starts with :xpath()).
    let set = rule.base ? unique(env.queryAll(null, rule.base)) : null;
    const steps = Array.isArray(rule.steps) ? rule.steps : [];
    for (const step of steps) {
      if (set !== null && set.length === 0) return [];
      switch (step && step.op) {
        case 'has-text': {
          const test = makeMatcher(step.text, false);
          if (!test || set === null) return [];
          set = set.filter((el) => test(String(env.text(el)).slice(0, MAX_TEXT)));
          break;
        }
        case 'matches-css': {
          const test = makeMatcher(step.value, true);
          if (!test || set === null || typeof step.property !== 'string') return [];
          set = set.filter((el) => {
            const value = env.computed(el, step.pseudo || null, step.property);
            return typeof value === 'string' && test(value.trim());
          });
          break;
        }
        case 'xpath': {
          if (typeof step.expr !== 'string') return [];
          const out = [];
          for (const ctx of set === null ? [null] : set) {
            for (const el of env.xpath(step.expr, ctx)) {
              out.push(el);
              if (out.length >= MAX_ELEMENTS) break;
            }
          }
          set = unique(out);
          break;
        }
        case 'upward': {
          if (set === null) return [];
          const out = [];
          for (const el of set) {
            let target = null;
            if (Number.isInteger(step.levels) && step.levels >= 1 && step.levels <= 255) {
              target = el;
              for (let i = 0; i < step.levels && target; i++) target = env.parent(target);
            } else if (typeof step.selector === 'string') {
              const parent = env.parent(el);
              target = parent ? env.closest(parent, step.selector) : null;
            }
            if (target) out.push(target);
          }
          set = unique(out);
          break;
        }
        case 'scope': {
          if (set === null || typeof step.selector !== 'string') return [];
          const out = [];
          for (const el of set) out.push(...env.queryAll(el, step.selector));
          set = unique(out);
          break;
        }
        default:
          return []; // an operator we do not know: do nothing, never guess
      }
    }
    return set === null ? [] : set;
  }

  /**
   * Applies `rules` (hide, or remove for `:remove()`), a time-boxed slice at
   * a time. `run()` returns `{acted, complete}`. While `complete` is false the
   * cycle is unfinished: call `run()` again and it continues with the next
   * rule, so each rule is evaluated exactly once per cycle.
   */
  function createRunner(rules, env, options) {
    const budgetMs = (options && options.budgetMs) || 30;
    let next = 0;
    function run() {
      const start = env.now();
      let acted = 0;
      for (let i = next; i < rules.length; i++) {
        let els;
        try {
          els = evaluate(rules[i], env);
        } catch (e) {
          els = [];
        }
        for (const el of els) {
          try {
            if (rules[i] && rules[i].remove) env.remove(el);
            else env.hide(el);
            acted++;
          } catch (e) {
            /* the page changed under us */
          }
        }
        if (env.now() - start > budgetMs && i + 1 < rules.length) {
          next = i + 1;
          return { acted, complete: false };
        }
      }
      next = 0;
      return { acted, complete: true };
    }
    return { run };
  }

  /** The real DOM, for use in a content script. */
  function domEnv(doc, win) {
    return {
      now: () => win.performance.now(),
      queryAll(root, selector) {
        try {
          const list = root ? root.querySelectorAll(':scope ' + selector) : doc.querySelectorAll(selector);
          return Array.from(list).slice(0, MAX_ELEMENTS);
        } catch (e) {
          return []; // a selector this browser does not understand
        }
      },
      closest(el, selector) {
        try {
          return el.closest(selector);
        } catch (e) {
          return null;
        }
      },
      parent: (el) => el.parentElement,
      text: (el) => el.textContent || '',
      computed(el, pseudo, property) {
        try {
          return win.getComputedStyle(el, pseudo ? '::' + pseudo : null).getPropertyValue(property);
        } catch (e) {
          return '';
        }
      },
      xpath(expr, ctx) {
        try {
          const result = doc.evaluate(expr, ctx || doc, null, win.XPathResult.ORDERED_NODE_SNAPSHOT_TYPE, null);
          const out = [];
          for (let i = 0; i < result.snapshotLength && out.length < MAX_ELEMENTS; i++) {
            const node = result.snapshotItem(i);
            if (node && node.nodeType === 1) out.push(node);
          }
          return out;
        } catch (e) {
          return [];
        }
      },
      hide: (el) => el.style.setProperty('display', 'none', 'important'),
      remove: (el) => el.remove(),
    };
  }

  return { makeMatcher, evaluate, createRunner, domEnv };
})();
