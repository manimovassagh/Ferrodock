# Changelog

All notable changes to IronStack are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
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
- Automatic reconnection when Docker starts after IronStack.
- App icon, and release builds for Windows, macOS (universal) and Linux.

[Unreleased]: https://github.com/manimovassagh/IronStack/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/manimovassagh/IronStack/releases/tag/v0.1.0
