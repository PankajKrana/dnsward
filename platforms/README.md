# Platform service definitions

* `linux/adblock.service`: systemd unit (unprivileged user,
  `CAP_NET_BIND_SERVICE` only, strict sandbox, `Type=notify`).
* `macos/com.adblock.daemon.plist`: LaunchDaemon running as `_adblock`.
* `macos/newsyslog-adblock.conf`: log rotation for
  `/Library/Logs/AdBlock/adblock.log`.

The platform *code* (DNS configuration, service control) lives in
`crates/adblock-platform`. These templates are embedded into the binary, so
`sudo adblock enable` can install the service itself (used by packages and
Homebrew), and the installers copy them as well. The path
`/usr/local/bin/adblock` is replaced with the actual binary location.
