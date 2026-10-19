# Ferrodock engine: what's inside

Ferrodock's engine is the Docker engine, packed for each platform: a small
Linux VM image for Apple Silicon Macs (`ferrodock-engine-arm64`), a WSL
distro for Windows (`ferrodock-engine-wsl-amd64`), and the engine's own
programs for Linux (`ferrodock-engine-linux-amd64`, `-arm64`). Everything
in it is free and open source software. Anyone may use it for any purpose, at no cost,
including in companies and enterprises. No license, subscription or sign-up
is needed.

| Part | License | Source |
| --- | --- | --- |
| ferrodock-agent (the VM's first process) | MIT | `agent/` in https://github.com/manimovassagh/Ferrodock |
| Linux kernel, Mac image only (LinuxKit build, with `engine/kernel/ferrodock.config`) | GPL-2.0 | `ferrodock-engine-arm64.kernel-source.tar.xz` in the same release as the image, plus `ferrodock-engine-arm64.kernel-config`. On Windows, WSL runs its own kernel, which is part of Windows; Ferrodock doesn't ship one. |
| Docker Engine (dockerd), Docker CLI, containerd, runc, BuildKit/buildx, Compose | Apache-2.0 | https://github.com/moby/moby, https://github.com/docker/cli, https://github.com/containerd/containerd, https://github.com/opencontainers/runc, https://github.com/docker/buildx, https://github.com/docker/compose |
| Alpine Linux base system (BusyBox, musl, iptables, e2fsprogs and others) | Each package's own license (GPL-2.0, MIT, BSD and similar) | https://pkgs.alpinelinux.org and https://gitlab.alpinelinux.org/alpine/aports |

The Docker Engine parts and the Alpine base come from the official
`docker:dind` image (arm64 for the Mac, amd64 for Windows), unmodified. The
Linux image has only the Docker Engine programs from that same image.
`packages.txt` lists every Alpine package in an image with its version and
license. It ships with each release as `ferrodock-engine-arm64.packages.txt`
and `ferrodock-engine-wsl-amd64.packages.txt`, and inside each image at
`/usr/share/ferrodock/packages.txt`.

The GPL parts (the kernel, BusyBox and others) stay under the GPL. Their
source is available from the links above, and the kernel's exact source is
published with every engine release. Using the engine, inside a company or
anywhere else, puts no obligations on you or on the containers you run.

Ferrodock itself is MIT licensed. See `LICENSE` at the root of the
repository.
