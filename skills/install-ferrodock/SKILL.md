---
name: install-ferrodock
description: Install Ferrodock, the free desktop app for Docker containers (macOS, Windows, Linux), on the user's computer from its official GitHub release, checking the download's SHA-256. Use when the user asks to install or set up Ferrodock.
---

# Install Ferrodock

You are helping someone install **Ferrodock**, a free, open-source (MIT)
desktop app for Docker containers, images, volumes and networks, with its
own Docker engine built in. These are the instructions to follow; the
person may not be technical, so explain each step in one plain sentence
before you run it, and report what happened after.

Official sources only: the GitHub repository
`https://github.com/manimovassagh/Ferrodock` and its releases. Never
download Ferrodock from anywhere else.

## Rules

- **Ask before** anything that needs an administrator password, changes a
  security setting (such as macOS's quarantine flag or Windows' download
  mark), or replaces an existing installation. Say what it does and why,
  then wait for a yes.
- **Check the download** against the release's `SHA256SUMS.txt` before
  opening or running anything. If the hash doesn't match, stop, delete the
  file, and tell the person. Don't retry from another source.
- Work in a temporary folder and delete it at the end.
- If a step fails, show the error, explain it simply, and stop rather
  than improvise a workaround that weakens security.

## 1. Find the computer and the latest version

Detect the operating system and processor (`uname -sm` on macOS and Linux;
on Windows, `$env:PROCESSOR_ARCHITECTURE` in PowerShell).

Supported: macOS 11 or newer (Apple Silicon and Intel), Windows 10 and 11
(x64), Linux (x64). Anything else: tell the person Ferrodock doesn't
support it yet, and stop.

Get the latest release (not a pre-release; tags starting with `engine-`
are the engine's own and are never what you install):

```bash
curl -fsSL https://api.github.com/repos/manimovassagh/Ferrodock/releases/latest
```

Its `tag_name` is `v<version>`, for example `v1.27.2`; `<version>` below is
that without the `v`. The files are at
`https://github.com/manimovassagh/Ferrodock/releases/download/v<version>/<file>`:

| System | File |
| --- | --- |
| macOS (Apple Silicon and Intel) | `Ferrodock-<version>-macos-universal.dmg` |
| Windows x64 | `Ferrodock-<version>-windows-x64.zip` |
| Linux x64 | `Ferrodock-<version>-linux-x64.tar.gz` |
| All | `SHA256SUMS.txt` |

## 2. Install

### macOS

**If Homebrew is installed** (`command -v brew`), use it: it also puts the
command-line tools on the PATH and needs no security change.

```bash
brew tap manimovassagh/ferrodock https://github.com/manimovassagh/Ferrodock
brew install --cask ferrodock
```

**Otherwise**, from the DMG:

```bash
tmp=$(mktemp -d) && cd "$tmp"
base=https://github.com/manimovassagh/Ferrodock/releases/download/v<version>
curl -fLO "$base/Ferrodock-<version>-macos-universal.dmg"
curl -fLO "$base/SHA256SUMS.txt"
shasum -a 256 -c SHA256SUMS.txt --ignore-missing   # must print ": OK"
mkdir "$tmp/mnt"
hdiutil attach -nobrowse -readonly -mountpoint "$tmp/mnt" Ferrodock-<version>-macos-universal.dmg
```

If `/Applications/Ferrodock.app` already exists, ask before replacing it
(quit Ferrodock first). Then:

```bash
cp -R "$tmp/mnt/Ferrodock.app" /Applications/
hdiutil detach "$tmp/mnt"
```

Ferrodock isn't notarized by Apple yet, so macOS blocks its first launch.
**Ask the person** whether to allow it now; explain that this removes the
"downloaded from the internet" mark from this one app, which they could
also do by hand in System Settings → Privacy & Security → Open Anyway. If
they agree:

```bash
xattr -dr com.apple.quarantine /Applications/Ferrodock.app
```

Then `open -a Ferrodock`, and delete `$tmp`.

### Windows (PowerShell)

```powershell
$version = "<version>"
$tmp = Join-Path $env:TEMP "ferrodock-install"
New-Item -ItemType Directory -Force $tmp | Out-Null
$base = "https://github.com/manimovassagh/Ferrodock/releases/download/v$version"
$zip = "Ferrodock-$version-windows-x64.zip"
Invoke-WebRequest "$base/$zip" -OutFile "$tmp\$zip"
Invoke-WebRequest "$base/SHA256SUMS.txt" -OutFile "$tmp\SHA256SUMS.txt"
$expected = (Select-String -Path "$tmp\SHA256SUMS.txt" -Pattern ([regex]::Escape($zip))).Line.Split(" ")[0]
$actual = (Get-FileHash "$tmp\$zip" -Algorithm SHA256).Hash.ToLower()
if ($actual -ne $expected) { throw "Checksum mismatch: don't run this file." }
```

Install into the person's own Programs folder (no administrator rights
needed); if it already exists, ask before replacing it, and quit Ferrodock
first:

