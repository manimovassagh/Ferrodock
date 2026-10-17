---
sidebar_position: 2
title: Installation
---

# Installation

Ferrodock needs a running Docker Engine: Docker Desktop on Windows and macOS,
or any other engine (Colima, Rancher Desktop, native `dockerd`, …).

## Download a release

Get the latest build from the
[Releases page](https://github.com/manimovassagh/Ferrodock/releases/latest).

### macOS

The quickest way is [Homebrew](https://brew.sh):

```bash
brew tap manimovassagh/ferrodock https://github.com/manimovassagh/Ferrodock
brew install --cask ferrodock
```

`brew upgrade --cask ferrodock` picks up new releases. To install by hand
instead:

1. Download `Ferrodock-<version>-macos-universal.dmg` (a `.zip` is also
   available). It runs natively on both Apple Silicon and Intel Macs,
   macOS 11 or newer.
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
thing.

Installed with Homebrew? Nothing to do: the cask allows the app for you.

### Windows

1. Download `Ferrodock-<version>-windows-x64.zip` and unzip it.
2. Run **Ferrodock.exe**. If SmartScreen appears, choose
   **More info → Run anyway**.

### Linux

```bash
tar -xzf Ferrodock-<version>-linux-x64.tar.gz
./ferrodock
```

Your user needs access to the Docker socket, usually by being in the `docker`
group.

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

On macOS, `packaging/macos/bundle.sh` builds a universal `Ferrodock.app` in
`dist/`.

## Updating

Ferrodock checks for a new version when it starts. When one is available,
**Update to x.y.z** appears at the bottom of the sidebar. Hover it for the
release notes, and click it to update:

1. The new build for your platform downloads in the background.
2. It's checked against the release's `SHA256SUMS.txt`, and refused if
   anything doesn't match.
3. It replaces the installed app in place (the whole `Ferrodock.app` on
   macOS).
4. Click **Restart to update** to switch to it.

You can also check by hand, or turn the automatic check off, in
**Settings → Updates**. Ferrodock only ever offers stable releases, never
pre-releases.

If you installed with Homebrew, `brew upgrade --cask ferrodock` works too.

