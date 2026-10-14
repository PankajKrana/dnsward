#!/bin/sh
# Assembles unpacked extensions:
#   dist/chromium  - Chrome, Chromium, Brave, Edge, Vivaldi (load unpacked)
#   dist/firefox   - Firefox 128+ (about:debugging, or sign via AMO)
#   dist/*.zip     - archives for store submission
set -eu
cd "$(dirname "$0")"
rm -rf dist
for target in chromium firefox; do
    out="dist/$target"
    mkdir -p "$out"
    cp -R common/. "$out/"
    cp "$target/manifest.json" "$out/manifest.json"
    if command -v zip >/dev/null 2>&1; then
        (cd "$out" && zip -qr "../fuck-ads-$target.zip" .)
    fi
    echo "built $out"
done
