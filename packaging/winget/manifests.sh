#!/usr/bin/env bash
# Writes the winget manifests of a release (the format microsoft/winget-pkgs
# takes): the Windows zip, with Ferrodock.exe, fdock.exe and
# ferrodock-mcp.exe in it as portable programs.
#
# Usage: packaging/winget/manifests.sh <version> <sha256 of the Windows zip> <out dir>
set -euo pipefail

version=$1 sha=$2 out=$3
id=ManiMovassagh.Ferrodock
schema=1.9.0
url="https://github.com/manimovassagh/Ferrodock/releases/download/v$version/Ferrodock-$version-windows-x64.zip"
mkdir -p "$out"

cat > "$out/$id.yaml" <<YAML
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.version.$schema.schema.json
PackageIdentifier: $id
PackageVersion: $version
DefaultLocale: en-US
ManifestType: version
ManifestVersion: $schema
YAML

cat > "$out/$id.installer.yaml" <<YAML
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.installer.$schema.schema.json
PackageIdentifier: $id
PackageVersion: $version
InstallerType: zip
NestedInstallerType: portable
NestedInstallerFiles:
- RelativeFilePath: Ferrodock.exe
  PortableCommandAlias: ferrodock
- RelativeFilePath: fdock.exe
  PortableCommandAlias: fdock
- RelativeFilePath: ferrodock-mcp.exe
  PortableCommandAlias: ferrodock-mcp
MinimumOSVersion: 10.0.19041.0
Installers:
- Architecture: x64
  InstallerUrl: $url
  InstallerSha256: ${sha^^}
ManifestType: installer
ManifestVersion: $schema
YAML

cat > "$out/$id.locale.en-US.yaml" <<YAML
# yaml-language-server: \$schema=https://aka.ms/winget-manifest.defaultLocale.$schema.schema.json
PackageIdentifier: $id
PackageVersion: $version
PackageLocale: en-US
Publisher: Mani Movassagh
PublisherUrl: https://github.com/manimovassagh
PublisherSupportUrl: https://github.com/manimovassagh/Ferrodock/issues
PackageName: Ferrodock
PackageUrl: https://manimovassagh.github.io/Ferrodock/
License: MIT
LicenseUrl: https://github.com/manimovassagh/Ferrodock/blob/main/LICENSE
ShortDescription: The native container manager, written in Rust
Description: Manage Docker or Podman containers, images, volumes, networks and Compose projects from a fast native app. Comes with fdock, a command-line tool, and ferrodock-mcp, an MCP server for AI tools.
Tags:
- docker
- podman
- containers
- compose
- kubernetes
ReleaseNotesUrl: https://github.com/manimovassagh/Ferrodock/releases/tag/v$version
ManifestType: defaultLocale
ManifestVersion: $schema
YAML
