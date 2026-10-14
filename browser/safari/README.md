# Safari

Safari does not load unpacked extensions or allow native messaging to an
arbitrary host. Every Safari extension must be packaged inside a signed macOS
app built with Xcode. FuckAds therefore supports Safari in two ways:

## 1. Content blocker rules (works today, no Xcode project needed to generate)

The service exports Safari Content Blocker JSON (network blocking and
site-specific element hiding):

```sh
fuck-ads browser export-safari ~/fuck-ads-safari.json
```

Load that JSON from a Content Blocker extension target in an Xcode app
(`NSExtensionPrincipalClass` = a `SFContentBlockerRequestHandler` that returns
the file). Re-export after `fuck-ads update` and call
`SFContentBlockerManager.reloadContentBlocker(withIdentifier:)` to refresh.

Safari limits: 150,000 rules per blocker, a regex subset without
alternation, and no scriptlets; YouTube blocking in Safari is therefore
limited to network and cosmetic rules.

## 2. Safari Web Extension (full feature set, requires Xcode + signing)

The cross-browser extension in `browser/common` can be converted:

```sh
./browser/build.sh
xcrun safari-web-extension-converter browser/dist/chromium \
    --app-name FuckAds --bundle-identifier com.fuckads.safari --macos-only
```

Native messaging in Safari goes to the *containing app*, not to
`fuck-ads-native-host`, so the app must relay messages to the control socket
(`/Library/Application Support/FuckAds/data/control.sock`) using the same
newline-delimited JSON protocol (see `crates/fuck-ads-core/src/protocol.rs`).
This relay and code signing are left to a distribution build; they cannot be
produced from this repository without an Apple Developer account.

DNS-level blocking applies to Safari regardless, since it uses the system
resolver (unless iCloud Private Relay is enabled; see
docs/troubleshooting.md).
