#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 2 ] || [ "$#" -gt 4 ]; then
  echo "Usage: $0 VERSION TARGET [BINARY [OUTPUT_DIR]]" >&2
  exit 1
fi

version="${1#v}"
target="$2"
binary="${3:-target/release/ferrited}"
output_dir="${4:-dist}"

if [[ ! "$version" =~ ^[0-9][0-9A-Za-z.+-]*$ ]]; then
  echo "Invalid release version: $version" >&2
  exit 1
fi
case "$target" in
  aarch64-apple-darwin|x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu) ;;
  *) echo "Unsupported server target: $target" >&2; exit 1 ;;
esac
if [ ! -f "$binary" ] || [ ! -x "$binary" ]; then
  echo "Server binary is missing or not executable: $binary" >&2
  exit 1
fi
if [ "$("$binary" --version)" != "ferrited $version" ]; then
  echo "Server binary version does not match release $version" >&2
  exit 1
fi

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
install -m 755 "$binary" "$stage/ferrited"
mkdir -p "$output_dir"
archive="$output_dir/ferrited-$version-$target.tar.gz"
COPYFILE_DISABLE=1 tar -czf "$archive" -C "$stage" ferrited
printf '%s\n' "$archive"
