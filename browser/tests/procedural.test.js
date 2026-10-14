// Run with: node --test "browser/tests/*.test.js"
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');

const ctx = { RegExp, Array, Number, Set, String, Math };
vm.createContext(ctx);
vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'common', 'procedural.js'), 'utf8'), ctx, {
  filename: 'procedural.js',
});
const P = vm.runInContext('fuckAdsProcedural', ctx);
/** Values built inside the sandbox live in another realm; copy arrays into this one. */
const ev = (rule, env) => [...P.evaluate(rule, env)];

/** A tiny fake page: elements are plain objects, selectors are looked up in tables. */
function page(spec = {}) {
  let clock = 0;
  const env = {
    now: () => clock,
    tick: (ms) => (clock += ms),
    queryAll: (root, sel) => (root ? root.scope?.[sel] ?? [] : spec.queries?.[sel] ?? []),
    closest: (el, sel) => {
      for (let n = el; n; n = n.parent) if (n.is?.includes(sel)) return n;
      return null;
    },
    parent: (el) => el.parent ?? null,
    text: (el) => el.text ?? '',
    computed: (el, pseudo, prop) => el.css?.[(pseudo ? pseudo + ':' : '') + prop] ?? '',
    xpath: (expr, c) => (c ? c.xpath?.[expr] ?? [] : spec.xpath?.[expr] ?? []),
    hide: (el) => (el.hidden = true),
    remove: (el) => (el.removed = true),
  };
  return env;
}

const el = (props = {}) => ({ ...props });
const text = (pattern) => ({ regex: false, pattern, flags: '' });
const re = (pattern, flags = '') => ({ regex: true, pattern, flags });

test('has-text: plain text means contains, and is case-sensitive', () => {
  const a = el({ text: 'Sponsored post' });
  const b = el({ text: 'sponsored post' });
  const c = el({ text: 'A normal post' });
  const env = page({ queries: { '.card': [a, b, c] } });
  const out = ev({ base: '.card', steps: [{ op: 'has-text', text: text('Sponsored') }] }, env);
  assert.deepEqual(out, [a]);
});

test('has-text: regular expressions and flags', () => {
  const a = el({ text: 'Promoted' });
  const b = el({ text: 'AD' });
  const c = el({ text: 'Plain' });
  const env = page({ queries: { '.x': [a, b, c] } });
  const out = ev({ base: '.x', steps: [{ op: 'has-text', text: re('^(promoted|ad)$', 'i') }] }, env);
  assert.deepEqual(out, [a, b]);
  // The "g" flag is stripped: test() must not carry state between elements.
  const g = P.makeMatcher(re('a', 'gi'), false);
  assert.equal(g('a'), true);
  assert.equal(g('a'), true);
});

test('has-text only examines the start of very long text', () => {
  const long = el({ text: 'x'.repeat(10000) + 'Sponsored' });
  const env = page({ queries: { '.x': [long] } });
  assert.deepEqual(ev({ base: '.x', steps: [{ op: 'has-text', text: text('Sponsored') }] }, env), []);
});

test('matches-css: exact values, regex values and pseudo-elements', () => {
  const shown = el({ css: { display: 'block' } });
  const hidden = el({ css: { display: 'none' } });
  const labelled = el({ css: { 'before:content': '"Ad"' } });
  const env = page({ queries: { div: [shown, hidden], span: [labelled] } });
  assert.deepEqual(
    ev({ base: 'div', steps: [{ op: 'matches-css', property: 'display', value: text('block') }] }, env),
    [shown],
  );
  // Plain CSS values are exact, not "contains".
  assert.deepEqual(
    ev({ base: 'div', steps: [{ op: 'matches-css', property: 'display', value: text('loc') }] }, env),
    [],
  );
  assert.deepEqual(
    ev({ base: 'span', steps: [{ op: 'matches-css', pseudo: 'before', property: 'content', value: re('^"A') }] }, env),
    [labelled],
  );
});

test('xpath: from the document, and relative to the current elements', () => {
  const ad = el({});
  const inner = el({});
  const row = el({ xpath: { './/span': [inner] } });
  const env = page({ xpath: { '//div[@class="ad"]': [ad] }, queries: { '.row': [row] } });
  assert.deepEqual(ev({ base: '', steps: [{ op: 'xpath', expr: '//div[@class="ad"]' }] }, env), [ad]);
  assert.deepEqual(ev({ base: '.row', steps: [{ op: 'xpath', expr: './/span' }] }, env), [inner]);
});

test('upward: by levels and by selector', () => {
  const article = el({ is: ['article'] });
  const wrapper = el({ parent: article });
  const label = el({ parent: wrapper });
  const env = page({ queries: { '.label': [label] } });
  assert.deepEqual(ev({ base: '.label', steps: [{ op: 'upward', levels: 1 }] }, env), [wrapper]);
  assert.deepEqual(ev({ base: '.label', steps: [{ op: 'upward', levels: 2 }] }, env), [article]);
  assert.deepEqual(ev({ base: '.label', steps: [{ op: 'upward', levels: 5 }] }, env), []);
  assert.deepEqual(ev({ base: '.label', steps: [{ op: 'upward', selector: 'article' }] }, env), [article]);
  // Only ancestors count, never the element itself.
  const lone = el({});
  const self = el({ is: ['article'], parent: lone });
  const env2 = page({ queries: { '.s': [self] } });
  assert.deepEqual(ev({ base: '.s', steps: [{ op: 'upward', selector: 'article' }] }, env2), []);
});

