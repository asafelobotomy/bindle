#!/usr/bin/env bash
# Build dist/Bindle-<version>-x86_64.AppImage.
#   --appdir-only   stage packaging/appimage/AppDir without running appimagetool
set -euo pipefail
cd "$(dirname "$0")/../.."

APPIMAGETOOL_VERSION="${APPIMAGETOOL_VERSION:-1.9.0}"
HERE=packaging/appimage
APPDIR=$HERE/AppDir
VERSION=$(cat VERSION)

# Static musl build when available (pacman -S rust-musl), else glibc.
if [ -d /usr/lib/rustlib/x86_64-unknown-linux-musl ] || rustup target list --installed 2>/dev/null | grep -q musl; then
    TARGET=x86_64-unknown-linux-musl
else
    TARGET=x86_64-unknown-linux-gnu
    echo "note: musl target not installed; building against glibc" >&2
fi
cargo build --release --locked -p bindle-cli --target "$TARGET"

rm -rf "$APPDIR"
install -Dm755 "target/$TARGET/release/bindle" "$APPDIR/usr/bin/bindle"
install -Dm755 "$HERE/AppRun" "$APPDIR/AppRun"
install -Dm644 "$HERE/bindle.desktop" "$APPDIR/bindle.desktop"
install -Dm644 branding/bindle.svg "$APPDIR/bindle.svg"
install -Dm644 branding/bindle.svg "$APPDIR/usr/share/icons/hicolor/scalable/apps/bindle.svg"
ln -sf bindle.svg "$APPDIR/.DirIcon"

if [ "${1:-}" = "--appdir-only" ]; then
    echo "Staged $APPDIR"
    exit 0
fi

TOOL=$HERE/tools/appimagetool-$APPIMAGETOOL_VERSION
if [ ! -x "$TOOL" ]; then
    mkdir -p "$HERE/tools"
    curl -fL -o "$TOOL" \
        "https://github.com/AppImage/appimagetool/releases/download/$APPIMAGETOOL_VERSION/appimagetool-x86_64.AppImage"
    chmod +x "$TOOL"
fi
mkdir -p dist
# appimagetool embeds the static type2 runtime (needs fusermount3, not libfuse2).
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" "$APPDIR" "dist/Bindle-$VERSION-x86_64.AppImage"
echo "Built dist/Bindle-$VERSION-x86_64.AppImage"
