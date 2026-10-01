# Security and threat model

AdBlock runs a network service, changes system DNS settings, and processes
data downloaded from the Internet. It is treated as security-sensitive
software.

## Assets

1. **Name resolution for the whole system.** If it breaks, the machine is
   effectively offline. If it is subverted, traffic can be redirected.
2. **Privacy of browsing activity.** DNS queries reveal which sites a user
   visits.
3. **Integrity of the host.** AdBlock has root during setup and runs a
   long-lived daemon.
4. **Integrity of web pages.** The extension injects CSS and scriptlets into
   pages.

## Trust boundaries and adversaries

| Boundary | Adversary | Mitigations |
|---|---|---|
| Filter-list servers → updater | Compromised or malicious list, man-in-the-middle, truncated download | HTTPS only (redirects too), streaming size limit, NUL/UTF-8 validation, optional SHA-256 pin, minimum rule count, shrink guard (>50% drop rejected), staged install, full engine build and self-test before activation, automatic rollback |
| Filter-list content → engine | Hostile rules aimed at the parser or at breaking the web | Fuzzed parser (cargo-fuzz targets plus 50k-input randomized test on stable), line length limit, linear-time regex engine with a compiled-size cap, rules matching every URL rejected, unknown options make the whole rule invalid, critical hosts never blocked |
| Filter-list content → browser pages | Rules trying to run code or inject CSS | **No filter-list text is ever executed.** Scriptlets are fixed functions shipped with the extension; lists supply only a name from an allowlist plus string arguments, embedded as a JSON literal (tested against `</script>`, U+2028 and quote-breaking payloads). AdGuard `#%#` (arbitrary JS), `#$#` (CSS injection), and HTML filters are refused. Selectors with braces, semicolons, comments, at-rules, or unbalanced brackets are rejected, and each selector becomes its own CSS rule. |
| Network → DNS listener | Off-path spoofing, packet floods, malformed packets, use as an open resolver | Listens on loopback only by default (non-loopback needs `allow_non_loopback`); hickory-proto parsing; random query IDs and source ports; upstream answers must match ID and question; concurrency caps (UDP semaphore, 256 TCP connections, 10 s TCP idle timeout); malformed input gets FORMERR or is dropped |
| Upstream resolver | Surveillance, tampering | DNS-over-HTTPS by default with certificate validation against the platform trust store; client EDNS options (including Client Subnet) stripped; local names and private reverse lookups never forwarded |
| Local users → control socket | Unprivileged users changing settings or reading data | Kernel-verified peer credentials (`SO_PEERCRED` / `getpeereid`); state-changing requests only from root or the service account; 256 KiB request limit; 64 concurrent connections; bounded per-request work |
| Browser → native host | A malicious web page or extension | Only allowlisted extension IDs may start the host (enforced by the browser via the manifest); the host forwards only read-only requests; 1 MiB message caps |
| Daemon → root CLI | A compromised daemon trying to escalate | The daemon runs unprivileged. Root-owned DNS backups (0600) live outside the daemon's writable directory. Root writes into system DNS settings only addresses from the root-owned configuration that are loopback addresses. |
| Installer → system | Supply chain, path tricks | Release archives verified against SHA-256 before install; no shell evaluation of downloaded data; service files rendered with a path validator; uninstall deletes only hard-coded locations, never configurable paths |

## Privilege model

* **Daemon.** Unprivileged account. On Linux it gets exactly
  `CAP_NET_BIND_SERVICE` plus a strict systemd sandbox: `ProtectSystem=strict`,
  `NoNewPrivileges`, `MemoryDenyWriteExecute`, syscall filter, address-family
  restrictions, `ProtectHome`, `PrivateDevices`, and memory/task limits. On
  macOS it runs as `_adblock` (port 53 needs no privileges on macOS 10.14+).
* **Privileged operations** (`enable`, `disable`, `start`, `stop`,
  `uninstall`) run only in the CLI under `sudo`. They call system tools with
  absolute paths and argument vectors, never through a shell.
* **Fail-safe DNS changes.** The backup is persisted *before* any change.
  After the change, resolution is verified, and the previous settings are
  restored automatically if names stop resolving. `stop` restores DNS so the
  system never points at a resolver that isn't running.

## File handling

* Every state file is written atomically (temp file with `O_EXCL`, fsync,
  rename, directory fsync) with explicit permissions.
* List names are restricted to `[a-z0-9_-]{1,64}` because they become file
  names. Configured paths must be absolute and free of `..`.
* The control socket is created only after verifying any existing file at
  that path is a stale socket.

## Logging and privacy

Logs never contain full URLs or allowed hostnames. Blocked hostnames are
logged only with `privacy.log_blocked = true`. Error messages from HTTP
clients have URLs stripped. See [privacy.md](privacy.md).

## Accepted risks and known limitations

* **Read-only status is visible to all local users,** including "top blocked
  domains" in `adblock stats`. On shared machines, set
  `stats.record_blocked_domains = false`.
* **Filter lists are trusted to decide what to block.** A malicious list can
  break sites (blocking is limited to filtering; it can never execute code or
  redirect traffic). Lists are chosen by the administrator; the defaults are
  widely reviewed community lists.
* **Bootstrap lookups of DoH hostnames use plain DNS.** This reveals only
  that you use a given resolver.
* **Scriptlet arguments can contain regular expressions** that run in the
  page (as in uBlock Origin). A hostile list could make a page slow, but not
  read data or run code.
* **The Firefox DoH canary** (`use-application-dns.net` → NXDOMAIN) is
  answered so Firefox keeps using system DNS. This is a documented,
  user-visible signal, not a hidden interception.

## Verification

* `cargo test --workspace`: unit and integration tests, including malformed
  packets, corrupt or truncated updates, rollback, concurrent queries, and
  privilege checks.
* `cd fuzz && cargo +nightly fuzz run parse_line` / `engine`: coverage-guided
  fuzzing (also run in CI).
* `node --test "browser/tests/*.test.js"`: scriptlet injection safety.
* CI runs `cargo audit` against the RustSec advisory database.
