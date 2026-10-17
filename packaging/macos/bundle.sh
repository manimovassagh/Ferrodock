#!/usr/bin/env bash
# Builds dist/Ferrodock.app (universal: Apple Silicon + Intel) and zips it.
# Run on macOS from the repository root: packaging/macos/bundle.sh
set -euo pipefail

version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
app=dist/Ferrodock.app

rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/ferrodock" \
    target/aarch64-apple-darwin/release/ferrodock \
    target/x86_64-apple-darwin/release/ferrodock
sed "s/__VERSION__/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
cp assets/icon.icns "$app/Contents/Resources/icon.icns"

# Ad-hoc signature: required for Apple Silicon. Not notarized, so the first
# launch needs right-click > Open (see README).
codesign --force --deep --sign - "$app"

ditto -c -k --keepParent "$app" "dist/Ferrodock-$version-macos-universal.zip"
echo "Built dist/Ferrodock-$version-macos-universal.zip"
