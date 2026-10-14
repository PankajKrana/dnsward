// Run with: node --test "browser/tests/*.test.js"
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');

const ctx = { URL, atob };
vm.createContext(ctx);
vm.runInContext(fs.readFileSync(path.join(__dirname, '..', 'common', 'debounce.js'), 'utf8'), ctx);
const debounce = (url) => ctx.fuckAdsDebounce(url);
const enc = encodeURIComponent;
const dest = 'https://example.com/a?x=1&y=2#frag';

test('extracts the destination from common link shims', () => {
  for (const url of [
    `https://www.google.com/url?sa=t&q=${enc(dest)}&usg=x`,
    `https://www.google.co.uk/url?url=${enc(dest)}`,
    `https://l.facebook.com/l.php?u=${enc(dest)}&h=AT0`,
    `https://l.instagram.com/?u=${enc(dest)}&e=x`,
    `https://out.reddit.com/t3_abc?url=${enc(dest)}&token=1`,
    `https://www.youtube.com/redirect?event=video_description&q=${enc(dest)}`,
    `https://steamcommunity.com/linkfilter/?u=${enc(dest)}`,
    `https://duckduckgo.com/l/?uddg=${enc(dest)}&rut=1`,
    `https://eur01.safelinks.protection.outlook.com/?url=${enc(dest)}&data=x`,
    `https://x.com/i/redirect?url=${enc(dest)}`,
  ]) {
    assert.equal(debounce(url), dest, url);
  }
});

test('decodes Bing base64 redirect values', () => {
  const b64 = Buffer.from(dest).toString('base64url');
  assert.equal(debounce(`https://www.bing.com/ck/a?!&&p=x&u=a1${b64}&ntb=1`), dest);
  assert.equal(debounce(`https://www.bing.com/ck/a?u=zz${b64}`), null, 'wrong prefix');
  assert.equal(debounce('https://www.bing.com/ck/a?u=a1!!!not-base64'), null);
});

test('follows chained redirectors but not forever', () => {
  const inner = `https://l.facebook.com/l.php?u=${enc(dest)}`;
  assert.equal(debounce(`https://www.google.com/url?q=${enc(inner)}`), dest);
  let url = dest;
  for (let i = 0; i < 6; i++) url = `https://www.google.com/url?q=${enc(url)}`;
  const out = debounce(url);
  assert.ok(out && out.includes('google.com/url'), 'stops after the hop limit');
});

test('refuses unsafe or unrelated destinations', () => {
  const g = (v) => `https://www.google.com/url?q=${enc(v)}`;
  assert.equal(debounce(g('javascript:alert(1)')), null);
  assert.equal(debounce(g('data:text/html,<script>1</script>')), null);
  assert.equal(debounce(g('file:///etc/passwd')), null);
  assert.equal(debounce(g('https://user:pw@example.com/')), null);
  assert.equal(debounce(g('not a url')), null);
  assert.equal(debounce(g('/relative/path')), null);
  assert.equal(debounce(g('https://example.com/' + 'a'.repeat(5000))), null);
});

test('ignores pages that merely look similar', () => {
  assert.equal(debounce(`https://www.google.com/search?q=${enc(dest)}`), null);
  assert.equal(debounce(`https://evil-google.com/url?q=${enc(dest)}`), null);
  assert.equal(debounce(`https://google.com.evil.example/url?q=${enc(dest)}`), null);
  assert.equal(debounce(`https://l.facebook.com/other?u=${enc(dest)}`), null);
  assert.equal(debounce('https://example.com/?q=' + enc(dest)), null);
  assert.equal(debounce('ftp://www.google.com/url?q=' + enc(dest)), null);
  assert.equal(debounce('not a url'), null);
  assert.equal(debounce(undefined), null);
});
