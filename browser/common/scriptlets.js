/*
 * AdBlock scriptlet runtime.
 *
 * Runs in the page's MAIN world at document_start. It is injected as
 *   (adblockScriptletRuntime.toString())(JSON.stringify(bundle))
 * so filter lists contribute *data only*: a scriptlet name (which must be
 * one of the functions below) and string arguments. Nothing from a filter
 * list is ever evaluated as code.
 *
 * The function must be self-contained: no references to outer scope.
 */
/* exported adblockScriptletRuntime */
function adblockScriptletRuntime(bundle) {
  'use strict';
  if (!bundle || !Array.isArray(bundle.rules)) return;
  const host = location.hostname;
  if (!host) return;

  // ---- hostname matching (mirrors the Rust engine) -------------------
  const MULTI = new Set(['co.uk', 'com.au', 'co.jp', 'co.nz', 'com.br', 'co.in', 'com.mx', 'co.kr', 'com.tr', 'co.za']);
  const sameOrSub = (h, d) => h === d || (h.length > d.length && h.endsWith(d) && h[h.length - d.length - 1] === '.');
  const entity = (h, e) => {
    let s = h;
    while (s) {
      if (s.startsWith(e + '.')) {
        const tld = s.slice(e.length + 1);
        return tld.length > 0 && (!tld.includes('.') || MULTI.has(tld));
      }
      const i = s.indexOf('.');
      if (i < 0) break;
      s = s.slice(i + 1);
    }
    return false;
  };
  const patternMatches = (p) =>
    (p.Domain !== undefined && sameOrSub(host, p.Domain)) || (p.Entity !== undefined && entity(host, p.Entity));
  const applies = (r) =>
    (r.include.length === 0 || r.include.some(patternMatches)) && !r.exclude.some(patternMatches);

  const exceptions = bundle.rules.filter((r) => r.exception && applies(r));
  if (exceptions.some((e) => e.name === '')) return;
  const sameArgs = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
  const selected = [];
  const seen = new Set();
  for (const r of bundle.rules) {
    if (r.exception || !applies(r)) continue;
    if (exceptions.some((e) => e.name === r.name && (e.args.length === 0 || sameArgs(e.args, r.args)))) continue;
    const key = r.name + '\u0000' + r.args.join('\u0000');
    if (seen.has(key)) continue;
    seen.add(key);
    selected.push(r);
  }
  if (selected.length === 0) return;

  // ---- helpers --------------------------------------------------------
  const safe = (fn) => {
    try {
      fn();
    } catch (e) {
      /* a failing scriptlet must never break the page */
    }
  };
  const toPattern = (s) => {
    if (!s) return null;
    const m = /^\/(.+)\/([gimsuy]*)$/.exec(s);
    if (m) {
      try {
        return new RegExp(m[1], m[2].replace('g', ''));
      } catch (e) {
        return null;
      }
    }
    return new RegExp(s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
  };
  const parseVarArgs = (args) => {
    const out = {};
    for (let i = 0; i + 1 < args.length; i += 2) out[args[i]] = args[i + 1];
    return out;
  };
  const noop = function () {};
  const constantValue = (v) => {
    switch (v) {
      case 'undefined': return { ok: true, value: undefined };
      case 'false': return { ok: true, value: false };
      case 'true': return { ok: true, value: true };
      case 'null': return { ok: true, value: null };
      case "''": case 'emptyStr': return { ok: true, value: '' };
      case '{}': case 'emptyObj': return { ok: true, value: {} };
      case '[]': case 'emptyArr': return { ok: true, value: [] };
      case 'noopFunc': return { ok: true, value: noop };
      case 'trueFunc': return { ok: true, value: () => true };
      case 'falseFunc': return { ok: true, value: () => false };
      case 'noopPromiseResolve': return { ok: true, value: () => Promise.resolve() };
      default:
        if (/^-?\d+(\.\d+)?$/.test(v) && v.length < 16) return { ok: true, value: Number(v) };
        return { ok: false };
    }
  };

  /* Deletes `path` (dot-separated, `*` / `[]` match any key) from obj. */
  const deletePath = (obj, path) => {
    const parts = path.split('.');
    let found = false;
    const walk = (o, i) => {
      if (o === null || typeof o !== 'object') return;
      const k = parts[i];
      const last = i === parts.length - 1;
      const keys = k === '*' || k === '[]' ? Object.keys(o) : Object.prototype.hasOwnProperty.call(o, k) ? [k] : [];
      for (const key of keys) {
        if (last) {
          delete o[key];
          found = true;
        } else {
          walk(o[key], i + 1);
        }
      }
    };
    walk(obj, 0);
    return found;
  };
  const hasPath = (obj, path) => {
    const parts = path.split('.');
    const walk = (o, i) => {
      if (i === parts.length) return true;
      if (o === null || typeof o !== 'object') return false;
      const k = parts[i];
      if (k === '*' || k === '[]') return Object.keys(o).some((key) => walk(o[key], i + 1));
      return Object.prototype.hasOwnProperty.call(o, k) && walk(o[k], i + 1);
    };
    return walk(obj, 0);
  };
  const makePruner = (propsToRemove, requiredProps) => {
    const remove = (propsToRemove || '').split(/\s+/).filter(Boolean);
    const required = (requiredProps || '').split(/\s+/).filter(Boolean);
    return (obj) => {
      if (remove.length === 0 || obj === null || typeof obj !== 'object') return obj;
      if (required.length && !required.every((p) => hasPath(obj, p))) return obj;
      for (const p of remove) deletePath(obj, p);
      return obj;
    };
  };
  const urlOf = (input) => {
    if (typeof input === 'string') return input;
    if (input instanceof Request) return input.url;
    try {
      return String(input);
    } catch (e) {
      return '';
    }
  };

  // ---- scriptlets -----------------------------------------------------
  const impl = {
    'set-constant'(chain, rawValue) {
      if (!chain) return;
      const cv = constantValue(rawValue);
      if (!cv.ok) return;
      const value = cv.value;
      const trap = (owner, parts) => {
        const prop = parts[0];
        if (parts.length === 1) {
          try {
            Object.defineProperty(owner, prop, {
              configurable: false,
              get: () => value,
              set: noop,
            });
          } catch (e) { /* non-configurable */ }
          return;
        }
        const rest = parts.slice(1);
        const current = owner[prop];
        if (current !== undefined && current !== null && typeof current === 'object') {
          trap(current, rest);
          return;
        }
        // Not yet defined: intercept assignment, then trap the new object.
        let stored = current;
        try {
          Object.defineProperty(owner, prop, {
            configurable: true,
            get: () => stored,
            set: (v) => {
              stored = v;
              if (v !== null && typeof v === 'object') trap(v, rest);
            },
          });
        } catch (e) { /* ignore */ }
      };
      trap(window, chain.split('.'));
    },

    'json-prune'(propsToRemove, requiredProps) {
      const prune = makePruner(propsToRemove, requiredProps);
      const parse = JSON.parse;
      JSON.parse = new Proxy(parse, {
        apply(target, thisArg, args) {
          return prune(Reflect.apply(target, thisArg, args));
        },
      });
      const json = Response.prototype.json;
      Response.prototype.json = new Proxy(json, {
        apply(target, thisArg, args) {
          return Reflect.apply(target, thisArg, args).then(prune);
        },
      });
    },

    'json-prune-fetch-response'(propsToRemove, requiredProps, ...rest) {
      const prune = makePruner(propsToRemove, requiredProps);
      const pattern = toPattern(parseVarArgs(rest).propsToMatch);
      const fetchOrig = window.fetch;
      window.fetch = new Proxy(fetchOrig, {
        apply(target, thisArg, args) {
          const p = Reflect.apply(target, thisArg, args);
          if (pattern && !pattern.test(urlOf(args[0]))) return p;
          return p.then((resp) =>
            resp
              .clone()
              .json()
              .then((obj) => {
                const pruned = prune(obj);
                return new Response(JSON.stringify(pruned), {
                  status: resp.status,
                  statusText: resp.statusText,
                  headers: resp.headers,
                });
              })
              .catch(() => resp),
          );
        },
      });
    },

    'json-prune-xhr-response'(propsToRemove, requiredProps, ...rest) {
      const prune = makePruner(propsToRemove, requiredProps);
      const pattern = toPattern(parseVarArgs(rest).propsToMatch);
      const XHR = XMLHttpRequest.prototype;
      const open = XHR.open;
      const urls = new WeakMap();
      XHR.open = function (method, url, ...a) {
        urls.set(this, String(url));
        return open.call(this, method, url, ...a);
      };
      const patch = (name) => {
        const desc = Object.getOwnPropertyDescriptor(XHR, name);
        if (!desc || !desc.get) return;
        Object.defineProperty(XHR, name, {
          configurable: true,
          get() {
            const value = desc.get.call(this);
            if (this.readyState !== 4 || (pattern && !pattern.test(urls.get(this) || ''))) return value;
            try {
              if (typeof value === 'string' && value) return JSON.stringify(prune(JSON.parse(value)));
              if (value && typeof value === 'object') return prune(value);
            } catch (e) { /* not JSON */ }
            return value;
          },
        });
      };
      patch('response');
      patch('responseText');
    },

    'abort-on-property-read'(chain) {
      if (!chain) return;
      const parts = chain.split('.');
      const last = parts.pop();
      let owner = window;
      for (const p of parts) {
        owner = owner[p];
        if (owner === null || typeof owner !== 'object') return;
      }
      const err = () => {
        throw new ReferenceError(last);
      };
      try {
        Object.defineProperty(owner, last, { configurable: false, get: err, set: noop });
      } catch (e) { /* ignore */ }
    },

    'abort-on-property-write'(prop) {
      if (!prop || prop.includes('.')) return;
      try {
        Object.defineProperty(window, prop, {
          configurable: false,
          get: () => undefined,
          set: () => {
            throw new ReferenceError(prop);
          },
        });
      } catch (e) { /* ignore */ }
    },

    'abort-current-script'(chain, needle) {
      if (!chain) return;
      const re = toPattern(needle);
      const parts = chain.split('.');
      const last = parts.pop();
      let owner = window;
      for (const p of parts) {
        owner = owner[p];
        if (owner === null || typeof owner !== 'object') return;
      }
      const desc = Object.getOwnPropertyDescriptor(owner, last);
      let value = owner[last];
      const check = () => {
        const s = document.currentScript;
        if (s instanceof HTMLScriptElement && (!re || re.test(s.src || s.textContent))) {
          throw new ReferenceError(last);
        }
      };
      try {
        Object.defineProperty(owner, last, {
          configurable: true,
          get() {
            check();
            return desc && desc.get ? desc.get.call(this) : value;
          },
          set(v) {
            check();
            if (desc && desc.set) desc.set.call(this, v);
            else value = v;
          },
        });
      } catch (e) { /* ignore */ }
    },

    'remove-attr'(attrs, selector) {
      const names = (attrs || '').split('|').map((s) => s.trim()).filter(Boolean);
      if (!names.length) return;
      const sel = selector || names.map((n) => `[${CSS.escape(n)}]`).join(',');
      const run = () => safe(() => {
        for (const el of document.querySelectorAll(sel)) names.forEach((n) => el.removeAttribute(n));
      });
      observe(run);
    },

    'remove-class'(classes, selector) {
      const names = (classes || '').split('|').map((s) => s.trim()).filter(Boolean);
      if (!names.length) return;
      const sel = selector || names.map((n) => '.' + CSS.escape(n)).join(',');
      const run = () => safe(() => {
        for (const el of document.querySelectorAll(sel)) el.classList.remove(...names);
      });
      observe(run);
    },

    'no-setTimeout-if'(needle, delay) {
      wrapTimer('setTimeout', needle, delay);
    },

    'no-setInterval-if'(needle, delay) {
      wrapTimer('setInterval', needle, delay);
    },

    'no-fetch-if'(props) {
      if (!props) return;
      const re = toPattern(props.replace(/^url:/, ''));
      const fetchOrig = window.fetch;
      window.fetch = new Proxy(fetchOrig, {
        apply(target, thisArg, args) {
          if (re && re.test(urlOf(args[0]))) return Promise.resolve(new Response('', { status: 200 }));
          return Reflect.apply(target, thisArg, args);
        },
      });
    },
  };

  function observe(run) {
    run();
    const start = () => {
      run();
      let queued = false;
      new MutationObserver(() => {
        if (queued) return;
        queued = true;
        requestAnimationFrame(() => {
          queued = false;
          run();
        });
      }).observe(document.documentElement, { childList: true, subtree: true, attributes: true });
    };
    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start, { once: true });
    else start();
  }

  function wrapTimer(name, needle, delay) {
    const re = toPattern(needle && needle.replace(/^!/, ''));
    const invert = !!needle && needle.startsWith('!');
    const wanted = delay ? Number(delay.replace(/^!/, '')) : null;
    const original = window[name];
    window[name] = new Proxy(original, {
      apply(target, thisArg, args) {
        const [cb, ms] = args;
        const text = String(cb);
        let hit = !re || re.test(text);
        if (invert) hit = !hit;
        if (hit && (wanted === null || Number(ms) === wanted)) return 0;
        return Reflect.apply(target, thisArg, args);
      },
    });
  }

  for (const r of selected) {
    const fn = impl[r.name];
    if (typeof fn === 'function') safe(() => fn(...r.args));
  }
}

if (typeof module !== 'undefined') module.exports = { adblockScriptletRuntime };
