# Releasing Ferrodock

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
platform has built. If a build fails, hangs (each has a time limit) or is
cancelled, the workflow opens an issue, "Release vX.Y.Z didn't finish",
saying how to finish it (see below).

When two release PRs are merged close together, their releases can finish
in either order. Only the highest version is marked **latest** and goes
into the Homebrew cask, so neither goes backwards.

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
**Actions → Release assets → Run workflow** with the tag (e.g. `v0.3.0`),
or `gh workflow run release-assets.yml -f tag=v0.3.0`. It rebuilds from
that tag, replaces the files, and publishes the release if it's still a
draft.

Re-running the **Release** workflow instead doesn't help: the tag already
exists, so it builds nothing.

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

## Signing and notarizing the macOS app

Releases are ad-hoc signed until these secrets exist; then the macOS job
signs every program with a Developer ID (hardened runtime, timestamp) and
notarizes and staples both the app and the DMG, so it opens without the
**Open Anyway** step.

| Secret | What |
| --- | --- |
| `MACOS_CERT_P12` | The **Developer ID Application** certificate with its private key, exported from Keychain Access as `.p12`, then `base64 -i cert.p12` |
| `MACOS_CERT_PW` | The password it was exported with |
| `AC_API_KEY` | An App Store Connect API key (Users and Access → Integrations → Keys, role Developer): the contents of its `.p8` file |
| `AC_KEY_ID` | That key's ID |
| `AC_ISSUER` | The issuer ID shown above the keys |

The certificate needs a paid Apple Developer Program membership. The
`com.apple.security.virtualization` entitlement works with a Developer ID
and needs no provisioning profile. Once a signed release is out, the
quarantine step can go from the Homebrew cask and the install docs.

## Submitting to winget

With a `WINGET_TOKEN` secret, each latest release opens a pull request on
[microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs) with the
manifests of `packaging/winget/manifests.sh` (package
`ManiMovassagh.Ferrodock`). It's a classic personal access token with the
`public_repo` scope: the pull request comes from that account's fork of
winget-pkgs, made the first time. winget's moderators review the first
submission; later versions are usually merged automatically.

## Publishing to crates.io

Each release publishes `ferrodock-core`, `ferrodock-cli`, `ferrodock-mcp`
and `ferrodock` to crates.io with **trusted publishing**: no stored token,
crates.io trusts this repository's release workflows and hands out a
token that only lasts for the publishing job. Set it up once for each of
the four crates, on crates.io → the crate → **Settings → Trusted
Publishing → Add**:

| Field | Value |
| --- | --- |
| Repository owner | `manimovassagh` |
| Repository name | `Ferrodock` |
| Workflow filename | `release.yml` |
| Environment | (empty) |

Add a second one with the workflow filename `release-assets.yml`, which
is used when the release assets are run again by hand. Until trusted
publishing works the `CARGO_REGISTRY_TOKEN` secret is used instead; once
a release shows "Publishing with a trusted-publishing token" in its
crates job, delete that secret.
