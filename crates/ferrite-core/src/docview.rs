//! What a Pane renders, decided by how much room it has.
//!
//! Semantic zoom is not a mode: nothing here is switched by the operator.
//! A cell's size is the whole input, so resizing the grid re-renders every
//! Pane at the altitude its cell can carry.

use crate::transcript::{Body, Todos, ToolBlock, ToolState, Transcript};

/// What L2 shows: the Thread's work, without reading the Thread.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Instruments {
    pub added: usize,
    pub removed: usize,
    /// Every file this Thread's edits touched, rolled up — the CHANGED
    /// strip's chips, in first-touch order. `added`/`removed` above are the
    /// same numbers summed.
    pub changed: Vec<FileChange>,
    /// How the most recent test run ended, if one has run at all.
    pub tests: Option<Tests>,
    /// Tool calls still in flight — what the Thread is doing right now.
    pub running: usize,
    /// The Thread's own plan, where it made one.
    pub todos: Option<Todos>,
    /// What is running right now, named — the newest in-flight tool, as one
    /// trimmed line for L2's `◐` activity row. L3 never pays for this
    /// (`Instruments::of` walks every Block); the wall shows status words.
    pub activity: Option<String>,
    /// The call id behind `activity`, for looking up the running call's
    /// clock in the cockpit's timings.
    pub running_call: Option<String>,
}

/// One touched file's rolled-up diff stat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub added: usize,
    pub removed: usize,
}

/// How the latest test run ended — with the run's own count where its
/// result line reported one, so a chip can say `✓ 41` rather than only
/// pass/fail. `None` is the honest fallback: a line with no number keeps
/// the countless chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tests {
    Passed { count: Option<usize> },
    Failed { count: Option<usize> },
}

impl Instruments {
    /// Read the Blocks a Pane already holds. Nothing here asks the provider
    /// for anything it did not already say.
    ///
    /// Cost: O(blocks), paid per frame by every L2 cell and by L1's CHANGED
    /// strip — never by the wall, whose 24-cell frame budget is why L3 reads
    /// status words instead. If a grid of near Panes ever dips, the deepening
    /// is instruments folded incrementally on `apply` — its own ticket, not a
    /// render-side cache.
    pub fn of(transcript: &Transcript) -> Self {
        let mut instruments = Instruments::default();
        for block in transcript.blocks() {
            let Body::Tool(tool) = &block.body else {
                continue;
            };
            if let Some(diff) = &tool.diff {
                instruments.added += diff.added;
                instruments.removed += diff.removed;
                match instruments
                    .changed
                    .iter_mut()
                    .find(|file| file.path == diff.path)
                {
                    Some(file) => {
                        file.added += diff.added;
                        file.removed += diff.removed;
                    }
                    None => instruments.changed.push(FileChange {
                        path: diff.path.clone(),
                        added: diff.added,
                        removed: diff.removed,
                    }),
                }
            }
            match &tool.state {
                ToolState::Running => {
                    instruments.running += 1;
                    // The newest still-running call wins the activity line —
                    // and names its call id, so the activity row can look up
                    // the call's clock.
                    instruments.activity = Some(activity_line(tool));
                    instruments.running_call = Some(tool.call.clone());
                }
                // The newest run wins: a Pane flying a stale red flag is
                // worse than one flying none.
                ToolState::Unavailable if is_test_run(tool) => instruments.tests = None,
                state if is_test_run(tool) => {
                    instruments.tests = Some(match state {
                        ToolState::Failed(message) => Tests::Failed {
                            count: test_count(message, &["failed", "failing"]),
                        },
                        _ => Tests::Passed {
                            count: tool.result_line.as_deref().and_then(passed_count),
                        },
                    })
                }
                _ => {}
            }
        }
        if instruments.activity.is_none() {
            instruments.activity = transcript.progress().caption();
        }
        instruments.todos = transcript.todos();
        instruments
    }

    /// How many distinct files this Thread has touched.
    pub fn files(&self) -> usize {
        self.changed.len()
    }
}

/// `41 passed (41)` → 41 — the pass count a runner's own result line
/// reports. Shared with the tool-row badge, so the row and the L2 chip can
/// never read the same line differently.
pub fn passed_count(line: &str) -> Option<usize> {
    test_count(line, &["passed", "pass"])
}

/// The number standing directly before one of `words` — `357 passed; 2
/// failed` asked for "failed" is 2. No number, no count: a chip that
/// guessed one would lie, so callers fall back to the countless label.
fn test_count(line: &str, words: &[&str]) -> Option<usize> {
    let line = line.to_lowercase();
    let tokens: Vec<&str> = line
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();
    tokens.windows(2).find_map(|pair| {
        words
            .contains(&pair[1])
            .then(|| pair[0].parse().ok())
            .flatten()
    })
}

