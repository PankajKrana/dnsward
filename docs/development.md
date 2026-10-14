# Development

## Prerequisites

* Rust stable (`rustup`); MSRV 1.85 (edition 2024)
* A C compiler, for the TLS stack (`aws-lc-rs`): Xcode Command Line Tools,
  or `gcc`/`clang` on Linux
* Node 20+, for the extension tests (optional)

## Build and test

```sh
cargo build --workspace
cargo test --workspace            # 113 Rust tests: unit + integration (tests/)
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

## Releasing

Push a `vX.Y.Z` tag. `.github/workflows/release.yml` tests, builds the four
targets, and attaches tarballs (`packaging/make-dist.sh`), `.deb` and `.rpm`
packages, checksums, and the installers to a GitHub release. Then update the
`sha256` in `packaging/homebrew/fuck-ads.rb` and `packaging/arch/PKGBUILD`.

The repository is `PankajKrana/fuck-ads`: the installers' `FUCK_ADS_REPO` default,
the workspace `repository`, the packaging files and the README install commands
all point at it. Before the first release, publish a GitHub release (the
installers download its archives) and fill in the `sha256` values in
`packaging/homebrew/fuck-ads.rb` and `packaging/arch/PKGBUILD`. The Homebrew tap
`PankajKrana/homebrew-tap` named in the formula does not exist yet.

## Code layout

See [architecture.md](architecture.md). Rules of thumb:

* Keep `fuck-ads-filter`, `fuck-ads-dns`, `fuck-ads-core` platform-independent.
* No blocking work on async request paths: rule compilation and file I/O go
  through `spawn_blocking`.
* Never log URLs or allowed hostnames.
* New filter syntax: add it to the parser with tests; if it can't be
  supported faithfully, return `ParseError::Unsupported`.
