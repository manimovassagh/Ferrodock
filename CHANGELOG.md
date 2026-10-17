# Changelog

All notable changes to Ferrodock (called IronStack before 0.3.0) are documented
here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.6.0](https://github.com/manimovassagh/Ferrodock/compare/v0.5.0...v0.6.0) - 2026-10-17

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

### Documentation

- explain how to open the app on macOS 15 and newer ([#64](https://github.com/manimovassagh/Ferrodock/pull/64))
- move the roadmap out of the README into issues ([#63](https://github.com/manimovassagh/Ferrodock/pull/63))
- add a landing page for the website ([#61](https://github.com/manimovassagh/Ferrodock/pull/61))

### Fixed

- publish the Homebrew cask and allow the app on install ([#65](https://github.com/manimovassagh/Ferrodock/pull/65))

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
