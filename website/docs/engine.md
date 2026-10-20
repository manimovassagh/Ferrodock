---
sidebar_position: 4
title: Ferrodock's own engine
---

# Ferrodock's own engine

On a Mac with Apple Silicon, on Windows and on Linux, Ferrodock can run
Docker by itself, with nothing else installed.

## Setting it up

When Ferrodock finds no Docker engine, it shows **No Docker engine yet**.
Click **Set up Ferrodock's engine**. Ferrodock downloads the engine once,
checks it, and starts it. From then on:

- it starts when Ferrodock opens (unless you turn that off in
  [Settings → Startup](./using/settings.md#startup)), and stops when you
  quit Ferrodock;
- it shows up as **Ferrodock** in the [engine list](./engines.md#switching-engines),
  so you can switch to and from it at any time;
- images, containers and volumes you create in it are kept between runs.

The first start can take a minute. Ferrodock connects as soon as the
engine is up.

## macOS

On Apple Silicon Macs the engine is a small Linux VM with the Docker engine
inside, run through Apple's Virtualization framework at native speed. The
download is about 120 MB.

- Everything lives in `~/.ferrodock`: the engine image, its data disk
  (images, containers and volumes; it only takes the space it uses) and the
  engine's log, `console.log`.
- The VM gets half your CPU cores (between 2 and 8) and 4 GB of memory.
  Change that in [Settings → Ferrodock's engine](./using/settings.md#ferrodocks-engine).
- Your home folder is shared with the engine at the same path, so bind
  mounts such as `-v ~/project:/app` work as you'd expect. Folders outside
  your home folder aren't shared.
- Published ports are on `localhost`: after `docker run -p 8080:80 nginx`,
  http://localhost:8080 opens it, as on Linux. Ferrodock passes on TCP
  ports while it runs; UDP ports are only reachable from inside the engine.
- The `docker` command can use it too:

  ```bash
  export DOCKER_HOST=unix://$HOME/.ferrodock/docker.sock
  ```

Intel Macs can't run Ferrodock's engine; Ferrodock connects to another
Docker engine there. See [Connecting to other engines](./engines.md).

## Windows

On Windows the engine is a WSL distro called `ferrodock`. WSL is part of
Windows; if it isn't turned on yet, run this once in a terminal opened as
administrator, then restart:

```powershell
wsl --install
```

- Everything lives in `%LOCALAPPDATA%\Ferrodock`: the distro's disk
  (images, containers and volumes) and the engine's log, `console.log`.
- Ferrodock talks to the engine privately, through WSL itself: no network
  port is opened.
- Ferrodock offers the engine on the named pipe
  `npipe:////./pipe/ferrodock_engine`. When nothing else uses Docker's usual
  pipe (`docker_engine`), it serves that one too, so the `docker` command
  works with no setup. Otherwise, point it at Ferrodock's:

  ```powershell
  docker context create ferrodock --docker host=npipe:////./pipe/ferrodock_engine
  docker context use ferrodock
  ```

- The engine trusts the certificates Windows trusts, so pulling images
  works behind company proxies that inspect TLS traffic.
- Your drives are in the engine under `/mnt`, so for bind mounts use paths
  such as `-v /mnt/c/Users/me/project:/app`.

## Linux

Docker runs natively on Linux, so there's no VM: Ferrodock's engine is the
Docker engine itself, run as a system service called `ferrodock-engine`.

- Setting it up downloads the engine once (about 70 MB) and asks for your
  admin password, because the Docker engine runs as root on Linux. You're
  asked only this once.
- It has its own socket, `/run/ferrodock/docker.sock`, which your user can
  use without `sudo`; its own data folder, `/var/lib/ferrodock`; and its
  programs in `/opt/ferrodock/engine/bin`. It doesn't touch another Docker
  installation.
- Like Docker on Linux, it starts with your computer and keeps running
  when you quit Ferrodock, so your containers keep running too.
- Published ports are on `localhost`, as usual on Linux.
- The `docker` command can use it:
  `export DOCKER_HOST=unix:///run/ferrodock/docker.sock`.
- Its log: `journalctl -u ferrodock-engine`.

If Docker is already installed as the `docker` service, Ferrodock uses
that instead and only offers its own engine when there's nothing else.

## Removing the engine

Quit Ferrodock first, so the engine is stopped.

- **macOS:** delete the `~/.ferrodock` folder.
- **Windows:** run `wsl --unregister ferrodock`, then delete the
  `%LOCALAPPDATA%\Ferrodock` folder.
- **Linux:** run
  `sudo systemctl disable --now ferrodock-engine && sudo rm -rf /etc/systemd/system/ferrodock-engine.service /opt/ferrodock /var/lib/ferrodock`.

This deletes every image, container and volume in Ferrodock's engine. You
can set it up again at any time.
