//! Following the agent: which checkout a Thread's Main is working in, read
//! from what it does rather than from where it started. The binding stays
//! the one truth every spawn and every header reads; this module only
//! decides when that truth should move.
//!
//! Three signals, each answering with a [`Move`] the cockpit applies:
//!
//! 1. **Creation.** A Main command that says `worktree add` (or Claude's
//!    `EnterWorktree`) freezes the worktree list as it stood when the call
//!    began. When git next lists the repo, a worktree that was not there
//!    before AND is named in the command is the one this Thread made. Two
//!    candidates or none: nothing moves — another Thread may have made it.
//! 2. **Exit.** Claude's `ExitWorktree` completing sends the Thread back
//!    where it entered from — the main checkout when that is not known. A
//!    bound worktree vanishing from git's list sends it to main.
//! 3. **Evidence.** Tool inputs name paths (`cwd`, `file_path`, …). A run of
//!    [`EVIDENCE_CALLS`] consecutive Main calls inside one other worktree of
//!    the same repo, with none in the current checkout between them, moves
//!    the Thread there. A single read elsewhere for comparison never does.
//!
//! Nothing here runs git or parses shell: the listing is handed in, and the
//! only text check is whether a command mentions a worktree's name.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Consecutive Main tool calls inside one other checkout before the Thread
/// follows them there.
pub const EVIDENCE_CALLS: u8 = 3;

/// Where a Thread should now be bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Move {
    /// This worktree — git's own path for it. The cockpit maps the main
    /// checkout's path back to a main binding.
    Into(PathBuf),
    /// The repo's main checkout.
    ToMain,
}

#[derive(Debug)]
enum Watched {
    Creation {
        before: Vec<PathBuf>,
        hint: String,
        completed: bool,
    },
    Exit,
}

/// One Thread's observations, owned by its live state.
#[derive(Debug, Default)]
pub struct Follow {
    /// Git's worktree list for the Thread's repo, canonical, as last handed
    /// in — the main checkout included.
    known: Vec<PathBuf>,
    /// Calls that may move the Thread once they finish, in call order.
    watched: Vec<(String, Watched)>,
    /// The other checkout recent Main calls landed in, and how many in a row.
    evidence: Option<(PathBuf, u8)>,
    /// Where the Thread was when Main entered a worktree — what
    /// `ExitWorktree` restores.
    origin: Option<PathBuf>,
}

impl Follow {
    /// A Main tool call settled. `current` is the checkout the Thread is
    /// bound to now; relative paths in the input resolve against it.
    pub fn observe_started(
        &mut self,
        id: &str,
        name: &str,
        input: &Value,
        current: &Path,
    ) -> Option<Move> {
        match name {
            "ExitWorktree" => {
                self.watched.push((id.to_string(), Watched::Exit));
                None
            }
            "EnterWorktree" => {
                let hint = input["path"]
                    .as_str()
                    .or(input["name"].as_str())
                    .unwrap_or_default()
                    .to_string();
                self.origin = Some(canonical(current));
                self.watch_creation(id, hint);
                None
            }
            _ => {
                if let Some(command) = command_text(input) {
                    if command.contains("worktree add") {
                        self.watch_creation(id, command);
                    }
                    // A command's own cwd is evidence; its text never is.
                    return input["cwd"]
                        .as_str()
                        .and_then(|cwd| self.evidence(Path::new(cwd), current));
                }
                let path = ["file_path", "notebook_path", "path"]
                    .iter()
                    .find_map(|key| input[*key].as_str())
                    .or_else(|| input["changes"][0]["path"].as_str())?;
                self.evidence(Path::new(path), current)
            }
        }
    }

    /// A Main tool call finished.
    pub fn observe_completed(&mut self, id: &str, is_error: bool) -> Option<Move> {
        let index = self.watched.iter().position(|(watched, _)| watched == id)?;
        if is_error {
            self.watched.remove(index);
            return None;
        }
        match &mut self.watched[index].1 {
            Watched::Exit => {
                self.watched.remove(index);
                self.evidence = None;
                Some(self.origin.take().map_or(Move::ToMain, Move::Into))
            }
            Watched::Creation { completed, .. } => {
                *completed = true;
                None
            }
        }
    }

    /// Whether a finished creation is waiting on a fresh listing to name
    /// what it made — the driver's cue to ask git now rather than on the
    /// next tick.
    pub fn wants_listing(&self) -> bool {
        self.watched.iter().any(|(_, watched)| {
            matches!(
                watched,
                Watched::Creation {
                    completed: true,
                    ..
                }
            )
        })
    }

