cask "ferrodock" do
  version "1.25.0"
  sha256 "8cd61bf9356d6974a503b8c73645728a2b4e8aa0637644eca964312930d261f2"

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
  # The command-line tools inside the app, onto the PATH.
  binary "#{appdir}/Ferrodock.app/Contents/MacOS/fdock"
  binary "#{appdir}/Ferrodock.app/Contents/MacOS/ferrodock-mcp"

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
