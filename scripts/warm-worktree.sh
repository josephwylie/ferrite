#!/usr/bin/env bash
#
# warm-worktree.sh — give a new git worktree the main checkout's build, so
# its first `cargo` run compiles only what its branch changes.
#
# A worktree builds into its own `target/` (cargo's default): two checkouts
# sharing one build directory rebuild the GPUI stack each time the other has
# built, and can run each other's code, since cargo trusts file times. A
# build directory of its own would start cold (several minutes, several GB),
# so this one starts as a copy-on-write clone of the main checkout's
# `target/debug`: seconds to make, and no disk until the two differ.
#
# Cargo calls a file built when it is older than what was built from it, and
# a fresh checkout stamps every file with now. So each file here that is
# byte-for-byte the main checkout's takes that file's time; one that differs
# keeps its own and is rebuilt. A file of ours that names
# CARGO_MANIFEST_DIR keeps its own time as well: what is built from it holds
# the path of the checkout it was built in, and must be built again here to
# read this checkout's fixtures.
#
# Run from the worktree's root; `scripts/git-hooks/post-checkout` does, for
# every new worktree, once `git config core.hooksPath scripts/git-hooks` is
# set. It does nothing in the main checkout, where a build directory already
# exists, where the main checkout has none to give, and off macOS (the clone
# is APFS's).

set -euo pipefail

[ "$(uname -s)" = "Darwin" ] || exit 0
command -v cargo >/dev/null || exit 0

HERE="$(git rev-parse --show-toplevel)"
MAIN="$(git worktree list --porcelain | sed -n '1s/^worktree //p')"
[ "$HERE" != "$MAIN" ] || exit 0

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
  fi
done
git grep -lz CARGO_MANIFEST_DIR -- crates | xargs -0 touch

# Cloned beside its place and moved in, so a clone cut short is never taken
# for a build directory.
mkdir -p "$(dirname "$TO")"
rm -rf "$TO.warming"
cp -cRp "$FROM" "$TO.warming"
mv "$TO.warming" "$TO"
echo "warm-worktree: build directory cloned from $MAIN" >&2