    /// Git's current worktree list for the Thread's repo. `bound` is the
    /// worktree the Thread is bound to, when it is one that Ferrite did not
    /// mint: gone from the list, the Thread returns to main.
    pub fn observe_listing(&mut self, listed: Vec<PathBuf>, bound: Option<&Path>) -> Option<Move> {
        let listed: Vec<PathBuf> = listed.iter().map(|path| canonical(path)).collect();
        self.known = listed.clone();
        if let Some(bound) = bound {
            if !listed.contains(&canonical(bound)) {
                self.reset();
                return Some(Move::ToMain);
            }
        }
        let mut moved = None;
        let mut index = 0;
        while index < self.watched.len() {
            let Watched::Creation {
                before,
                hint,
                completed: true,
            } = &self.watched[index].1
            else {
                index += 1;
                continue;
            };
            let named: Vec<&PathBuf> = listed
                .iter()
                .filter(|path| !before.contains(path) && mentions(hint, path))
                .collect();
            if moved.is_none() && named.len() == 1 {
                moved = Some(Move::Into(named[0].clone()));
            }
            self.watched.remove(index);
        }
        if moved.is_some() {
            self.evidence = None;
        }
        moved
    }

    /// Forget everything gathered toward a move — after one lands. Where an
    /// entered worktree was entered from is kept: that is for the exit.
    pub fn reset(&mut self) {
        self.watched.clear();
        self.evidence = None;
    }

    fn watch_creation(&mut self, id: &str, hint: String) {
        self.watched.push((
            id.to_string(),
            Watched::Creation {
                before: self.known.clone(),
                hint,
                completed: false,
            },
        ));
    }

    fn evidence(&mut self, path: &Path, current: &Path) -> Option<Move> {
        let current = canonical(current);
        let path = canonical(&if path.is_absolute() {
            path.to_path_buf()
        } else {
            current.join(path)
        });
        let checkout = self
            .known
            .iter()
            .filter(|known| path.starts_with(known))
            .max_by_key(|known| known.components().count())?
            .clone();
        if checkout == current {
            self.evidence = None;
            return None;
        }
        let count = match &self.evidence {
            Some((seen, count)) if *seen == checkout => count + 1,
            _ => 1,
        };
        if count >= EVIDENCE_CALLS {
            self.evidence = None;
            return Some(Move::Into(checkout));
        }
        self.evidence = Some((checkout, count));
        None
    }
}

