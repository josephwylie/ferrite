#!/usr/bin/env bash
#
# install-app.sh — build Ferrite in release and install it as
# /Applications/Ferrite.app, the one copy on the machine, so it opens from
# the Dock, Spotlight and Launchpad like any other app.
#
# The bundle is the release binary, its application icon, and an Info.plist
# naming both. Local builds receive an ad-hoc signature; release builds set
# MACOS_SIGNING_IDENTITY to a Developer ID Application identity and can emit a
# signed drag-to-Applications DMG. The build directory is asked of cargo rather
# than assumed — a
# `target-dir` in ~/.cargo/config.toml moves it — and a missing binary is a
# loud failure, never a silent install of nothing.
#
# Visuals (ADR 0013): the app is built with `--features cef`, and the bundle
# carries Chromium Embedded Framework plus the five helper apps CEF starts its
# processes from (`Contents/Frameworks/ferrite Helper*.app`). The binary is its
# own helper (`visual::boot` spots `--type=`), so each helper holds a hard link
# to it (ad-hoc builds) or a signed copy of it (Developer ID builds: codesign
# rewrites the file, which breaks the link). Building CEF's C++ wrapper needs
# cmake and ninja (`brew install cmake ninja`); CEF itself (~130 MB download,
# ~320 MB unpacked) is fetched once into $CEF_PATH (default
# ~/.local/share/cef). FERRITE_NO_CEF=1 builds without visuals.
#
#   --bundle-only    build the .app in the target dir, don't install it
#   --dmg <path>     ...and package it as a disk image

set -euo pipefail

BUNDLE_ONLY=false
DMG=""
case "${1:-}" in
  "") ;;
  --bundle-only)
    if [ "$#" -ne 1 ]; then
      echo "usage: $0 [--bundle-only | --dmg <path>]" >&2
      exit 2
    fi
    BUNDLE_ONLY=true
    ;;
  --dmg)
    if [ "$#" -ne 2 ]; then
      echo "usage: $0 [--bundle-only | --dmg <path>]" >&2
      exit 2
    fi
    BUNDLE_ONLY=true
    DMG="$2"
    ;;
  *)
    echo "usage: $0 [--bundle-only | --dmg <path>]" >&2
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

FEATURES=()
if [ -z "${FERRITE_NO_CEF:-}" ]; then
  for tool in cmake ninja; do
    command -v "$tool" >/dev/null || {
      echo "$tool not found (needed to build Chromium Embedded Framework's wrapper): brew install $tool" >&2
      echo "or set FERRITE_NO_CEF=1 to build without visuals" >&2
      exit 1
    }
  done
  export CEF_PATH="${CEF_PATH:-$HOME/.local/share/cef}"
  FEATURES=(--features cef)
fi

cargo build --release --locked --manifest-path "$ROOT/Cargo.toml" -p ferrite "${FEATURES[@]+"${FEATURES[@]}"}"

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
SIGNING_IDENTITY="${MACOS_SIGNING_IDENTITY:--}"

