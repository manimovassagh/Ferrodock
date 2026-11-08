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

# Signed with a Developer ID when MACOS_SIGN_IDENTITY names one in the
# keychain (CI imports it from the release secrets), and notarized when
# the App Store Connect key is there too (NOTARY_KEY, NOTARY_KEY_ID,
# NOTARY_ISSUER). Without them: an ad-hoc signature, which Apple Silicon
# requires, and the first launch has to be allowed once (see README).
# The entitlement lets Ferrodock run its own engine's VM.
identity=${MACOS_SIGN_IDENTITY:--}
entitlements=packaging/macos/ferrodock.entitlements
if [ "$identity" = "-" ]; then
    codesign --force --deep --sign - --entitlements "$entitlements" "$app"
else
    # Inside out, each program on its own (not --deep), with the hardened
    # runtime and a timestamp, as notarization requires.
    for program in fdock ferrodock-mcp; do
        codesign --force --options runtime --timestamp --sign "$identity" "$app/Contents/MacOS/$program"
    done
    codesign --force --options runtime --timestamp --sign "$identity" \
        --entitlements "$entitlements" "$app/Contents/MacOS/ferrodock"
    codesign --force --options runtime --timestamp --sign "$identity" \
        --entitlements "$entitlements" "$app"
fi
codesign --verify --deep --strict "$app"

notarize() {
    xcrun notarytool submit "$1" --key "$NOTARY_KEY" --key-id "$NOTARY_KEY_ID" \
        --issuer "$NOTARY_ISSUER" --wait
}
if [ "$identity" != "-" ] && [ -n "${NOTARY_KEY:-}" ]; then
    ditto -c -k --keepParent "$app" "dist/notarize.zip"
    notarize "dist/notarize.zip"
    rm "dist/notarize.zip"
    # The ticket goes into the app, so it opens offline too.
    xcrun stapler staple "$app"
fi

ditto -c -k --keepParent "$app" "dist/Ferrodock-$version-macos-universal.zip"

# Disk image: the app next to a link to /Applications, so installing is a
# single drag.
staging=$(mktemp -d)
cp -R "$app" "$staging/"
ln -s /Applications "$staging/Applications"
hdiutil create -volname "Ferrodock $version" -srcfolder "$staging" -ov \
    -format UDZO "dist/Ferrodock-$version-macos-universal.dmg"
rm -rf "$staging"
if [ "$identity" != "-" ]; then
    codesign --force --timestamp --sign "$identity" "dist/Ferrodock-$version-macos-universal.dmg"
    if [ -n "${NOTARY_KEY:-}" ]; then
        notarize "dist/Ferrodock-$version-macos-universal.dmg"
        xcrun stapler staple "dist/Ferrodock-$version-macos-universal.dmg"
    fi
fi

echo "Built dist/Ferrodock-$version-macos-universal.zip and .dmg"