/// A command tool's text: Claude's `command` string, or Codex's argv.
fn command_text(input: &Value) -> Option<String> {
    match &input["command"] {
        Value::String(text) => Some(text.clone()),
        Value::Array(words) => Some(
            words
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => None,
    }
}

/// Whether a command names a worktree: its whole path, or its leaf.
fn mentions(hint: &str, path: &Path) -> bool {
    hint.contains(&*path.to_string_lossy())
        || path
            .file_name()
            .is_some_and(|leaf| hint.contains(&*leaf.to_string_lossy()))
}

/// The path as git would print it: symlinks resolved. A path that does
/// not exist yet resolves through its nearest standing ancestor, so a file
/// about to be written still names its checkout.
pub fn canonical(path: &Path) -> PathBuf {
    if let Ok(resolved) = fs::canonicalize(path) {
        return resolved;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(leaf)) => canonical(parent).join(leaf),
        _ => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrite-follow-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        canonical(&dir)
    }

    /// A repo layout on disk: the main checkout and worktrees under it.
    fn layout(name: &str, worktrees: &[&str]) -> (PathBuf, Vec<PathBuf>) {
        let main = scratch(name);
        let mut listed = vec![main.clone()];
        for worktree in worktrees {
            let path = main.join(".worktrees").join(worktree);
            fs::create_dir_all(&path).unwrap();
            listed.push(path);
        }
        (main, listed)
    }

    #[test]
    fn a_worktree_the_command_made_and_named_moves_the_thread() {
        let (main, before) = layout("creation", &[]);
        let mut follow = Follow::default();
        assert_eq!(follow.observe_listing(before, None), None);

        let started = follow.observe_started(
            "t1",
            "Bash",
            &json!({ "command": "git worktree add .worktrees/feat-a -b feat-a" }),
            &main,
        );
        assert_eq!(started, None);
        assert!(
            !follow.wants_listing(),
            "nothing to list until the command finishes"
        );
        assert_eq!(follow.observe_completed("t1", false), None);
        assert!(follow.wants_listing());

        let (_, after) = layout("creation", &["feat-a"]);
        assert_eq!(
            follow.observe_listing(after.clone(), None),
            Some(Move::Into(after[1].clone()))
        );
        assert!(!follow.wants_listing());
    }

    #[test]
    fn a_worktree_another_thread_made_does_not_move_this_one() {
        let (main, before) = layout("other-thread", &[]);
        let mut follow = Follow::default();
        follow.observe_listing(before, None);
        follow.observe_started(
            "t1",
            "Bash",
            &json!({ "command": "git worktree add .worktrees/mine -b mine" }),
            &main,
        );
        follow.observe_completed("t1", false);

        // Only somebody else's appeared: this Thread's own must have failed
        // silently or landed elsewhere — either way, stay.
        let (_, after) = layout("other-thread", &["theirs"]);
        assert_eq!(follow.observe_listing(after, None), None);
    }

    #[test]
    fn two_new_worktrees_both_named_is_ambiguous_and_stays() {
        let (main, before) = layout("ambiguous", &[]);
        let mut follow = Follow::default();
        follow.observe_listing(before, None);
        follow.observe_started(
            "t1",
            "Bash",
            &json!({ "command": "git worktree add .worktrees/a && git worktree add .worktrees/b" }),
            &main,
        );
        follow.observe_completed("t1", false);
        let (_, after) = layout("ambiguous", &["a", "b"]);
        assert_eq!(follow.observe_listing(after, None), None);
    }

    #[test]
    fn a_failed_creation_is_forgotten() {
        let (main, before) = layout("failed", &[]);
        let mut follow = Follow::default();
        follow.observe_listing(before, None);
        follow.observe_started(
            "t1",
            "Bash",
            &json!({ "command": "git worktree add .worktrees/x" }),
            &main,
        );
        assert_eq!(follow.observe_completed("t1", true), None);
        assert!(!follow.wants_listing());
        let (_, after) = layout("failed", &["x"]);
        assert_eq!(follow.observe_listing(after, None), None);
    }

    #[test]
    fn codex_argv_commands_count_too() {
        let (main, before) = layout("argv", &[]);
        let mut follow = Follow::default();
        follow.observe_listing(before, None);
        follow.observe_started(
            "c1",
            "commandExecution",
            &json!({ "command": ["git", "worktree", "add", ".worktrees/argv-wt"], "cwd": main }),
            &main,
        );
        follow.observe_completed("c1", false);
        let (_, after) = layout("argv", &["argv-wt"]);
        assert_eq!(
            follow.observe_listing(after.clone(), None),
            Some(Move::Into(after[1].clone()))
        );
    }

    #[test]
    fn claude_entering_a_worktree_by_name_moves_the_thread() {
        let (main, before) = layout("enter", &[]);
        let mut follow = Follow::default();
        follow.observe_listing(before, None);
        follow.observe_started("e1", "EnterWorktree", &json!({ "name": "spike" }), &main);
        follow.observe_completed("e1", false);
        let (_, after) = layout("enter", &["spike"]);
        assert_eq!(
            follow.observe_listing(after.clone(), None),
            Some(Move::Into(after[1].clone()))
        );
    }

    #[test]
    fn claude_exiting_a_worktree_returns_to_where_it_entered_from() {
        let (_, listed) = layout("exit", &["home"]);
        let home = listed[1].clone();
        let mut follow = Follow::default();
        follow.observe_listing(listed, None);
        follow.observe_started("e1", "EnterWorktree", &json!({ "name": "spike" }), &home);
        follow.observe_completed("e1", false);
        let (_, after) = layout("exit", &["home", "spike"]);
        assert_eq!(
            follow.observe_listing(after.clone(), None),
            Some(Move::Into(after[2].clone()))
        );
        follow.reset();

        assert_eq!(
            follow.observe_started("x1", "ExitWorktree", &json!({}), &after[2]),
            None
        );
        assert_eq!(
            follow.observe_completed("x1", false),
            Some(Move::Into(home))
        );
    }

    #[test]
    fn claude_exiting_with_no_remembered_origin_returns_to_main() {
        let mut follow = Follow::default();
        let main = scratch("exit-cold");
        follow.observe_started("x1", "ExitWorktree", &json!({}), &main);
        assert_eq!(follow.observe_completed("x1", false), Some(Move::ToMain));
    }

    #[test]
    fn a_bound_worktree_gone_from_the_list_returns_to_main() {
        let (_, listed) = layout("vanished", &["gone"]);
        let bound = listed[1].clone();
        let mut follow = Follow::default();
        assert_eq!(follow.observe_listing(listed.clone(), Some(&bound)), None);
        assert_eq!(
            follow.observe_listing(vec![listed[0].clone()], Some(&bound)),
            Some(Move::ToMain)
        );
    }

    #[test]
    fn a_run_of_edits_in_another_worktree_moves_the_thread() {
        let (main, listed) = layout("evidence", &["wt"]);
        let worktree = listed[1].clone();
        let mut follow = Follow::default();
        follow.observe_listing(listed, None);
        let edit = |follow: &mut Follow, id: &str, path: &Path| {
            follow.observe_started(
                id,
                "Edit",
                &json!({ "file_path": path.join("src/lib.rs") }),
                &main,
            )
        };
        assert_eq!(edit(&mut follow, "1", &worktree), None);
        assert_eq!(edit(&mut follow, "2", &worktree), None);
        assert_eq!(
            edit(&mut follow, "3", &worktree),
            Some(Move::Into(worktree.clone()))
        );
    }

    #[test]
    fn a_touch_in_the_current_checkout_resets_the_run() {
        let (main, listed) = layout("reset", &["wt"]);
        let worktree = listed[1].clone();
        let mut follow = Follow::default();
        follow.observe_listing(listed, None);
        let edit = |follow: &mut Follow, id: &str, path: &Path| {
            follow.observe_started(
                id,
                "Read",
                &json!({ "file_path": path.join("a.rs") }),
                &main,
            )
        };
        edit(&mut follow, "1", &worktree);
        edit(&mut follow, "2", &worktree);
        // Back home for one call: the run starts over.
        edit(&mut follow, "3", &main);
        assert_eq!(edit(&mut follow, "4", &worktree), None);
        assert_eq!(edit(&mut follow, "5", &worktree), None);
        assert_eq!(
            edit(&mut follow, "6", &worktree),
            Some(Move::Into(worktree))
        );
    }

    #[test]
    fn paths_outside_every_known_checkout_are_ignored() {
        let (main, listed) = layout("outside", &["wt"]);
        let elsewhere = scratch("outside-elsewhere");
        let mut follow = Follow::default();
        follow.observe_listing(listed, None);
        for id in ["1", "2", "3", "4"] {
            assert_eq!(
                follow.observe_started(
                    id,
                    "Read",
                    &json!({ "file_path": elsewhere.join("settings.json") }),
                    &main
                ),
                None
            );
        }
    }

    #[test]
    fn a_relative_path_resolves_against_the_current_checkout() {
        let (main, listed) = layout("relative", &["wt"]);
        let worktree = listed[1].clone();
        let mut follow = Follow::default();
        follow.observe_listing(listed, None);
        for id in ["1", "2"] {
            follow.observe_started(
                id,
                "Edit",
                &json!({ "file_path": ".worktrees/wt/main.rs" }),
                &main,
            );
        }
        assert_eq!(
            follow.observe_started(
                "3",
                "Edit",
                &json!({ "file_path": ".worktrees/wt/main.rs" }),
                &main
            ),
            Some(Move::Into(worktree))
        );
    }

    #[test]
    fn a_codex_command_cwd_is_evidence_but_its_text_is_not() {
        let (main, listed) = layout("codex-cwd", &["wt"]);
        let worktree = listed[1].clone();
        let mut follow = Follow::default();
        follow.observe_listing(listed, None);
        // Text mentioning the worktree moves nothing on its own.
        for id in ["1", "2", "3"] {
            assert_eq!(
                follow.observe_started(
                    "t",
                    "commandExecution",
                    &json!({ "command": format!("ls {}", worktree.display()), "cwd": main, "id": id }),
                    &main
                ),
                None
            );
        }
        for id in ["4", "5"] {
            follow.observe_started(
                id,
                "commandExecution",
                &json!({ "command": "cargo test", "cwd": worktree }),
                &main,
            );
        }
        assert_eq!(
            follow.observe_started(
                "6",
                "commandExecution",
                &json!({ "command": "cargo test", "cwd": worktree }),
                &main
            ),
            Some(Move::Into(worktree))
        );
    }
}
