---
sidebar_position: 2
title: Installation
---

# Installation

Ferrodock runs on macOS 11 or newer (Apple Silicon and Intel), Windows 10
and 11 (x64), and Linux (x64). Downloads are on the
[Releases page](https://github.com/manimovassagh/Ferrodock/releases/latest).

## Let your AI assistant install it

Not sure how to install apps from a terminal? If you use an AI assistant
that can run commands, such as Claude Code, Codex, GitHub Copilot or Cursor,
give it this:

```text
Install Ferrodock for me, following https://manimovassagh.github.io/Ferrodock/install.md
```

It installs from the official release, checks the download's SHA-256, and
asks you before anything that needs your password or changes a security
setting. Then it can connect itself to Ferrodock's
[MCP server](./cli-and-mcp.md), so it can manage your containers too.

The guide is an Agent Skill in the repository's
[`skills/`](https://github.com/manimovassagh/Ferrodock/tree/main/skills)
folder, next to one that connects an AI tool to Ferrodock. To keep them in
Claude Code, copy their folders into `~/.claude/skills/`; then asking
"install Ferrodock" is enough.

## macOS

The quickest way is [Homebrew](https://brew.sh):

```bash
brew tap manimovassagh/ferrodock https://github.com/manimovassagh/Ferrodock
brew install --cask ferrodock
```

To install by hand instead:

1. Download `Ferrodock-<version>-macos-universal.dmg` (a `.zip` is also
   available). One app runs natively on both Apple Silicon and Intel.
2. Open the DMG and drag **Ferrodock.app** to **Applications**.
3. Ferrodock isn't notarized by Apple yet, so macOS blocks the first launch.
   Allow it once, either from a terminal:

   ```bash
   xattr -dr com.apple.quarantine /Applications/Ferrodock.app
   ```

   or in **System Settings → Privacy & Security**: double-click Ferrodock,
   click **Done** when it's blocked, then click **Open Anyway** next to
   *"Ferrodock" was blocked* and confirm.

On macOS 14 and older, **right-click the app → Open** also works. If macOS
says the app "is damaged", run the `xattr` command above; it means the same
thing. Installed with Homebrew? Nothing to do: the cask allows the app for
you.

**Docker engine:** click **Set up Ferrodock's engine** on first launch.
See [Ferrodock's own engine](./engine.md). Ferrodock also connects to a
Docker engine you already run; see
[Connecting to other engines](./engines.md).

## Windows

1. Download `Ferrodock-<version>-windows-x64.zip` and unzip it.
2. Run **Ferrodock.exe**. If SmartScreen appears, choose
   **More info → Run anyway**.
3. Click **Set up Ferrodock's engine**. Ferrodock runs Docker in WSL,
   which is part of Windows. If WSL isn't installed yet, Ferrodock installs
   it first (Windows asks for administrator approval); restart Windows when
   Ferrodock asks, then open it again to finish.

More in [Ferrodock's own engine](./engine.md#windows).

## Linux

```bash
tar -xzf Ferrodock-<version>-linux-x64.tar.gz
./ferrodock
```

When Ferrodock finds no Docker engine, click **Set up Ferrodock's engine**.
It installs the Docker engine as a system service and asks for your admin
password once. More in [Ferrodock's own engine](./engine.md#linux).

Already have Docker installed? Ferrodock uses it. Make sure your user may
use it, usually by being in the `docker` group:

```bash
sudo usermod -aG docker $USER   # then log out and back in
```

If the engine isn't running when Ferrodock opens, Ferrodock starts the
`docker` service for you (it may ask for your password).

## Check your download

Every release has a `SHA256SUMS.txt`. Run this in the folder you downloaded
to:

```bash
shasum -a 256 -c SHA256SUMS.txt --ignore-missing   # macOS
sha256sum -c SHA256SUMS.txt --ignore-missing       # Linux
```

On Windows, run `Get-FileHash .\Ferrodock-<version>-windows-x64.zip` in
PowerShell and compare the hash with the line in `SHA256SUMS.txt`.

## Build from source

You need [Rust](https://rustup.rs) 1.95 or newer.

- **macOS:** run `xcode-select --install` once for Apple's build tools.
- **Windows:** let the Rust installer set up the Visual Studio C++ Build
  Tools.

```bash
git clone https://github.com/manimovassagh/Ferrodock.git
cd Ferrodock
cargo run --release
```

`cargo install ferrodock` installs the latest release from crates.io. On
macOS, `packaging/macos/bundle.sh` builds a universal `Ferrodock.app` in
`dist/`.

## Updating

Ferrodock checks for new versions itself and updates in one click. See
[Updates and releases](./releases.md).

## Uninstalling

If you turned on **Open Ferrodock when you log in**, turn it off in
Settings first. Then:

- **macOS:** quit Ferrodock and move **Ferrodock.app** to the Trash, or run
  `brew uninstall --cask ferrodock`.
- **Windows:** quit Ferrodock (from its icon in the notification area) and
  delete the folder you unzipped it to.
- **Linux:** delete the `ferrodock` executable.

If you set up Ferrodock's own engine, it keeps its images, containers and
volumes separately. [Removing the engine](./engine.md#removing-the-engine)
explains how to delete them too.
