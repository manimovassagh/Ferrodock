<div align="center">

<img src="assets/icon-256.png" alt="Ferrodock icon" width="96">

# Ferrodock

**Docker, at native speed.**<br>
A fast, free desktop app for your containers, images, volumes and networks,<br>
written entirely in Rust. For macOS, Windows and Linux.

[![Release](https://img.shields.io/github/v/release/manimovassagh/Ferrodock?display_name=tag&sort=semver)](https://github.com/manimovassagh/Ferrodock/releases/latest)
[![CI](https://github.com/manimovassagh/Ferrodock/actions/workflows/ci.yml/badge.svg)](https://github.com/manimovassagh/Ferrodock/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/ferrodock.svg)](https://crates.io/crates/ferrodock)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust 1.95+](https://img.shields.io/badge/rust-1.95%2B-orange.svg)](https://www.rust-lang.org)

**[Download](https://github.com/manimovassagh/Ferrodock/releases/latest)** ·
**[Website](https://manimovassagh.github.io/Ferrodock/)** ·
**[Documentation](https://manimovassagh.github.io/Ferrodock/docs)**

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/dark-mode.png">
  <source media="(prefers-color-scheme: light)" srcset="docs/screenshots/containers.png">
  <img src="docs/screenshots/containers.png" alt="Ferrodock showing the container list">
</picture>

</div>

Ferrodock is a single native app: no Electron, no web view, no background
service, and no CPU used while idle. It talks straight to the Docker Engine
API, and on Macs, Windows and Linux it can even run **its own Docker
engine**, so there's nothing else to install.

## Highlights

- **Containers** with live CPU and memory, one-click start / stop / pause /
  restart, clickable ports, bulk actions, and grouping by Compose project.
- **Logs, stats, terminal and files**: colourful, searchable live logs;
  CPU and memory charts; a real shell inside any container (even ones
  without a shell); and a file browser with upload and download.
- **Compose**: start a project from its `compose.yaml`, rebuild services,
  and read every service's logs together.
- **Images**: search Docker Hub as you type, pull with progress, run with a
  form that shows the `docker run` command, and tag, push, save or load.
- **Quick start**: Postgres, MySQL, Redis, MongoDB, RabbitMQ, MinIO, Mailpit
  or Nginx in one click, with the connection string ready to copy.
- **Volumes and networks**: create them, browse a volume's files, connect
  and disconnect containers.
- **Disk**: see where the space goes and free it in one step, with a preview
  of exactly what's deleted. Or start over with **Reset Docker**.
- **Command palette** (⌘K / Ctrl+K) and keyboard shortcuts for everything.
- **Any engine**: Ferrodock's own, a local `dockerd`, or any Docker context,
  including remote ones over TLS or SSH. Switch from the sidebar.
- **At home on your desktop**: light and dark themes, a menu bar /
  notification area icon, crash notifications, launch at login, and
  one-click updates.

<table>
  <tr>
    <td><img src="docs/screenshots/logs.png" alt="Live container logs"></td>
    <td><img src="docs/screenshots/stats.png" alt="Live CPU and memory stats"></td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/disk.png" alt="Disk usage and cleanup"></td>
    <td><img src="docs/screenshots/volumes.png" alt="Volumes page"></td>
  </tr>
</table>

## Install

**macOS** (with [Homebrew](https://brew.sh)):

```bash
brew tap manimovassagh/ferrodock https://github.com/manimovassagh/Ferrodock
brew install --cask ferrodock
```

Or download a build from the
**[latest release](https://github.com/manimovassagh/Ferrodock/releases/latest)**:

| Platform | Download | First launch |
|---|---|---|
| **macOS** 11+, Apple Silicon and Intel | `Ferrodock-<version>-macos-universal.dmg` (or `.zip`) | Drag **Ferrodock.app** to Applications. It isn't notarized yet, so allow it once with **Open Anyway** in System Settings → Privacy & Security (Homebrew does this for you). |
| **Windows** 10 / 11, x64 | `Ferrodock-<version>-windows-x64.zip` | Unzip and run **Ferrodock.exe**. If SmartScreen appears, choose **More info → Run anyway**. |
| **Linux** x64 | `Ferrodock-<version>-linux-x64.tar.gz` | `tar -xzf Ferrodock-*.tar.gz && ./ferrodock` |

Each release has a `SHA256SUMS.txt` to check your download. Ferrodock
updates itself: when a new version is out, click **Update to x.y.z** in the sidebar
(or run `brew upgrade --cask ferrodock`).

The [installation guide](https://manimovassagh.github.io/Ferrodock/docs/installation)
has the details for each platform.

## Ferrodock's own engine

When Ferrodock finds no Docker engine, click **Set up Ferrodock's engine**.
It downloads the engine once, and from then on it starts and stops with
Ferrodock.

- **Macs** (Apple Silicon and Intel): a small Linux VM with the Docker
  engine, run through Apple's Virtualization framework. Your home folder is
  shared, so bind mounts just work. On Apple Silicon, Intel-only
  (`linux/amd64`) images run too, through Rosetta.
- **Windows**: a WSL distro called `ferrodock`. WSL is part of Windows; if
  it's missing, Ferrodock installs it (with an administrator prompt).
- **Linux**: the Docker engine itself, as a system service. Setting it up
  asks for your admin password once; then it starts with your computer.

Already run Docker somewhere? Ferrodock connects to it through
`DOCKER_HOST` or your Docker contexts. See
[Ferrodock's own engine](https://manimovassagh.github.io/Ferrodock/docs/engine)
and [Connecting to other engines](https://manimovassagh.github.io/Ferrodock/docs/engines).

## Command line and AI tools

Ferrodock comes with two more programs, built on the same Rust core:

- **`fdock`**, the command line: `fdock ps`, `fdock logs web -f`,
  `fdock quickstart postgres`, `fdock df` and more. Every command takes
  `--json` for compact output that scripts and AI tools read cheaply.
- **`ferrodock-mcp`**, an [MCP](https://modelcontextprotocol.io) server, so
  AI tools can list containers, read logs, start services and clean up.
  Tools that change something ask first; `--read-only` leaves them out.

  ```bash
  claude mcp add ferrodock -- ferrodock-mcp
  ```

Homebrew puts both on your PATH; the Windows zip and Linux tarball have them
next to Ferrodock. See [Command line and AI tools](https://manimovassagh.github.io/Ferrodock/docs/cli-and-mcp).

## Documentation

Everything else lives on the **[documentation site](https://manimovassagh.github.io/Ferrodock/docs)**:
[getting started](https://manimovassagh.github.io/Ferrodock/docs/installation),
[using Ferrodock](https://manimovassagh.github.io/Ferrodock/docs/category/using-ferrodock),
[updates and releases](https://manimovassagh.github.io/Ferrodock/docs/releases)
and [troubleshooting](https://manimovassagh.github.io/Ferrodock/docs/troubleshooting).
What changed in each version is in [CHANGELOG.md](CHANGELOG.md).

## Build from source

You need [Rust](https://rustup.rs) 1.95 or newer (on macOS, also run
`xcode-select --install` once; on Windows, let the Rust installer set up the
Visual Studio C++ Build Tools).

```bash
git clone https://github.com/manimovassagh/Ferrodock.git
cd Ferrodock
cargo run --release
```

`cargo install ferrodock` works too. On macOS, `packaging/macos/bundle.sh`
builds a universal `Ferrodock.app`.

## Contributing

Bug reports, ideas and pull requests are welcome: start with an
[issue](https://github.com/manimovassagh/Ferrodock/issues/new/choose), and
see [CONTRIBUTING.md](CONTRIBUTING.md) for the development setup and
guidelines. Releases are automated; [RELEASING.md](RELEASING.md) explains
how. Please report security issues privately, as described in
[SECURITY.md](SECURITY.md).

## Author

Created and maintained by **Mani Movassagh**
([@manimovassagh](https://github.com/manimovassagh)).

## License

Ferrodock is released under the [MIT License](LICENSE). It's free to use for
personal, commercial and organisational projects, with no account,
subscription or fee. Every dependency is under a permissive open source
license, checked in CI. Ferrodock's own engine is built only from free and
open source software: see [engine/NOTICE.md](engine/NOTICE.md).
