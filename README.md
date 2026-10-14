# AdBlock

System-wide ad and tracker blocking for **Linux** and **macOS**, with browser
extensions for in-page filtering. Written in Rust.

```text
$ adblock status
AdBlock
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

AdBlock works in two layers:

1. **System-wide DNS filtering.** A local resolver on `127.0.0.1:53` answers
   requests for ad and tracker domains with `0.0.0.0` and forwards everything
   else over encrypted DNS-over-HTTPS. This covers every app, not just browsers.
2. **Browser filtering.** Extensions for Chromium-based browsers and Firefox
   block ad requests on otherwise legitimate domains, hide ad elements, and
   apply YouTube compatibility rules. The browser enforces these rules itself;
   AdBlock does not intercept HTTPS.

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
| macOS 11+ | Intel, Apple Silicon | LaunchDaemon, unprivileged `_adblock` user | every network service (`networksetup`) |

| Browser | Support |
|---|---|
| Chrome, Chromium, Brave, Edge, Vivaldi (120+) | Full: network rules, cosmetic filtering, scriptlets |
| Firefox (128+) | Full |
| Safari | DNS filtering, plus exported content-blocker rules. See [browser/safari](browser/safari/README.md) |

## Installation

**Linux**
```sh
curl -fsSL https://example.com/install.sh | sh
```

**macOS**
```sh
curl -fsSL https://example.com/install-macos.sh | sh
```

**From a source checkout** (needs Rust from <https://rustup.rs>):
```sh
git clone https://github.com/OWNER/adblock && cd adblock
./installers/install.sh          # Linux
./installers/install-macos.sh    # macOS
```

**Homebrew** (once the tap is published):
```sh
brew install OWNER/tap/adblock
sudo adblock enable
```

**Packages:** `.deb` and `.rpm` files are attached to each release. An Arch
`PKGBUILD` lives in [`packaging/arch`](packaging/arch). After installing a
package, run `sudo adblock enable`.

The installers detect your OS and CPU and verify the download's SHA-256 (or
build from source). They create an unprivileged service account, install the
service, write a default configuration, enable protection, download the
filter lists, and finish with `adblock doctor`.

## Quick start

```sh
sudo adblock enable      # turn protection on (survives reboots)
adblock status           # what is running
adblock doctor           # check everything, with fixes for any problem
sudo adblock disable     # turn it off and restore your previous DNS settings
```

For in-page and YouTube ad blocking, also install the
[browser extension](#browser-integration).

## CLI

| Command | Purpose |
|---|---|
| `adblock status` | Protection status (exit code 3 if the service is not running) |
| `sudo adblock enable` / `disable` | Turn protection on/off (service at boot + system DNS) |
| `sudo adblock enforce-dns on` / `off` / `status`, `adblock enforce-dns show` | Optional: refuse public resolvers for apps that skip system DNS (see [docs/proxy-design.md](docs/proxy-design.md)) |
| `sudo adblock start` / `stop` / `restart` | Control the service. `stop` restores system DNS while stopped |
| `sudo adblock pause` / `resume` | Temporarily stop filtering without touching system settings |
| `sudo adblock update [--force]` | Download filter-list updates now |
| `adblock rules` | Filter lists, rule counts, update status |
| `adblock check <domain or URL>` | Whether something is blocked, and by which rule |
| `adblock stats [--days N]` / `sudo adblock stats reset` | Local statistics |
| `adblock logs [-f] [-n N]` | Service logs (journald on Linux, log file on macOS) |
| `adblock config [show\|path\|validate\|default]` | Configuration |
| `adblock doctor` | Diagnose problems |
| `sudo adblock allow <domain>` / `block <domain>` | Never/always block a domain and its subdomains |
| `sudo adblock remove-allow <domain>` / `remove-block <domain>` | Undo the above |
| `adblock allowlist` / `blocklist` | List your allow/block entries |
| `adblock browser install\|uninstall\|status\|export-safari` | Browser integration |
| `sudo adblock uninstall [--purge]` | Remove AdBlock and restore DNS |

Global flags: `--json` for machine-readable output, `--no-color`,
`--config FILE`, and `--user` (per-user mode, see
[development](docs/development.md)).

Changes made with `allow`/`block` apply immediately. No restart or reboot is
needed. Commands that change system-wide state need `sudo`. Read-only
commands do not.

## Configuration

| Platform | File |
|---|---|
| Linux | `/etc/adblock/config.toml` |
| macOS | `/Library/Application Support/AdBlock/config.toml` |

`adblock config default` prints the fully commented default file. Common
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

[[filters.lists]]              # add any list in adblock, hosts or domain format
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
run `adblock config validate` and then `sudo adblock restart`.

Advanced users can also change the blocking response, cache size and TTLs,
upstream timeouts, bootstrap resolvers for DoH hostnames, the DNR rule budget,
statistics retention, and data paths. Every setting is documented in the
default file.

## Browser integration

```sh
adblock browser install       # register with your browsers (no sudo needed)
```

Then load the extension:

* **Chrome / Brave / Edge / Vivaldi:** open `chrome://extensions`, turn on
  *Developer mode*, choose *Load unpacked*, and select
  `/usr/local/share/adblock/browser/chromium`. For the most reliable YouTube
  blocking, also turn on **Allow User Scripts** in the extension's details.
