# Development

## Prerequisites

* Rust stable (`rustup`); minimum supported version **1.88** (edition 2024 and
  `let` chains), checked in CI
* A C compiler, for the TLS stack (`aws-lc-rs`): Xcode Command Line Tools,
  or `gcc`/`clang` on Linux
* Node 22, for the extension tests (optional); `shellcheck` for shell scripts (optional)

## Build and test

```sh
cargo build --workspace
cargo test --workspace            # unit + integration tests (tests/)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
node --test "browser/tests/*.test.js"
```

The integration tests in `tests/` start a real daemon in-process on random
loopback ports, with a mock upstream DNS server and a mock list fetcher
(`tests/support/lib.rs`). They need no root and no network access.

## Running a development daemon (no root)

```sh
mkdir -p ~/.config/fuck-ads
cat > ~/.config/fuck-ads/config.toml <<'TOML'
[dns]
listen = ["127.0.0.1:5354"]
[paths]
bundled_filters = "/path/to/checkout/filters"
TOML
cargo run -p fuck-ads-cli -- --user daemon
```

In another terminal:

```sh
dig @127.0.0.1 -p 5354 doubleclick.net     # → 0.0.0.0
cargo run -p fuck-ads-cli -- status          # finds the per-user daemon automatically
cargo run -p fuck-ads-cli -- check https://www.youtube.com/api/stats/ads
```

The per-user daemon keeps its data in `~/.local/share/fuck-ads` (Linux) or
`~/Library/Application Support/FuckAds` (macOS). On macOS, Unix socket paths
are limited to 103 bytes; set `[paths] socket` if your home path is long.

## Benchmarks

```sh
cargo bench --bench filter_lookup
cargo bench --bench dns_processing
FUCK_ADS_BENCH_LIST=~/.local/share/fuck-ads/lists/easylist.txt cargo bench --bench filter_lookup
```

Reference results (Apple M-series, release build):

| Benchmark | Time |
|---|---|
| Parse and index 250k rules | ~127 ms |
| Hostname lookup (blocked / deep allowed) | 44 ns / 122 ns |
| URL check (pattern / long allowed URL) | 0.33 µs / 0.33 µs |
| Cosmetic lookup, 200 page tokens | 2.2 µs |
| DNS query, blocked / cache hit (in-process) | 1.06 µs / 1.24 µs |

A live daemon with about 145k rules from the four default lists uses about
78 MB RSS and 0% CPU when idle.

## Fuzzing

```sh
cargo install cargo-fuzz
cd fuzz
cargo +nightly fuzz run parse_line
cargo +nightly fuzz run engine
```

## Browser extension

```sh
./browser/build.sh     # → browser/dist/{chromium,firefox}
```

Load `browser/dist/chromium` unpacked, then run `fuck-ads browser install` so
the browser can reach the native host. In development, the host is created
as a symlink next to `target/debug/fuck-ads`.

## CI/CD

All workflows live in `.github/workflows/`. Actions are pinned to commit SHAs
(the version is in the comment beside each one); Dependabot proposes updates
weekly for Cargo, `fuzz/` and the actions themselves.

| Workflow | When | What it does |
|---|---|---|
| `ci.yml` | every push to `main`, every pull request, weekly | formatting and docs; clippy + tests on Linux x86-64/arm64 and macOS 14/15; build on the minimum Rust (1.88); browser tests; shell scripts and shellcheck; `.deb`/`.rpm` builds; a real install, enable, root-drop and uninstall on a systemd host; fuzzing (2 min per target, 10 on the weekly run); RustSec advisories; license/ban/source policy (`deny.toml`); dependency review on pull requests. **`CI passed`** summarises all of it. |
| `codeql.yml` | `main`, pull requests, weekly | CodeQL analysis for Rust and JavaScript |
| `release.yml` | a `v*` tag (or run by hand as a dry run) | checks the tag against the code version, builds and tests four targets, builds packages and the extension zips, signs build-provenance attestations, publishes the release with generated notes, then bumps the Homebrew tap |

Pull requests from the same branch cancel their previous run; runs on `main`
always finish. Every job has a timeout and read-only permissions unless it
needs more (the release job needs to write the release and the attestation).

### Recommended repository settings

These are settings in GitHub, not files, so they have to be switched on once:

- **Branch protection on `main`:** require a pull request, require the
  **`CI passed`** check, require review from Code Owners, block force pushes.
- **Security:** enable secret scanning with push protection, Dependabot
  security updates, and private vulnerability reporting.
- **Release secret:** add a repository secret `TAP_GITHUB_TOKEN` (a
  fine-grained token with *Contents: write* on the `homebrew-tap` repository)
  to let the release workflow update the Homebrew formula. Without it that job
  only prints a notice.

## Releasing

1. Set the new version in `Cargo.toml` (`[workspace.package]`), in
   `browser/chromium/manifest.json` and `browser/firefox/manifest.json`, and
   in `packaging/arch/PKGBUILD` (`pkgver`); run `cargo build` to refresh
   `Cargo.lock`. Commit and wait for `CI passed` on `main`.
2. Tag and push: `git tag -a vX.Y.Z -m "FuckAds X.Y.Z" && git push origin vX.Y.Z`.
   A tag with a suffix (`v0.3.0-rc.1`) publishes a pre-release and does not
   touch Homebrew. The version in the code must match the tag exactly or the
   workflow stops before building.
3. The workflow attaches tarballs (`packaging/make-dist.sh`), `.deb` and `.rpm`
   packages, the extension zips, `SHA256SUMS`, build-provenance attestations
   and the installers to the release, and bumps the formula in
   `PankajKrana/homebrew-tap` (needs `TAP_GITHUB_TOKEN`; otherwise edit the
   tap by hand).
4. Afterwards, update the `sha256` in `packaging/arch/PKGBUILD` and
   `packaging/homebrew/fuck-ads.rb` (the copies kept in this repository) to
   the new source tarball checksum:
   `curl -fsSL https://github.com/PankajKrana/fuck-ads/archive/refs/tags/vX.Y.Z.tar.gz | shasum -a 256`.

Users can verify a download with
`gh attestation verify <file> --repo PankajKrana/fuck-ads`.

## Code layout

See [architecture.md](architecture.md). Rules of thumb:

* Keep `fuck-ads-filter`, `fuck-ads-dns`, `fuck-ads-core` platform-independent.
* No blocking work on async request paths: rule compilation and file I/O go
  through `spawn_blocking`.
* Never log URLs or allowed hostnames.
* New filter syntax: add it to the parser with tests; if it can't be
  supported faithfully, return `ParseError::Unsupported`.
