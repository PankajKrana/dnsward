# Browser integration

## How it works

The extension contains no filter lists and never downloads anything itself.
It talks to the local FuckAds service through **native messaging**: the
browser starts `fuck-ads-native-host`, which forwards a fixed set of
read-only requests to the daemon's control socket.

| Feature | Mechanism |
|---|---|
| Network blocking | `declarativeNetRequest` dynamic rules, evaluated by the browser. Up to 30,000 rules; hostname rules are packed into multi-domain rules, so the four default lists fit with room to spare. |
| Cosmetic filtering | The content script reports the page's class/id tokens; the service returns matching selectors; the background injects them as a **user-origin** stylesheet (`scripting.insertCSS`), which pages cannot remove. Procedural filters (`:has-text()`, `:matches-css()`, `:xpath()`, `:upward()`, `:remove()`) arrive with the first reply and are evaluated by `procedural.js` in the page. |
| Scriptlets (YouTube and others) | Built-in functions in `scriptlets.js`, registered with `userScripts` (MAIN world, `document_start`), or injected with `scripting.executeScript` at navigation commit when `userScripts` isn't available. |
| URL cleaning | `$removeparam` filters become `declarativeNetRequest` `queryTransform` rules; `$redirect` filters serve the stand-ins in `resources/` (declared in `web_accessible_resources`, so a page can detect that the extension is installed). |
| Debouncing | `debounce.js` holds fixed rules for link shims (Google, Facebook, Instagram, Reddit, YouTube, Bing, DuckDuckGo, Outlook Safe Links and others). On a top-level navigation to one, `webNavigation.onBeforeNavigate` sends the tab straight to the destination from the query parameter. Only http(s) destinations without embedded credentials are accepted, chains are followed at most three hops, and the rules cannot be extended by a filter list. Pausing the redirector's site turns it off. Chromium MV3 cannot cancel the first request, so the shim's server may still see that one click; the benefit is skipping the redirect hop and its tracking page. |
| Per-site pause | Popup button; stored in extension storage; adds an `allowAllRequests` rule and suppresses cosmetics and scriptlets for that site. |

The extension re-syncs every 5 minutes, and immediately after rule updates
(it compares the service's rule revision).

## Install

```sh
fuck-ads browser install                 # per user, for every detected browser
sudo fuck-ads browser install --system   # all users (the installers do this)
```

The system-wide registration covers Chrome, Chromium, Edge and Firefox. Brave
and Vivaldi are only registered by the per-user command.

Get the extension folder from the release (`fuck-ads-chromium.zip`,
`fuck-ads-firefox.zip`), from `<prefix>/share/fuck-ads/` (the script installers
use `browser/chromium`; Homebrew, `.deb`, `.rpm` and Arch use `chromium` and
`firefox` directly), or from `browser/dist/` after `./browser/build.sh`. Then:

* **Chromium family (Chrome, Brave, Edge, Vivaldi):** `chrome://extensions`
  (`brave://`, `edge://`, `vivaldi://`) → Developer mode → Load unpacked →
  `chromium/`. In the extension's *Details* turn on **Allow User Scripts**
  (Chrome 138+), which makes YouTube blocking reliable.
* **Firefox 128+:** `about:debugging#/runtime/this-firefox` → Load Temporary
  Add-on → `firefox/manifest.json`. Temporary add-ons are removed on restart;
  for permanent use, sign the build through addons.mozilla.org (it can be
  self-distributed). In `about:addons`, grant the extension access to all
  websites.

The extension ID for unpacked Chromium installs is fixed by the public key
in `browser/chromium/manifest.json` (`chlpacjmjgeglnhaedpfiffkdfakeana`).
Store-published builds get a different ID; register it with
`fuck-ads browser install --extension-id <id>`.

## Safari

See [browser/safari/README.md](../browser/safari/README.md). Safari requires
a signed Xcode app for any extension; FuckAds exports Safari content-blocker
rules with `fuck-ads browser export-safari <file>`.

## Why not system-wide HTTPS filtering?

Filtering inside HTTPS from outside the browser requires a man-in-the-middle
proxy and a custom root certificate. That weakens the security of every TLS
connection on the machine, so FuckAds deliberately does not do it. Browser
APIs give the same filtering power safely.

A proposal for what could be done for non-browser apps, and under what
conditions, is in [proxy-design.md](proxy-design.md). It is not implemented.
