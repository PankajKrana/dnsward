#!/bin/sh
# FuckAds installer for Linux (x86_64, arm64) with systemd.
#
#   curl -fsSL https://raw.githubusercontent.com/PankajKrana/fuck-ads/main/installers/install.sh | sh
#   ./installers/install.sh                 # from a source checkout (builds with cargo)
#
# Environment:
#   FUCK_ADS_REPO=owner/repo       GitHub repository for release downloads
#   FUCK_ADS_VERSION=0.1.0         release to install (default: latest)
#   FUCK_ADS_FROM_SOURCE=1         force building from source
#   FUCK_ADS_NO_ENABLE=1           install but do not turn protection on
#   FUCK_ADS_PREFIX=/usr/local     installation prefix
set -eu

REPO="${FUCK_ADS_REPO:-PankajKrana/fuck-ads}"
VERSION="${FUCK_ADS_VERSION:-latest}"
PREFIX="${FUCK_ADS_PREFIX:-/usr/local}"
BIN_DIR="$PREFIX/bin"
SHARE_DIR="$PREFIX/share/fuck-ads"
CONFIG_DIR=/etc/fuck-ads
STATE_DIR=/var/lib/fuck-ads
UNIT=/etc/systemd/system/fuck-ads.service
SERVICE_USER=fuck-ads

say()  { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"; }

# --- 1. Detect OS and architecture -----------------------------------------
[ "$(uname -s)" = "Linux" ] || die "this installer is for Linux; on macOS use install-macos.sh"
case "$(uname -m)" in
    x86_64|amd64)  TARGET=x86_64-unknown-linux-gnu ;;
    aarch64|arm64) TARGET=aarch64-unknown-linux-gnu ;;
    *) die "unsupported architecture: $(uname -m) (supported: x86_64, arm64)" ;;
esac
say "Detected Linux $(uname -m)"

# --- 2. Privileges -----------------------------------------------------------
if [ "$(id -u)" -eq 0 ]; then
    SUDO=""
else
    need sudo
    SUDO="sudo"
fi

# --- 3. Dependencies ---------------------------------------------------------
[ -d /run/systemd/system ] || die "systemd is required (no /run/systemd/system found)"
need systemctl
need install
need mktemp

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT INT TERM

# --- 4. Obtain the binary and data files ------------------------------------
SCRIPT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")" 2>/dev/null && pwd || echo "")"
SRC_ROOT=""
if [ -n "$SCRIPT_DIR" ] && [ -f "$SCRIPT_DIR/../Cargo.toml" ] && [ -d "$SCRIPT_DIR/../crates/fuck-ads-cli" ]; then
    SRC_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
fi

