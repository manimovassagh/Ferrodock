#!/usr/bin/env bash
# Points the Scoop manifest (packaging/scoop/ferrodock.json) at a release.
# Usage: packaging/scoop/update.sh <version> <sha256 of the Windows zip>
set -euo pipefail
version=$1 sha=$2
manifest="$(dirname "$0")/ferrodock.json"
jq --arg v "$version" --arg h "$sha" '
    .version = $v
    | .architecture."64bit".url = "https://github.com/manimovassagh/Ferrodock/releases/download/v\($v)/Ferrodock-\($v)-windows-x64.zip"
    | .architecture."64bit".hash = $h' "$manifest" > "$manifest.new"
mv "$manifest.new" "$manifest"
