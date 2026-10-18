#!/usr/bin/env bash
# Builds the arm64 kernel for Ferrodock's engine: LinuxKit's kernel (made
# for containers in a VM) with the options in ferrodock.config on top.
#
# Usage: engine/kernel/build.sh <linux source dir> <base .config> <out dir>
#
# Both the source and the base config come from the linuxkit/kernel image
# (linux.tar.xz and kernel-dev.tar). On an x86_64 machine set
# CROSS_COMPILE=aarch64-linux-gnu-.
set -euo pipefail

src=$1 base=$2 out=$3
here=$(cd "$(dirname "$0")" && pwd)
jobs=${JOBS:-$(nproc)}

cp "$base" "$src/.config"
while read -r line; do
    case $line in
    '' | '#'*) ;;
    *) "$src/scripts/config" --file "$src/.config" --set-val "${line%%=*}" "${line#*=}" ;;
    esac
done <"$here/ferrodock.config"
make -C "$src" ARCH=arm64 olddefconfig

# Fail early if an option didn't stick (a dependency was missing).
while read -r line; do
    case $line in
    '' | '#'*) ;;
    *) grep -qx "CONFIG_$line" "$src/.config" || { echo "missing: $line" >&2; exit 1; } ;;
    esac
done <"$here/ferrodock.config"

make -C "$src" ARCH=arm64 -j"$jobs" Image modules
mkdir -p "$out/modules"
# The module folder otherwise follows the build machine's kmod (newer
# distributions say /usr/lib/modules); the engine's root has /lib/modules.
make -C "$src" ARCH=arm64 INSTALL_MOD_PATH="$out/modules" INSTALL_MOD_STRIP=1 \
    KERNEL_MODULE_DIRECTORY=/lib/modules modules_install
cp "$src/arch/arm64/boot/Image" "$out/Image"
cp "$src/.config" "$out/config"