```powershell
$dest = Join-Path $env:LOCALAPPDATA "Programs\Ferrodock"
Expand-Archive "$tmp\$zip" -DestinationPath $dest -Force
$lnk = Join-Path ([Environment]::GetFolderPath("Programs")) "Ferrodock.lnk"
$shell = New-Object -ComObject WScript.Shell
$s = $shell.CreateShortcut($lnk); $s.TargetPath = "$dest\Ferrodock.exe"; $s.Save()
Remove-Item -Recurse -Force $tmp
Start-Process "$dest\Ferrodock.exe"
```

Windows SmartScreen may say it "protected your PC" because the app is new
and not yet widely downloaded. Tell the person to choose **More info → Run
anyway**; don't remove the download mark yourself without asking.

### Linux (x64)

```bash
tmp=$(mktemp -d) && cd "$tmp"
base=https://github.com/manimovassagh/Ferrodock/releases/download/v<version>
curl -fLO "$base/Ferrodock-<version>-linux-x64.tar.gz"
curl -fLO "$base/SHA256SUMS.txt"
sha256sum -c SHA256SUMS.txt --ignore-missing   # must print ": OK"
dest="$HOME/.local/opt/ferrodock"
mkdir -p "$dest" "$HOME/.local/share/applications"
tar -xzf Ferrodock-<version>-linux-x64.tar.gz -C "$dest"
cat > "$HOME/.local/share/applications/ferrodock.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Ferrodock
Comment=Containers, images, volumes and networks
Exec=$dest/ferrodock
Categories=Development;
EOF
cd / && rm -rf "$tmp"
"$dest/ferrodock" >/dev/null 2>&1 &
```

## 3. First start

Tell the person what happens next, in their words:

- **No Docker engine yet?** Ferrodock shows **Set up Ferrodock's engine**.
  One click downloads its own engine (about 120 MB). On Windows it may
  first install WSL (Windows asks for administrator approval, and may ask
  for a restart); on Linux it asks for the admin password once.
- **Already run Docker?** Ferrodock connects to it by itself.
- On Windows and Linux, Ferrodock puts its command line `fdock` and its
  MCP server `ferrodock-mcp` on the PATH the first time it starts (open a
  new terminal to use them). On macOS without Homebrew, **CLI & MCP** in
  Ferrodock's sidebar has a one-click **Install fdock command**.

Check it worked: the Ferrodock window is open, and after setup, `fdock ps`
(in a new terminal) lists containers, or says there are none.

## 4. Connect your AI tool (optional)

Ask whether they'd like their AI tool (you) to see and manage their
containers through Ferrodock's MCP server. If yes, follow the
**connect-ferrodock-mcp** skill:
https://manimovassagh.github.io/Ferrodock/skills/connect-ferrodock-mcp.md

## Updating and uninstalling

Ferrodock updates itself from inside the app; there's no need to run this
again. To uninstall, quit Ferrodock (from its menu bar or tray icon too),
then remove the app (`brew uninstall --cask ferrodock`,
`/Applications/Ferrodock.app`, `%LOCALAPPDATA%\Programs\Ferrodock` and its
shortcut, or `~/.local/opt/ferrodock` and its `.desktop` file). Its data
and engine are in `~/.ferrodock` (Windows: `%LOCALAPPDATA%\Ferrodock`, and
the engine's WSL distro, removed with `wsl --unregister ferrodock`); ask
before deleting those, since they hold the person's containers and volumes. More: https://manimovassagh.github.io/Ferrodock/docs/installation
