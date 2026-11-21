#!/usr/bin/env bash
#
# warm-worktree-windows.sh — scripts/warm-worktree.sh for Windows: give a git
# worktree the main checkout's build, so its next `cargo` run compiles only
# what its branch changes. Runs in Git for Windows' bash.
#
# What it does, and why, is as in scripts/warm-worktree.sh. Two things
# differ. Windows has no APFS clone, so robocopy copies `target/debug`: every
# byte on NTFS (about a minute, and the build's full size on disk), a clone on
# a ReFS Dev Drive. And file times are stamped in one Python process, since a
# `cmp` and a `touch` for each file take minutes here.
#
# Nothing reclaims a worktree's build on Windows yet
# (scripts/reclaim-worktree-builds.py needs fcntl and lsof): remove a
# worktree once its work is done.

set -euo pipefail

case "$(uname -s)" in MINGW* | MSYS*) ;; *) exit 0 ;; esac
command -v cargo >/dev/null || exit 0

HERE="$(git rev-parse --show-toplevel)"
MAIN="$(git worktree list --porcelain | sed -n '1s/^worktree //p')"
[ "$HERE" != "$MAIN" ] || exit 0

# The copy is only reusable by the compiler that built it.
if command -v rustup >/dev/null; then
  toolchain="$(cd "$MAIN" && rustup show active-toolchain | cut -d' ' -f1)"
  rustup override set "$toolchain" --path "$HERE" >/dev/null
fi

# Cargo writes `C:\\Users\\...`, read here as `C:/Users/...`.
build_dir() {
  (cd "$1" && cargo metadata --no-deps --format-version 1 2>/dev/null) |
    sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' | sed 's|\\\\|/|g'
}
FROM="$(build_dir "$MAIN")/debug"
TO="$(build_dir "$HERE")/debug"
[ "$FROM" != "$TO" ] || exit 0
[ -d "$FROM" ] || exit 0
[ ! -e "$TO" ] || exit 0

cd "$HERE"
python3 - "$MAIN" <<'STAMP'
import filecmp, os, subprocess, sys

main = sys.argv[1]

def listed(*git):
    out = subprocess.run(["git", *git], capture_output=True).stdout
    return [path for path in out.decode().split("\0") if path]

for file in listed("ls-files", "-z"):
    theirs = os.path.join(main, file)
    if os.path.isfile(theirs) and os.path.isfile(file) and filecmp.cmp(theirs, file, shallow=False):
        stat = os.stat(theirs)
        os.utime(file, ns=(stat.st_atime_ns, stat.st_mtime_ns))
    elif os.path.isfile(file):
        os.utime(file)
untracked = listed("ls-files", "-z", "--others", "--exclude-standard")
for file in untracked + listed("grep", "-lz", "CARGO_MANIFEST_DIR", "--", "crates"):
    os.utime(file)
STAMP

# Copied beside its place and moved in, so a copy cut short is never taken
# for a build directory. robocopy's exit codes below 8 are successes.
mkdir -p "$(dirname "$TO")"
rm -rf "$TO.warming"
MSYS_NO_PATHCONV=1 robocopy "$(cygpath -w "$FROM")" "$(cygpath -w "$TO.warming")" \
  /E /COPY:DAT /DCOPY:T /MT:32 /NFL /NDL /NJH /NJS /NP >/dev/null || [ $? -lt 8 ]
mv "$TO.warming" "$TO"
touch "$TO/.warmed"
echo "warm-worktree: build directory copied from $MAIN" >&2