# Chromium: the framework and the helper apps (see the header).
HELPERS=()
if [ -n "${FEATURES[*]:-}" ]; then
  # The framework cef-dll-sys downloaded for the `cef` crate in Cargo.lock
  # (crate 154.5.0+154.0.34 -> CEF 154.0.34).
  CEF_VERSION="$(sed -n '/^name = "cef"$/{n;s/^version = ".*+\(.*\)"$/\1/p;}' "$ROOT/Cargo.lock")"
  FRAMEWORK_NAME="Chromium Embedded Framework.framework"
  FRAMEWORK=""
  for candidate in "$CEF_PATH/$CEF_VERSION"/*/"$FRAMEWORK_NAME" "$CEF_PATH/$FRAMEWORK_NAME"; do
    if [ -d "$candidate" ]; then FRAMEWORK="$candidate"; break; fi
  done
  if [ -z "$FRAMEWORK" ]; then
    echo "CEF $CEF_VERSION framework not found under $CEF_PATH" >&2
    exit 1
  fi
  mkdir -p "$BUILT/Contents/Frameworks"
  ditto "$FRAMEWORK" "$BUILT/Contents/Frameworks/$FRAMEWORK_NAME"
  for suffix in "" " (GPU)" " (Renderer)" " (Plugin)" " (Alerts)"; do
    helper="ferrite Helper$suffix"
    helper_app="$BUILT/Contents/Frameworks/$helper.app"
    helper_id="$IDENTIFIER.helper$(printf '%s' "$suffix" | tr -d ' ()' | tr '[:upper:]' '[:lower:]' | sed 's/^./.&/')"
    mkdir -p "$helper_app/Contents/MacOS"
    ln "$BUILT/Contents/MacOS/ferrite" "$helper_app/Contents/MacOS/$helper"
    cat >"$helper_app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>$helper</string>
  <key>CFBundleIdentifier</key>
  <string>$helper_id</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>$helper</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleVersion</key>
  <string>$VERSION</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
  <key>LSUIElement</key>
  <string>1</string>
  <key>NSSupportsAutomaticGraphicsSwitching</key>
  <true/>
</dict>
</plist>
PLIST
    plutil -lint -s "$helper_app/Contents/Info.plist"
    HELPERS+=("$helper_app")
  done
fi

if [ "$SIGNING_IDENTITY" = "-" ]; then
  # Nested code first, the app last. Installed locally, the helpers keep
  # their hard links: the linker already signed the binary, and signing the
  # app rewrites only its own copy. A disk image leaves this machine, where
  # every nested bundle must carry its own seal: each helper is signed (and
  # becomes a copy).
  if [ -n "${FEATURES[*]:-}" ]; then
    codesign --force --sign - "$BUILT/Contents/Frameworks/$FRAMEWORK_NAME"
    if [ -n "$DMG" ]; then
      for helper_app in "${HELPERS[@]}"; do
        codesign --force --sign - "$helper_app"
      done
    fi
  fi
  codesign --force --sign - "$BUILT"
else
  if [ -n "${FEATURES[*]:-}" ]; then
    # Chromium's helpers JIT (V8) and load the framework: under the
    # hardened runtime they need these entitlements. The renderers stay in
    # Chromium's own sandbox regardless.
    ENTITLEMENTS="$TARGET/release/bundle/macos/helper.entitlements"
    cat >"$ENTITLEMENTS" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>com.apple.security.cs.allow-jit</key>
  <true/>
  <key>com.apple.security.cs.allow-unsigned-executable-memory</key>
  <true/>
  <key>com.apple.security.cs.disable-library-validation</key>
  <true/>
</dict>
</plist>
PLIST
    codesign --force --options runtime --timestamp --sign "$SIGNING_IDENTITY" \
      "$BUILT/Contents/Frameworks/$FRAMEWORK_NAME"
    for helper_app in "${HELPERS[@]}"; do
      codesign --force --options runtime --timestamp --entitlements "$ENTITLEMENTS" \
        --sign "$SIGNING_IDENTITY" "$helper_app"
    done
    rm -f "$ENTITLEMENTS"
  fi
  codesign --force --options runtime --timestamp --sign "$SIGNING_IDENTITY" "$BUILT"
  codesign --verify --deep --strict --verbose=2 "$BUILT"
fi

if [ "$BUNDLE_ONLY" = true ]; then
  if [ -n "$DMG" ]; then
    DMG_ROOT="$TARGET/release/bundle/dmg"
    rm -rf "$DMG_ROOT"
    mkdir -p "$DMG_ROOT"
    ditto "$BUILT" "$DMG_ROOT/$NAME.app"
    ln -s /Applications "$DMG_ROOT/Applications"
    mkdir -p "$(dirname "$DMG")"
    rm -f "$DMG"
    hdiutil create -volname "$NAME" -srcfolder "$DMG_ROOT" -ov -format UDZO "$DMG"
    if [ "$SIGNING_IDENTITY" != "-" ]; then
      codesign --force --timestamp --sign "$SIGNING_IDENTITY" "$DMG"
      codesign --verify --verbose=2 "$DMG"
    fi
    echo "Packaged → $DMG"
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
