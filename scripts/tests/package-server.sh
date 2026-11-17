#!/usr/bin/env bash
set -euo pipefail

script="$(cd "$(dirname "$0")/.." && pwd)/package-server.sh"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/input directory" "$scratch/extracted"
binary="$scratch/input directory/server"
printf '#!/bin/sh\nif [ "${1:-}" = --version ]; then printf "ferrited 1.2.3\\n"; else printf "server fixture\\n"; fi\n' > "$binary"
chmod +x "$binary"

for target in aarch64-apple-darwin x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu; do
  archive="$("$script" v1.2.3 "$target" "$binary" "$scratch/output directory")"
  test "$archive" = "$scratch/output directory/ferrited-1.2.3-$target.tar.gz"
  test "$(tar -tzf "$archive")" = ferrited
  tar -xzf "$archive" -C "$scratch/extracted"
  cmp "$binary" "$scratch/extracted/ferrited"
  test -x "$scratch/extracted/ferrited"
  test "$("$scratch/extracted/ferrited")" = 'server fixture'
done

if "$script" 1.2.3 unsupported "$binary" "$scratch/rejected" 2>/dev/null; then
  echo "An unsupported target was accepted" >&2
  exit 1
fi
if "$script" '../1.2.3' aarch64-apple-darwin "$binary" "$scratch/rejected" 2>/dev/null; then
  echo "An invalid version was accepted" >&2
  exit 1
fi
if "$script" 1.2.4 aarch64-apple-darwin "$binary" "$scratch/rejected" 2>/dev/null; then
  echo "A binary with a different release version was accepted" >&2
  exit 1
fi
chmod -x "$binary"
if "$script" 1.2.3 aarch64-apple-darwin "$binary" "$scratch/rejected" 2>/dev/null; then
  echo "A non-executable binary was accepted" >&2
  exit 1
fi
test ! -e "$scratch/rejected"
echo 'PASS: server archive names, contents, permissions, and input validation'