/// How much of a running tool's name+argument the activity line keeps —
/// one glanceable fragment, not the whole command.
const ACTIVITY_CHARS: usize = 40;

/// `Bash cargo test --workspace` → the fragment an L2 cell's `◐` row shows.
fn activity_line(tool: &ToolBlock) -> String {
    let line = if tool.summary.is_empty() {
        tool.name.clone()
    } else {
        format!("{} {}", tool.name, tool.summary)
    };
    if line.chars().count() <= ACTIVITY_CHARS {
        return line;
    }
    line.chars().take(ACTIVITY_CHARS).chain(['…']).collect()
}

/// Tools that run a command, whose summary is therefore the command itself.
/// Every other tool's summary is a path, and a path is not a test result.
const COMMAND_RUNNERS: [&str; 2] = ["Bash", "commandExecution"];

/// A tool row that ran a test suite. Gated on the tool actually being a
/// command run: an Edit of `tests/foo.rs` or a Read under `tests/` would
/// otherwise clear a red suite that nobody had rerun. Public because the
/// tool row's pass badge asks the same question the instruments do.
pub fn is_test_run(tool: &ToolBlock) -> bool {
    if !COMMAND_RUNNERS.contains(&tool.name.as_str()) {
        return false;
    }
    let command = tool.summary.to_lowercase();
    // Whole words: "inspect" is not a spec run, and "latest" is not a test.
    command
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| {
            matches!(
                word,
                "test" | "tests" | "spec" | "specs" | "vitest" | "pytest"
            )
        })
}

/// A Pane's cell, in logical pixels. The renderer measures it; nothing here
/// knows what a pixel looks like.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub width: f32,
    pub height: f32,
}

impl Cell {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// How much a Pane can say at its current size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Far: one signal, read across the room.
    Wall,
    /// Mid: what the Thread is doing, without reading it.
    Instruments,
    /// Near: the transcript itself, and a Composer to answer it.
    Transcript,
}

/// The zoom ladder, by both axes. A transcript with its Composer reads
/// from 300px wide and 220px tall — three Panes across a laptop window,
/// or a 3×3 board, are that size and were showing instruments in a
/// column of empty ground. Under that, instruments from 200px wide and
/// 120px tall; under that, the wall's one signal.
const TRANSCRIPT_WIDTH: f32 = 300.0;
const TRANSCRIPT_HEIGHT: f32 = 220.0;
const INSTRUMENTS_WIDTH: f32 = 200.0;
const INSTRUMENTS_HEIGHT: f32 = 120.0;

impl Level {
    /// How many Blocks this level draws. A wall cell draws none — it shows a
    /// signal, not text — and per the Cockpit board L2 is instruments only
    /// ("no transcript, no prompt"); only the near view reads the Thread.
    pub fn visible_blocks(self) -> usize {
        match self {
            Level::Wall | Level::Instruments => 0,
            Level::Transcript => 200,
        }
    }

