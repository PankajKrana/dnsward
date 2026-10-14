/*
 * FuckAds extension background script (Chromium MV3 service worker and
 * Firefox MV3 background script).
 *
 * - Connects to the FuckAds service through native messaging
 *   (com.fuckads.native). The service owns the filter lists; the extension
 *   ships none and never downloads anything itself.
 * - Installs network rules as declarativeNetRequest dynamic rules, evaluated
 *   by the browser itself: no request data leaves the browser.
 * - Injects cosmetic CSS (as user-origin stylesheets) and scriptlets.
 * - Debounces link-shim redirectors (google.com/url?q=...) by navigating
 *   straight to the destination, using fixed rules from debounce.js.
 * - Never sends browsing history anywhere: only the hostname of the current
 *   frame and class/id tokens are sent to the local service, to select
 *   cosmetic filters.
 */
'use strict';

if (typeof importScripts === 'function') {
  importScripts('scriptlets.js', 'debounce.js'); // Chromium service worker
}

const api = globalThis.browser ?? globalThis.chrome;
const HOST = 'com.fuckads.native';
const IS_FIREFOX = typeof globalThis.browser !== 'undefined' && navigator.userAgent.includes('Firefox');
const BROWSER_NAME = IS_FIREFOX ? 'Firefox' : navigator.userAgentData?.brands?.find((b) => !/Not|Chromium/.test(b.brand))?.brand ?? 'Chromium';

/** DNR rule IDs at or above this value are per-site pauses set from the popup. */
const SITE_RULE_BASE = 9_000_000;
const SYNC_ALARM = 'fuck-ads-sync';
const SCRIPTLET_SCRIPT_ID = 'fuck-ads-scriptlets';

// ---------------------------------------------------------------------------
// Native messaging
// ---------------------------------------------------------------------------

let port = null;
let nextId = 1;
const pending = new Map();
let lastError = null;

function connect() {
  port = api.runtime.connectNative(HOST);
  port.onMessage.addListener((msg) => {
    const entry = pending.get(msg.id);
    if (!entry) return;
    pending.delete(msg.id);
    clearTimeout(entry.timer);
    const resp = msg.response;
    if (resp && resp.type === 'error') entry.reject(new Error(resp.data.message));
    else entry.resolve(resp);
  });
  port.onDisconnect.addListener(() => {
    lastError = api.runtime.lastError?.message || 'native host disconnected';
    for (const entry of pending.values()) {
      clearTimeout(entry.timer);
      entry.reject(new Error(lastError));
    }
    pending.clear();
    port = null;
  });
}

