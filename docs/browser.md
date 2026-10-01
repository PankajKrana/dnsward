# Browser integration

## How it works

The extension contains no filter lists and never downloads anything itself.
It talks to the local AdBlock service through **native messaging**: the
browser starts `adblock-native-host`, which forwards a fixed set of
read-only requests to the daemon's control socket.

| Feature | Mechanism |
|---|---|
| Network blocking | `declarativeNetRequest` dynamic rules, evaluated by the browser. Up to 30,000 rules; hostname rules are packed into multi-domain rules, so the four default lists fit with room to spare. |
| Cosmetic filtering | The content script reports the page's class/id tokens; the service returns matching selectors; the background injects them as a **user-origin** stylesheet (`scripting.insertCSS`), which pages cannot remove. |
| Scriptlets (YouTube and others) | Built-in functions in `scriptlets.js`, registered with `userScripts` (MAIN world, `document_start`), or injected with `scripting.executeScript` at navigation commit when `userScripts` isn't available. |
| Per-site pause | Popup button; stored in extension storage; adds an `allowAllRequests` rule and suppresses cosmetics and scriptlets for that site. |

The extension re-syncs every 5 minutes, and immediately after rule updates
(it compares the service's rule revision).

## Install

```sh
adblock browser install            # per user, for every detected browser
sudo adblock browser install --system   # all users (the installer does this)
```

Then load the extension from `<prefix>/share/adblock/browser/`, or from
`browser/dist/` after `./browser/build.sh`:

* **Chromium family:** `chrome://extensions` → Developer mode → Load
  unpacked → `chromium/`. Open the extension's *Details* and turn on
  **Allow User Scripts** (Chrome 138+), which makes YouTube blocking
  reliable.
* **Firefox 128+:** `about:debugging#/runtime/this-firefox` → Load Temporary
  Add-on → `firefox/manifest.json`. Temporary add-ons are removed on restart;
  for permanent use, sign the build through addons.mozilla.org (it can be
  self-distributed). In `about:addons`, grant the extension access to all
  websites.

The extension ID for unpacked Chromium installs is fixed by the public key
in `browser/chromium/manifest.json` (`chlpacjmjgeglnhaedpfiffkdfakeana`).
Store-published builds get a different ID; register it with
`adblock browser install --extension-id <id>`.

## Safari

See [browser/safari/README.md](../browser/safari/README.md). Safari requires
a signed Xcode app for any extension; AdBlock exports Safari content-blocker
rules with `adblock browser export-safari <file>`.

## Why not system-wide HTTPS filtering?

Filtering inside HTTPS from outside the browser requires a man-in-the-middle
proxy and a custom root certificate. That weakens the security of every TLS
connection on the machine, so AdBlock deliberately does not do it. Browser
APIs give the same filtering power safely.
