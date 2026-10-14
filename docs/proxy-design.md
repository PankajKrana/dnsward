# Design proposal: filtering beyond DNS for non-browser apps

**Status: proposal. Nothing here is implemented.** It is written for review
before any code, because it touches the most sensitive trust decision in the
project (see "Existing decision" below).

## Why this exists

Brave blocks ads inside the browser: its network handler sees every request
URL *before* TLS and applies the rules per URL, resource type, and first or
third party. Cosmetic filters and scriptlets then run inside the page. AdBlock
reproduces that for browsers with the extension (`declarativeNetRequest`,
`$removeparam`, `$redirect`, cosmetic filters, scriptlets, debouncing).

For everything else on the machine, AdBlock only sees **hostnames** through
DNS. The question is how much more it can do for non-browser apps, and at what
cost.

## Existing decision

`docs/browser.md` says system-wide HTTPS filtering is deliberately not done,
because it needs a man-in-the-middle proxy and a custom root certificate,
which weakens TLS for the whole machine. This proposal does **not** assume
that decision is wrong. Its first job is to find out how much of the gap can
be closed without reversing it, and to make the cost of reversing it explicit.

## What a proxy could and could not add

| Capability | DNS today | Proxy without decryption | Proxy with TLS interception |
|---|---|---|---|
| Block a whole hostname | yes | yes | yes |
| Block by URL path or query | no | no | yes |
| Strip tracking parameters, serve stub resources | no | no | yes (apps only; see below) |
| Hide page elements, run scriptlets | browser only | no | **no**: apps have no DOM, so this stays browser-only |
| Attribute traffic to a process | no | partly (peer lookup on loopback) | partly |
| Stop apps bypassing system DNS (hardcoded DoH/DoT/IPs) | no | partly | partly |

**Honest value assessment.** Dedicated ad and tracker hostnames (the large
majority of what filter lists target) are already blocked by DNS. URL-level
filtering only adds something when ads are served from the *same hostname* as
content. In the notable cases (in-app video and audio ads) the ad and the
content are often stitched together server-side, so even full interception
cannot separate them. The extra benefit of interception for non-browser apps
is therefore expected to be small. This must be measured, not assumed (see
the decision gates).

## Options

**A. Stay with DNS + browser extension.** No new risk. Gap: apps that bypass
the system resolver, and URL-level rules in non-browser apps.

**B. Close the DNS bypass gap (no decryption).** Redirect or block plain port
53 and known DoH/DoT endpoints so apps cannot skip the resolver. Same trust
model as today. This targets the most common real escape (apps and devices
with hardcoded resolvers) and is useful on its own.

**C. Explicit forward proxy without decryption.** A local HTTP `CONNECT`
proxy that filters by the requested host (and SNI where available) and tunnels
the encrypted bytes. No certificate is installed. Gains: per-app attribution
via the loopback peer, a consistent decision point, and blocking of IP-literal
and hardcoded-host connections that DNS never saw. Limit: hostname-level only.

**D. Opt-in TLS-intercepting proxy, scoped.** Everything in C plus decryption
for an allowlisted set of hosts. This is the only option that can apply
URL-level rules to non-browser apps, and the only one that changes the trust
model. It is specified in detail below so the cost is visible.

**E. OS-level filter (macOS Network Extension, Linux eBPF/cgroup).** Per-flow
and per-process decisions without decrypting. Strong attribution, no URL
visibility. On macOS this needs a system extension, notarization and Apple
entitlements, a distribution constraint rather than a technical one.

**Recommendation: build B, then C, and stop there unless the decision gates
justify D.** B and C keep the current trust model. D should only be built for
a concrete, named list of apps where measurement shows a benefit.

## Phase 1: DNS bypass protection (option B)

* Config `[system] enforce_dns = false` (default off). When enabled, `adblock
  enable` installs a firewall anchor that:
  * redirects outbound UDP/TCP port 53 from everyone except the daemon's
    account to the daemon;
  * rejects connections to a maintained list of public DoH/DoT addresses
    (port 853 outright), so apps fall back to system DNS.
