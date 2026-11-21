#!/usr/bin/env bash
#
# warm-worktree.sh — give a git worktree the main checkout's build, so its
# next `cargo` run compiles only what its branch changes.
#
# A worktree builds into its own `target/` (cargo's default): two checkouts
# sharing one build directory rebuild the GPUI stack each time the other has
# built, and can run each other's code, since cargo trusts file times. A
# build directory of its own would start cold (20 minutes or more, several
# GB), so this one starts as a copy of the main checkout's `target/debug`. On
# macOS the copy is an APFS clone: seconds to make, and no disk until the two
# differ. On Linux it is a reflink clone where the filesystem has them (Btrfs,
# XFS) and a full copy where it has not (ext4): a minute or two and the
# build's full size on disk, so it is skipped when the disk has less than
# three times that free.
#
# Cargo calls a file built when it is older than what was built from it. So
# each file here that is byte-for-byte the main checkout's takes that file's
# time, and every other file is stamped now and rebuilt: one that differs,
# and one git does not track yet. A file of ours that names
# CARGO_MANIFEST_DIR is stamped now as well: what is built from it holds the
# path of the checkout it was built in, and must be built again here to read
# this checkout's fixtures.
#
# Run from the worktree's root. `scripts/git-hooks/post-checkout` does, for
# every new worktree, once `git config core.hooksPath scripts/git-hooks` is
# set; `scripts/reclaim-worktree-builds.py` does, for an idle one whose build
# it has taken away. It does nothing in the main checkout, where a build
# directory already exists, where the main checkout has none to give, and
# off macOS and Linux (Windows has scripts/warm-worktree-windows.sh).
# `target/debug/.warmed` marks the moment: what is newer than it was built
# here.

set -euo pipefail

case "$(uname -s)" in Darwin | Linux) ;; *) exit 0 ;; esac
command -v cargo >/dev/null || exit 0

HERE="$(git rev-parse --show-toplevel)"
MAIN="$(git worktree list --porcelain | sed -n '1s/^worktree //p')"
[ "$HERE" != "$MAIN" ] || exit 0

# The clone is only reusable by the compiler that built it, and a rustup
# directory override on the main checkout does not reach a worktree beside
# it. So the worktree gets the main checkout's toolchain as its own override.
if command -v rustup >/dev/null; then
  toolchain="$(cd "$MAIN" && rustup show active-toolchain | cut -d' ' -f1)"
  rustup override set "$toolchain" --path "$HERE" >/dev/null
fi

# Asked of cargo, not assumed: a `target-dir` in a cargo config moves it.
build_dir() {
  (cd "$1" && cargo metadata --no-deps --format-version 1) |
    sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p'
}
FROM="$(build_dir "$MAIN")/debug"
TO="$(build_dir "$HERE")/debug"
[ "$FROM" != "$TO" ] || exit 0
[ -d "$FROM" ] || exit 0
[ ! -e "$TO" ] || exit 0

mkdir -p "$(dirname "$TO")"
if [ "$(uname -s)" = "Linux" ]; then
  # A one-file clone tells a reflink filesystem from one that copies bytes.
  probe="$(dirname "$TO")/.warm-probe"
  sample="$(find "$FROM" -maxdepth 1 -type f -size +0 | head -n 1)"
  if ! cp --reflink=always "$sample" "$probe" 2>/dev/null; then
    need_kb=$(($(du -sk "$FROM" | cut -f1) * 3))
    free_kb=$(df -Pk "$(dirname "$TO")" | awk 'NR == 2 { print $4 }')
    if [ "$free_kb" -lt "$need_kb" ]; then
      rm -f "$probe"
      echo "warm-worktree: $((free_kb / 1048576)) GB free, under three times the" \
        "main checkout's build ($((need_kb / 3 / 1048576)) GB); the first build here starts cold" >&2
      exit 0
    fi
  fi
  rm -f "$probe"
fi

cd "$HERE"
git ls-files -z | while IFS= read -r -d '' file; do
  if cmp -s "$MAIN/$file" "$file"; then
    touch -r "$MAIN/$file" "$file"
  elif [ -f "$file" ]; then
    touch "$file"
  fi
done
git ls-files -z --others --exclude-standard | xargs -0 -r touch
{ git grep -lz CARGO_MANIFEST_DIR -- crates || true; } | xargs -0 -r touch

# Cloned beside its place and moved in, so a clone cut short is never taken
# for a build directory.
rm -rf "$TO.warming"
case "$(uname -s)" in
  Darwin) cp -cRp "$FROM" "$TO.warming" ;;
  Linux) cp --reflink=auto -Rp --preserve=links "$FROM" "$TO.warming" ;;
esac
mv "$TO.warming" "$TO"
touch "$TO/.warmed"
echo "warm-worktree: build directory cloned from $MAIN" >&2
