'use strict';
const api = globalThis.browser ?? globalThis.chrome;
const $ = (id) => document.getElementById(id);
const fmt = (n) => (typeof n === 'number' ? n.toLocaleString() : '–');

function setState(kind, text) {
  $('state').className = `state ${kind}`;
  $('state-text').textContent = text;
}

async function render() {
  const [tab] = await api.tabs.query({ active: true, currentWindow: true });
  const info = await api.runtime.sendMessage({ type: 'popup-status', url: tab?.url });
  const s = info.status;
  if (!s) {
    setState('bad', 'AdBlock service not reachable');
    $('hint').hidden = false;
    $('hint').textContent =
      (info.error ? info.error + '. ' : '') + 'Make sure AdBlock is installed and run "adblock browser install".';
    return;
  }
  if (info.error) {
    setState('warn', 'Using last known rules');
    $('hint').hidden = false;
    $('hint').textContent = info.error;
  } else if (!s.filtering) {
    setState('warn', 'Filtering paused');
  } else if (info.paused) {
    setState('warn', 'Paused on this site');
  } else {
    setState('ok', 'Protection active');
  }
  $('numbers').hidden = false;
  $('blocked').textContent = fmt(s.blocked_today);
  $('rules').textContent = fmt(s.rule_count);
  $('dns').textContent = s.dns_enabled ? 'On' : 'Off';
  if (info.hasUserScriptsApi === false || info.userScripts === false) {
    $('hint').hidden = false;
    $('hint').textContent +=
      ' For the most reliable YouTube blocking, enable "Allow User Scripts" for AdBlock in the browser\'s extension settings.';
  }
  if (info.host) {
    $('site').hidden = false;
    $('host').textContent = info.host;
    $('toggle').textContent = info.paused ? 'Resume on this site' : 'Pause on this site';
    $('toggle').onclick = async () => {
      $('toggle').disabled = true;
      await api.runtime.sendMessage({ type: 'toggle-site', host: info.host });
      if (tab?.id !== undefined) api.tabs.reload(tab.id);
      window.close();
    };
  }
}

$('resync').onclick = async () => {
  $('resync').disabled = true;
  await api.runtime.sendMessage({ type: 'resync' });
  location.reload();
};

render().catch((e) => setState('bad', String(e.message || e)));
