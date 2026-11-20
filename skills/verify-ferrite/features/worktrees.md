# Worktrees

A Thread can work in its own git worktree on a new branch, so parallel agents don't
share a checkout. Ferrite creates the worktree under the repo's `.worktrees/`, shows
the branch in the Pane header, and removes a clean worktree on request (its branch stays).

## Sub-features

- `worktree-create` makes `.worktrees/<branch>` on a new branch from the current HEAD.
- `worktree-thread` starts a Thread directly in a new worktree.
- `worktree-remove` removes a clean worktree and parks its Threads; the branch stays.

## How to get to it (user POV)

- `cmd-shift-n`: new Thread in its own worktree; the new-thread picker's worktree/branch options.
- Scripting: `ferrite worktree create|remove`, `ferrite thread start --worktree --branch <b>`.

## Driving it with fv

Preconditions:

- Baseline run; the seed repo is clean on `main`.

- **Create.** Run `fv cli -- worktree create --cwd /tmp/fv-$FV_RUN/repo --branch fv-branch`. Answer `path` ends `/repo/.worktrees/fv-branch`, `branch: "fv-branch"`, `project: "1"`. `git -C /tmp/fv-$FV_RUN/repo worktree list` shows it on `[fv-branch]`.
- **Thread in a new worktree.** Run `fv cli -- thread start --cwd /tmp/fv-$FV_RUN/repo --worktree --branch fv-thread --title "WT probe" --focus --ready --timeout-ms 60000 --args=--model --args=haiku`. The Thread's `cwd` is under `.worktrees/`; `fv shot worktree-thread` shows `fv-thread` in the Pane header.
- **Remove.** Park that Thread or keep it, then `fv cli -- worktree remove <the path create answered>`. Answer `removed` with that path and `parked` listing any Threads in it. `git worktree list` no longer shows it; `git -C /tmp/fv-$FV_RUN/repo branch` still lists `fv-branch`.

## Gotchas

- `worktree remove` is never forced: a dirty worktree is refused. Commit or clean it first.
- On macOS paths come back under `/private/tmp`; pass the path as answered to `worktree remove`.
- `worktree-thread` was not driven when this map was seeded; if its flags are refused, record the refusal here.
