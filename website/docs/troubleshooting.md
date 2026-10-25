---
sidebar_position: 7
title: Troubleshooting and FAQ
description: "Fixes for common Ferrodock problems: opening the app on macOS and Windows, connecting to Docker, Compose, and the built-in engine."
---

# Troubleshooting and FAQ

## Installing and opening

**macOS says Ferrodock "can't be opened" or "is damaged".**
Ferrodock isn't notarized by Apple yet. Allow it once with **Open Anyway**
in System Settings → Privacy & Security, or run
`xattr -dr com.apple.quarantine /Applications/Ferrodock.app`. See
[Installation](./installation.md#macos).

**Windows shows a SmartScreen warning.**
Choose **More info → Run anyway**.

## Connecting to Docker

**"Docker isn't running".**
Ferrodock can't reach the engine. Click **Start**, or start the engine
yourself; Ferrodock keeps trying every few seconds and connects as soon as
it's up. On Linux: `sudo systemctl start ferrodock-engine` for
Ferrodock's own engine, or `sudo systemctl start docker`. Check that the right
engine is picked in the [engine list](./engines.md#switching-engines).

**"Docker won't let Ferrodock in".**
The engine runs, but your user may not use it. On Linux, add yourself to
the `docker` group, then log out and back in:

```bash
sudo usermod -aG docker $USER
```

**"No Docker engine found".**
Ferrodock found no engine to start. See
[Setting up an engine](./engines.md#setting-up-an-engine).

## Ferrodock's own engine

**Setting up the engine on Windows fails while installing WSL.**
Ferrodock installs WSL itself, which needs administrator approval. If that
doesn't work, run `wsl --install --no-distribution` in a terminal opened as
administrator, then restart Windows and try again. WSL needs Windows 10
version 2004 or later, or Windows 11.

**The engine doesn't start.**
The engine's log is `console.log` in `~/.ferrodock` (macOS) or
`%LOCALAPPDATA%\Ferrodock` (Windows). Please attach it if you
[report a problem](https://github.com/manimovassagh/Ferrodock/issues/new/choose).

**A bind mount shows up empty.**
On a Mac, only your home folder is shared with the engine; move the folder
there. On Windows, use the path inside WSL, such as
`/mnt/c/Users/me/project`, not `C:\Users\me\project`.

**Pulling images fails behind a company proxy.**
On Windows, the engine trusts the certificates Windows trusts, so make sure
your company's certificate is installed in Windows. Then quit and reopen
Ferrodock so the engine picks it up.

**How do I use the `docker` command with Ferrodock's engine?**
See the [macOS](./engine.md#macos) and [Windows](./engine.md#windows)
sections of Ferrodock's own engine.

**How do I remove the engine and its data?**
See [Removing the engine](./engine.md#removing-the-engine).

## Using Ferrodock

**Compose's Up, Rebuild or Logs fail.**
They run Docker Compose: the one on your computer if there is one,
otherwise a copy Ferrodock downloads once (see
[Compose](./using/compose.md)). The error under the project says what
Compose reported. If the download itself fails (offline, or a proxy in the
way), installing Docker Compose yourself works too.

**Pulling or pushing a private image fails.**
Ferrodock uses the logins of the Docker CLI. Run `docker login <registry>`
once, then try again.

**The Disk page says the engine doesn't report disk usage.**
That page needs Docker Engine 29 or newer.

**The debug shell can't pull `busybox`.**
Set `FERRODOCK_DEBUG_IMAGE` to an image you can reach, such as one on your
own registry, before starting Ferrodock.

**I closed the window but Ferrodock still runs.**
On macOS and Windows, Ferrodock keeps running in the menu bar or
notification area. Use **Quit Ferrodock** in its menu, or turn off **Keep
running there when the window is closed** in
[Settings](./using/settings.md).

## Updates

**"Couldn't check for updates".**
Ferrodock asks GitHub for new releases. Check your internet connection, or
download the new version from the
[Releases page](https://github.com/manimovassagh/Ferrodock/releases/latest).

## Still stuck?

[Open an issue](https://github.com/manimovassagh/Ferrodock/issues/new/choose)
with your OS, your Docker engine and the Ferrodock version (in Settings →
About).
