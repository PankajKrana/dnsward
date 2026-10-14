# Troubleshooting

Always start with:

```sh
fuck-ads doctor
```

It checks each layer and prints a specific fix for every failure.

**Emergency:** `sudo fuck-ads disable` restores the DNS settings you had
before `enable`, exactly. If the binary is missing, run
`installers/uninstall.sh`; it also restores DNS.

## "DNS configuration ✗: 127.0.0.1:53 is running, but the system is not using it"

Run `sudo fuck-ads enable`. If the check still fails:

* **VPN clients** (WireGuard, OpenVPN, corporate VPNs) often push their own
  DNS settings while connected. Disconnect to confirm. On Linux with
  systemd-resolved, FuckAds's global `Domains=~.` usually takes precedence;
  VPNs that set `~.` on their own interface still win.
* **macOS: a new network interface** (for example a new USB Ethernet
  adapter) added after `enable` is not configured yet. Run
  `sudo fuck-ads enable` again.
* **Another local DNS service** (dnsmasq, unbound, Pi-hole, a container
  DNS) may own the configuration.

## The DNS listener does not start: "address in use" on port 53

Another DNS server is listening. Find it with:

```sh
sudo lsof -nP -i :53          # macOS / Linux
sudo ss -lunp 'sport = :53'   # Linux
```

On Linux, systemd-resolved listens on `127.0.0.53`/`127.0.0.54`, not
`127.0.0.1`, so it doesn't conflict. If dnsmasq or similar holds
`127.0.0.1:53`, stop it. Alternatively, on systems with systemd-resolved,
use another port:

```toml
[dns]
listen = ["127.0.0.1:5353"]
```

(resolved supports ports; plain `/etc/resolv.conf` and macOS do not.)

## Ads still appear in the browser

* DNS filtering cannot block ads served from the site's own domain. Install
  the browser extension.
* **Browser "secure DNS"** (Chrome: *Settings → Privacy → Use secure DNS*;
  Brave and Edge have the same setting) bypasses system DNS. Either set it
  to *off* or *use your current service provider*, or rely on the
  extension.
* **iCloud Private Relay** (Safari) bypasses system DNS. FuckAds does not
  silently disable it; turn it off per network if you want DNS filtering in
  Safari.
* Check whether a specific URL is covered:
  `fuck-ads check 'https://ads.example.com/ad.js' --source https://site.example/ --type script`.

## A website is broken

1. Click the extension icon → **Pause on this site** (browser only), or
2. `sudo fuck-ads allow site.example` (system-wide, applies immediately),
3. `fuck-ads check <domain>` shows which list and rule caused a block.

Undo with `sudo fuck-ads remove-allow site.example`.

## YouTube ads still play

See [youtube.md](youtube.md). In short: make sure the extension is
connected (`fuck-ads browser status`), turn on **Allow User Scripts** for the
extension in Chrome, and run `sudo fuck-ads update`.

## Filter updates fail

`fuck-ads rules` shows each list's last error. Typical causes are no network
access to the list server, a captive portal, or a list that changed format.
The previous working rules stay active, and retries back off from 1 minute
to 1 hour. Force a retry with `sudo fuck-ads update`; accept a deliberate
large shrink with `--force`.

## Local network names (printer.lan, NAS) stopped resolving

FuckAds does not forward local names to public resolvers (that would leak
them). Point local zones at your router:

```toml
[dns.forward_zones]
"lan" = ["192.168.1.1"]
"home.arpa" = ["192.168.1.1"]
"168.192.in-addr.arpa" = ["192.168.1.1"]
```

`.local` (mDNS) is unaffected; macOS and Linux resolve it separately.

## Logs

```sh
fuck-ads logs -f          # journald on Linux; /Library/Logs/FuckAds/fuck-ads.log on macOS
```

For verbose logging, set `general.log_level = "debug"` and run
`sudo fuck-ads restart`. Add `privacy.log_blocked = true` to see each blocked
hostname.

## Browser extension says "FuckAds service not reachable"

* `fuck-ads status`: is the service running?
* `fuck-ads browser status`: is the native host registered for this browser?
  If not, run `fuck-ads browser install`.
* If you use a store build of the extension with a different ID, add the ID
  to the registration: `fuck-ads browser install --extension-id <id>`.
