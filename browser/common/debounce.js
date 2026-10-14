/*
 * Debouncing: skips "link shim" redirectors by navigating straight to the
 * destination they carry in a query parameter (google.com/url?q=...,
 * l.facebook.com/l.php?u=...).
 *
 * Rules are fixed data shipped with the extension: nothing is downloaded and
 * no list can add a rule. A destination is used only if it parses as a plain
 * http(s) URL without embedded credentials.
 *
 * Rule fields:
 *   host        RegExp tested against the (lowercase) hostname
 *   path        exact pathname, or pathPrefix for a prefix match
 *   params      query parameters to try, in order
 *   base64      optional { prefix }: the value is base64url after `prefix`
 */
'use strict';

const ADBLOCK_DEBOUNCE_RULES = [
  { host: /^(www\.)?google\.[a-z]{2,3}(\.[a-z]{2})?$/, path: '/url', params: ['q', 'url'] },
  { host: /^(l|lm)\.facebook\.com$/, path: '/l.php', params: ['u'] },
  { host: /^l\.messenger\.com$/, path: '/l.php', params: ['u'] },
  { host: /^(www\.)?facebook\.com$/, path: '/flx/warn/', params: ['u'] },
  { host: /^l\.instagram\.com$/, path: '/', params: ['u'] },
  { host: /^out\.reddit\.com$/, pathPrefix: '/', params: ['url'] },
  { host: /^(www\.|m\.)?youtube\.com$/, path: '/redirect', params: ['q'] },
  { host: /^steamcommunity\.com$/, path: '/linkfilter/', params: ['u', 'url'] },
  { host: /^slack-redir\.net$/, path: '/link', params: ['url'] },
  { host: /^(www\.)?linkedin\.com$/, path: '/safety/go/', params: ['url'] },
  { host: /^(away\.)?vk\.com$/, path: '/away.php', params: ['to'] },
  { host: /^duckduckgo\.com$/, path: '/l/', params: ['uddg'] },
  { host: /^(www\.)?bing\.com$/, path: '/ck/a', params: ['u'], base64: { prefix: 'a1' } },
  { host: /^getpocket\.com$/, path: '/redirect', params: ['url'] },
  { host: /^(www\.)?(twitter|x)\.com$/, path: '/i/redirect', params: ['url'] },
  { host: /^[a-z0-9-]+\.safelinks\.protection\.outlook\.com$/, pathPrefix: '/', params: ['url'] },
];

const ADBLOCK_DEBOUNCE_MAX_URL = 4096;
const ADBLOCK_DEBOUNCE_MAX_HOPS = 3;

function adblockDebounceDecode(value, rule) {
  if (!rule.base64) return value;
  const { prefix } = rule.base64;
  if (!value.startsWith(prefix)) return null;
  let b64 = value.slice(prefix.length).replace(/-/g, '+').replace(/_/g, '/');
  while (b64.length % 4) b64 += '=';
  try {
    return atob(b64);
  } catch (e) {
    return null;
  }
}

/** One hop: the destination carried by `url`, or null. */
function adblockDebounceStep(url) {
  if (typeof url !== 'string' || url.length > ADBLOCK_DEBOUNCE_MAX_URL) return null;
  let u;
  try {
    u = new URL(url);
  } catch (e) {
    return null;
  }
  if (u.protocol !== 'https:' && u.protocol !== 'http:') return null;
  const host = u.hostname.toLowerCase();
  for (const rule of ADBLOCK_DEBOUNCE_RULES) {
    if (!rule.host.test(host)) continue;
    if (rule.path !== undefined ? u.pathname !== rule.path : !u.pathname.startsWith(rule.pathPrefix)) continue;
    for (const name of rule.params) {
      const raw = u.searchParams.get(name);
      if (!raw) continue;
      const value = adblockDebounceDecode(raw, rule);
      if (!value || value.length > ADBLOCK_DEBOUNCE_MAX_URL) continue;
      let target;
      try {
        target = new URL(value);
      } catch (e) {
        continue;
      }
      if (target.protocol !== 'https:' && target.protocol !== 'http:') continue;
      if (!target.hostname || target.username || target.password) continue;
      if (target.href === u.href) continue;
      return target.href;
    }
  }
  return null;
}

/**
 * The final destination of a chain of redirectors (a Google link wrapping a
 * Facebook link wrapping the page), or null if `url` is not a redirector.
 */
function adblockDebounce(url) {
  let current = url;
  let changed = false;
  for (let hop = 0; hop < ADBLOCK_DEBOUNCE_MAX_HOPS; hop++) {
    const next = adblockDebounceStep(current);
    if (!next) break;
    current = next;
    changed = true;
  }
  return changed ? current : null;
}
