// Run with: node --test "browser/tests/*.test.js"
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');

const dir = path.join(__dirname, '..', 'common', 'resources');

/** Runs a stub in a fresh page-like context and returns it. */
function run(file, globals = {}) {
  const ctx = { setTimeout, console, ...globals };
  ctx.window = ctx;
  vm.createContext(ctx);
  vm.runInContext(fs.readFileSync(path.join(dir, file), 'utf8'), ctx, { filename: file });
  return ctx;
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 5));

test('every script stub parses and runs without a page', () => {
  for (const file of fs.readdirSync(dir).filter((f) => f.endsWith('.js'))) {
    assert.doesNotThrow(() => run(file), file);
  }
});

test('ga.js runs queued and later commands and returns a usable tracker', () => {
  let ran = 0;
  const w = run('ga.js', { _gaq: [() => ran++] });
  assert.equal(ran, 1);
  w._gaq.push(() => ran++);
  assert.equal(ran, 2);
  const t = w._gat._getTracker('UA-1');
  assert.doesNotThrow(() => t._trackPageview().anything('x'));
});

test('analytics.js drains the queue and fires hit callbacks', async () => {
  let created;
  let hit = false;
  const q = [['create', 'UA-1'], [(t) => (created = t)], ['send', { hitCallback: () => (hit = true) }]];
  const w = run('analytics.js', { ga: { q } });
  assert.ok(created, 'ready callback received a tracker');
  assert.equal(w.ga.loaded, true);
  await tick();
  assert.equal(hit, true);
  assert.deepEqual(Array.from(w.ga.getAll()), []);
});

test('gtm.js keeps dataLayer working and fires eventCallback', async () => {
  let fired = false;
  const w = run('gtm.js');
  w.dataLayer.push({ event: 'x', eventCallback: () => (fired = true) });
  assert.equal(w.dataLayer.length, 1);
  await tick();
  assert.equal(fired, true);
});

test('gpt.js executes commands and supports chained calls', () => {
  let ran = false;
  const w = run('gpt.js', { googletag: { cmd: [() => (ran = true)] } });
  assert.equal(ran, true);
  w.googletag.cmd.push(() => (ran = 'later'));
  assert.equal(ran, 'later');
  assert.doesNotThrow(() => {
    w.googletag.defineSlot('/1/x', [300, 250], 'div').addService(w.googletag.pubads()).setTargeting('a', 'b');
    w.googletag.pubads().enableSingleRequest();
    w.googletag.enableServices();
    w.googletag.display('div');
  });
  assert.deepEqual(Array.from(w.googletag.pubads().getSlots()), []);
});

test('apstag.js completes bid requests with no bids', async () => {
  let bids;
  const w = run('apstag.js');
  w.apstag.init({});
  w.apstag.fetchBids({}, (b) => (bids = b));
  await tick();
  assert.deepEqual(Array.from(bids), []);
});

test('beacon.js defines COMSCORE.beacon', () => {
  const w = run('beacon.js');
  assert.doesNotThrow(() => w.COMSCORE.beacon({ c1: 2 }));
});
