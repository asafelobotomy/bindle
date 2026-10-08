#!/usr/bin/env bash
# Build dist/Bindle-<version>-x86_64.AppImage.
#   --appdir-only   stage packaging/appimage/AppDir without running appimagetool
set -euo pipefail
cd "$(dirname "$0")/../.."

# Pinned and verified: this runs in the release job with write access.
APPIMAGETOOL_VERSION=1.9.0
APPIMAGETOOL_SHA256=46fdd785094c7f6e545b61afcfb0f3d98d8eab243f644b4b17698c01d06083d1
RUNTIME_VERSION=20251108
RUNTIME_SHA256=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
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
install -Dm644 LICENSE "$APPDIR/usr/share/licenses/bindle/LICENSE"
install -Dm644 branding/bindle.svg "$APPDIR/usr/share/icons/hicolor/scalable/apps/bindle.svg"
ln -sf bindle.svg "$APPDIR/.DirIcon"

if [ "${1:-}" = "--appdir-only" ]; then
    echo "Staged $APPDIR"
    exit 0
fi

fetch() { # url sha256 dest
    if [ ! -f "$3" ] || ! echo "$2  $3" | sha256sum -c --quiet - 2>/dev/null; then
        mkdir -p "$(dirname "$3")"
        curl -fL -o "$3.part" "$1"
        echo "$2  $3.part" | sha256sum -c --quiet - || { rm -f "$3.part"; echo "checksum mismatch: $1" >&2; exit 1; }
        mv "$3.part" "$3"
    fi
}
TOOL=$HERE/tools/appimagetool-$APPIMAGETOOL_VERSION
RUNTIME=$HERE/tools/runtime-x86_64-$RUNTIME_VERSION
fetch "https://github.com/AppImage/appimagetool/releases/download/$APPIMAGETOOL_VERSION/appimagetool-x86_64.AppImage" \
    "$APPIMAGETOOL_SHA256" "$TOOL"
fetch "https://github.com/AppImage/type2-runtime/releases/download/$RUNTIME_VERSION/runtime-x86_64" "$RUNTIME_SHA256" "$RUNTIME"
chmod +x "$TOOL"
mkdir -p dist
# The static type2 runtime needs fusermount3, not libfuse2.
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" --runtime-file "$RUNTIME" "$APPDIR" "dist/Bindle-$VERSION-x86_64.AppImage"
# Stable name for releases/latest/download links.
cp "dist/Bindle-$VERSION-x86_64.AppImage" dist/Bindle-x86_64.AppImage
echo "Built dist/Bindle-$VERSION-x86_64.AppImage"
