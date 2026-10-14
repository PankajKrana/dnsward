## What and why

<!-- What does this change, and why is it needed? Link the issue (Fixes #123). -->

## How it was tested

<!-- Commands you ran, platforms you tried (macOS / Linux, which browser). -->

## Checklist

- [ ] `cargo fmt --all`, `cargo clippy --workspace --all-targets` and `cargo test --workspace` pass
- [ ] `node --test "browser/tests/*.test.js"` passes (if the extension changed)
- [ ] Tests added or updated for the change
- [ ] Docs updated (README, `docs/`) if behaviour, commands or paths changed
- [ ] No telemetry, no storing or logging allowed domains or URLs (see [CONTRIBUTING](../CONTRIBUTING.md))
- [ ] No filter-list content is executed; no HTTPS interception or root certificates

### Security-sensitive areas

<!-- Tick if this touches code that runs as root, edits system DNS/firewall/proxy
settings, parses untrusted input, or changes installers/packaging. Describe the
failure modes and how they are handled (rollback, validation, limits). -->

- [ ] Touches privileged code, system settings, or untrusted-input parsing
