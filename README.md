# FuckAds

[![CI](https://github.com/PankajKrana/fuck-ads/actions/workflows/ci.yml/badge.svg)](https://github.com/PankajKrana/fuck-ads/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/PankajKrana/fuck-ads)](https://github.com/PankajKrana/fuck-ads/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

System-wide ad and tracker blocking for **Linux** and **macOS**, with browser
extensions for in-page filtering. Written in Rust.

```text
$ fuck-ads status
FuckAds
──────────────────────────────
Status:          ● Active
Platform:        macOS
DNS filtering:   ● Active
Browser filter:  ● Active (Brave)
Filter rules:    145,012
Blocked today:   12,842
Ads:             8,551
Trackers:        4,291
Last update:     4 minutes ago
```

FuckAds works in two layers:

1. **System-wide DNS filtering.** A local resolver on `127.0.0.1:53` answers
   requests for ad and tracker domains with `0.0.0.0` and forwards everything
   else over encrypted DNS-over-HTTPS. This covers every app, not just browsers.
2. **Browser filtering.** Extensions for Chromium-based browsers and Firefox
   block ad requests on otherwise legitimate domains, hide ad elements, and
   apply YouTube compatibility rules. The browser enforces these rules itself;
   FuckAds does not intercept HTTPS.

Both layers use the same widely used open filter lists: EasyList,
EasyPrivacy, uBlock filters, and Peter Lowe's list.

> **Limitations.** No ad blocker catches 100% of ads. DNS filtering cannot
> block ads served from the same domain as the content (YouTube, Facebook,
> many news sites). That is what the browser extension is for. Even with the
> extension, sites that change how they serve ads can stay ahead of the
> filter lists for a while. See [Limitations](#limitations).

## Supported platforms

| Platform | Architectures | Service | DNS integration |
|---|---|---|---|
| Linux with systemd | x86_64, arm64 | systemd unit, sandboxed, unprivileged user | systemd-resolved, NetworkManager, or `/etc/resolv.conf` |
| macOS 11+ | Intel, Apple Silicon | LaunchDaemon, unprivileged `_fuck-ads` user | every network service (`networksetup`) |

| Browser | Support |
|---|---|
| Chrome, Chromium, Brave, Edge, Vivaldi (120+) | Full: network rules, cosmetic filtering, scriptlets |
| Firefox (128+) | Full |
| Safari | DNS filtering, plus exported content-blocker rules. See [browser/safari](browser/safari/README.md) |

## Installation

**Linux**
```sh
curl -fsSL https://raw.githubusercontent.com/PankajKrana/fuck-ads/main/installers/install.sh | sh
```

**macOS**
```sh
curl -fsSL https://raw.githubusercontent.com/PankajKrana/fuck-ads/main/installers/install-macos.sh | sh
```

**From a source checkout** (needs Rust from <https://rustup.rs>):
```sh
git clone https://github.com/PankajKrana/fuck-ads && cd fuck-ads
./installers/install.sh          # Linux
./installers/install-macos.sh    # macOS
```

**Homebrew:**
```sh
brew install PankajKrana/tap/fuck-ads
sudo fuck-ads enable
```

**Packages:** `.deb` and `.rpm` files are attached to each release. An Arch
`PKGBUILD` lives in [`packaging/arch`](packaging/arch). After installing a
package, run `sudo fuck-ads enable`.

The installers detect your OS and CPU and verify the download's SHA-256 (or
build from source). They create an unprivileged service account, install the
service, write a default configuration, enable protection, download the
filter lists, and finish with `fuck-ads doctor`.

## Uninstall

**Normal way** (any install method; the `fuck-ads` command must still work):

```sh
sudo fuck-ads uninstall          # asks first; add --yes to skip the question
sudo fuck-ads uninstall --purge  # also delete the configuration file
```

This restores your previous DNS settings first (and turns off the optional
`enforce-dns` firewall rules and the system proxy if you enabled them), then
stops and removes the service and its `_fuck-ads` / `fuck-ads` account,
unregisters the browser native-messaging host, and deletes the downloaded
filter lists, statistics, logs and the `fuck-ads` binary. Your configuration
file is kept unless you pass `--purge`.

**If the `fuck-ads` command is broken or missing**, run the uninstall script. It
falls back to removing everything by hand:

```sh
curl -fsSL https://raw.githubusercontent.com/PankajKrana/fuck-ads/main/installers/uninstall.sh | sh
./installers/uninstall.sh --purge     # from a source checkout
```

**Homebrew:** run the uninstall *before* removing the formula, because the
service and DNS settings live outside Homebrew's directories:

```sh
sudo fuck-ads uninstall --yes
brew uninstall fuck-ads
```

**`.deb` / `.rpm` packages:** removing the package restores DNS and stops the
service for you.

```sh
sudo apt remove fuck-ads     # or: sudo apt purge fuck-ads   (also deletes config and data)
sudo dnf remove fuck-ads
sudo pacman -R fuck-ads
```

**Browser extension:** remove it yourself in the browser's extensions page
(for an unpacked install, delete it there; the `browser/dist` folder can then
be deleted).

**Check it worked:**

```sh
which fuck-ads                          # should print nothing
networksetup -getdnsservers Wi-Fi      # macOS: "There aren't any DNS Servers set" (or your own servers)
resolvectl status | head               # Linux: no 127.0.0.1 as the DNS server
```

If DNS is still pointing at `127.0.0.1` after an uninstall, set it back by hand
(macOS: `sudo networksetup -setdnsservers Wi-Fi Empty`, repeated for each network
service; Linux: re-enable your normal resolver or edit `/etc/resolv.conf`).

To switch protection off **without** uninstalling, use `sudo fuck-ads disable`
(and `sudo fuck-ads enable` to turn it back on), or `sudo fuck-ads pause` to stop
filtering temporarily while leaving everything installed.

## Quick start

```sh
sudo fuck-ads enable      # turn protection on (survives reboots)
fuck-ads status           # what is running
fuck-ads doctor           # check everything, with fixes for any problem
sudo fuck-ads disable     # turn it off and restore your previous DNS settings
```

For in-page and YouTube ad blocking, also install the
[browser extension](#browser-integration).

## CLI

| Command | Purpose |
|---|---|
| `fuck-ads status` | Protection status (exit code 3 if the service is not running) |
| `sudo fuck-ads enable` / `disable` | Turn protection on/off (service at boot + system DNS) |
| `fuck-ads proxy status` / `show`, `sudo fuck-ads proxy on` / `off` | Optional local filtering proxy: use it as the macOS system proxy (needs `[proxy] enabled = true`); see [docs/proxy-design.md](docs/proxy-design.md) |
| `sudo fuck-ads enforce-dns on` / `off` / `status`, `fuck-ads enforce-dns show` | Optional: refuse public resolvers for apps that skip system DNS (see [docs/proxy-design.md](docs/proxy-design.md)) |
| `sudo fuck-ads start` / `stop` / `restart` | Control the service. `stop` restores system DNS while stopped |
| `sudo fuck-ads pause` / `resume` | Temporarily stop filtering without touching system settings |
| `sudo fuck-ads update [--force]` | Download filter-list updates now |
| `fuck-ads rules` | Filter lists, rule counts, update status |
| `fuck-ads check <domain or URL>` | Whether something is blocked, and by which rule |
| `fuck-ads stats [--days N]` / `sudo fuck-ads stats reset` | Local statistics |
| `fuck-ads logs [-f] [-n N]` | Service logs (journald on Linux, log file on macOS) |
| `fuck-ads config [show\|path\|validate\|default]` | Configuration |
| `fuck-ads doctor` | Diagnose problems |
| `sudo fuck-ads allow <domain>` / `block <domain>` | Never/always block a domain and its subdomains |
| `sudo fuck-ads remove-allow <domain>` / `remove-block <domain>` | Undo the above |
| `fuck-ads allowlist` / `blocklist` | List your allow/block entries |
| `fuck-ads browser install\|uninstall\|status\|export-safari` | Browser integration |
| `sudo fuck-ads uninstall [--purge]` | Remove FuckAds and restore DNS |

Global flags: `--json` for machine-readable output, `--no-color`,
`--config FILE`, and `--user` (per-user mode, see
[development](docs/development.md)).

Changes made with `allow`/`block` apply immediately. No restart or reboot is
needed. Commands that change system-wide state need `sudo`. Read-only
commands do not.

## Configuration

| Platform | File |
|---|---|
| Linux | `/etc/fuck-ads/config.toml` |
| macOS | `/Library/Application Support/FuckAds/config.toml` |

`fuck-ads config default` prints the fully commented default file. Common
settings:

```toml
[dns]
listen = ["127.0.0.1:53", "[::1]:53"]
upstreams = [
    "https://cloudflare-dns.com/dns-query",   # DNS-over-HTTPS
    "https://dns.google/dns-query",
]
blocking_mode = "null"         # null (0.0.0.0) | nxdomain | refused

[dns.forward_zones]            # local network names go to your router
"lan" = ["192.168.1.1"]

[filters]
auto_update = true
update_interval_hours = 12

[[filters.lists]]              # add any list in fuck-ads, hosts or domain format
name = "my-list"
url = "https://example.org/list.txt"
category = "trackers"
sha256 = "…"                   # optional pin for lists you host yourself

[stats]
record_blocked_domains = true  # set false to keep counts only

[custom]                       # administrator rules
allow = ["intranet.example"]
rules = ["||annoying.example^", "example.org##.promo"]
```

Unknown keys are rejected so typos are not silently ignored. After editing,
run `fuck-ads config validate` and then `sudo fuck-ads restart`.

Advanced users can also change the blocking response, cache size and TTLs,
upstream timeouts, bootstrap resolvers for DoH hostnames, the DNR rule budget,
statistics retention, and data paths. Every setting is documented in the
default file.

## Browser integration

The extension adds in-page blocking (cosmetic filters, YouTube, URL rules). It
is **not on any browser store yet**, so you load it by hand. It downloads
nothing on its own: it gets its rules from the local FuckAds service through
native messaging, so the service must be running (`sudo fuck-ads enable`).

### 1. Register your browser (once)

```sh
fuck-ads browser install     # per user, no sudo. Chrome, Chromium, Brave, Edge, Vivaldi, Firefox
fuck-ads browser status      # which browsers are registered
```

The installers register Chrome, Chromium, Edge and Firefox system-wide, but
**not Brave or Vivaldi**, so run the command above for those.

### 2. Get the extension folder

The simplest way, for any install method: download
`fuck-ads-chromium.zip` (Chrome, Brave, Edge, Vivaldi) or `fuck-ads-firefox.zip`
(Firefox) from the [latest release](https://github.com/PankajKrana/fuck-ads/releases/latest)
and unzip it somewhere permanent (browsers load an unpacked extension from that
folder, so don't delete it).

Or use the copy your install already put on disk:

| Installed with | Folders |
|---|---|
| `install.sh` / `install-macos.sh` | `/usr/local/share/fuck-ads/browser/chromium` and `…/firefox` |
| Homebrew | `$(brew --prefix)/share/fuck-ads/chromium` and `…/firefox` |
| `.deb`, `.rpm`, Arch | `/usr/share/fuck-ads/chromium` and `…/firefox` |
| Source checkout | run `sh browser/build.sh`, then `browser/dist/chromium` and `browser/dist/firefox` |

### 3. Load it

| Browser | Steps |
|---|---|
| **Chrome** | Open `chrome://extensions` → turn on **Developer mode** → **Load unpacked** → choose the `chromium` folder. In the extension's *Details*, turn on **Allow User Scripts** (Chrome 138+) for reliable YouTube blocking. |
| **Brave** | Open `brave://extensions` → **Developer mode** → **Load unpacked** → the `chromium` folder. Turn on **Allow User Scripts** if your version shows it. Remember `fuck-ads browser install` (Brave is not in the system-wide registration). |
| **Edge** | Open `edge://extensions` → **Developer mode** → **Load unpacked** → the `chromium` folder. |
| **Vivaldi** | Open `vivaldi://extensions` → **Developer mode** → **Load unpacked** → the `chromium` folder. Run `fuck-ads browser install` first. |
| **Firefox 128+** | Open `about:debugging#/runtime/this-firefox` → **Load Temporary Add-on** → pick `manifest.json` in the `firefox` folder (or the zip). Then in `about:addons` give the extension access to all websites. Temporary add-ons are removed when Firefox restarts; for a permanent install you need a build signed through addons.mozilla.org. |
| **Safari** | No loadable extension: Safari requires a signed Xcode app. Export content-blocker rules instead (`fuck-ads browser export-safari rules.json`; network and element blocking only). See [browser/safari](browser/safari/README.md). |
| **Opera and others** | Not registered by `fuck-ads browser install`, so the extension cannot reach the service. |

All Chromium-based browsers use the same extension ID
(`chlpacjmjgeglnhaedpfiffkdfakeana`, fixed by the key in the manifest), so
nothing needs configuring.

### 4. Check it works

`fuck-ads status` should show **Browser filter: Active** (it says *Not
connected* until a browser has talked to the service), and the extension's
popup shows its status and can pause blocking for the current site. More
detail: [docs/browser.md](docs/browser.md).

## YouTube

YouTube serves ads from the same servers as videos, so DNS blocking alone
cannot stop them. The extension handles YouTube with:

* network rules for ad and ad-measurement endpoints,
* **scriptlets**, built into the extension, that strip ad placement data from
  player responses before the player reads them,
* cosmetic rules that hide ad slots in the page.

These rules live in a separate ruleset ([`filters/youtube.txt`](filters/youtube.txt))
and in the uBlock filters list. Both update without a new FuckAds binary.
FuckAds does not touch DRM, authentication, or any access control. See
[docs/youtube.md](docs/youtube.md).

## Privacy

* **No telemetry.** FuckAds contacts only your configured DNS upstreams and
  filter-list servers.
* **No browsing history.** The DNS resolver never records allowed domains,
  query timestamps, or client addresses. Statistics are daily counters plus,
  optionally, counts of *blocked* domains. All of it stays on your machine.
* **No full URLs in logs.** Blocked hostnames are logged only if you enable
  `privacy.log_blocked`.
* DNS is forwarded over HTTPS by default, and client EDNS options (such as
  Client Subnet) are stripped.
* Private reverse lookups and local names (`.local`, `.lan`, `.home.arpa`)
  never leave your machine.

The full list of stored data is in [docs/privacy.md](docs/privacy.md).

## Troubleshooting

Start with `fuck-ads doctor`. It checks every layer and prints the fix for
each problem. Common situations, including VPNs, browser secure-DNS settings,
iCloud Private Relay, conflicts on port 53, and a site that is broken by
blocking, are covered in [docs/troubleshooting.md](docs/troubleshooting.md).

If anything goes wrong with DNS, `sudo fuck-ads disable` restores your
previous settings exactly.

## Limitations

* **Not every ad is blocked.** First-party ads, server-side ad insertion, and
  newly changed ad formats can get through until the filter lists catch up.
* **Encrypted DNS in apps or browsers bypasses system DNS filtering.**
  Browsers' "secure DNS" options, apps with hard-coded DoH resolvers, and
  iCloud Private Relay all skip the system resolver. FuckAds signals Firefox
  to disable its automatic DoH, and the extension still filters inside the
  browser.
* **VPNs** often install their own DNS settings while connected.
* **Safari** cannot use the cross-browser extension without a signed Xcode
  app. See [browser/safari](browser/safari/README.md).
* **Procedural cosmetic filters** (`:has-text()` and similar), `$redirect`,
  `$removeparam`, `$csp`, and HTML filtering are not implemented. Rules that
  use them are skipped rather than approximated.
* On Chrome without *Allow User Scripts*, scriptlets are injected when
  navigation commits. This usually, but not always, happens before page
  scripts run.

## Development

```sh
cargo test --workspace                 # unit + integration tests
node --test "browser/tests/*.test.js"  # extension scriptlet tests
cargo bench                            # rule lookup and DNS benchmarks
cargo run -p fuck-ads-cli -- --user daemon   # run a per-user daemon
```

See [docs/development.md](docs/development.md).

## Architecture

```text
fuck-ads (single binary): CLI | daemon | native-host
                │  control API (Unix socket, JSON)
┌────────────────┴─────────────────────────────────────────┐
│ fuck-ads-core: rules, updates, stats, control            │
├──────────────────────┬─────────────────┬─────────────────┤
│ fuck-ads-filter      │ fuck-ads-dns    │ fuck-ads-stats  │
├──────────────────────┴─────────────────┴─────────────────┤
│ fuck-ads-platform: systemd | launchd (service + DNS)     │
└──────────────────────────────────────────────────────────┘
filter, dns and stats are platform-independent.
```

See [docs/architecture.md](docs/architecture.md).

## Security

Threat model and hardening: [docs/security.md](docs/security.md). Report
vulnerabilities as described in [SECURITY.md](SECURITY.md).

## Contributing

Contributions are welcome: bug reports, fixes, docs, tests and rules.

```sh
git clone https://github.com/PankajKrana/fuck-ads && cd fuck-ads
cargo build --workspace
cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo test --workspace
```

- Read [CONTRIBUTING.md](CONTRIBUTING.md) first: ground rules (privacy first,
  no HTTPS interception, never execute list content), setup, the pull-request
  checklist and how releases are made.
- Use the [issue forms](https://github.com/PankajKrana/fuck-ads/issues/new/choose)
  for bugs and feature requests. Report security problems **privately**:
  [SECURITY.md](SECURITY.md).
- Everyone is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).
- CI runs formatting, clippy and tests on Linux (x86-64, arm64) and macOS,
  a real install on a systemd host, package builds, fuzzing, and advisory and
  license checks. See [docs/development.md](docs/development.md#cicd).

## License

MIT. See [LICENSE](LICENSE). Filter lists are downloaded at runtime and
remain under their own licenses.
