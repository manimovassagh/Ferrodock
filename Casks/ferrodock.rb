cask "ferrodock" do
  version "1.40.0"
  sha256 "b4a98f8c290ff96c3e9a2ef0c6f353d124b3d47b41ea8b05111bd653dc7acfd7"

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
