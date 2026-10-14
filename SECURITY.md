# Security policy

## Reporting a vulnerability

Please **do not open a public issue** for security problems. Use GitHub's
private vulnerability reporting ("Security" tab → "Report a vulnerability")
or email security@example.com. Include:

* affected version (`fuck-ads --version`) and platform,
* steps to reproduce or a proof of concept,
* impact as you understand it.

We aim to acknowledge reports within 3 days and to ship fixes for confirmed
high-severity issues within 14 days, crediting reporters who wish to be
named.

## Scope

In scope: the daemon, CLI, installers, browser extension, native messaging
host, and update mechanism. Out of scope: the content of third-party filter
lists (report those upstream) and issues that require existing root access.

The threat model is in [docs/security.md](docs/security.md).
