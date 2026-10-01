// Run with: node --test "browser/tests/*.test.js"
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');

/** Values from the sandbox live in another realm; compare their JSON form. */
const plain = (x) => (x === undefined ? undefined : JSON.parse(JSON.stringify(x)));

const runtimeSource = fs.readFileSync(path.join(__dirname, '..', 'common', 'scriptlets.js'), 'utf8');

/** Creates a page-like sandbox for `hostname` and injects the runtime the
 * same way the extension does: function source + JSON data. */
function page(hostname, rules, extra = {}) {
  // Per-page Response class so prototype patches do not leak between
  // sandboxes (JSON, Object, Proxy... are the context's own intrinsics).
  class PageResponse extends Response {}
  const sandbox = {
    location: { hostname },
    Response: PageResponse,
    Request,
    setTimeout: (fn) => 1,
    setInterval: (fn) => 2,
    fetch: async () => new PageResponse(JSON.stringify({ playerAds: [1], adPlacements: [2], videoDetails: { id: 'x' } })),
    XMLHttpRequest: function () {},
    ...extra,
  };
  sandbox.window = sandbox;
  vm.createContext(sandbox);
  vm.runInContext(runtimeSource, sandbox);
  const code = `(${sandbox.adblockScriptletRuntime.toString()})(${JSON.stringify({ rules })});`;
  vm.runInContext(code, sandbox);
  return sandbox;
}

const rule = (name, args, include = ['youtube.com'], extra = {}) => ({
  include: include.map((d) => ({ Domain: d })),
  exclude: [],
  name,
  args,
  exception: false,
  ...extra,
});

test('json-prune removes ad placements from JSON.parse results', () => {
  const p = page('www.youtube.com', [rule('json-prune', ['playerResponse.adPlacements adPlacements playerAds'])]);
  const out = vm.runInContext(
    `JSON.parse('{"adPlacements":[1],"playerAds":[2],"playerResponse":{"adPlacements":[3],"streamingData":{}},"title":"t"}')`,
    p,
  );
  assert.equal(out.adPlacements, undefined);
  assert.equal(out.playerAds, undefined);
  assert.equal(out.playerResponse.adPlacements, undefined);
  assert.deepEqual(plain(out.playerResponse.streamingData), {});
  assert.equal(out.title, 't');
});

test('json-prune honours required properties', () => {
  const p = page('youtube.com', [rule('json-prune', ['ads', 'mustExist'])]);
  assert.deepEqual(plain(vm.runInContext(`JSON.parse('{"ads":1}')`, p)), { ads: 1 });
  assert.deepEqual(plain(vm.runInContext(`JSON.parse('{"ads":1,"mustExist":2}')`, p)), { mustExist: 2 });
});

test('json-prune supports wildcard paths', () => {
  const p = page('youtube.com', [rule('json-prune', ['items.*.adSlotRenderer'])]);
  const out = plain(vm.runInContext(`JSON.parse('{"items":[{"adSlotRenderer":1,"v":1},{"v":2}]}')`, p));
  assert.deepEqual(out, { items: [{ v: 1 }, { v: 2 }] });
});

test('set-constant traps a not-yet-defined chain', () => {
  const p = page('www.youtube.com', [rule('set-constant', ['ytInitialPlayerResponse.playerAds', 'undefined'])]);
  vm.runInContext(`window.ytInitialPlayerResponse = { playerAds: [1], videoDetails: { a: 1 } };`, p);
  assert.equal(vm.runInContext('ytInitialPlayerResponse.playerAds', p), undefined);
  assert.deepEqual(plain(vm.runInContext('ytInitialPlayerResponse.videoDetails', p)), { a: 1 });
});

test('set-constant rejects arbitrary values', () => {
  const p = page('youtube.com', [rule('set-constant', ['foo', 'alert(1)'])]);
  vm.runInContext('window.foo = 5;', p);
  assert.equal(vm.runInContext('foo', p), 5);
});

test('json-prune-fetch-response only touches matching URLs', async () => {
  const p = page('www.youtube.com', [
    rule('json-prune-fetch-response', ['playerAds adPlacements', '', 'propsToMatch', '/\\/player\\b/']),
  ]);
  const pruned = await vm.runInContext(`fetch('https://www.youtube.com/youtubei/v1/player?x').then(r => r.json())`, p);
  assert.equal(pruned.playerAds, undefined);
  assert.equal(pruned.adPlacements, undefined);
  assert.deepEqual(plain(pruned.videoDetails), { id: 'x' });
  const untouched = await vm.runInContext(`fetch('https://www.youtube.com/other').then(r => r.json())`, p);
  assert.deepEqual(plain(untouched.playerAds), [1]);
});

test('hostname scoping and exceptions', () => {
  const rules = [rule('json-prune', ['ads'])];
  assert.deepEqual(plain(vm.runInContext(`JSON.parse('{"ads":1}')`, page('example.com', rules))), { ads: 1 });
  assert.deepEqual(plain(vm.runInContext(`JSON.parse('{"ads":1}')`, page('notyoutube.com', rules))), { ads: 1 });
  const withException = rules.concat([rule('json-prune', ['ads'], ['m.youtube.com'], { exception: true })]);
  assert.deepEqual(plain(vm.runInContext(`JSON.parse('{"ads":1}')`, page('m.youtube.com', withException))), { ads: 1 });
  assert.deepEqual(plain(vm.runInContext(`JSON.parse('{"ads":1}')`, page('www.youtube.com', withException))), {});
  const disableAll = rules.concat([{ include: [{ Domain: 'youtube.com' }], exclude: [], name: '', args: [], exception: true }]);
  assert.deepEqual(plain(vm.runInContext(`JSON.parse('{"ads":1}')`, page('youtube.com', disableAll))), { ads: 1 });
});

test('entity hostnames', () => {
  const rules = [{ include: [{ Entity: 'google' }], exclude: [], name: 'json-prune', args: ['ads'], exception: false }];
  assert.deepEqual(plain(vm.runInContext(`JSON.parse('{"ads":1}')`, page('www.google.co.uk', rules))), {});
  assert.deepEqual(plain(vm.runInContext(`JSON.parse('{"ads":1}')`, page('google.evil.example', rules))), { ads: 1 });
});

test('abort-on-property-read throws on access', () => {
  const p = page('youtube.com', [rule('abort-on-property-read', ['adTracker'])]);
  assert.throws(() => vm.runInContext('window.adTracker', p), /adTracker/);
});

test('unknown scriptlet names are ignored, not evaluated', () => {
  const p = page('youtube.com', [rule('alert(1)//', ['x'])]);
  assert.equal(vm.runInContext('typeof JSON.parse', p), 'function');
});

test('data containing script-like strings stays data', () => {
  const evil = "</script><script>window.pwned=1</script> ');window.pwned=1;//";
  const p = page('youtube.com', [rule('json-prune', [evil])]);
  assert.equal(vm.runInContext('typeof window.pwned', p), 'undefined');
});
