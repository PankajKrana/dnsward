#!/bin/sh
# Builds a Debian/Ubuntu package from a release binary:
#   packaging/deb/build-deb.sh <path-to-fuck-ads-binary> <version> <amd64|arm64> [outdir]
set -eu
BIN="$1"; VERSION="$2"; ARCH="$3"; OUT="${4:-dist}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PKG="$(mktemp -d)/fuck-ads_${VERSION}_${ARCH}"
trap 'rm -rf "$(dirname "$PKG")"' EXIT
mkdir -p "$PKG/DEBIAN" "$PKG/usr/bin" "$PKG/usr/share/fuck-ads/filters" "$PKG/lib/systemd/system" \
         "$PKG/etc/fuck-ads" "$PKG/usr/share/doc/fuck-ads"
install -m 0755 "$BIN" "$PKG/usr/bin/fuck-ads"
ln -s fuck-ads "$PKG/usr/bin/fuck-ads-native-host"
install -m 0644 "$ROOT"/filters/*.txt "$PKG/usr/share/fuck-ads/filters/"
sed 's#/usr/local/bin/fuck-ads#/usr/bin/fuck-ads#g' "$ROOT/platforms/linux/fuck-ads.service" > "$PKG/lib/systemd/system/fuck-ads.service"
"$PKG/usr/bin/fuck-ads" config default > "$PKG/etc/fuck-ads/config.toml" 2>/dev/null || cp "$ROOT/crates/fuck-ads-config/src/default-config.toml" "$PKG/etc/fuck-ads/config.toml"
"$ROOT/browser/build.sh" >/dev/null && cp -R "$ROOT/browser/dist/chromium" "$ROOT/browser/dist/firefox" "$PKG/usr/share/fuck-ads/"
install -m 0644 "$ROOT/README.md" "$PKG/usr/share/doc/fuck-ads/"
install -m 0644 "$ROOT/LICENSE" "$PKG/usr/share/doc/fuck-ads/copyright"
sed -e "s/@VERSION@/$VERSION/" -e "s/@ARCH@/$ARCH/" "$ROOT/packaging/deb/control" > "$PKG/DEBIAN/control"
echo "/etc/fuck-ads/config.toml" > "$PKG/DEBIAN/conffiles"
for s in postinst prerm postrm; do install -m 0755 "$ROOT/packaging/deb/$s" "$PKG/DEBIAN/$s"; done
mkdir -p "$OUT"
dpkg-deb --root-owner-group --build "$PKG" "$OUT/fuck-ads_${VERSION}_${ARCH}.deb"
