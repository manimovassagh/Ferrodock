#!/usr/bin/env bash
# Builds dist/Ferrodock.app (universal: Apple Silicon + Intel), with the
# fdock CLI and the ferrodock-mcp server inside it, then a zip and a .dmg.
# Run on macOS from the repository root: packaging/macos/bundle.sh
set -euo pipefail

# From Cargo: the version lives in [workspace.package], shared by every crate.
version=$(cargo metadata --no-deps --format-version 1 |
    jq -r '.packages[] | select(.name == "ferrodock") | .version')
app=dist/Ferrodock.app

rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --workspace --target aarch64-apple-darwin
cargo build --release --workspace --target x86_64-apple-darwin

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
# The app, and the command-line tools that come with it (Homebrew links
# them onto the PATH).
for program in ferrodock fdock ferrodock-mcp; do
    lipo -create -output "$app/Contents/MacOS/$program" \
        "target/aarch64-apple-darwin/release/$program" \
        "target/x86_64-apple-darwin/release/$program"
done
sed "s/__VERSION__/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
cp assets/icon.icns "$app/Contents/Resources/icon.icns"

# Ad-hoc signature: required for Apple Silicon. Not notarized, so the first
# launch has to be allowed once (see README). The entitlement lets
# Ferrodock run its own engine's VM.
codesign --force --deep --sign - --entitlements packaging/macos/ferrodock.entitlements "$app"

ditto -c -k --keepParent "$app" "dist/Ferrodock-$version-macos-universal.zip"

# Disk image: the app next to a link to /Applications, so installing is a
# single drag.
staging=$(mktemp -d)
cp -R "$app" "$staging/"
ln -s /Applications "$staging/Applications"
hdiutil create -volname "Ferrodock $version" -srcfolder "$staging" -ov \
    -format UDZO "dist/Ferrodock-$version-macos-universal.dmg"
rm -rf "$staging"

echo "Built dist/Ferrodock-$version-macos-universal.zip and .dmg"
