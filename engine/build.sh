#!/usr/bin/env bash
# Builds Ferrodock's engine image for Macs: arm64 for Apple Silicon,
# amd64 for Intel.
#
#   kernel       the Linux kernel, as Apple's Virtualization.framework
#                boots it (on arm64 the uncompressed Image, on amd64 the
#                bzImage)
#   rootfs.img   a read-only ext4 disk: the Docker engine (from the
#                official docker:dind image), the kernel's modules, and
#                ferrodock-agent as the first process
#
# Usage: engine/build.sh <arm64|amd64> <kernel> <modules dir> <agent binary> <out dir>
#
# The kernel is built by engine/kernel/build.sh, the agent with
# `cargo build --manifest-path agent/Cargo.toml --release --target <arch>-unknown-linux-musl`
# (aarch64 or x86_64).
set -euo pipefail

arch=$1 kernel=$2 modules=$3 agent=$4 out=$5
# The Docker engine release inside the VM.
engine_image=${ENGINE_IMAGE:-docker:28-dind}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
root=$work/root
mkdir -p "$root" "$out"

echo "==> root filesystem from $engine_image ($arch)"
id=$(docker create --platform "linux/$arch" "$engine_image" /x)
docker export "$id" | tar -x -C "$root"
docker rm "$id" >/dev/null
# docker export leaves placeholder files where the kernel puts devices.
rm -rf "$root/dev" "$root/.dockerenv" && mkdir -p "$root/dev"

echo "==> kernel modules"
rm -rf "$root/lib/modules"
installed=$modules/lib/modules
[ -d "$installed" ] || installed=$modules/usr/lib/modules
cp -a "$installed" "$root/lib/"

echo "==> ferrodock-agent"
install -m 0755 "$agent" "$root/sbin/ferrodock-agent"

# The root filesystem is read-only; what changes lives in /run (tmpfs)
# or on the data disk (/data).
ln -sf ../run/resolv.conf "$root/etc/resolv.conf"
# The DHCP client writes resolv.conf next to itself first, so point it
# straight at /run.
mkdir -p "$root/etc/udhcpc"
echo 'RESOLV_CONF="/run/resolv.conf"' >"$root/etc/udhcpc/udhcpc.conf"
echo ferrodock >"$root/etc/hostname"
mkdir -p "$root/data" "$root/Users" "$root/var/lib/docker" "$root/var/lib/containerd"

echo "==> notice and package list"
mkdir -p "$root/usr/share/ferrodock"
cp "$(dirname "$0")/NOTICE.md" "$root/usr/share/ferrodock/NOTICE.md"
# Every Alpine package in the image, with its version and license.
awk -F: '/^P:/{p=$2} /^V:/{v=$2} /^L:/{print p, v, $2}' \
    "$root/lib/apk/db/installed" | sort >"$out/packages.txt"
cp "$out/packages.txt" "$root/usr/share/ferrodock/packages.txt"

echo "==> rootfs.img"
size_mb=$(($(du -sm "$root" | cut -f1) * 13 / 10 + 64))
rm -f "$out/rootfs.img"
mkfs.ext4 -q -L ferrodock-root -d "$root" "$out/rootfs.img" "${size_mb}M"
cp "$kernel" "$out/kernel"

echo "==> done"
ls -la "$out"
