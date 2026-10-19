---
sidebar_position: 8
title: How Ferrodock works
---

# How Ferrodock works

## A native app

Ferrodock is written entirely in Rust. Its window is drawn with
[egui](https://github.com/emilk/egui), so there's no bundled browser
engine: it's one small executable that starts fast and uses little memory.

## Talking to Docker

Ferrodock talks to the Docker engine through the
[Docker Engine API](https://docs.docker.com/reference/api/engine/), the
same API the `docker` command uses, over a Unix socket, a Windows named
pipe, TCP with TLS, or SSH. That's why it works with any Docker engine, and
why what you do in a terminal shows up in Ferrodock and the other way
around.

- All Docker calls run on a background thread, so the window never waits
  on the engine.
- Ferrodock follows the engine's event stream, so changes appear as they
  happen.
- Logs and stats stream only for the container you're looking at. When
  nothing changes, nothing is redrawn, so Ferrodock uses no CPU while idle.

The one exception is Compose: the Engine API has no Compose support, so
**Up**, **Rebuild**, **Pull & recreate** and merged project logs run the
`docker compose` command, pointed at the engine you picked.

## Ferrodock's own engine

Docker containers need a Linux kernel. Where your computer doesn't have one
to spare, Ferrodock brings a small Linux system with the Docker engine
inside, built only from free and open source software:

- **Apple Silicon Macs**: a lightweight virtual machine, run through
  Apple's Virtualization framework. Ferrodock passes the engine's socket
  through to `~/.ferrodock/docker.sock` on your Mac, and shares your home
  folder with the VM.
- **Windows**: a WSL distro. Ferrodock talks to the engine through WSL
  itself, with no network port, and offers it on a Windows named pipe.
- **Linux** has the kernel already, so there's no VM: Ferrodock installs
  the Docker engine as a system service, `ferrodock-engine`.

Each time, the engine is downloaded once and checked against its published
checksum. On a Mac and on Windows it runs only while Ferrodock does; on
Linux it starts with your computer, as Docker does there. See
[Ferrodock's own engine](./engine.md) for the details.

## Your data

Ferrodock has no account, sends no telemetry, and needs no server of its
own. It contacts GitHub to check for updates (which you can turn off) and
to download updates and its engine. Searching and pulling images goes
through your Docker engine, like the `docker` command.
