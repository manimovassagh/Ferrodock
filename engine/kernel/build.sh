#!/usr/bin/env bash
# Builds the kernel for Ferrodock's engine on a Mac: LinuxKit's kernel
# (made for containers in a VM) with the options in ferrodock.config on
# top. Writes <out dir>/kernel in the format Apple's Virtualization
# framework boots: on arm64 the uncompressed Image, on amd64 (Intel Macs)
# the bzImage.
#
# Usage: engine/kernel/build.sh <arm64|amd64> <linux source dir> <base .config> <out dir>
#
# Both the source and the base config come from the linuxkit/kernel image
# (linux.tar.xz and kernel-dev.tar), for the same architecture. To build
# for the other architecture than the machine's, set CROSS_COMPILE (such
# as aarch64-linux-gnu-).
set -euo pipefail

case ${1:-} in
arm64) arch=arm64 boot=arch/arm64/boot/Image ;;
amd64) arch=x86_64 boot=arch/x86/boot/bzImage ;;
*) echo "usage: $0 <arm64|amd64> <linux source dir> <base .config> <out dir>" >&2; exit 2 ;;
esac
src=$2 base=$3
# Absolute: make runs inside the source folder (-C).
out=$(mkdir -p "$4" && cd "$4" && pwd)
here=$(cd "$(dirname "$0")" && pwd)
jobs=${JOBS:-$(nproc)}

cp "$base" "$src/.config"
while read -r line; do
    case $line in
    '' | '#'*) ;;
    *) "$src/scripts/config" --file "$src/.config" --set-val "${line%%=*}" "${line#*=}" ;;
    esac
done <"$here/ferrodock.config"
make -C "$src" ARCH="$arch" olddefconfig

# Fail early if an option didn't stick (a dependency was missing).
while read -r line; do
    case $line in
    '' | '#'*) ;;
    *) grep -qx "CONFIG_$line" "$src/.config" || { echo "missing: $line" >&2; exit 1; } ;;
    esac
done <"$here/ferrodock.config"

make -C "$src" ARCH="$arch" -j"$jobs" "${boot##*/}" modules
mkdir -p "$out/modules"
# The module folder otherwise follows the build machine's kmod (newer
# distributions say /usr/lib/modules); the engine's root has /lib/modules.
make -C "$src" ARCH="$arch" INSTALL_MOD_PATH="$out/modules" INSTALL_MOD_STRIP=1 \
    KERNEL_MODULE_DIRECTORY=/lib/modules modules_install
cp "$src/$boot" "$out/kernel"
cp "$src/.config" "$out/config"
