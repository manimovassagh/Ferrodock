cask "ferrodock" do
  version "1.1.0"
  sha256 "8530a9a4eb8f8dfd0782a0c70ab30a1620f56015088186cdb425982614695733"

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

  # Ferrodock isn't notarized by Apple yet; without this, macOS refuses to
  # open it on first launch.
  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-dr", "com.apple.quarantine", "#{appdir}/Ferrodock.app"]
  end

  zap trash: [
    "~/Library/Application Support/Ferrodock",
    "~/Library/Saved Application State/io.github.manimovassagh.ferrodock.savedState",
  ]
end
