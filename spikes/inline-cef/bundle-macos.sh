#!/usr/bin/env bash
# Build the spike with Chromium (--features cef) and assemble a runnable macOS
# .app: the executable, Chromium Embedded Framework, and the five helper apps
# CEF launches its child processes from.
#
#   ./bundle-macos.sh                      # target/bundle/inline-cef.app (debug)
#   ./bundle-macos.sh --release            # optimised build
#   ./bundle-macos.sh --example cef_smoke  # bundle an example instead
#   ./bundle-macos.sh --run [-- args]      # ...and run it in this terminal
#   ./bundle-macos.sh --open               # ...and `open` it
#   ./bundle-macos.sh --features shots     # extra cargo features (cef is implied)
#
# The executable is its own CEF helper (web::cef::boot() spots `--type=`), so
# each helper app holds a hard link to it rather than a separate binary.
#
# Needs: cmake and ninja on PATH (cef-dll-sys builds CEF's C++ wrapper with
# them; `brew install cmake ninja`). CEF itself (~130 MB download, ~320 MB
# unpacked) is fetched once into $CEF_PATH (default ~/.local/share/cef).
#
# Why not cef-rs's `bundle-cef-app`: it always runs `cargo build --bin <name>`
# without features and wants a separate helper binary; this spike needs
# `--features cef`, examples, and a self-hosting helper. The layout below is
# the same one it produces (CEF's general_usage "macOS" section).
set -euo pipefail

cd "$(dirname "$0")"

profile=debug
cargo_profile=()
target_kind=bin
name=inline-cef
action=none
features=cef
run_args=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --release) profile=release; cargo_profile=(--release) ;;
    --example) target_kind=example; name="$2"; shift ;;
    --run) action=run ;;
    --features) features="cef,$2"; shift ;;
    --open) action=open ;;
    --) shift; run_args=("$@"); break ;;
    -h|--help) sed -n '2,23p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

[[ "$(uname)" == Darwin ]] || { echo "macOS only" >&2; exit 1; }
for tool in cmake ninja; do
  command -v "$tool" >/dev/null || { echo "$tool not found: brew install $tool" >&2; exit 1; }
done

export CEF_PATH="${CEF_PATH:-$HOME/.local/share/cef}"

echo "==> cargo build --features $features ${cargo_profile[*]:-} --$target_kind $name"
cargo build --features "$features" "${cargo_profile[@]+"${cargo_profile[@]}"}" "--$target_kind" "$name"

target_dir="$(cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
if [[ "$target_kind" == example ]]; then
  exe="$target_dir/$profile/examples/$name"
else
  exe="$target_dir/$profile/$name"
fi
[[ -x "$exe" ]] || { echo "built executable not found: $exe" >&2; exit 1; }

# The framework cef-dll-sys downloaded for the cef crate in Cargo.lock
# (crate 154.5.0+154.0.34 -> CEF 154.0.34), or CEF_PATH itself if it is an
# export-cef-dir layout.
cef_version="$(sed -n '/^name = "cef"$/{n;s/^version = ".*+\(.*\)"$/\1/p;}' Cargo.lock)"
framework_name="Chromium Embedded Framework.framework"
framework=""
for candidate in "$CEF_PATH/$cef_version"/*/"$framework_name" "$CEF_PATH/$framework_name"; do
  if [[ -d "$candidate" ]]; then framework="$candidate"; break; fi
done
[[ -n "$framework" ]] || { echo "CEF $cef_version framework not found under $CEF_PATH" >&2; exit 1; }

app="$target_dir/bundle/$name.app"
identifier="dev.ferrite.spike.${name//_/-}"
echo "==> $app (CEF $cef_version)"

plist() { # path executable identifier is_helper
  local ui_element=""
  [[ "$4" == yes ]] && ui_element="<key>LSUIElement</key><string>1</string>"
  cat > "$1" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>$2</string>
  <key>CFBundleIdentifier</key><string>$3</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>$2</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>0.1.0</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>LSEnvironment</key><dict><key>MallocNanoZone</key><string>0</string></dict>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  $ui_element
</dict></plist>
PLIST
}

mkdir -p "$app/Contents/MacOS" "$app/Contents/Frameworks" "$app/Contents/Resources"
rm -f "$app/Contents/MacOS/$name"
cp "$exe" "$app/Contents/MacOS/$name"
plist "$app/Contents/Info.plist" "$name" "$identifier" no

# rsync so re-bundling only copies what changed (the framework is ~320 MB).
rsync -a --delete "$framework/" "$app/Contents/Frameworks/$framework_name/"

for suffix in "" " (GPU)" " (Renderer)" " (Plugin)" " (Alerts)"; do
  helper="$name Helper$suffix"
  helper_app="$app/Contents/Frameworks/$helper.app"
  mkdir -p "$helper_app/Contents/MacOS"
  rm -f "$helper_app/Contents/MacOS/$helper"
  ln "$app/Contents/MacOS/$name" "$helper_app/Contents/MacOS/$helper"
  helper_id="$identifier.helper$(echo "$suffix" | tr -d ' ()' | tr '[:upper:]' '[:lower:]' | sed 's/^./.&/')"
  plist "$helper_app/Contents/Info.plist" "$helper" "$helper_id" yes
done

du -sh "$app" | sed 's/^/==> size: /'

case "$action" in
  run) exec "$app/Contents/MacOS/$name" "${run_args[@]+"${run_args[@]}"}" ;;
  open) open "$app" ;;
  none) echo "Run: $app/Contents/MacOS/$name   (or: open \"$app\")" ;;
esac
