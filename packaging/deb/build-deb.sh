#!/bin/sh
# Builds a Debian/Ubuntu package from a release binary:
#   packaging/deb/build-deb.sh <path-to-adblock-binary> <version> <amd64|arm64> [outdir]
set -eu
BIN="$1"; VERSION="$2"; ARCH="$3"; OUT="${4:-dist}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PKG="$(mktemp -d)/adblock_${VERSION}_${ARCH}"
trap 'rm -rf "$(dirname "$PKG")"' EXIT
mkdir -p "$PKG/DEBIAN" "$PKG/usr/bin" "$PKG/usr/share/adblock/filters" "$PKG/lib/systemd/system" \
         "$PKG/etc/adblock" "$PKG/usr/share/doc/adblock"
install -m 0755 "$BIN" "$PKG/usr/bin/adblock"
ln -s adblock "$PKG/usr/bin/adblock-native-host"
install -m 0644 "$ROOT"/filters/*.txt "$PKG/usr/share/adblock/filters/"
sed 's#/usr/local/bin/adblock#/usr/bin/adblock#g' "$ROOT/platforms/linux/adblock.service" > "$PKG/lib/systemd/system/adblock.service"
"$PKG/usr/bin/adblock" config default > "$PKG/etc/adblock/config.toml" 2>/dev/null || cp "$ROOT/crates/adblock-config/src/default-config.toml" "$PKG/etc/adblock/config.toml"
"$ROOT/browser/build.sh" >/dev/null && cp -R "$ROOT/browser/dist/chromium" "$ROOT/browser/dist/firefox" "$PKG/usr/share/adblock/"
install -m 0644 "$ROOT/README.md" "$PKG/usr/share/doc/adblock/"
install -m 0644 "$ROOT/LICENSE" "$PKG/usr/share/doc/adblock/copyright"
sed -e "s/@VERSION@/$VERSION/" -e "s/@ARCH@/$ARCH/" "$ROOT/packaging/deb/control" > "$PKG/DEBIAN/control"
echo "/etc/adblock/config.toml" > "$PKG/DEBIAN/conffiles"
for s in postinst prerm postrm; do install -m 0755 "$ROOT/packaging/deb/$s" "$PKG/DEBIAN/$s"; done
mkdir -p "$OUT"
dpkg-deb --root-owner-group --build "$PKG" "$OUT/adblock_${VERSION}_${ARCH}.deb"