if [ -n "$SRC_ROOT" ] || [ "${FUCK_ADS_FROM_SOURCE:-0}" = "1" ]; then
    [ -n "$SRC_ROOT" ] || die "FUCK_ADS_FROM_SOURCE=1 requires running from a source checkout"
    if ! command -v cargo >/dev/null 2>&1 && [ -x "$HOME/.cargo/bin/cargo" ]; then
        PATH="$HOME/.cargo/bin:$PATH"
    fi
    command -v cargo >/dev/null 2>&1 || die "cargo not found; install Rust from https://rustup.rs or use a release build"
    command -v cc >/dev/null 2>&1 || die "a C compiler is required to build (install gcc or clang)"
    say "Building from source in $SRC_ROOT (this takes a few minutes the first time)"
    (cd "$SRC_ROOT" && cargo build --release --locked -p fuck-ads-cli) || die "build failed"
    mkdir -p "$WORK/pkg/share/filters" "$WORK/pkg/platforms"
    cp "$SRC_ROOT/target/release/fuck-ads" "$WORK/pkg/fuck-ads"
    cp "$SRC_ROOT"/filters/*.txt "$WORK/pkg/share/filters/"
    cp "$SRC_ROOT/platforms/linux/fuck-ads.service" "$WORK/pkg/platforms/"
    if [ -x "$SRC_ROOT/browser/build.sh" ]; then
        "$SRC_ROOT/browser/build.sh" >/dev/null && cp -R "$SRC_ROOT/browser/dist" "$WORK/pkg/share/browser"
    fi
else
    need curl
    need tar
    if [ "$VERSION" = "latest" ]; then
        BASE="https://github.com/$REPO/releases/latest/download"
    else
        BASE="https://github.com/$REPO/releases/download/v$VERSION"
    fi
    ARCHIVE="fuck-ads-$TARGET.tar.gz"
    say "Downloading $ARCHIVE"
    curl -fsSL --proto '=https' --tlsv1.2 -o "$WORK/$ARCHIVE" "$BASE/$ARCHIVE" || die "download failed: $BASE/$ARCHIVE"
    curl -fsSL --proto '=https' --tlsv1.2 -o "$WORK/$ARCHIVE.sha256" "$BASE/$ARCHIVE.sha256" || die "checksum download failed"
    say "Verifying checksum"
    EXPECTED="$(cut -d' ' -f1 < "$WORK/$ARCHIVE.sha256")"
    if command -v sha256sum >/dev/null 2>&1; then
        ACTUAL="$(sha256sum "$WORK/$ARCHIVE" | cut -d' ' -f1)"
    else
        ACTUAL="$(shasum -a 256 "$WORK/$ARCHIVE" | cut -d' ' -f1)"
    fi
    [ "$EXPECTED" = "$ACTUAL" ] || die "checksum mismatch for $ARCHIVE (expected $EXPECTED, got $ACTUAL)"
    mkdir -p "$WORK/pkg"
    tar -xzf "$WORK/$ARCHIVE" -C "$WORK/pkg" --no-same-owner
    [ -f "$WORK/pkg/fuck-ads" ] || die "archive does not contain the fuck-ads binary"
fi

"$WORK/pkg/fuck-ads" --version >/dev/null || die "the downloaded binary does not run on this system"

# --- 5. Install files ---------------------------------------------------------
say "Creating service account '$SERVICE_USER'"
if ! id -u "$SERVICE_USER" >/dev/null 2>&1; then
    NOLOGIN="$(command -v nologin || echo /usr/sbin/nologin)"
    if command -v useradd >/dev/null 2>&1; then
        $SUDO useradd --system --no-create-home --home-dir "$STATE_DIR" --shell "$NOLOGIN" "$SERVICE_USER"
    elif command -v adduser >/dev/null 2>&1; then
        $SUDO adduser -S -D -H -h "$STATE_DIR" -s "$NOLOGIN" "$SERVICE_USER"
    else
        die "cannot create a system user (no useradd/adduser)"
    fi
fi

say "Installing to $PREFIX"
# If a previous version is running, stop it cleanly first (DNS is restored).
if [ -x "$BIN_DIR/fuck-ads" ] && systemctl is-active --quiet fuck-ads.service 2>/dev/null; then
    UPGRADE=1
else
    UPGRADE=0
fi
$SUDO install -d -m 0755 "$BIN_DIR" "$SHARE_DIR" "$SHARE_DIR/filters"
$SUDO install -m 0755 "$WORK/pkg/fuck-ads" "$BIN_DIR/fuck-ads.new"
$SUDO mv -f "$BIN_DIR/fuck-ads.new" "$BIN_DIR/fuck-ads"
$SUDO ln -sf "$BIN_DIR/fuck-ads" "$BIN_DIR/fuck-ads-native-host"
$SUDO install -m 0644 "$WORK"/pkg/share/filters/*.txt "$SHARE_DIR/filters/"
if [ -d "$WORK/pkg/share/browser" ]; then
    $SUDO rm -rf "$SHARE_DIR/browser"
    $SUDO cp -R "$WORK/pkg/share/browser" "$SHARE_DIR/browser"
    $SUDO chmod -R a+rX "$SHARE_DIR/browser"
fi

$SUDO install -d -m 0755 "$CONFIG_DIR" "$STATE_DIR"
$SUDO install -d -m 0755 -o "$SERVICE_USER" -g "$SERVICE_USER" "$STATE_DIR/data"
if [ ! -f "$CONFIG_DIR/config.toml" ]; then
    "$WORK/pkg/fuck-ads" config default > "$WORK/config.toml"
    $SUDO install -m 0644 "$WORK/config.toml" "$CONFIG_DIR/config.toml"
    say "Wrote default configuration to $CONFIG_DIR/config.toml"
else
    say "Keeping existing configuration $CONFIG_DIR/config.toml"
fi

# --- 6. Configure the service ------------------------------------------------
sed "s#/usr/local/bin/fuck-ads#$BIN_DIR/fuck-ads#g" "$WORK/pkg/platforms/fuck-ads.service" > "$WORK/fuck-ads.service"
$SUDO install -m 0644 "$WORK/fuck-ads.service" "$UNIT"
$SUDO systemctl daemon-reload

# Register the browser native-messaging host for all users.
$SUDO "$BIN_DIR/fuck-ads" browser install --system >/dev/null || warn "could not register browser integration"

# --- 7. Validate ---------------------------------------------------------------
"$BIN_DIR/fuck-ads" config validate >/dev/null || die "configuration is invalid: run 'fuck-ads config validate'"

# --- 8. Start ------------------------------------------------------------------
if [ "$UPGRADE" = "1" ]; then
    say "Restarting the upgraded service"
    $SUDO "$BIN_DIR/fuck-ads" restart
elif [ "${FUCK_ADS_NO_ENABLE:-0}" != "1" ]; then
    say "Enabling protection"
    $SUDO "$BIN_DIR/fuck-ads" enable || die "enable failed; your previous DNS settings are unchanged. See 'fuck-ads doctor'."
fi

# --- 9. Initial filter lists and health check ----------------------------------
if systemctl is-active --quiet fuck-ads.service; then
    say "Downloading filter lists"
    $SUDO "$BIN_DIR/fuck-ads" update || warn "filter download failed; bundled rules are active and updates will retry automatically"
    echo
    "$BIN_DIR/fuck-ads" doctor || true
fi

cat <<EOF

FuckAds is installed.

  fuck-ads status          show status
  sudo fuck-ads disable    turn protection off
  fuck-ads doctor          diagnose problems

Browser extension (recommended, needed for in-page and YouTube ads):
  $SHARE_DIR/browser/chromium   (chrome://extensions → Developer mode → Load unpacked)
  $SHARE_DIR/browser/firefox    (about:debugging → Load Temporary Add-on)
EOF
