---
sidebar_position: 9
title: Settings, menu bar and startup
---

# Settings, menu bar and startup

## Settings

Click **Settings** at the bottom of the sidebar, or press **⌘,** (Ctrl+, on
Windows and Linux). Settings, the window size and its position are
remembered between runs. **Reset to defaults** puts everything back.

### Appearance

- **Theme**: Light (the default), Dark, or System to follow your operating
  system.
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

### Menu bar / Notification area

**Keep running there when the window is closed** (macOS and Windows). Turn
it off to quit when you close the window.

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