    /// Size decides — both of the cell's dimensions: a wide strip too
    /// short for a transcript is instruments, a tall sliver too narrow
    /// for one likewise.
    pub fn for_cell(cell: Cell) -> Self {
        if cell.width >= TRANSCRIPT_WIDTH && cell.height >= TRANSCRIPT_HEIGHT {
            Level::Transcript
        } else if cell.width >= INSTRUMENTS_WIDTH && cell.height >= INSTRUMENTS_HEIGHT {
            Level::Instruments
        } else {
            Level::Wall
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::{Input, Transcript};
    use crate::{Hunk, SessionEvent, ToolResult};

    fn tool(id: &str, name: &str, command: &str) -> Input {
        Input::Event(SessionEvent::ToolStarted {
            id: id.into(),
            name: name.into(),
            input: serde_json::json!({ "command": command }),
        })
    }

    fn finished(id: &str, is_error: bool, result: ToolResult) -> Input {
        Input::Event(SessionEvent::ToolCompleted {
            id: id.into(),
            output: String::new(),
            is_error,
            result,
        })
    }

    fn edit(path: &str, lines: &[&str]) -> ToolResult {
        ToolResult::FileEdit {
            path: path.into(),
            hunks: vec![Hunk {
                old_start: 1,
                old_lines: 1,
                new_start: 1,
                new_lines: 1,
                lines: lines.iter().map(|line| line.to_string()).collect(),
            }],
        }
    }

    /// Everything L2 shows comes out of Blocks the Pane already folded — no
    /// new provider event, and nothing the transcript did not already know.
    #[test]
    fn instruments_are_derived_from_blocks_already_folded() {
        let mut transcript = Transcript::default();
        transcript.apply(tool("t1", "Edit", "edit one"));
        transcript.apply(finished(
            "t1",
            false,
            edit("a.rs", &["+one", "+two", "-gone"]),
        ));
        transcript.apply(tool("t2", "Edit", "edit two"));
        transcript.apply(finished("t2", false, edit("b.rs", &["+three"])));
        transcript.apply(tool("t3", "Bash", "cargo test --workspace"));
        transcript.apply(finished("t3", true, ToolResult::Opaque));

        let instruments = Instruments::of(&transcript);

        assert_eq!((instruments.added, instruments.removed), (3, 1));
        assert_eq!(instruments.files(), 2);
        assert_eq!(
            instruments.changed,
            vec![
                FileChange {
                    path: "a.rs".into(),
                    added: 2,
                    removed: 1,
                },
                FileChange {
                    path: "b.rs".into(),
                    added: 1,
                    removed: 0,
                },
            ]
        );
        assert_eq!(instruments.tests, Some(Tests::Failed { count: None }));
        assert_eq!(instruments.todos, None, "this Thread never planned");
    }

    #[test]
    fn a_threads_plan_reaches_the_instruments() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "t1".into(),
            name: "TaskCreate".into(),
            input: serde_json::json!({ "subject": "tidy" }),
        }));

        assert_eq!(
            Instruments::of(&transcript).todos,
            Some(Todos { done: 0, total: 1 })
        );
    }

    /// Editing a file under tests/ is not a test result. The instrument reads
    /// commands that ran, never the paths other tools touched — otherwise a
    /// tidy-up after a red suite quietly turns the Pane green.
    #[test]
    fn touching_a_test_file_is_not_a_test_result() {
        let mut transcript = Transcript::default();
        transcript.apply(tool("t1", "Bash", "cargo test --workspace"));
        transcript.apply(finished("t1", true, ToolResult::Opaque));
        assert_eq!(
            Instruments::of(&transcript).tests,
            Some(Tests::Failed { count: None })
        );

        // An Edit whose path merely contains "test".
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "t2".into(),
            name: "Edit".into(),
            input: serde_json::json!({ "file_path": "tests/foo.rs" }),
        }));
        transcript.apply(finished("t2", false, edit("tests/foo.rs", &["+ok"])));
        assert_eq!(
            Instruments::of(&transcript).tests,
            Some(Tests::Failed { count: None }),
            "an edit under tests/ must not clear a red suite"
        );

        // And a command that merely contains "spec".
        transcript.apply(tool("t3", "Bash", "git inspect-nothing"));
        transcript.apply(finished("t3", false, ToolResult::Opaque));
        assert_eq!(
            Instruments::of(&transcript).tests,
            Some(Tests::Failed { count: None })
        );
    }

    #[test]
    fn the_latest_test_run_is_the_one_that_counts() {
        let mut transcript = Transcript::default();
        assert_eq!(Instruments::of(&transcript).tests, None);

        transcript.apply(tool("t1", "Bash", "cargo test"));
        transcript.apply(finished("t1", true, ToolResult::Opaque));
        transcript.apply(tool("t2", "Bash", "cargo test"));
        transcript.apply(finished("t2", false, ToolResult::Opaque));

        // Green after red is green: the wall must not keep flying an old flag.
        assert_eq!(
            Instruments::of(&transcript).tests,
            Some(Tests::Passed { count: None })
        );

        // A command that is not a test run says nothing about tests.
        transcript.apply(tool("t3", "Bash", "git status"));
        transcript.apply(finished("t3", true, ToolResult::Opaque));
        assert_eq!(
            Instruments::of(&transcript).tests,
            Some(Tests::Passed { count: None })
        );
    }

    /// #22 C8: a runner's own result line gives the chip its count — `✓ 41`
    /// instead of a bare pass flag — and a line with no number falls back
    /// honestly to `None`, which renders the countless chip.
    #[test]
    fn test_counts_fold_from_the_runners_own_lines() {
        let mut transcript = Transcript::default();
        transcript.apply(tool("t1", "Bash", "vitest run tests/unit"));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "t1".into(),
            output: "41 passed (41)".into(),
            is_error: false,
            result: ToolResult::Opaque,
        }));
        assert_eq!(
            Instruments::of(&transcript).tests,
            Some(Tests::Passed { count: Some(41) })
        );

        transcript.apply(tool("t2", "Bash", "cargo test --workspace"));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "t2".into(),
            output: "test result: FAILED. 357 passed; 2 failed".into(),
            is_error: true,
            result: ToolResult::Opaque,
        }));
        assert_eq!(
            Instruments::of(&transcript).tests,
            Some(Tests::Failed { count: Some(2) })
        );

        // A run whose line carries no number stays countless — never a
        // guessed digit.
        transcript.apply(tool("t3", "Bash", "cargo test --workspace"));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "t3".into(),
            output: "all suites green".into(),
            is_error: false,
            result: ToolResult::Opaque,
        }));
        assert_eq!(
            Instruments::of(&transcript).tests,
            Some(Tests::Passed { count: None })
        );

        // Whole tokens only, and both failure words.
        assert_eq!(passed_count("41 passed (41)"), Some(41));
        assert_eq!(passed_count("ok. 359 passed; 0 failed"), Some(359));
        assert_eq!(passed_count("compassed nothing"), None);
        assert_eq!(passed_count("all passed"), None);
        assert_eq!(test_count("2 FAILING checks", &["failing"]), Some(2));
        assert_eq!(test_count("it failed", &["failed", "failing"]), None);
    }

    #[test]
    fn each_level_draws_only_what_it_can_show() {
        // The budget is the frame's, not the transcript's: walking history
        // nobody can read is what turns a 120fps wall into a 30fps one.
        // L2 draws instruments, not prose (Cockpit board), so it too reads
        // no Blocks.
        assert_eq!(Level::Wall.visible_blocks(), 0);
        assert_eq!(Level::Instruments.visible_blocks(), 0);
        assert!(Level::Transcript.visible_blocks() >= 100);
    }

    /// glance.md's ladder, on its own example cells: UNDER 200PX → Wall,
    /// 200–380PX → Instruments, OVER 380PX → Transcript.
    #[test]
    fn size_alone_decides_the_level() {
        // The three cells the ZoomLadder board draws.
        assert_eq!(Level::for_cell(Cell::new(400.0, 264.0)), Level::Transcript);
        assert_eq!(Level::for_cell(Cell::new(280.0, 176.0)), Level::Instruments);
        assert_eq!(Level::for_cell(Cell::new(160.0, 100.0)), Level::Wall);
        // Three across a 1440px window, and a 3×3 board: transcripts.
        assert_eq!(Level::for_cell(Cell::new(372.0, 880.0)), Level::Transcript);
        assert_eq!(Level::for_cell(Cell::new(372.0, 285.0)), Level::Transcript);
        // A strip too short for a transcript is instruments however wide;
        // too short for instruments, the wall.
        assert_eq!(Level::for_cell(Cell::new(900.0, 200.0)), Level::Instruments);
        assert_eq!(Level::for_cell(Cell::new(900.0, 100.0)), Level::Wall);
        // The wall's own computed cell (~142px wide) stays at wall level.
        assert_eq!(Level::for_cell(Cell::new(142.3, 115.5)), Level::Wall);
        // Boundaries: 300×220 reads, a hair under does not; 200 wide is
        // instruments, a hair under is the wall.
        assert_eq!(Level::for_cell(Cell::new(300.0, 220.0)), Level::Transcript);
        assert_eq!(Level::for_cell(Cell::new(299.9, 500.0)), Level::Instruments);
        assert_eq!(Level::for_cell(Cell::new(500.0, 219.9)), Level::Instruments);
        assert_eq!(Level::for_cell(Cell::new(200.0, 500.0)), Level::Instruments);
        assert_eq!(Level::for_cell(Cell::new(199.9, 500.0)), Level::Wall);
    }

    /// The comps' running cells name the tool in flight; the activity line
    /// is folded here so the Pane never re-derives it.
    #[test]
    fn the_newest_running_tool_names_the_activity_line() {
        let mut transcript = Transcript::default();
        assert_eq!(Instruments::of(&transcript).activity, None);

        transcript.apply(tool("t1", "Bash", "vitest run tests/unit"));
        assert_eq!(
            Instruments::of(&transcript).activity.as_deref(),
            Some("Bash vitest run tests/unit")
        );

        // A second call supersedes the first; a settled one names nothing.
        transcript.apply(tool("t2", "Bash", "cargo check"));
        assert_eq!(
            Instruments::of(&transcript).activity.as_deref(),
            Some("Bash cargo check")
        );
        transcript.apply(finished("t2", false, ToolResult::Opaque));
        assert_eq!(
            Instruments::of(&transcript).activity.as_deref(),
            Some("Bash vitest run tests/unit"),
            "t1 is still in flight"
        );
        transcript.apply(finished("t1", false, ToolResult::Opaque));
        assert_eq!(
            Instruments::of(&transcript).activity.as_deref(),
            Some("Working")
        );

        // An overlong command is cut to a glanceable fragment.
        transcript.apply(tool("t3", "Bash", &"x".repeat(100)));
        let line = Instruments::of(&transcript).activity.unwrap();
        assert_eq!(line.chars().count(), 41);
        assert!(line.ends_with('…'));
    }
}
