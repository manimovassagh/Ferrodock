#!/usr/bin/env bash
# Packs Ferrodock as a .deb (Debian, Ubuntu and their relatives): the app,
# fdock and ferrodock-mcp in /usr/bin, with a desktop entry and an icon so
# it shows up in the applications menu.
#
# Usage: packaging/linux/deb.sh <amd64|arm64> <version> <folder with the binaries> <out dir>
set -euo pipefail

arch=$1 version=$2 bin=$3 out=$4
root=$(mktemp -d)
chmod 755 "$root"
trap 'rm -rf "$root"' EXIT

install -Dm755 -t "$root/usr/bin" "$bin/ferrodock" "$bin/fdock" "$bin/ferrodock-mcp"
install -Dm644 "$(dirname "$0")/ferrodock.desktop" "$root/usr/share/applications/ferrodock.desktop"
install -Dm644 assets/icon-256.png "$root/usr/share/icons/hicolor/256x256/apps/ferrodock.png"
install -Dm644 LICENSE "$root/usr/share/doc/ferrodock/copyright"

size=$(du -sk "$root/usr" | cut -f1)
mkdir -p "$root/DEBIAN"
cat > "$root/DEBIAN/control" <<CONTROL
Package: ferrodock
Version: $version
Architecture: $arch
Maintainer: Mani Movassagh <74688476+manimovassagh@users.noreply.github.com>
Installed-Size: $size
Depends: libc6 (>= 2.35)
Recommends: libxkbcommon0, libgl1, libvulkan1
Section: admin
Priority: optional
Homepage: https://manimovassagh.github.io/Ferrodock/
Description: The native container manager, written in Rust
 Manage Docker or Podman containers, images, volumes, networks and
 Compose projects from a fast native app. Comes with fdock, a
 command-line tool, and ferrodock-mcp, an MCP server for AI tools.
CONTROL

mkdir -p "$out"
dpkg-deb --root-owner-group --build "$root" "$out/Ferrodock-$version-linux-$arch.deb"
