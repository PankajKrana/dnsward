# Contributing to FuckAds

Thanks for helping! This guide covers the ground rules, how to set up, what a
good pull request looks like, and how changes reach a release. By taking part
you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

## Ground rules

These keep FuckAds trustworthy. Changes that break them will not be merged.

1. **Privacy first.** No telemetry, no remote logging, no storing allowed
   domains or URLs. Anything that adds a network connection must be documented
   in [`docs/privacy.md`](docs/privacy.md).
2. **Never execute filter-list content.** New scriptlets are new reviewed
   functions in `browser/common/scriptlets.js`, plus an entry in
   `fuck_ads_filter::cosmetic::SCRIPTLETS`, plus tests.
3. **No HTTPS interception, no root certificates** (see
   [`docs/proxy-design.md`](docs/proxy-design.md) for what is and is not planned).
4. **Keep the core platform-independent.** OS-specific code belongs in
   `fuck-ads-platform`.
5. **Don't hard-code block lists in source.** Rules belong in list files.
6. **Anything that runs as root or changes system settings must fail safe:**
   persist the undo information first, verify the result, and roll back
   automatically. Follow the pattern in `crates/fuck-ads-cli/src/system.rs`.

## Ways to contribute

- **Report a bug** or **suggest a feature** with the issue forms. Include
  `fuck-ads --version` and the output of `fuck-ads doctor`.
- **Report a security problem privately**, never as a public issue: see
  [SECURITY.md](SECURITY.md).
- **Improve docs**, or add tests for behaviour that is not covered yet.
- **Send a fix or a feature.** For anything larger than a small fix, open an
  issue first so we can agree on the approach before you spend the time.
- **Rule contributions.** YouTube compatibility rules live in
  `filters/youtube.txt`; please say which page type (watch, home, search,
  shorts, mobile) the rule fixes. General ad rules belong upstream in EasyList
  or uBlock filters.

## Development setup

You need:

- Rust **1.88 or newer** (`rustup`), because the code uses edition 2024 and
  `let` chains.
- A C compiler for the TLS stack (Xcode Command Line Tools on macOS, `gcc` or
  `clang` on Linux).
- Node 22 for the browser-extension tests, and `shellcheck` if you edit shell
  scripts (both optional).

```sh
git clone https://github.com/PankajKrana/fuck-ads && cd fuck-ads
cargo build --workspace
```

Run a development daemon **without root** and without touching your system DNS:
see "Running a development daemon" in [`docs/development.md`](docs/development.md).
Do not run `fuck-ads enable`, `enforce-dns on` or `proxy on` while developing
unless you are testing exactly those commands and know how to undo them
(`sudo fuck-ads disable`).

## Before you open a pull request

Run what CI runs:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked     # CI treats warnings as errors
cargo test --workspace --locked
node --test "browser/tests/*.test.js"               # if you touched browser/
shellcheck -s sh installers/*.sh                    # if you touched shell scripts
```

- **Add tests with every change.** Parser changes need cases in
  `tests/filter_parser.rs`; DNS behaviour changes need cases in
  `tests/dns_blocking.rs`; proxy changes need cases in `tests/proxy.rs` or the
  crate's own tests. The integration tests start a real daemon in-process on
  random loopback ports and need no root and no network.
- **Update the docs** whenever commands, paths, config keys or behaviour
  change (`README.md`, `docs/`). Config changes also update
  `crates/fuck-ads-config/src/default-config.toml`; a test keeps it in sync
  with the defaults.
- **Keep changes focused.** One logical change per pull request; unrelated
  clean-ups go in their own.
- **Match the surrounding code** (naming, comment density, error handling).
  `cargo fmt` is the only formatter.

## Commits and pull requests

- Branch from `main`; name it for the change (`fix-resolver-cache`,
  `feat-proxy-status`).
- Commit messages: imperative mood, a short summary line, details in the body.
  [Conventional Commits](https://www.conventionalcommits.org/) prefixes
  (`fix:`, `feat:`, `docs:`, `ci:`) are welcome and make release notes easier.
- Open the pull request against `main` and fill in the template. Link the
  issue it fixes.
- **CI must be green** before review. The "CI passed" check summarises every
  job (formatting, clippy and tests on Linux x86-64/arm64 and macOS 14/15, MSRV,
  browser tests, packages, a real install on a systemd host, fuzzing, advisory
  and license checks).
- Code owners review every change (`.github/CODEOWNERS`). Changes to
  privileged code, installers, packaging or workflows get a closer look; say in
  the PR how failure is handled.
- Maintainers squash-merge, so your branch history does not need to be tidy,
  but the PR title and description should be.

## Security-sensitive changes

If your change touches code that runs as root, edits DNS/firewall/proxy
settings, parses untrusted input (filter lists, DNS packets, proxy requests,
native-messaging JSON), or changes installers and packaging, also:

- explain the threat and the mitigation in the PR description,
- add a test for the failure case (malformed input, rollback, limits),
- update [`docs/security.md`](docs/security.md) if the trust boundary changes.

## Releases (maintainers)

1. Update the version in `Cargo.toml` (`[workspace.package]`), both
   `browser/*/manifest.json` files and `packaging/arch/PKGBUILD`; refresh
   `Cargo.lock`.
2. Make sure CI is green on `main`.
3. Tag and push: `git tag -a vX.Y.Z -m "FuckAds X.Y.Z" && git push origin vX.Y.Z`.
   The release workflow checks that the tag matches the code version, builds
   and tests every target, attests the artifacts, and publishes the release.

Details, including the Homebrew tap, are in
[`docs/development.md`](docs/development.md#releasing).

## License

By contributing you agree that your contributions are licensed under the
project's [MIT License](LICENSE).
