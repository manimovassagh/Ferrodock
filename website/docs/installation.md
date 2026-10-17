---
sidebar_position: 2
title: Installation
---

# Installation

IronStack needs a running Docker Engine: Docker Desktop on Windows and macOS,
or any other engine (Colima, Rancher Desktop, native `dockerd`, …).

## Download a release

Get the latest build from the
[Releases page](https://github.com/manimovassagh/IronStack/releases/latest).

### macOS

1. Download `IronStack-<version>-macos-universal.zip`. It runs natively on
   both Apple Silicon and Intel Macs, macOS 11 or newer.
2. Unzip it and move **IronStack.app** to **Applications**.
3. The first time, **right-click the app → Open**, then confirm. IronStack
   isn't notarized by Apple yet, so a plain double-click is blocked once.

If macOS still refuses, clear the quarantine flag from a terminal:

```bash
xattr -cr /Applications/IronStack.app
```

### Windows

1. Download `IronStack-<version>-windows-x64.zip` and unzip it.
2. Run **IronStack.exe**. If SmartScreen appears, choose
   **More info → Run anyway**.

### Linux

```bash
tar -xzf IronStack-<version>-linux-x64.tar.gz
./ironstack
```

Your user needs access to the Docker socket, usually by being in the `docker`
group.

## Build from source

You need [Rust](https://rustup.rs) 1.95 or newer.

- **macOS:** run `xcode-select --install` once for Apple's build tools.
- **Windows:** let the Rust installer set up the Visual Studio C++ Build
  Tools.

```bash
git clone https://github.com/manimovassagh/IronStack.git
cd IronStack
cargo run --release
```

On macOS, `packaging/macos/bundle.sh` builds a universal `IronStack.app` in
`dist/`.
