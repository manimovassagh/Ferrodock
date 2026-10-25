---
sidebar_position: 6
title: Updates and releases
description: "Where to download Ferrodock, how its in-app updates work, and how versions and release notes are made."
---

# Updates and releases

## Where to download

Every version is on the
[Releases page](https://github.com/manimovassagh/Ferrodock/releases) on
GitHub, with release notes listing what was added, fixed and changed. The
full history is in
[CHANGELOG.md](https://github.com/manimovassagh/Ferrodock/blob/main/CHANGELOG.md).

Each release has these files:

| File | For |
|---|---|
| `Ferrodock-<version>-macos-universal.dmg` | macOS 11+, Apple Silicon and Intel |
| `Ferrodock-<version>-macos-universal.zip` | The same app, as a zip |
| `Ferrodock-<version>-windows-x64.zip` | Windows 10 and 11, x64 |
| `Ferrodock-<version>-linux-x64.tar.gz` | Linux, x64 |
| `SHA256SUMS.txt` | Checksums of all of the above |

To check a download, see [Installation](./installation.md#check-your-download).

## Updating in the app

Ferrodock checks for a new version when it starts. When there is one,
**Update to x.y.z** appears at the bottom of the sidebar. Hover it for the
release notes, and click it to update:

1. The new build for your platform downloads in the background.
2. It's checked against the release's `SHA256SUMS.txt`, and refused if it
   doesn't match.
3. It replaces the installed app in place: the whole `Ferrodock.app` on
   macOS, the executable on Windows and Linux.
4. Click **Restart to update** to switch to it.

In **Settings → Updates** you can check by hand with **Check now**, or turn
off **Check for updates when Ferrodock starts**. Ferrodock only ever offers
stable releases, never pre-releases.

## Updating with Homebrew

If you installed with Homebrew, you can also update with:

```bash
brew upgrade --cask ferrodock
```

This keeps Homebrew's records in step; Settings reminds you of it when
Ferrodock was installed this way.

## Version numbers

Ferrodock follows [Semantic Versioning](https://semver.org):

| Change | Example |
|---|---|
| Bug fixes | patch: 1.13.1 → 1.13.2 |
| New features | minor: 1.13.2 → 1.14.0 |
| Breaking changes | major: 1.14.0 → 2.0.0 |

**Pre-releases**, such as `2.0.0-rc.1`, are marked as pre-releases on
GitHub. They're never shown as the latest version, and the in-app updater
never offers them.

## Making a release

Releases are automated. If you maintain Ferrodock, see
[RELEASING.md](https://github.com/manimovassagh/Ferrodock/blob/main/RELEASING.md).
