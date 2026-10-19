#!/usr/bin/env bash
# Builds Ferrodock's engine for Linux: the Docker engine's programs, taken
# from the official docker:dind image. They're static, so they run on any
# distribution. Ferrodock installs them as the `ferrodock-engine` service.
#
# Usage: engine/linux/build.sh <amd64|arm64> <out dir>
# Writes <out>/ferrodock-engine-linux-<arch>.tar.zst.
set -euo pipefail

arch=$1 out=$2
here=$(cd "$(dirname "$0")" && pwd)
# The Docker engine release, the same as in the Mac and Windows images.
engine_image=${ENGINE_IMAGE:-docker:28-dind}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin" "$out"

echo "==> the engine's programs from $engine_image ($arch)"
id=$(docker create --platform "linux/$arch" "$engine_image" /x)
for program in dockerd containerd containerd-shim-runc-v2 ctr runc docker-init docker-proxy docker; do
    docker cp "$id:/usr/local/bin/$program" "$work/bin/$program" >/dev/null
done
docker rm "$id" >/dev/null
cp "$here/../NOTICE.md" "$work/NOTICE.md"

echo "==> ferrodock-engine-linux-$arch.tar.zst"
tar -C "$work" --owner=0 --group=0 -cf - bin NOTICE.md | zstd -19 -T0 >"$out/ferrodock-engine-linux-$arch.tar.zst"
ls -la "$out"
