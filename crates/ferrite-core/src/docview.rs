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
    /// How many distinct files this Thread has touched.
    pub files: usize,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tests {
    Passed,
    Failed,
}

impl Instruments {
    /// Read the Blocks a Pane already holds. Nothing here asks the provider
    /// for anything it did not already say.
    pub fn of(transcript: &Transcript) -> Self {
        let mut instruments = Instruments::default();
        let mut files: Vec<&str> = Vec::new();
        for block in transcript.blocks() {
            let Body::Tool(tool) = &block.body else {
                continue;
            };
            if let Some(diff) = &tool.diff {
                instruments.added += diff.added;
                instruments.removed += diff.removed;
                if !files.contains(&diff.path.as_str()) {
                    files.push(&diff.path);
                }
            }
            match &tool.state {
                ToolState::Running => {
                    instruments.running += 1;
                    // The newest still-running call wins the activity line.
                    instruments.activity = Some(activity_line(tool));
                }
                // The newest run wins: a Pane flying a stale red flag is
                // worse than one flying none.
                state if is_test_run(tool) => {
                    instruments.tests = Some(match state {
                        ToolState::Failed(_) => Tests::Failed,
                        _ => Tests::Passed,
                    })
                }
                _ => {}
            }
        }
        instruments.files = files.len();
        instruments.todos = transcript.todos();
        instruments
    }
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
/// otherwise clear a red suite that nobody had rerun.
fn is_test_run(tool: &ToolBlock) -> bool {
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

/// glance.md's zoom ladder: UNDER 200PX → Wall, 200–380PX → Instruments,
/// OVER 380PX → Transcript. The captions give one number per level, and
/// only width fits all three example cells (L2 280×176 and L1 400×264 would
/// both fail on height) — so width is the query axis, resolving glance.md
/// §8.1 on the comps' own evidence.
const TRANSCRIPT_OVER: f32 = 380.0;
const INSTRUMENTS_FROM: f32 = 200.0;

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

    /// Size decides — the cell's width, per the ladder's captions.
    pub fn for_cell(cell: Cell) -> Self {
        if cell.width > TRANSCRIPT_OVER {
            Level::Transcript
        } else if cell.width >= INSTRUMENTS_FROM {
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
        assert_eq!(instruments.files, 2);
        assert_eq!(instruments.tests, Some(Tests::Failed));
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
        assert_eq!(Instruments::of(&transcript).tests, Some(Tests::Failed));

        // An Edit whose path merely contains "test".
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "t2".into(),
            name: "Edit".into(),
            input: serde_json::json!({ "file_path": "tests/foo.rs" }),
        }));
        transcript.apply(finished("t2", false, edit("tests/foo.rs", &["+ok"])));
        assert_eq!(
            Instruments::of(&transcript).tests,
            Some(Tests::Failed),
            "an edit under tests/ must not clear a red suite"
        );

        // And a command that merely contains "spec".
        transcript.apply(tool("t3", "Bash", "git inspect-nothing"));
        transcript.apply(finished("t3", false, ToolResult::Opaque));
        assert_eq!(Instruments::of(&transcript).tests, Some(Tests::Failed));
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
        assert_eq!(Instruments::of(&transcript).tests, Some(Tests::Passed));

        // A command that is not a test run says nothing about tests.
        transcript.apply(tool("t3", "Bash", "git status"));
        transcript.apply(finished("t3", true, ToolResult::Opaque));
        assert_eq!(Instruments::of(&transcript).tests, Some(Tests::Passed));
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
        // The wall's own computed cell (~142px wide) stays at wall level.
        assert_eq!(Level::for_cell(Cell::new(142.3, 115.5)), Level::Wall);
        // Boundaries: "under 200" and "over 380" are strict.
        assert_eq!(Level::for_cell(Cell::new(200.0, 500.0)), Level::Instruments);
        assert_eq!(Level::for_cell(Cell::new(380.0, 500.0)), Level::Instruments);
        assert_eq!(Level::for_cell(Cell::new(380.5, 500.0)), Level::Transcript);
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
        assert_eq!(Instruments::of(&transcript).activity, None);

        // An overlong command is cut to a glanceable fragment.
        transcript.apply(tool("t3", "Bash", &"x".repeat(100)));
        let line = Instruments::of(&transcript).activity.unwrap();
        assert_eq!(line.chars().count(), 41);
        assert!(line.ends_with('…'));
    }
}
