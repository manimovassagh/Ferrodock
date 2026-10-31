---
slug: /
sidebar_position: 1
title: Introduction
description: "Ferrodock is a free, open-source native container manager written in Rust: containers, Compose, images, volumes and networks in one window."
---

# Ferrodock

**A fast, native desktop app for managing Docker, written entirely in Rust.**

Ferrodock shows everything in your Docker engine (containers, Compose
projects, images, volumes and networks) in one clean window, and lets you
act on it in a click. It's a single native app for macOS, Windows and
Linux: no Electron, no web view, no background service, and no CPU used
while idle. It's free and open source under the MIT License.

![The Ferrodock containers page](/img/containers.png)

## What you can do

- **Containers**: start, stop, pause, restart, rename, edit, duplicate and
  remove; open published ports in your browser; act on many at once.
- **Logs, stats, terminal and files**: live logs, CPU and memory charts, a
  shell inside the container, and a file browser.
- **Compose**: projects grouped together, started from a `compose.yaml`,
  rebuilt service by service.
- **Images**: search Docker Hub, pull, run, tag, push, save and load.
- **Quick start**: popular databases and services in one click.
- **Linux machines**: a small Ubuntu, Debian, Fedora, Alpine or Arch to
  work in, sharing a folder with your computer.
- **Kubernetes**: a local cluster only when you want one, ready for
  `kubectl`.
- **Volumes and networks**: create, browse, connect and clean up.
- **Disk**: see where the space goes and free it safely.

## Where Docker runs

On **macOS**, **Windows** and **Linux**, Ferrodock can run its
own Docker engine, so there's nothing else to install. It also connects
to any other Docker engine you already have, local or remote.

## Next steps

- [Install Ferrodock](./installation.md) and set up an engine.
- [Learn your way around](./using/basics.md).
- [Ferrodock's own engine](./engine.md) and
  [connecting to other engines](./engines.md).
- Stuck? See [Troubleshooting](./troubleshooting.md).
