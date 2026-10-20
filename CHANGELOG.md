# Changelog

All notable changes to Ferrodock (called IronStack before 0.3.0) are documented
here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.28.3](https://github.com/manimovassagh/Ferrodock/compare/v1.28.2...v1.28.3) - 2026-10-20

### Documentation

- AI skills to install Ferrodock and connect it to AI tools; a tighter landing hero

## [1.28.2](https://github.com/manimovassagh/Ferrodock/compare/v1.28.1...v1.28.2) - 2026-10-20

### Other

- Merge pull request #185 from manimovassagh/claude/demo-gif
- Add a demo animation of the real app to the README and site hero ([#166](https://github.com/manimovassagh/Ferrodock/pull/166))

## [1.28.1](https://github.com/manimovassagh/Ferrodock/compare/v1.28.0...v1.28.1) - 2026-10-20

### Documentation

- no full stops in headlines
- a comparison page with team pricing, download sizes and engine memory, plus the benchmark script

## [1.28.0](https://github.com/manimovassagh/Ferrodock/compare/v1.27.2...v1.28.0) - 2026-10-20

### Added

- *(engine)* engine 0.3.0, with Rosetta, Intel Macs and growing data disks

### Fixed

- keep dialogs within the window and scroll their bodies ([#162](https://github.com/manimovassagh/Ferrodock/pull/162))
- quit the old Ferrodock on Restart to update, and allow only one to run ([#163](https://github.com/manimovassagh/Ferrodock/pull/163))

## [1.27.2](https://github.com/manimovassagh/Ferrodock/compare/v1.27.1...v1.27.2) - 2026-10-20

### Documentation

- lead with the native, cross-platform, own-engine story and compare honestly

## [1.27.1](https://github.com/manimovassagh/Ferrodock/compare/v1.27.0...v1.27.1) - 2026-10-20

### Other

- Add six more editor themes: Dracula, One Dark/Light, GitHub Dark/Light, Solarized Light

## [1.27.0](https://github.com/manimovassagh/Ferrodock/compare/v1.26.0...v1.27.0) - 2026-10-20

### Added

- *(cli, mcp)* engine start, clean, reset and compose ([#116](https://github.com/manimovassagh/Ferrodock/pull/116))

### Other

- Merge pull request #172 from manimovassagh/claude/cli-engine-compose

## [1.26.0](https://github.com/manimovassagh/Ferrodock/compare/v1.25.0...v1.26.0) - 2026-10-20

### Added

- *(engine)* Ferrodock's own engine on Intel Macs
- *(engine)* run x86_64 images on Apple Silicon through Rosetta

### Other

- Merge pull request #165 from manimovassagh/claude/engine-mac-rosetta-intel
- Merge remote-tracking branch 'origin/main' into claude/engine-mac-rosetta-intel

## [1.25.0](https://github.com/manimovassagh/Ferrodock/compare/v1.24.0...v1.25.0) - 2026-10-20

### Added

- *(engine)* start, stop, reset and disk size in Settings ([#114](https://github.com/manimovassagh/Ferrodock/pull/114))

### Other

- Merge remote-tracking branch 'origin/main' into claude/engine-controls
- Merge remote-tracking branch 'origin/main' into claude/engine-controls

## [1.24.0](https://github.com/manimovassagh/Ferrodock/compare/v1.23.0...v1.24.0) - 2026-10-20

### Added

- engine image updates and a `ferrodock` Docker context

### Fixed

- count installs from before version records as engine 0.2.0

### Other

- Merge remote-tracking branch 'origin/main' into claude/engine-context-updates

## [1.23.0](https://github.com/manimovassagh/Ferrodock/compare/v1.22.0...v1.23.0) - 2026-10-20

### Added

- *(engine)* install WSL on Windows when setting up the engine

### Other

- Merge pull request #158 from manimovassagh/claude/engine-wsl-install

## [1.22.0](https://github.com/manimovassagh/Ferrodock/compare/v1.21.0...v1.22.0) - 2026-10-20

### Added

- put fdock and ferrodock-mcp on the PATH

## [1.21.0](https://github.com/manimovassagh/Ferrodock/compare/v1.20.0...v1.21.0) - 2026-10-20

### Added

- MCP setup for Claude Code, Codex, GitHub Copilot, Gemini CLI, Cursor, Windsurf and Claude Desktop
- editor themes: Tokyo Night, Catppuccin, Gruvbox, Nord and Rosé Pine

### Other

- Merge branch 'claude/sidebar-ai' into claude/themes

## [1.20.0](https://github.com/manimovassagh/Ferrodock/compare/v1.19.0...v1.20.0) - 2026-10-20

### Added

- CLI & MCP card in the sidebar, with a setup dialog
- Compose works without the docker command

## [1.19.0](https://github.com/manimovassagh/Ferrodock/compare/v1.18.0...v1.19.0) - 2026-10-20

### Added

- Settings → Ferrodock's engine: CPUs, memory, restart and log
- published ports on localhost with Ferrodock's engine on macOS

### Other

- Merge remote-tracking branch 'origin/main' into claude/mac-localhost-ports
- Merge pull request #146 from manimovassagh/claude/engine-settings

## [1.18.0](https://github.com/manimovassagh/Ferrodock/compare/v1.17.0...v1.18.0) - 2026-10-20

### Added

- Compose Open… button with the system file picker

## [1.17.0](https://github.com/manimovassagh/Ferrodock/compare/v1.16.0...v1.17.0) - 2026-10-20

### Added

- ship fdock and ferrodock-mcp with every download
- ferrodock-mcp, an MCP server for AI tools

### Other

- Merge remote-tracking branch 'origin/main' into claude/mcp-server

## [1.16.0](https://github.com/manimovassagh/Ferrodock/compare/v1.15.1...v1.16.0) - 2026-10-20

### Added

- fdock, Ferrodock on the command line

### Other

- Merge pull request #141 from manimovassagh/claude/elegant-hopper-qhtyv6

## [1.15.1](https://github.com/manimovassagh/Ferrodock/compare/v1.15.0...v1.15.1) - 2026-10-20

### Changed

- move the Docker layer into the ferrodock-core crate

## [1.15.0](https://github.com/manimovassagh/Ferrodock/compare/v1.14.0...v1.15.0) - 2026-10-19

### Added

- Ferrodock's own engine on Linux

### Documentation

- Ferrodock's own engine on Linux
- refresh the README and rewrite the docs site as user documentation

## [1.14.0](https://github.com/manimovassagh/Ferrodock/compare/v1.13.2...v1.14.0) - 2026-10-19

### Added

- the Windows engine trusts what Windows trusts
- Ferrodock's own engine on Windows, in a WSL distro

### Fixed

- use as_chunks for wsl.exe's UTF-16 output (newer clippy)

## [1.13.1](https://github.com/manimovassagh/Ferrodock/compare/v1.13.0...v1.13.1) - 2026-10-19

### Fixed

- ship engine/VERSION in the crate so crates.io publishing works

## [1.13.0](https://github.com/manimovassagh/Ferrodock/compare/v1.12.0...v1.13.0) - 2026-10-19

### Added

- Ferrodock's own engine on Apple Silicon Macs

### Documentation

- no other engine products named; lead with Ferrodock's own engine

### Fixed

- never name other products in the app; share the home folder on a read-only root

## [1.12.0](https://github.com/manimovassagh/Ferrodock/compare/v1.11.1...v1.12.0) - 2026-10-19

### Added

- ferrodock-agent and the engine image for Apple Silicon

### Fixed

- build the VM agent as its own crate so releases work again

## [1.11.1](https://github.com/manimovassagh/Ferrodock/compare/v1.11.0...v1.11.1) - 2026-10-18

### Fixed

- no other products named; find the engine's app from the engine

## [1.11.0](https://github.com/manimovassagh/Ferrodock/compare/v1.10.1...v1.11.0) - 2026-10-18

### Added

- Quick start templates, Run from Docker Hub, menu bar sections

## [1.10.0](https://github.com/manimovassagh/Ferrodock/compare/v1.9.0...v1.10.0) - 2026-10-18

### Added

- start the Docker engine when it isn't running
- select all, bulk remove and Reset Docker

## [1.9.0](https://github.com/manimovassagh/Ferrodock/compare/v1.8.0...v1.9.0) - 2026-10-18

### Added

- search Docker Hub as you type in the Pull box

## [1.8.0](https://github.com/manimovassagh/Ferrodock/compare/v1.7.0...v1.8.0) - 2026-10-18

### Added

- group the container list by Compose project
- system notifications when a container crashes

## [1.7.0](https://github.com/manimovassagh/Ferrodock/compare/v1.6.0...v1.7.0) - 2026-10-17

### Added

- open Ferrodock at login
- Compose dev loop: up from a file, rebuild services, merged logs
- switch between Docker engines and contexts from the sidebar
- tag, push, save and load images, and search Docker Hub
- debug shell for containers without a shell

## [1.6.0](https://github.com/manimovassagh/Ferrodock/compare/v1.5.0...v1.6.0) - 2026-10-17

### Added

- menu bar and notification area icon with a container menu

### Other

- Merge pull request #104 from manimovassagh/claude/elegant-hopper-qhtyv6

## [1.5.0](https://github.com/manimovassagh/Ferrodock/compare/v1.4.0...v1.5.0) - 2026-10-17

### Added

- browse a volume's files, read-only ([#103](https://github.com/manimovassagh/Ferrodock/pull/103))
- open a shell inside a container in the Terminal tab ([#101](https://github.com/manimovassagh/Ferrodock/pull/101))

## [1.4.0](https://github.com/manimovassagh/Ferrodock/compare/v1.3.0...v1.4.0) - 2026-10-17

### Added

- browse a container's files, preview them, and copy files in and out ([#98](https://github.com/manimovassagh/Ferrodock/pull/98))

## [1.3.0](https://github.com/manimovassagh/Ferrodock/compare/v1.2.0...v1.3.0) - 2026-10-17

### Added

- copy values with a click, read charts on hover, and a more actions menu ([#95](https://github.com/manimovassagh/Ferrodock/pull/95))

## [1.2.0](https://github.com/manimovassagh/Ferrodock/compare/v1.1.1...v1.2.0) - 2026-10-17

### Added

- edit and recreate containers, and change their CPU and memory limits ([#92](https://github.com/manimovassagh/Ferrodock/pull/92))

## [1.1.1](https://github.com/manimovassagh/Ferrodock/compare/v1.1.0...v1.1.1) - 2026-10-17

### Documentation

- download buttons for macOS, Windows and Linux on the landing page ([#89](https://github.com/manimovassagh/Ferrodock/pull/89))

## [1.1.0](https://github.com/manimovassagh/Ferrodock/compare/v1.0.0...v1.1.0) - 2026-10-17

### Added

- rename, duplicate and change the restart policy of containers ([#88](https://github.com/manimovassagh/Ferrodock/pull/88))
- link related items and label chart scales ([#87](https://github.com/manimovassagh/Ferrodock/pull/87))
- live CPU and memory for every container in the list ([#86](https://github.com/manimovassagh/Ferrodock/pull/86))
- select several containers and act on them at once ([#84](https://github.com/manimovassagh/Ferrodock/pull/84))

## [1.0.0](https://github.com/manimovassagh/Ferrodock/compare/v0.5.0...v1.0.0) - 2026-10-17

The first stable release. Ferrodock gets a new look, keeps itself up to
date, reacts to Docker the moment something changes, and can now pull and
run images, inspect anything, and manage volumes and networks.

### Added

- create volumes and networks, connect containers to networks ([#82](https://github.com/manimovassagh/Ferrodock/pull/82))
- processes tab and image history ([#81](https://github.com/manimovassagh/Ferrodock/pull/81))
- friendly empty, loading and no-Docker states ([#80](https://github.com/manimovassagh/Ferrodock/pull/80))
- colourful, searchable logs with timestamps and save ([#79](https://github.com/manimovassagh/Ferrodock/pull/79))
- instant updates from Docker events, with crash alerts ([#78](https://github.com/manimovassagh/Ferrodock/pull/78))
- update Ferrodock from inside the app ([#77](https://github.com/manimovassagh/Ferrodock/pull/77))
- pull images and run them as containers ([#76](https://github.com/manimovassagh/Ferrodock/pull/76))
- modern look with vector icons and the Inter typeface ([#75](https://github.com/manimovassagh/Ferrodock/pull/75))
- settings for theme, density and interface size ([#74](https://github.com/manimovassagh/Ferrodock/pull/74))
- inspect containers, images, volumes and networks ([#66](https://github.com/manimovassagh/Ferrodock/pull/66))

### Fixed

- icons showed as empty boxes, because egui no longer bundles symbol fonts ([#75](https://github.com/manimovassagh/Ferrodock/pull/75))
- publish the Homebrew cask and allow the app on install ([#65](https://github.com/manimovassagh/Ferrodock/pull/65))

### Documentation

- explain how to open the app on macOS 15 and newer ([#64](https://github.com/manimovassagh/Ferrodock/pull/64))
- move the roadmap out of the README into issues ([#63](https://github.com/manimovassagh/Ferrodock/pull/63))
- add a landing page for the website ([#61](https://github.com/manimovassagh/Ferrodock/pull/61))

## [0.5.0](https://github.com/manimovassagh/Ferrodock/compare/v0.4.1...v0.5.0) - 2026-10-17

### Added

- disk usage and clean-up page ([#60](https://github.com/manimovassagh/Ferrodock/pull/60))
- command palette and keyboard shortcuts ([#58](https://github.com/manimovassagh/Ferrodock/pull/58))
- open published ports in the browser, pause and resume ([#57](https://github.com/manimovassagh/Ferrodock/pull/57))
- DMG and Homebrew install for macOS ([#38](https://github.com/manimovassagh/Ferrodock/pull/38))

### Documentation

- mention the DMG and Homebrew in release notes ([#59](https://github.com/manimovassagh/Ferrodock/pull/59))

## [0.4.1](https://github.com/manimovassagh/Ferrodock/compare/v0.4.0...v0.4.1) - 2026-10-17

### Documentation

- refresh the README intro and roadmap ([#36](https://github.com/manimovassagh/Ferrodock/pull/36))

## [0.4.0](https://github.com/manimovassagh/Ferrodock/compare/v0.3.0...v0.4.0) - 2026-10-17

### Added

- new app icon ([#33](https://github.com/manimovassagh/Ferrodock/pull/33))

### Documentation

- document releases and release notes ([#34](https://github.com/manimovassagh/Ferrodock/pull/34))

## [0.3.0](https://github.com/manimovassagh/Ferrodock/compare/v0.2.1...v0.3.0) - 2026-10-17

### Changed

- [**breaking**] Renamed from IronStack to **Ferrodock**: the app, the binary
  (`ferrodock`), `Ferrodock.exe`, `Ferrodock.app` and the download files
  ([#31](https://github.com/manimovassagh/Ferrodock/pull/31))
- Releases are automated: every release has its changelog as release notes,
  downloads for all platforms and a `SHA256SUMS.txt` file
  ([#30](https://github.com/manimovassagh/Ferrodock/pull/30))

## [0.2.1] - 2026-10-17

### Added
- Documentation website (Docusaurus), published to GitHub Pages.

### Changed
- Ferrodock now uses no CPU while idle. The background worker checks
  Docker on its own timer and only wakes the UI when something actually
  changed, instead of the UI redrawing every few seconds.

## [0.2.0] - 2026-10-17

### Added
- **Compose** page: containers grouped by Docker Compose project, with
  start, stop, restart and remove for the whole project.
- **Logs** tab for containers: the last 500 lines, then live output, with a
  filter and copy.
- **Stats** tab for containers: live CPU and memory charts, network and disk
  totals.

### Changed
- Source tree reorganised: `src/` holds only `main.rs` plus the `app`,
  `docker`, `theme` and `worker` modules.

## [0.1.0] - 2026-10-17

### Added
- Containers page: list, filter (All / Running / Stopped), search, start,
  stop, restart and remove.
- Images, Volumes and Networks pages with "in use" information and
  removal, including "Remove unused".
- Details pane for every item, with copyable IDs.
- Confirmation dialog before anything is deleted.
- Light and dark themes that follow the system setting.
- Automatic reconnection when Docker starts after Ferrodock.
- App icon, and release builds for Windows, macOS (universal) and Linux.

[Unreleased]: https://github.com/manimovassagh/Ferrodock/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/manimovassagh/Ferrodock/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/manimovassagh/Ferrodock/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/manimovassagh/Ferrodock/releases/tag/v0.1.0