function call(request, timeoutMs = 30_000) {
  return new Promise((resolve, reject) => {
    try {
      if (!port) connect();
    } catch (e) {
      reject(e);
      return;
    }
    const id = nextId++;
    const timer = setTimeout(() => {
      if (pending.delete(id)) reject(new Error('timed out waiting for the FuckAds service'));
    }, timeoutMs);
    pending.set(id, { resolve, reject, timer });
    port.postMessage({ id, request });
  });
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

async function getState() {
  const s = await api.storage.local.get(['revision', 'pausedSites', 'lastStatus', 'lastSync', 'lastSyncError']);
  return {
    revision: s.revision ?? 0,
    pausedSites: s.pausedSites ?? [],
    lastStatus: s.lastStatus ?? null,
    lastSync: s.lastSync ?? 0,
    lastSyncError: s.lastSyncError ?? null,
  };
}

function hostnameOf(url) {
  try {
    return new URL(url).hostname;
  } catch (e) {
    return '';
  }
}

function isPaused(host, pausedSites) {
  return pausedSites.some((d) => host === d || host.endsWith('.' + d));
}

// ---------------------------------------------------------------------------
// Network rules (declarativeNetRequest)
// ---------------------------------------------------------------------------

async function fetchAllDnrRules() {
  const rules = [];
  let offset = 0;
  let total = Infinity;
  while (offset < total) {
    const resp = await call({ cmd: 'dnr_rules', offset, limit: 5000 }, 60_000);
    total = resp.data.total;
    if (resp.data.rules.length === 0) break;
    rules.push(...resp.data.rules);
    offset += resp.data.rules.length;
  }
  return rules;
}

/** Adds rules, isolating any the browser rejects instead of failing all. */
async function addRulesResilient(rules) {
  const dnr = api.declarativeNetRequest;
  try {
    await dnr.updateDynamicRules({ addRules: rules });
    return 0;
  } catch (e) {
    if (rules.length === 1) return 1;
  }
  const mid = Math.ceil(rules.length / 2);
  return (await addRulesResilient(rules.slice(0, mid))) + (await addRulesResilient(rules.slice(mid)));
}

async function installNetworkRules(rules, pausedSites) {
  const dnr = api.declarativeNetRequest;
  const existing = await dnr.getDynamicRules();
  await dnr.updateDynamicRules({ removeRuleIds: existing.map((r) => r.id) });
  const limit = dnr.MAX_NUMBER_OF_DYNAMIC_RULES ?? dnr.MAX_NUMBER_OF_DYNAMIC_AND_SESSION_RULES ?? 30000;
  const siteRules = pausedSites.map((site, i) => ({
    id: SITE_RULE_BASE + i,
    priority: 100,
    action: { type: 'allowAllRequests' },
    condition: { requestDomains: [site], resourceTypes: ['main_frame', 'sub_frame'] },
  }));
  const budget = Math.max(0, limit - siteRules.length);
  const rejected = await addRulesResilient([...siteRules, ...rules.slice(0, budget)]);
  if (rejected > 0) console.warn(`FuckAds: browser rejected ${rejected} rules`);
}

// ---------------------------------------------------------------------------
// Scriptlets
// ---------------------------------------------------------------------------

let scriptletBundle = { rules: [] };
let userScriptsActive = false;

function scriptletCode(bundle) {
  // Data is embedded as a JSON literal; only built-in functions run.
  return `(${fuckAdsScriptletRuntime.toString()})(${JSON.stringify(bundle)});`;
}

async function registerScriptlets(bundle, pausedSites) {
  scriptletBundle = {
    rules: bundle.rules.concat(
      pausedSites.length
        ? [{ include: pausedSites.map((d) => ({ Domain: d })), exclude: [], name: '', args: [], exception: true }]
        : [],
    ),
  };
  userScriptsActive = false;
  const us = api.userScripts;
  if (!us) return;
  try {
    await us.unregister({ ids: [SCRIPTLET_SCRIPT_ID] }).catch(() => {});
    if (scriptletBundle.rules.length === 0) return;
    await us.register([
      {
        id: SCRIPTLET_SCRIPT_ID,
        matches: ['<all_urls>'],
        js: [{ code: scriptletCode(scriptletBundle) }],
        runAt: 'document_start',
        world: 'MAIN',
        allFrames: true,
      },
    ]);
    userScriptsActive = true;
  } catch (e) {
    // "Allow User Scripts" is off (Chrome 138+) or unsupported: fall back to
    // injection at navigation commit, which is usually but not always early
    // enough.
    userScriptsActive = false;
  }
}

/** Fallback injection when the userScripts API is unavailable. */
api.webNavigation.onCommitted.addListener((details) => {
  if (userScriptsActive || scriptletBundle.rules.length === 0) return;
  if (!/^https?:/.test(details.url)) return;
  api.scripting
    .executeScript({
      target: { tabId: details.tabId, frameIds: [details.frameId] },
      world: 'MAIN',
      injectImmediately: true,
      func: fuckAdsScriptletRuntime,
      args: [scriptletBundle],
    })
    .catch(() => {});
});

// ---------------------------------------------------------------------------
// Debouncing
// ---------------------------------------------------------------------------

const DEBOUNCE_WINDOW_MS = 2000;
const DEBOUNCE_MAX_PER_WINDOW = 3;
const debounceRecent = new Map(); // tabId -> timestamps of recent redirects

/**
 * Replaces a top-level navigation to a redirector with one to its
 * destination. The browser may already have sent the request to the
 * redirector; extensions cannot stop that in Chromium (MV3), so this saves
 * the page load and the tracking redirect, not the first request.
 */
api.webNavigation.onBeforeNavigate.addListener((details) => {
  if (details.frameId !== 0) return;
  const target = fuckAdsDebounce(details.url);
  if (!target) return;
  // Guard against ping-pong with a site that redirects back.
  const now = Date.now();
  const recent = (debounceRecent.get(details.tabId) ?? []).filter((t) => now - t < DEBOUNCE_WINDOW_MS);
  if (recent.length >= DEBOUNCE_MAX_PER_WINDOW) return;
  recent.push(now);
  debounceRecent.set(details.tabId, recent);
  getState()
    .then(({ pausedSites }) => {
      if (isPaused(hostnameOf(details.url), pausedSites)) return;
      return api.tabs.update(details.tabId, { url: target });
    })
    .catch(() => {});
});
api.tabs.onRemoved.addListener((tabId) => debounceRecent.delete(tabId));

// ---------------------------------------------------------------------------
// Sync with the service
// ---------------------------------------------------------------------------

let syncing = null;

async function sync(force = false) {
  if (syncing) return syncing;
  syncing = (async () => {
    const state = await getState();
    try {
      const hello = await call({ cmd: 'browser_hello', browser: BROWSER_NAME, version: api.runtime.getManifest().version });
      const status = hello.data;
      await api.storage.local.set({ lastStatus: status, lastSyncError: null });
      if (force || status.revision !== state.revision) {
        const [rules, scriptlets] = await Promise.all([fetchAllDnrRules(), call({ cmd: 'scriptlets' }, 60_000)]);
        await installNetworkRules(rules, state.pausedSites);
        await registerScriptlets(scriptlets.data, state.pausedSites);
        cosmeticCache.clear();
        await api.storage.local.set({ revision: status.revision, lastSync: Date.now() });
      } else if (scriptletBundle.rules.length === 0) {
        // Service worker restarted: in-memory scriptlet data was lost.
        const scriptlets = await call({ cmd: 'scriptlets' }, 60_000);
        await registerScriptlets(scriptlets.data, state.pausedSites);
      }
    } catch (e) {
      // Keep existing dynamic rules: they persist and continue to work.
      await api.storage.local.set({ lastSyncError: String(e.message || e) });
    } finally {
      syncing = null;
    }
  })();
  return syncing;
}

// ---------------------------------------------------------------------------
// Cosmetic filtering
// ---------------------------------------------------------------------------

const cosmeticCache = new Map(); // hostname -> selectors (specific + complex)
const CACHE_MAX = 200;
const MAX_CSS_SELECTORS = 4000;

function cssFor(selectors) {
  // One rule per selector: an invalid selector only drops its own rule.
  return selectors.slice(0, MAX_CSS_SELECTORS).map((s) => `${s}{display:none!important}`).join('\n');
}

async function cosmetic(sender, msg) {
  const tabId = sender.tab?.id;
  if (tabId === undefined) return;
  const host = hostnameOf(sender.url || '');
  if (!host) return;
  const { pausedSites } = await getState();
  if (isPaused(host, pausedSites)) return;

  const classes = Array.isArray(msg.classes) ? msg.classes.slice(0, 5000) : [];
  const ids = Array.isArray(msg.ids) ? msg.ids.slice(0, 5000) : [];
  let selectors;
  if (msg.complex && cosmeticCache.has(host) && classes.length === 0 && ids.length === 0) {
    selectors = cosmeticCache.get(host);
  } else {
    const resp = await call({ cmd: 'cosmetic', hostname: host, classes, ids, complex: !!msg.complex });
    selectors = resp.data.selectors;
    if (msg.complex && classes.length === 0 && ids.length === 0) {
      if (cosmeticCache.size >= CACHE_MAX) cosmeticCache.delete(cosmeticCache.keys().next().value);
      cosmeticCache.set(host, selectors);
    }
  }
  if (!selectors || selectors.length === 0) return;
  await api.scripting.insertCSS({
    target: { tabId, frameIds: [sender.frameId ?? 0] },
    css: cssFor(selectors),
    origin: 'USER',
  });
}

// ---------------------------------------------------------------------------
// Messages from content scripts and the popup
// ---------------------------------------------------------------------------

api.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (!msg || typeof msg.type !== 'string') return false;
  if (msg.type === 'cosmetic' && sender.tab) {
    cosmetic(sender, msg).catch(() => {});
    return false;
  }
  if (sender.tab) return false; // everything below is popup-only
  if (msg.type === 'popup-status') {
    (async () => {
      await sync();
      const state = await getState();
      const host = msg.url ? hostnameOf(msg.url) : '';
      sendResponse({
        status: state.lastStatus,
        error: state.lastSyncError,
        host,
        paused: host ? isPaused(host, state.pausedSites) : false,
        userScripts: userScriptsActive,
        hasUserScriptsApi: !!api.userScripts,
      });
    })();
    return true;
  }
  if (msg.type === 'toggle-site' && typeof msg.host === 'string' && msg.host) {
    (async () => {
      const state = await getState();
      const host = msg.host.replace(/^www\./, '');
      const pausedSites = isPaused(msg.host, state.pausedSites)
        ? state.pausedSites.filter((d) => !(msg.host === d || msg.host.endsWith('.' + d)))
        : [...state.pausedSites, host];
      await api.storage.local.set({ pausedSites });
      await sync(true);
      sendResponse({ paused: isPaused(msg.host, pausedSites) });
    })();
    return true;
  }
  if (msg.type === 'resync') {
    sync(true).then(() => sendResponse({ ok: true }));
    return true;
  }
  return false;
});

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

api.alarms.create(SYNC_ALARM, { periodInMinutes: 5 });
api.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === SYNC_ALARM) sync();
});
api.runtime.onInstalled.addListener(() => sync(true));
api.runtime.onStartup.addListener(() => sync());
if (api.declarativeNetRequest.setExtensionActionOptions) {
  api.declarativeNetRequest.setExtensionActionOptions({ displayActionCountAsBadgeText: true }).catch?.(() => {});
}
sync();