* **Firefox:** open `about:debugging` → *This Firefox* → *Load Temporary
  Add-on*, and pick `manifest.json` in `…/browser/firefox`. For permanent
  installation, use a signed build from addons.mozilla.org.

The extension downloads nothing on its own. It gets its rules from the local
AdBlock service through native messaging. Its popup shows status and can
pause blocking for the current site. Details:
[docs/browser.md](docs/browser.md).

## YouTube

YouTube serves ads from the same servers as videos, so DNS blocking alone
cannot stop them. The extension handles YouTube with:

* network rules for ad and ad-measurement endpoints,
* **scriptlets**, built into the extension, that strip ad placement data from
  player responses before the player reads them,
* cosmetic rules that hide ad slots in the page.

These rules live in a separate ruleset ([`filters/youtube.txt`](filters/youtube.txt))
and in the uBlock filters list. Both update without a new AdBlock binary.
AdBlock does not touch DRM, authentication, or any access control. See
[docs/youtube.md](docs/youtube.md).

## Privacy

* **No telemetry.** AdBlock contacts only your configured DNS upstreams and
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

Start with `adblock doctor`. It checks every layer and prints the fix for
each problem. Common situations, including VPNs, browser secure-DNS settings,
iCloud Private Relay, conflicts on port 53, and a site that is broken by
blocking, are covered in [docs/troubleshooting.md](docs/troubleshooting.md).

If anything goes wrong with DNS, `sudo adblock disable` restores your
previous settings exactly.

## Limitations

* **Not every ad is blocked.** First-party ads, server-side ad insertion, and
  newly changed ad formats can get through until the filter lists catch up.
* **Encrypted DNS in apps or browsers bypasses system DNS filtering.**
  Browsers' "secure DNS" options, apps with hard-coded DoH resolvers, and
  iCloud Private Relay all skip the system resolver. AdBlock signals Firefox
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
cargo run -p adblock-cli -- --user daemon   # run a per-user daemon
```

See [docs/development.md](docs/development.md).

## Architecture

```text
             adblock (single binary)
   ┌──────────────┬──────────────┬──────────────────┐
   │ CLI          │ daemon       │ native-host      │
   └──────┬───────┴──────┬───────┴────────┬─────────┘
          │ control API (Unix socket, JSON)│
   ┌──────┴──────────────┴─────────────────┴────────┐
   │ adblock-core: rules, updates, stats, control   │
   ├──────────────┬───────────────┬─────────────────┤
   │ adblock-filter│ adblock-dns  │ adblock-stats   │  platform-independent
   ├──────────────┴───────────────┴─────────────────┤
   │ adblock-platform: systemd/resolved | launchd/networksetup │
   └────────────────────────────────────────────────┘
```

See [docs/architecture.md](docs/architecture.md).

## Security

Threat model and hardening: [docs/security.md](docs/security.md). Report
vulnerabilities as described in [SECURITY.md](SECURITY.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT. See [LICENSE](LICENSE). Filter lists are downloaded at runtime and
remain under their own licenses.
