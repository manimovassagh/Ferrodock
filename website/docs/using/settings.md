---
sidebar_position: 11
title: Settings, menu bar and startup
description: "Ferrodock's settings: themes, density, interface size, the menu bar or tray icon, and opening at login."
---

# Settings, menu bar and startup

## Settings

Click **Settings** at the bottom of the sidebar, or press **⌘,** (Ctrl+, on
Windows and Linux). Settings, the window size and its position are
remembered between runs. **Reset to defaults** puts everything back.

### Appearance

- **Theme**: Light (the default), Dark, or System to follow your operating
  system.
- **Editor themes**: the colours of popular code editor themes, to match
  your editor and terminal: Tokyo Night, Catppuccin Mocha, Catppuccin
  Latte, Gruvbox Dark, Nord, Rosé Pine, Dracula, One Dark, One Light,
  GitHub Dark, GitHub Light and Solarized Light. Pick Light, Dark or
  System to go back to Ferrodock's own.
- **Density**: Comfortable shows two lines per row; Compact fits about
  twice as many rows on screen.
- **Interface size**: 90% to 125%.

### Notifications

When a container crashes or runs out of memory while Ferrodock is in the
background (or hidden in the menu bar), you get a system notification. On
by default.

### Startup

- **Start the Docker engine when Ferrodock opens, if it isn't running** (on
  by default). See [When Docker isn't running](../engines.md#when-docker-isnt-running).
- **Open Ferrodock when you log in**. On macOS and Windows it then starts
  in the menu bar / notification area, without opening the window.

### Ferrodock's engine

Shown once [Ferrodock's own engine](../engine.md) is set up.

- Whether it's running.
- **CPUs**, **Memory** and **Disk** (macOS): what the engine's VM gets. The
  default is half your CPU cores (2 to 8), 4 GB of memory and a 128 GB
  disk; the disk can be 32, 64, 128 or 256 GB and only takes the space
  that's used. Changes take effect when the engine restarts. The disk only
  grows: to make it smaller, pick the size, then **Reset engine…**.
- **Start engine** / **Stop engine**. On Linux, where the engine is a
  system service, stopping it asks for your admin password, and it stays
  stopped until you start it again or restart the computer.
- **Restart engine** (macOS and Windows), and **Open log**, the engine's
  `console.log`. On Linux its log is `journalctl -u ferrodock-engine`.
- **Reset engine…** deletes everything in the engine: all containers,
  images, volumes (with their data), networks and build cache. You type
  `reset` to confirm. The engine then starts again, empty, as on first use:
  - macOS: its disk, `~/.ferrodock/data.img`, is deleted and made again at
    the chosen size.
  - Windows: the `ferrodock` WSL distro is removed and set up again, which
    downloads the engine again.
  - Linux: its data folder, `/var/lib/ferrodock`, is deleted. This asks for
    your admin password.

### Menu bar / Notification area

**Keep running there when the window is closed** (macOS and Windows). Turn
it off to quit when you close the window.

### Registries

Sign in to a registry to push your images and pull private ones. Leave
**Registry** empty for Docker Hub, or type one such as `ghcr.io`; use a
password, or an access token where the registry offers one (Docker Hub and
GitHub do).

The login is checked with the registry first, then kept where the Docker
CLI keeps it: in your system's keychain through a credential helper
(`docker-credential-osxkeychain`, `-wincred`, `-secretservice` or `-pass`)
when one is set up or installed, otherwise in `~/.docker/config.json`,
readable only by you. So `docker`, Compose and Ferrodock all use the same
logins. **Sign out** forgets one.

### Updates

**Check for updates when Ferrodock starts**, and **Check now**. See
[Updates and releases](../releases.md).

### About

The version you run, and links to the website, GitHub and **Report a
problem**.

## Menu bar and notification area

On macOS, Ferrodock puts an icon in the menu bar showing how many
containers are running; on Windows it's in the notification area. Its menu
follows the sidebar:

- **Open Ferrodock**, and whether Docker is running.
- **Containers**: each container (running ones first, ● running, ○
  stopped) with **Start**, **Stop**, **Restart** and **Show in Ferrodock**.
- **Compose**: each project with **Start all** / **Stop all**.
- **Images**, **Volumes** and **Networks**: each lists its items. Click one
  to open it in Ferrodock.
- **Quick start**: lists the services. Click one to run it right away with
  its defaults. **Customize…** opens the [Quick start](./quick-start.md)
  page.
- **Quit Ferrodock**.

Closing the window keeps Ferrodock running there, so the icon stays;
**Quit Ferrodock** in its menu really quits. On Linux there's no icon yet,
so closing the window quits.

## Launch at login

**Open Ferrodock when you log in** (in Settings → Startup) works on every
platform: a login item on macOS, a startup entry on Windows, and an
autostart entry on Linux. Turn it off in the same place.
