#!/usr/bin/env bash
# Writes Casks/ferrodock.rb for a released version.
# usage: packaging/homebrew/update-cask.sh VERSION SHA256_OF_DMG
#
# Users install with:
#   brew tap manimovassagh/ferrodock https://github.com/manimovassagh/Ferrodock
#   brew install --cask ferrodock
set -euo pipefail

version=$1
sha256=$2
mkdir -p Casks
cat > Casks/ferrodock.rb <<CASK
cask "ferrodock" do
  version "${version}"
  sha256 "${sha256}"

  url "https://github.com/manimovassagh/Ferrodock/releases/download/v#{version}/Ferrodock-#{version}-macos-universal.dmg"
  name "Ferrodock"
  desc "Fast, native desktop app for managing Docker"
  homepage "https://manimovassagh.github.io/Ferrodock/"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :big_sur"

  app "Ferrodock.app"

  caveats <<~EOS
    Ferrodock isn't notarized by Apple yet. If macOS refuses to open it, run:
      xattr -dr com.apple.quarantine "#{appdir}/Ferrodock.app"
  EOS
end
CASK
echo "Wrote Casks/ferrodock.rb for ${version}"
