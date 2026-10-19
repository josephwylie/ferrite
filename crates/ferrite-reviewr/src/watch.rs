//! Ferrite's stand-in for upstream's `watch` (the worktree watcher, dropped): the gathering span
//! turn tracking's race window is sized by. Ferrite runs no watcher, so every turn is judged at
//! its end by the worktree itself ([`crate::turn::TurnHost::watcher_down`]).

use std::time::Duration;

/// How long a batch gathers after its first event. `FSEvents` itself coalesces for the same span.
pub(crate) const GATHER: Duration = Duration::from_millis(50);
