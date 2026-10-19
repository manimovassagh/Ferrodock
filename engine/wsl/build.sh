#!/usr/bin/env bash
# Builds Ferrodock's engine for Windows: a root filesystem that WSL
# imports as the `ferrodock` distro. The Docker engine (from the official
# docker:dind image) and ferrodock-agent; WSL brings its own kernel.
#
# Usage: engine/wsl/build.sh <agent binary> <out dir>
#
# The agent is built with
# `cargo build --manifest-path agent/Cargo.toml --release --target x86_64-unknown-linux-musl`.
# Writes <out>/ferrodock-engine-wsl-amd64.tar.zst and <out>/packages.txt.
set -euo pipefail

agent=$1 out=$2
here=$(cd "$(dirname "$0")" && pwd)
# The Docker engine release inside the distro.
engine_image=${ENGINE_IMAGE:-docker:28-dind}

work=$(mktemp -d)
trap 'sudo rm -rf "$work"' EXIT
mkdir -p "$out"

echo "==> root filesystem from $engine_image (amd64)"
id=$(docker create --platform linux/amd64 "$engine_image" /x)
mkdir "$work/root"
docker export "$id" | sudo tar -x -C "$work/root" --numeric-owner
docker rm "$id" >/dev/null
root=$work/root
# Left by docker export; WSL writes its own hosts and resolv.conf.
sudo rm -f "$root/.dockerenv" "$root/etc/hosts" "$root/etc/resolv.conf" "$root/etc/hostname"

echo "==> ferrodock-agent, WSL settings, notice and package list"
sudo install -m 0755 "$agent" "$root/sbin/ferrodock-agent"
sudo install -m 0644 "$here/wsl.conf" "$root/etc/wsl.conf"
sudo mkdir -p "$root/usr/share/ferrodock"
sudo install -m 0644 "$here/../NOTICE.md" "$root/usr/share/ferrodock/NOTICE.md"
# Every Alpine package in the image, with its version and license.
awk -F: '/^P:/{p=$2} /^V:/{v=$2} /^L:/{print p, v, $2}' \
    "$root/lib/apk/db/installed" | sort >"$out/packages.txt"
sudo cp "$out/packages.txt" "$root/usr/share/ferrodock/packages.txt"

echo "==> ferrodock-engine-wsl-amd64.tar.zst"
sudo tar -C "$root" --numeric-owner -cf - . | zstd -19 -T0 >"$out/ferrodock-engine-wsl-amd64.tar.zst"

echo "==> done"
ls -la "$out"
