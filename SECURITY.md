# Security policy

## Reporting a vulnerability

Please **do not open a public issue** for security problems. Use GitHub's
private vulnerability reporting: open the repository's **Security** tab and
choose **Report a vulnerability**
([direct link](https://github.com/PankajKrana/fuck-ads/security/advisories/new)).
Include:

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

## Supported versions

Only the latest release receives security fixes. Please update before
reporting.

## Verifying a release

Release artifacts carry a signed build-provenance attestation produced by the
release workflow, and each archive has a `.sha256` file (plus `SHA256SUMS`).
To check a download against the attestation:

```sh
gh attestation verify fuck-ads-x86_64-unknown-linux-gnu.tar.gz --repo PankajKrana/fuck-ads
```

The installers verify the archive checksum before installing anything.
