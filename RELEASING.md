# Releasing IronStack

Releases are automated with [release-plz](https://release-plz.dev) and
GitHub Actions. Nobody edits version numbers or the changelog by hand.

## How a release happens

1. **Merge pull requests into `main` as usual.** PRs are squash-merged, and
   their titles follow [Conventional Commits](https://www.conventionalcommits.org)
   (a check enforces this).
2. **release-plz keeps a release PR open**, titled `chore: release vX.Y.Z`.
   On every merge it updates that PR: the new version in `Cargo.toml`, and
   the new entries in `CHANGELOG.md`.
3. **When you want to ship, merge the release PR.** The
   [Release workflow](.github/workflows/release.yml) then:
   1. pushes the `vX.Y.Z` tag and creates a **draft** GitHub Release with
      the changelog as release notes;
   2. builds Windows, macOS (universal) and Linux binaries
      ([release-assets.yml](.github/workflows/release-assets.yml));
   3. attaches them, along with `SHA256SUMS.txt`, and **publishes** the
      release.

Users never see a release without downloads: it stays a draft until every
platform has built.

## How the version is chosen

[Semantic Versioning](https://semver.org), worked out from the commit
types since the last tag:

| Commit / PR title | Version bump | Changelog section |
|---|---|---|
| `feat: …` | minor (0.2.1 → 0.3.0) | Added |
| `fix: …` | patch (0.2.1 → 0.2.2) | Fixed |
| `perf: …` | patch | Performance |
| `refactor: …` | patch | Changed |
| `docs: …` | patch | Documentation |
| `feat!: …` or a `BREAKING CHANGE:` footer | major (minor before 1.0) | marked **breaking** |
| `chore:`, `ci:`, `build:`, `test:`, `style:` | none on their own | not listed |

Only commits that touch the app's files count. A docs-site-only change,
for example, doesn't trigger a release.

To edit the release notes, change `CHANGELOG.md` in the release PR before
merging it.

## Pre-releases

To ship a release candidate, change the version in the release PR to a
pre-release such as `1.0.0-rc.1`. It's published as a GitHub
**pre-release** and isn't marked as the latest version.

## Rebuilding the binaries of a release

If a platform build fails, or a release needs fresh binaries, run
**Actions → Release assets → Run workflow** with the tag (e.g. `v0.3.0`).
It rebuilds from that tag, replaces the files, and publishes the release if
it's still a draft.

## One-time repository settings

- **Settings → Actions → General → Workflow permissions:** allow GitHub
  Actions to **create and approve pull requests**, so release-plz can open
  the release PR.
- **Settings → General → Pull Requests:** allow **squash merging** (the
  default commit message should be the PR title).
- Optional: add a fine-grained personal access token as the
  `RELEASE_PLZ_TOKEN` secret (contents and pull requests: read/write). CI
  then also runs on the release PR, which PRs opened with the default
  token don't trigger.
