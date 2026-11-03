# Lane brief (every implementation lane reads this first)

Worktree: `/Users/josephwylie/Desktop/Projects/ferrite/.worktrees/transcript-app-style`, branch `transcript-app-style`. Base: `21d4810e` (pane.rs split + token pass).

You are one of 8 lanes editing this worktree **at the same time**. Your lane's row in `PLAN.md` lists the files you own. Edit only those files, plus test files that test them. If a change truly needs a file you don't own, don't make it. Write the exact change you need in your report instead.

## Spec

- `PLAN.md`: your lane's row, which is the scope.
- `geometry.md`, `typecolour.md`, `components.md`, `motion-measured.md`: the measured target values, with evidence. Implement the rows for your components. Use the tokens in `crates/ferrite-ui/src/theme.rs`. Add a token only when a value is new; don't hard-code.
- The images in `claude/`, `codex/` and `ferrite/` (Ferrite is 2x) show what each target looks like. View them.
- `docs/research/claude-desktop-transcript-handoff.md`: how Claude Desktop builds its transcript. Read the sections your lane names.
- Project rules: clean code, YAGNI. Delete what the change makes dead. Refactor before piling on.

## Rules

- **Compile:** `CARGO_TARGET_DIR=$HOME/.cache/codingos-target cargo check -p <your crate> --tests -j 4`, at most 3 times. Other lanes hold the build lock at times, so just wait for it. Never run two cargo commands yourself at once.
- **If check fails in a file you don't own:** that's another lane mid-edit. Re-run once later; if it still fails, report it and carry on.
- **Tests:** no `cargo test`, no visual reference, no clippy. Update tests that pin the old behaviour you changed, and add tests for new behaviour, but don't run them. The orchestrator runs them at the checkpoint and sends failures back to you.
- **Git:** no commits, no stash, no checkout, no reset. The orchestrator commits per lane.
- **Off limits:** don't touch any running Ferrite app or `ferrite server`, don't install anything, never `rm` through a shell variable.

## Report

- Files changed.
- What you implemented, against which spec row.
- What you skipped and why.
- Changes you need in files you don't own.
- Tests you edited or added.
- The last cargo check result.
