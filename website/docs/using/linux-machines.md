---
sidebar_position: 8
title: Linux machines
description: "Create a small Ubuntu, Debian, Fedora, Alpine, Arch, AlmaLinux, openSUSE or Amazon Linux machine in Ferrodock, open its shell, and share a folder from your computer with it."
---

# Linux machines

The **Linux machines** page, under **START** in the sidebar, gives you small
Linux computers to work and try things in: test an install script, check
how your code builds on Fedora, or poke around a fresh Ubuntu.

<video
  controls
  muted
  playsInline
  preload="metadata"
  width="100%"
  poster={require('@site/static/img/linux-machines-video.jpg').default}
  src={require('@site/static/video/linux-machines.mp4').default}
/>

| Distribution | Versions |
|---|---|
| Ubuntu | 24.04, 26.04, 22.04 |
| Debian | 13, 12 |
| Fedora | 44, 43 |
| Alpine | 3.24, 3.23 |
| Arch Linux | latest (Intel and AMD only) |
| AlmaLinux (compatible with Red Hat) | 10, 9 |
| openSUSE Leap | 16.0, 15.6 |
| Amazon Linux | 2023, 2027 |

On Apple Silicon Macs, machines run the native arm64 build of each
distribution, so they run at full speed. Arch Linux has no arm64 build.

## Creating a machine

Click **Create…** on a distribution's card. The dialog asks for:

- **Name**, which is also the machine's hostname.
- **Version**.
- **Shared folder** (optional): a folder of your computer to work on in the
  machine. Click **Choose…** or type its path.
- **Throwaway**: delete the machine when it stops. Leave it off to keep the
  machine and everything in it until you remove it.

Ferrodock downloads the distribution if it isn't on your machine yet, then
sets the machine up:

- **You**, as a user with your login name, and `sudo` without a password.
- **bash, git, curl, nano** and a few other everyday tools.

The row in **Your machines** says how it's going. Setting up takes a few
seconds to a minute, as the tools are downloaded from the distribution's
mirrors. When it's ready, its shell opens.

## Working in a machine

**Open shell** opens a terminal in the machine, across the page. **←
Machines** goes back to the list. A shell keeps running while you look at
other pages.

From your own terminal, the same shell is:

```bash
docker exec -it <name> bash
```

**Stop** and **Start** keep everything you installed and changed. **Remove**
(the ✕ on its row) deletes the machine and everything in it.

## Sharing a folder

A shared folder shows up in your home folder in the machine, under its own
name: sharing `~/code/my-app` gives `~/my-app` in the machine, and that's
where its shell starts. Changes show on both sides right away. Files you
make in the machine belong to you on your computer, not to `root`.

Removing the machine leaves the folder on your computer as it is.

## What a machine is

A Linux machine is a container that keeps running, so it starts in seconds
and uses almost no memory. That also means:

- It shares the Linux kernel of your computer (or of the engine's virtual
  machine on macOS and Windows), so you can't try another kernel or kernel
  modules.
- It runs no desktop, and no `systemd`: services you install don't start by
  themselves. Start them by hand, for example `sudo nginx`.
- It shows up on the **Containers** page too, like any other container.

:::tip
Setup needs the internet from containers. If creating a machine says it
couldn't download its tools, check your network or proxy settings for
Docker.
:::