* macOS: a `pf` anchor loaded with `pfctl`, referenced from a dedicated anchor
  file, never by editing `/etc/pf.conf`. Linux: an `nftables` table owned by
  AdBlock (`inet adblock`), with a skip rule keyed on the daemon's uid to
  avoid redirect loops.
* Same fail-safe rule as DNS settings: persist the undo information first,
  verify resolution after applying, remove the rules on `stop`/`disable`, and
  remove them automatically if the daemon is not answering.
* Known limits, stated in the docs: DoH to a CDN hostname shared with real
  content cannot be blocked by address; Encrypted Client Hello hides SNI from
  anything that is not the endpoint.

## Phase 2: explicit forward proxy (option C)

New crate `adblock-proxy`, same dependency position as `adblock-dns` (depends
on `adblock-filter`, `adblock-config`, `adblock-stats`; no platform code).

```text
app ──CONNECT host:443──► adblock-proxy (127.0.0.1:8118)
                           ├─ peer uid/pid lookup (loopback socket owner)
                           ├─ Engine::check_hostname(host)  ──blocked──► 403 + close
                           └─ allowed: TCP tunnel, bytes untouched
```

* Listens on loopback only. Refuses non-loopback peers and any `CONNECT`
  target that is loopback, link-local or private (no SSRF into the local
  network or the daemon's control socket). Ports other than 443/80 are
  rejected unless configured.
* Reuses the engine unchanged: same lists, user allow/block rules, critical
  host protection, CNAME uncloaking (resolution goes through the daemon's
  resolver so the DNS filter sees every lookup).
* Clients are directed to it with a generated **PAC file** served from
  loopback and set with the system proxy settings (`networksetup
  -setautoproxyurl` on macOS). Apps that ignore system proxy settings are
  unaffected, which is the correct failure direction. PAC routes only what is
  configured (for example a list of apps' hosts), never "everything".
* Fail-safe: a dead proxy must not take the machine offline. The PAC returns
  `PROXY 127.0.0.1:8118; DIRECT`, and `enable`/`disable` use the same
  persist-verify-rollback pattern as DNS. A health probe in the daemon
  removes the proxy setting if the listener is down.
* Logs and stats follow `privacy.md`: hostnames of **blocked** requests only,
  nothing else. No URLs, no client identity beyond an in-memory counter.

## Phase 3 (only if gated in): scoped TLS interception (option D)

### Hard requirements

1. **Off by default, per-app opt-in.** No global switch. The user names
   specific hosts (or apps) to intercept; everything else is tunnelled
   untouched as in Phase 2. A typed confirmation explains what is being
   installed.
2. **Name-constrained CA.** Generate the root with an X.509 *name constraints*
   extension limited to the opted-in hosts, so a stack that honours it will
   not accept a certificate for any other name. Whether each platform's
   verifier enforces name constraints on a user-installed root **must be
   tested, not assumed**. A client that ignores them trusts the CA for
   everything, which is why requirements 3 and 4 exist.
3. **Two-tier keys.** The root key is generated on the machine, stored
   root-owned `0600` (macOS: System keychain with a non-exportable item if
   feasible), and used only by the privileged CLI to mint a short-lived
   (for example 30-day), name-constrained **intermediate**. The proxy process
   (unprivileged, its own account distinct from the DNS daemon) holds only
   the intermediate. Compromising the proxy then yields a constrained,
   expiring key, not the root.
4. **Never silently broaden trust.** Adding a host to the constraint set
   means minting a new root, because constraints cannot be changed on an
   installed root. The CLI says so and replaces the old root in the trust
   store.
5. **Uninstall removes the CA** from the system trust store and verifies it
   is gone; `doctor` reports the CA's presence, expiry and constraint set. A
   CA that is installed but whose proxy is not running is flagged as a
   problem.
6. **No retention.** Decrypted data lives in memory for the life of the
   connection. No URLs, headers or bodies are logged or stored.

### Behaviour

* Terminate the client TLS with a leaf certificate minted for the SNI host
  (cached, a few minutes of validity at most), open a fresh verified TLS
  connection upstream (validating against the platform trust store, never
  trusting the proxy's own CA upstream), and relay HTTP/1.1 and HTTP/2.
* Apply the existing URL rules: block (synthetic 403/empty response),
  `$redirect` (serve the packaged stubs), `$removeparam` (forward the cleaned
  URL). Cosmetic filters and scriptlets are **not applicable** to non-browser
  apps and are not attempted.
* **Certificate pinning and failures.** An app that pins will reject the leaf
  and abort the handshake. Treat a client-side handshake failure for a host as
  "this app pins": add the host to an automatic bypass for a period, tunnel it
  untouched from then on, and surface it in `adblock doctor`. Never retry in a
  way that downgrades security.
* **QUIC / HTTP/3.** UDP 443 bypasses a TCP proxy. Blocking it (so clients
  fall back to TCP) is part of the scoped firewall rules in Phase 1.
* **Encrypted Client Hello** defeats SNI-based leaf minting; those hosts fall
  back to untouched tunnelling.
* Apps with their own trust stores (Firefox/NSS, Java, some Python and Node
  setups) will not trust the CA unless configured separately. This limits the
  blast radius and is documented, not worked around.

### What this changes in the threat model

New assets: the CA keys. New adversaries: local malware that steals the proxy
key (bounded by the intermediate's scope and lifetime), a hostile peer on
the machine using the proxy as a relay (loopback-only, SSRF filtering,
per-uid policy), and an attacker who persuades the user to install the CA
(typed confirmation, no silent paths). `docs/security.md` and
`docs/privacy.md` need new sections before this ships, and the "Why not
system-wide HTTPS filtering?" paragraph in `docs/browser.md` must be
rewritten to describe the scoped opt-in honestly rather than deleted.

## Platform notes

| | macOS | Linux |
|---|---|---|
| Proxy settings | `networksetup` PAC/HTTP(S) proxy per network service | No universal system proxy; env vars and per-app settings, or transparent redirect |
| Firewall redirect | `pf` anchor | `nftables` table with uid-keyed skip |
| CA trust | `security add-trusted-cert` (admin prompt) | `update-ca-certificates` / p11-kit anchors; distro-specific |
| Transparent mode | possible with `pf rdr` + original-destination lookup | `REDIRECT`/`TPROXY` + `SO_ORIGINAL_DST` |
| Windows | out of scope here; same `Platform` trait extension as the DNS work |

Transparent redirection (no per-app proxy setting) is possible on both but
is **not** part of the first iteration: it removes the user's ability to see
which apps are affected and makes pinning failures look like random outages.

## Verification plan

* Unit and property tests for the PAC generator, `CONNECT` parsing (fuzzed,
  like the list parser) and the SSRF filter.
* Integration tests with a local TLS server: allowed, blocked, redirected,
  param-stripped, pinned client (handshake abort → bypass), HTTP/2.
* Name-constraint test: a verifier built on the platform stack is asked to
  validate out-of-scope names against the generated root, on each supported
  OS, as an automated check of requirement 2.
* Chaos test: kill the proxy mid-traffic; assert connectivity returns within
  a bounded time and settings are restored (same harness as the DNS
  rollback tests).
* Key-handling tests: file modes, ownership, expiry of the intermediate, root
  never readable by the proxy account.
* `cargo audit` and the existing fuzz CI cover the new crate.

## Decision gates

1. **After Phase 1:** measure how many real apps on a test machine used
   hardcoded resolvers. If few, Phase 2's attribution value is the main
   remaining reason to continue.
2. **After Phase 2:** build a list of concrete non-browser apps whose ads
   are on shared hostnames and are blockable by URL. Do the measurement with
   the Phase 2 proxy in logging-off "dry run" counters, not with interception.
   If the list is empty or the ads are server-stitched, **stop**.
3. **Before Phase 3:** a named owner for the CA key-handling design, the
   documentation rewrites above, and a platform test that confirms
   name-constraint enforcement. Without all three, do not start.

## Open questions

* Is per-app attribution (Phase 2) valuable enough on its own to justify the
  proxy, even with no decryption?
* Which non-browser apps matter most? The answer decides whether Phase 3 has
  any target at all.
* Is a macOS system extension (option E) acceptable as a distribution
  model, given the signing and entitlement requirements?
* Should Phase 1's firewall rules be part of `adblock enable`, or a separate
  explicit command? The current preference is a separate command, because
  they affect traffic beyond DNS.
