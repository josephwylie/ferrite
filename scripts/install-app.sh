#!/usr/bin/env bash
#
# install-app.sh — build Ferrite in release and install it as
# /Applications/Ferrite.app, the one copy on the machine, so it opens from
# the Dock, Spotlight and Launchpad like any other app.
#
# The bundle is the release binary, an Info.plist naming it, and an ad-hoc
# code signature so macOS keeps treating it as the same app across rebuilds.
# Fonts and icons are compiled into the binary, so there are no resources
# to carry. The build directory is asked of cargo rather than assumed — a
# `target-dir` in ~/.cargo/config.toml moves it — and a missing binary is a
# loud failure, never a silent install of nothing.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NAME="Ferrite"
IDENTIFIER="com.github.josephwylie.ferrite"
DEST="/Applications/$NAME.app"

if [ "$(uname -s)" != "Darwin" ]; then
  echo "install-app.sh builds a macOS app bundle; this is $(uname -s)" >&2
  exit 1
fi

cargo build --release --manifest-path "$ROOT/Cargo.toml" -p ferrite

metadata="$(cargo metadata --no-deps --format-version 1 --manifest-path "$ROOT/Cargo.toml")"
TARGET="$(printf '%s' "$metadata" | /usr/bin/python3 -c \
  'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
VERSION="$(printf '%s' "$metadata" | /usr/bin/python3 -c \
  'import json, sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "ferrite"))')"

BIN="$TARGET/release/ferrite"
if [ ! -x "$BIN" ]; then
  echo "no release binary at $BIN" >&2
  exit 1
fi

BUILT="$TARGET/release/bundle/macos/$NAME.app"
rm -rf "$BUILT"
mkdir -p "$BUILT/Contents/MacOS"
cp "$BIN" "$BUILT/Contents/MacOS/ferrite"
cat >"$BUILT/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>$NAME</string>
  <key>CFBundleExecutable</key>
  <string>ferrite</string>
  <key>CFBundleIdentifier</key>
  <string>$IDENTIFIER</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>$NAME</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleVersion</key>
  <string>$VERSION</string>
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.developer-tools</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key>
  <true/>
</dict>
</plist>
PLIST
plutil -lint -s "$BUILT/Contents/Info.plist"
codesign --force --sign - "$BUILT"

rm -rf "$DEST"
ditto "$BUILT" "$DEST"
# One copy, not two: the bundle in the build directory is a build artifact,
# and leaving it there is how a stale app gets launched by mistake.
rm -rf "$BUILT"

echo "Installed → $DEST"
