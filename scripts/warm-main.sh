#!/usr/bin/env bash
#
# warm-main.sh — build, in the main checkout, what a new worktree starts
# with: scripts/warm-worktree.sh clones this checkout's `target/debug`, so a
# worktree compiles everything that changed since these builds last ran.
#
# One build for each set of compiled output the commonest commands use. The
# app, the Cockpit tests and the core tests each compile the GPUI stack their
# own way (tests turn on its test support), and `cargo check` keeps its own
# output for each of those again. A command left out still works in a new
# worktree; it only starts from further back there.
#
# scripts/install-app.sh runs this after an install, so the build a worktree
# starts with is as new as the installed app. Does nothing in a worktree.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
HERE="$(git rev-parse --show-toplevel)"
MAIN="$(git worktree list --porcelain | sed -n '1s/^worktree //p')"
[ "$HERE" = "$MAIN" ] || exit 0

cargo build --locked -p ferrite
cargo test --locked --no-run -p ferrite-cockpit --lib
cargo test --locked --no-run -p ferrite-core
cargo check --locked -p ferrite
cargo check --locked -p ferrite-cockpit --tests
cargo check --locked --workspace --tests
