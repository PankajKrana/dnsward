# Contributing

Thanks for helping! A few ground rules keep AdBlock trustworthy:

1. **Privacy first.** No telemetry, no remote logging, no storing allowed
   domains or URLs. Changes that add network connections must be documented
   in `docs/privacy.md`.
2. **Never execute filter-list content.** New scriptlets are new reviewed
   functions in `browser/common/scriptlets.js`, plus an entry in
   `adblock_filter::cosmetic::SCRIPTLETS`, plus tests.
3. **No HTTPS interception, no root certificates.**
4. **Keep the core platform-independent.** OS-specific code goes into
   `adblock-platform`.
5. **Don't hard-code block lists in source.** Rules belong in list files.

## Workflow

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
node --test "browser/tests/*.test.js"
```

Add tests with every change. Parser changes need cases in
`tests/filter_parser.rs`; DNS behaviour changes need cases in
`tests/dns_blocking.rs`.

Commit messages: imperative mood, with a short summary line and details in
the body.

## Rule contributions

YouTube compatibility rules live in `filters/youtube.txt`. Please include
the YouTube page type (watch, home, search, shorts, mobile) and what the
rule fixes. General ad rules belong upstream in EasyList or uBlock filters.
