#!/usr/bin/env bash
#
# install-app.sh — build Ferrite in release and install it as
# /Applications/Ferrite.app, the one copy on the machine, so it opens from
# the Dock, Spotlight and Launchpad like any other app.
#
# The bundle is the release binary, its application icon, an Info.plist naming
# both, and an ad-hoc code signature so macOS keeps treating it as the same app
# across rebuilds. The build directory is asked of cargo rather than assumed — a
# `target-dir` in ~/.cargo/config.toml moves it — and a missing binary is a
# loud failure, never a silent install of nothing.

set -euo pipefail

BUNDLE_ONLY=false
ARCHIVE=""
case "${1:-}" in
  "") ;;
  --bundle-only)
    if [ "$#" -ne 1 ]; then
      echo "usage: $0 [--bundle-only | --archive <path>]" >&2
      exit 2
    fi
    BUNDLE_ONLY=true
    ;;
  --archive)
    if [ "$#" -ne 2 ]; then
      echo "usage: $0 [--bundle-only | --archive <path>]" >&2
      exit 2
    fi
    BUNDLE_ONLY=true
    ARCHIVE="$2"
    ;;
  *)
    echo "usage: $0 [--bundle-only | --archive <path>]" >&2
    exit 2
    ;;
esac

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NAME="Ferrite"
IDENTIFIER="com.github.josephwylie.ferrite"
DEST="/Applications/$NAME.app"

if [ "$(uname -s)" != "Darwin" ]; then
  echo "install-app.sh builds a macOS app bundle; this is $(uname -s)" >&2
  exit 1
fi

cargo build --release --locked --manifest-path "$ROOT/Cargo.toml" -p ferrite

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
mkdir -p "$BUILT/Contents/MacOS" "$BUILT/Contents/Resources"
cp "$BIN" "$BUILT/Contents/MacOS/ferrite"

# iconutil expects this exact set of filenames. Generate the platform resource
# from the checked-in 1254px source so the Dock gets sharp 1x and 2x variants.
ICONSET="$TARGET/release/bundle/macos/Ferrite.iconset"
rm -rf "$ICONSET"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$ROOT/crates/ferrite/assets/app-icon.png" \
    --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$ROOT/crates/ferrite/assets/app-icon.png" \
    --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$BUILT/Contents/Resources/Ferrite.icns"
rm -rf "$ICONSET"
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
  <key>CFBundleIconFile</key>
  <string>Ferrite</string>
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

if [ "$BUNDLE_ONLY" = true ]; then
  if [ -n "$ARCHIVE" ]; then
    tar -czf "$ARCHIVE" -C "$(dirname "$BUILT")" "$(basename "$BUILT")"
    echo "Archived → $ARCHIVE"
    exit 0
  fi
  echo "Built → $BUILT"
  exit 0
fi

rm -rf "$DEST"
ditto "$BUILT" "$DEST"
# One copy, not two: the bundle in the build directory is a build artifact,
# and leaving it there is how a stale app gets launched by mistake.
rm -rf "$BUILT"

echo "Installed → $DEST"
