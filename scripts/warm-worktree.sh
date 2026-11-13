#!/usr/bin/env bash
#
# warm-worktree.sh — give a git worktree the main checkout's build, so its
# next `cargo` run compiles only what its branch changes.
#
# A worktree builds into its own `target/` (cargo's default): two checkouts
# sharing one build directory rebuild the GPUI stack each time the other has
# built, and can run each other's code, since cargo trusts file times. A
# build directory of its own would start cold (several minutes, several GB),
# so this one starts as a copy-on-write clone of the main checkout's
# `target/debug`: seconds to make, and no disk until the two differ.
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
# off macOS (the clone is APFS's). `target/debug/.warmed` marks the moment:
# what is newer than it was built here.

set -euo pipefail

[ "$(uname -s)" = "Darwin" ] || exit 0
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

cd "$HERE"
git ls-files -z | while IFS= read -r -d '' file; do
  if cmp -s "$MAIN/$file" "$file"; then
    touch -r "$MAIN/$file" "$file"
  elif [ -f "$file" ]; then
    touch "$file"
  fi
done
git ls-files -z --others --exclude-standard | xargs -0 touch
{ git grep -lz CARGO_MANIFEST_DIR -- crates || true; } | xargs -0 touch

# Cloned beside its place and moved in, so a clone cut short is never taken
# for a build directory.
mkdir -p "$(dirname "$TO")"
rm -rf "$TO.warming"
cp -cRp "$FROM" "$TO.warming"
mv "$TO.warming" "$TO"
touch "$TO/.warmed"
echo "warm-worktree: build directory cloned from $MAIN" >&2