test('chains: text filter, then native CSS inside, then upward', () => {
  const badge = el({});
  const card = el({ text: 'Sponsored', scope: { '> span.label': [badge] } });
  const other = el({ text: 'News', scope: { '> span.label': [el({})] } });
  const outer = el({});
  badge.parent = outer;
  const env = page({ queries: { '.feed div': [card, other] } });
  const rule = {
    base: '.feed div',
    steps: [
      { op: 'has-text', text: text('Sponsored') },
      { op: 'scope', selector: '> span.label' },
      { op: 'upward', levels: 1 },
    ],
  };
  assert.deepEqual(ev(rule, env), [outer]);
});

test('the runner hides matches, or removes them for :remove()', () => {
  const a = el({ text: 'Ad' });
  const b = el({ text: 'Ad' });
  const env = page({ queries: { '.a': [a], '.b': [b] } });
  const runner = P.createRunner(
    [
      { base: '.a', steps: [{ op: 'has-text', text: text('Ad') }] },
      { base: '.b', steps: [{ op: 'has-text', text: text('Ad') }], remove: true },
    ],
    env,
  );
  const result = runner.run();
  assert.deepEqual({ acted: result.acted, complete: result.complete }, { acted: 2, complete: true });
  assert.equal(a.hidden, true);
  assert.equal(a.removed, undefined);
  assert.equal(b.removed, true);
  assert.equal(b.hidden, undefined);
});

test('the runner works in time-boxed slices and resumes where it stopped', () => {
  const els = [el({}), el({}), el({}), el({})];
  const queries = {};
  const rules = els.map((e, i) => {
    queries['.r' + i] = [e];
    return { base: '.r' + i, steps: [] };
  });
  const env = page({ queries });
  // Every query costs 20 ms against a 30 ms budget.
  const slow = { ...env, queryAll: (root, sel) => (env.tick(20), env.queryAll(root, sel)) };
  const runner = P.createRunner(rules, slow, { budgetMs: 30 });
  let rounds = 0;
  let acted = 0;
  for (;;) {
    const r = runner.run();
    acted += r.acted;
    rounds++;
    if (r.complete || rounds > 10) break;
  }
  assert.equal(rounds, 2, 'a slow run is split into slices');
  assert.equal(acted, 4, 'every rule runs exactly once per cycle');
  assert.ok(els.every((e) => e.hidden));
});

test('bad data is ignored, never fatal and never guessed at', () => {
  const a = el({ text: 'Ad' });
  const env = page({ queries: { '.a': [a] } });
  for (const rule of [
    null,
    'x',
    {},
    { base: '.a', steps: [{ op: 'frobnicate' }] },
    { base: '.a', steps: [{ op: 'has-text', text: re('(') }] }, // invalid regex
    { base: '.a', steps: [{ op: 'has-text' }] },
    { base: '.a', steps: [{ op: 'matches-css', value: text('x') }] }, // no property
    { base: '.a', steps: [{ op: 'upward', levels: 0 }] },
    { base: '.a', steps: [{ op: 'upward', levels: 1000 }] },
    { base: '.a', steps: [{ op: 'xpath' }] },
    { base: '', steps: [{ op: 'has-text', text: text('Ad') }] }, // nothing to start from
  ]) {
    assert.deepEqual(ev(rule, env), [], JSON.stringify(rule));
  }
  // An exception inside the environment is contained by the runner.
  const boom = { ...env, queryAll: () => { throw new Error('page changed'); } };
  assert.doesNotThrow(() => P.createRunner([{ base: '.a', steps: [] }], boom).run());
});

test('results are de-duplicated and capped', () => {
  const same = el({});
  const env = page({ queries: { '.d': [same, same, same] } });
  assert.deepEqual(ev({ base: '.d', steps: [] }, env), [same]);
  const many = Array.from({ length: 5000 }, () => el({}));
  const env2 = page({ queries: { '.m': many } });
  assert.equal(ev({ base: '.m', steps: [] }, env2).length, 2000);
});

test('domEnv: scoped queries use :scope, errors are swallowed, hiding is !important', () => {
  const calls = [];
  const node = (extra = {}) => ({
    querySelectorAll: (sel) => {
      calls.push(sel);
      if (sel.includes('INVALID')) throw new Error('SyntaxError');
      return [{ nodeType: 1 }];
    },
    ...extra,
  });
  const doc = node({
    evaluate: () => {
      throw new Error('bad xpath');
    },
  });
  const win = { performance: { now: () => 7 }, XPathResult: { ORDERED_NODE_SNAPSHOT_TYPE: 7 }, getComputedStyle: () => ({ getPropertyValue: () => 'block' }) };
  const env = P.domEnv(doc, win);
  assert.equal(env.queryAll(node(), '> span').length, 1);
  assert.deepEqual(calls, [':scope > span']);
  env.queryAll(null, '.x');
  assert.equal(calls[1], '.x');
  assert.deepEqual([...env.queryAll(null, 'INVALID')], []);
  assert.deepEqual([...env.xpath('//bad', null)], []);
  assert.equal(env.computed({}, 'before', 'display'), 'block');
  assert.equal(env.now(), 7);
  const style = [];
  env.hide({ style: { setProperty: (...a) => style.push(a) } });
  assert.deepEqual(style, [['display', 'none', 'important']]);
});
