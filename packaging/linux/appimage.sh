#!/usr/bin/env bash
# Packs Ferrodock as an AppImage: one file that runs on most Linux
# distributions, with the app, fdock and ferrodock-mcp inside and a
# desktop entry and icon for desktop integration tools.
#
# Usage: packaging/linux/appimage.sh <x86_64|aarch64> <version> <folder with the binaries> <out dir>
set -euo pipefail

arch=$1 version=$2 bin=$3 out=$4
here=$(cd "$(dirname "$0")" && pwd)

# appimagetool, pinned and checked.
tool_version=1.9.0
case "$arch" in
    x86_64) tool_sha=46fdd785094c7f6e545b61afcfb0f3d98d8eab243f644b4b17698c01d06083d1 ;;
    aarch64) tool_sha=04f45ea45b5aa07bb2b071aed9dbf7a5185d3953b11b47358c1311f11ea94a96 ;;
    *) echo "unknown architecture $arch" >&2; exit 1 ;;
esac
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
tool="$work/appimagetool"
curl -fsSL -o "$tool" \
    "https://github.com/AppImage/appimagetool/releases/download/$tool_version/appimagetool-$arch.AppImage"
echo "$tool_sha  $tool" | sha256sum -c -
chmod +x "$tool"

app="$work/Ferrodock.AppDir"
install -Dm755 -t "$app/usr/bin" "$bin/ferrodock" "$bin/fdock" "$bin/ferrodock-mcp"
install -Dm644 "$here/ferrodock.desktop" "$app/ferrodock.desktop"
install -Dm644 "$here/ferrodock.desktop" "$app/usr/share/applications/ferrodock.desktop"
install -Dm644 assets/icon-256.png "$app/ferrodock.png"
install -Dm644 assets/icon-256.png "$app/usr/share/icons/hicolor/256x256/apps/ferrodock.png"
ln -s usr/bin/ferrodock "$app/AppRun"

mkdir -p "$out"
# No FUSE on CI runners: the tool unpacks itself instead.
APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$arch" VERSION="$version" \
    "$tool" --no-appstream "$app" "$out/Ferrodock-$version-linux-$arch.AppImage"
