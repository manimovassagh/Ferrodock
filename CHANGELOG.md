# Changelog

All notable changes to Ferrodock (called IronStack before 0.3.0) are documented
here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
