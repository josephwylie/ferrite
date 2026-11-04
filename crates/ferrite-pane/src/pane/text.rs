//! Logical copy fragments, collected without allocating GPUI elements.
//!
//! Keep disclosure/order rules aligned with the row renderer. The native
//! rendering tests compare both projections, including hidden tool details.
use super::*;
use ferrite_core::transcript::tools::{self, ToolKind};

/// What a call whose result never came says after its call line.
pub(super) const NO_RESULT: &str = "no result";
/// What a call the operator stopped says (a denial, or the turn's stop).
pub(super) const STOPPED: &str = "stopped";

/// How a turn's end reads in the transcript, if it says anything: a
/// completed turn `Worked for 5m 22s` once it took `TURN_STAMP_MIN_MS` (a
/// quicker one, or one whose time was never observed, says nothing); an
/// interrupted or failed one always, as core words it (`failed · 0.1s ·
/// <message>`).
pub(crate) fn turn_stamp(end: &ferrite_core::transcript::TurnEnd) -> Option<String> {
    if !end.completed() {
        return Some(end.text());
    }
    let elapsed = end
        .elapsed_ms
        .filter(|ms| *ms >= theme::TURN_STAMP_MIN_MS)?;
    Some(format!(
        "Worked for {}",
        ferrite_core::progress::settled_duration_label(std::time::Duration::from_millis(elapsed))
    ))
}

/// What kind of work a call did, for its call line's verb and its group's
/// summary: core's [`ToolKind`] (decided by the tool alone: a command is a
/// command whatever it ran — `cat` is not read as a read without knowing
/// what the shell made of it), and the name a call with no verb of its own
/// goes by (`github`, `TodoWrite`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Work<'a> {
    kind: ToolKind<'a>,
    /// An MCP call's server; the shown name of a call with no verb; empty
    /// for the rest.
    name: &'a str,
}

impl<'a> Work<'a> {
    fn of(tool: &'a ToolBlock) -> Self {
        let policy = tools::policy(&tool.name);
        let name = match policy.kind {
            ToolKind::Mcp(server) => server,
            ToolKind::Todo | ToolKind::Question | ToolKind::Visual | ToolKind::Other => {
                policy.display
            }
            _ => "",
        };
        Self {
            kind: policy.kind,
            name,
        }
    }

    /// `Ran 3 commands`, `Reading 1 file`, `Called github 2 times`: the
    /// verb in the present tense while one of its calls runs.
    fn phrase(&self, count: usize, running: bool) -> String {
        let counted = |active: &str, done: &str, one: &str, many: &str| {
            let verb = if running { active } else { done };
            format!("{verb} {count} {}", if count == 1 { one } else { many })
        };
        let verb = match self.kind {
            ToolKind::Command => return counted("Running", "Ran", "command", "commands"),
            ToolKind::Read => return counted("Reading", "Read", "file", "files"),
            ToolKind::Edit => return counted("Editing", "Edited", "file", "files"),
            ToolKind::Write => return counted("Writing", "Wrote", "file", "files"),
            ToolKind::Search => {
                return counted("Searching for", "Searched for", "pattern", "patterns")
            }
            ToolKind::List => return counted("Listing", "Listed", "directory", "directories"),
            ToolKind::Fetch => return counted("Fetching", "Fetched", "page", "pages"),
            ToolKind::WebSearch => return counted("Running", "Ran", "web search", "web searches"),
            ToolKind::Agent => return counted("Running", "Ran", "agent", "agents"),
            ToolKind::Mcp(_) if running => "Calling",
            ToolKind::Mcp(_) => "Called",
            ToolKind::Todo | ToolKind::Question | ToolKind::Visual | ToolKind::Other if running => {
                "Using"
            }
            ToolKind::Todo | ToolKind::Question | ToolKind::Visual | ToolKind::Other => "Used",
        };
        match count {
            1 => format!("{verb} {}", self.name),
            count => format!("{verb} {} {count} times", self.name),
        }
    }

    /// One call's verb, in the present tense while it runs: `Ran`, `Read`,
    /// `Edited`, `Ran agent`; a tool with no verb of its own is named
    /// (`github`, `TodoWrite`).
    fn verb(&self, running: bool) -> &'a str {
        let tense = |active, done| if running { active } else { done };
        match self.kind {
            ToolKind::Command => tense("Running", "Ran"),
            ToolKind::Read => tense("Reading", "Read"),
            ToolKind::Edit => tense("Editing", "Edited"),
            ToolKind::Write => tense("Writing", "Wrote"),
            ToolKind::Search => tense("Searching for", "Searched for"),
            ToolKind::List => tense("Listing", "Listed"),
            ToolKind::Fetch => tense("Fetching", "Fetched"),
            ToolKind::WebSearch => tense("Searching the web for", "Searched the web for"),
            ToolKind::Agent => tense("Running agent", "Ran agent"),
            ToolKind::Mcp(_)
            | ToolKind::Todo
            | ToolKind::Question
            | ToolKind::Visual
            | ToolKind::Other => self.name,
        }
    }

    /// Whether a call of this kind takes a literal argument (a command, a
    /// path, a pattern, a URL), set in Geist Mono; an agent's task, a web
    /// query or another tool's subject is words, set as the line is.
    fn literal(&self) -> bool {
        matches!(
            self.kind,
            ToolKind::Command
                | ToolKind::Read
                | ToolKind::Edit
                | ToolKind::Write
                | ToolKind::Search
                | ToolKind::List
                | ToolKind::Fetch
        )
    }

    /// An edit or a write: what its group's summary leads with and names.
    fn changes_files(&self) -> bool {
        matches!(self.kind, ToolKind::Edit | ToolKind::Write)
    }
}

/// A call line as it reads and copies, in three runs: the verb in the app's
/// words (`Ran `, its space carried when an argument follows; empty when the
/// row leads with the provider's description), the argument as written
/// (`cargo test -p ferrite-core`, a path; the description's words; empty when
/// the call has none) — `literal` when it is a command, a path or a pattern —
/// and what came of it after its seam (` · 954 passed`), if anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallLine {
    pub verb: String,
    pub argument: String,
    pub literal: bool,
    pub result: Option<String>,
}

/// The seam before a call line's result.
pub(crate) const RESULT_SEAM: &str = " \u{b7} ";

/// The tag a failed call wears after its call line, shut or open: the one
/// red word on the row, so the failure is never behind a click.
pub(crate) const FAILED_TAG: &str = "Failed";

impl CallLine {
    /// The call line as one Markdown run, so it selects and copies as the
    /// one line it reads: the verb and the result as words, a literal
    /// argument as inline code — a path (`path`) as a link that opens it
    /// (`rich::PATH_SCHEME`) — anything else as words.
    pub(crate) fn markdown(&self, path: bool) -> String {
        let mut source = escape_words(&self.verb);
        if path {
            source.push_str(&path_link(&self.argument, &self.argument));
        } else if self.literal && !self.argument.is_empty() {
            source.push_str(&code_span(&self.argument));
        } else {
            source.push_str(&escape_words(&self.argument));
        }
        if let Some(result) = &self.result {
            source.push_str(&escape_words(result));
        }
        source
    }
}

/// A path in an activity line, as Markdown: `shown` linking to `path`
/// (`rich::PATH_SCHEME`). A path with `<` or `>` in it reads as words.
pub(crate) fn path_link(shown: &str, path: &str) -> String {
    if path.contains(['<', '>']) {
        return escape_words(shown);
    }
    format!(
        "[{}](<{}{path}>)",
        escape_words(shown),
        crate::rich::PATH_SCHEME
    )
}

/// A call's line as its one Markdown run (`CallLine::markdown`), less a
/// failed call's verdict (`call_verdict`), which stands apart from it. A
/// call on a file is drawn in pieces instead (`call_pieces`).
pub(crate) fn call_markdown(tool: &ToolBlock) -> String {
    let mut line = call_line(tool);
    if call_failed(tool) {
        line.result = None;
    }
    line.markdown(argument_is_path(tool))
}

/// A call's line as the row draws it. A call on a file (`Read src/x.rs`)
/// has its path in a run of its own, a link painted over the row's press
/// overlay so that a click opens the file instead of folding the row; the
/// verb before it is chrome (`Read `), not text a selection takes, so
/// copying the row gives the path. Any other call is one run.
pub(crate) enum CallPieces {
    Whole(String),
    File {
        verb: String,
        /// The path, as Markdown linking to it (`path_link`).
        path: String,
        /// What came of it, as Markdown, after the path; empty for none.
        trail: String,
    },
}

pub(crate) fn call_pieces(tool: &ToolBlock) -> CallPieces {
    if !argument_is_path(tool) {
        return CallPieces::Whole(call_markdown(tool));
    }
    let mut line = call_line(tool);
    if call_failed(tool) {
        line.result = None;
    }
    CallPieces::File {
        path: path_link(&line.argument, &line.argument),
        trail: line.result.as_deref().map(escape_words).unwrap_or_default(),
        verb: line.verb,
    }
}

/// A failed call's tag after its call line (`Failed`, `Failed · 357 passed;
/// 2 failed`): its own run, so its lead word alone wears the failure ink and
/// it stays in view when a long line is cut.
pub(crate) fn call_verdict(tool: &ToolBlock) -> Option<String> {
    if !call_failed(tool) {
        return None;
    }
    Some(match call_line(tool).result {
        Some(result) => format!("{FAILED_TAG}{result}"),
        None => FAILED_TAG.to_owned(),
    })
}

/// Words as Markdown that reads them exactly: every ASCII punctuation mark
/// escaped, so nothing in them is markup.
pub(crate) fn escape_words(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii_punctuation() {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Text as one inline code span that reads it exactly: fenced by one more
/// backtick than its longest run, padded where it starts or ends with one.
fn code_span(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for ch in text.chars() {
        run = if ch == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let fence = "`".repeat(longest + 1);
    let pad = if text.starts_with(['`', ' ']) || text.ends_with(['`', ' ']) {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{text}{pad}{fence}")
}

/// A call's line. A command whose provider wrote what it is for (Claude's
/// Bash `description`, carried as `ToolBlock::title`) leads with those
/// words, the command behind its disclosure (`$ cmd` in its open well);
/// otherwise the verb and the argument, a command less a leading `cd <dir>
/// &&` (the open well keeps it).
pub(crate) fn call_line(tool: &ToolBlock) -> CallLine {
    let work = Work::of(tool);
    let result = call_result(tool).map(|result| format!("{RESULT_SEAM}{result}"));
    if work.kind == ToolKind::Command {
        if let Some(description) = tool.title.as_deref().filter(|words| !words.is_empty()) {
            return CallLine {
                verb: String::new(),
                argument: description.to_owned(),
                literal: false,
                result,
            };
        }
    }
    let argument = tool.argument();
    let argument = if work.kind == ToolKind::Command {
        without_cd(&argument).to_owned()
    } else {
        argument.into_owned()
    };
    let mut verb = work.verb(tool.state == ToolState::Running).to_owned();
    if !argument.is_empty() {
        verb.push(' ');
    }
    CallLine {
        verb,
        argument,
        literal: work.literal(),
        result,
    }
}

/// What the working line says of the call it carries (`working::caption_of`),
/// as Claude's reads: the provider's description of the call where it
/// wrote one (Claude's Bash `description`: `Waiting 15 seconds then
/// printing done`), else the verb and a short target (`Reading main.rs`,
/// `Running cargo build`: `tools::short_subject`).
pub(crate) fn working_words(tool: &ToolBlock) -> String {
    if let Some(description) = tool.title.as_deref().map(str::trim) {
        if !description.is_empty() {
            return description.to_owned();
        }
    }
    let work = Work::of(tool);
    let argument = tool.argument();
    let argument: &str = if work.kind == ToolKind::Command {
        without_cd(&argument)
    } else {
        &argument
    };
    let target = tools::short_subject(work.kind, argument);
    let verb = work.verb(tool.state == ToolState::Running);
    if target.is_empty() {
        verb.to_owned()
    } else {
        format!("{verb} {target}")
    }
}

/// A command less a leading `cd <dir> &&`: where it ran is the thread's
/// business, not the row's (`cd crates/x && cargo test` → `cargo test`).
fn without_cd(command: &str) -> &str {
    let trimmed = command.trim_start();
    match trimmed
        .strip_prefix("cd ")
        .and_then(|rest| rest.split_once("&&"))
    {
        Some((_, rest)) if !rest.trim().is_empty() => rest.trim_start(),
        _ => command,
    }
}

/// What a shut call row says after its call line: a test run's verdict
/// (`954 passed`; a failed run's `357 passed; 2 failed` after its tag) and a
/// call whose result never came (`no result`). Nothing else previews on a
/// shut row: a command's output, a read's size and a failure's words are
/// behind its disclosure, the failure itself in the row's `Failed` tag.
fn call_result(tool: &ToolBlock) -> Option<String> {
    match &tool.state {
        ToolState::Running => None,
        ToolState::Unavailable => Some(NO_RESULT.to_owned()),
        ToolState::Stopped => Some(STOPPED.to_owned()),
        ToolState::Failed(_) | ToolState::Ok => {
            // A tally only: an early failure's elbow heads with the run's
            // first line, which is no verdict.
            test_elbow(tool)
                .map(|elbow| elbow.rest)
                .filter(|rest| rest.contains("passed"))
        }
    }
}

/// An agent call's line, as it reads and copies (`agents::render_agent`):
/// `Ran agent Count .rs files`, and the range of its task, the words the
/// row brightens; while the agent runs, `Running agent …` and what it is
/// doing after the seam (its summary, carried as the call's result line
/// while it runs: `AgentFaces::apply`).
pub(crate) fn agent_line(tool: &ToolBlock) -> (String, std::ops::Range<usize>) {
    let running = tool.state == ToolState::Running;
    let mut line = Work::of(tool).verb(running).to_owned();
    let task = tool.argument();
    if !task.is_empty() {
        line.push(' ');
    }
    let start = line.len();
    line.push_str(&task);
    let range = start..line.len();
    if let Some(summary) = tool
        .result_line
        .as_deref()
        .map(str::trim)
        .filter(|summary| running && !summary.is_empty())
    {
        line.push_str(RESULT_SEAM);
        line.push_str(summary);
    }
    (line, range)
}

/// A group's summary line, as it reads and copies: the provider's own
/// caption of its calls (`ToolBlock::caption`) where its newest call carries
/// one, else what its calls did by kind, the first phrase capitalised (`Ran
/// 12 commands, read 1 file`) — an edit or a write of one file names it
/// (`edited notes.md`), and either says what its diffs changed (`+5 −1`).
/// Either way, then what went wrong, in the same words' grey (`(1 failed)`,
/// `(1 failed, 1 without a result)`). Edits and writes lead (the named file
/// is the summary's most useful word); the rest keep the order they first
/// appear. At most `SUMMARY_KINDS` kinds are named; the other calls fold
/// into `and 2 more`. `marks` are the ranges that wear their own ink: a
/// named file and its counts.
///
/// Provenance: the failure count is Ferrite's own, built from the group's
/// calls, and always said, so no caption can read a failed group as a clean
/// one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GroupSummary {
    pub text: String,
    pub marks: Vec<(std::ops::Range<usize>, SummaryMark)>,
}

/// What a marked range of a group's summary is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SummaryMark {
    File,
    Added,
    Removed,
}

/// How many kinds a group's summary names before `and N more`.
const SUMMARY_KINDS: usize = 2;

/// One kind's share of a group: its calls, whether one runs, the files it
/// touched (an edit's or a write's) and what their diffs added and removed.
struct KindTally<'a> {
    work: Work<'a>,
    count: usize,
    running: bool,
    files: Vec<String>,
    added: usize,
    removed: usize,
}

pub(crate) fn group_summary(members: &[Block]) -> GroupSummary {
    let caption = group_tools(members)
        .last()
        .and_then(|tool| tool.caption.as_deref())
        .map(str::trim)
        .filter(|caption| !caption.is_empty());
    let (mut text, marks) = match caption {
        Some(caption) => (caption.to_owned(), Vec::new()),
        None => kinds_summary(members),
    };
    let tools: Vec<&ToolBlock> = group_tools(members).collect();
    let failed = tools.iter().filter(|tool| call_failed(tool)).count();
    let unanswered = tools
        .iter()
        .filter(|tool| tool.state == ToolState::Unavailable)
        .count();
    let stopped = tools
        .iter()
        .filter(|tool| tool.state == ToolState::Stopped)
        .count();
    let mut wrong = Vec::new();
    if stopped > 0 {
        wrong.push(format!("{stopped} {STOPPED}"));
    }
    if failed > 0 {
        wrong.push(format!("{failed} {}", theme::words::FAILED));
    }
    if unanswered > 0 {
        wrong.push(format!(
            "{unanswered} without {}",
            if unanswered == 1 {
                "a result"
            } else {
                "results"
            }
        ));
    }
    if !wrong.is_empty() {
        text.push_str(&format!(" ({})", wrong.join(", ")));
    }
    GroupSummary { text, marks }
}

/// What a group's calls did by kind, as [`GroupSummary`] words it, and its
/// marks.
fn kinds_summary(members: &[Block]) -> (String, Vec<(std::ops::Range<usize>, SummaryMark)>) {
    let mut kinds: Vec<KindTally> = Vec::new();
    for tool in group_tools(members) {
        let work = Work::of(tool);
        let at = match kinds.iter().position(|tally| tally.work == work) {
            Some(at) => at,
            None => {
                kinds.push(KindTally {
                    work,
                    count: 0,
                    running: false,
                    files: Vec::new(),
                    added: 0,
                    removed: 0,
                });
                kinds.len() - 1
            }
        };
        let tally = &mut kinds[at];
        tally.count += 1;
        tally.running |= tool.state == ToolState::Running;
        if tally.work.changes_files() {
            let paths: Vec<&str> = if tool.diffs.is_empty() {
                vec![tool.summary.as_str()]
            } else {
                tool.diffs.iter().map(|diff| diff.path.as_str()).collect()
            };
            for path in paths.into_iter().filter(|path| !path.is_empty()) {
                if !tally.files.iter().any(|seen| seen == path) {
                    tally.files.push(path.to_owned());
                }
            }
            for diff in &tool.diffs {
                tally.added += diff.added;
                tally.removed += diff.removed;
            }
        }
    }
    // Stable: edits and writes first, the rest in their first order.
    kinds.sort_by_key(|tally| !tally.work.changes_files());
    let mut text = String::new();
    let mut marks = Vec::new();
    let mut mark = |text: &mut String, words: &str, kind: SummaryMark| {
        let start = text.len();
        text.push_str(words);
        marks.push((start..text.len(), kind));
    };
    for (index, tally) in kinds.iter().take(SUMMARY_KINDS).enumerate() {
        // One file edited or written is named, as the app's summary does.
        let named = match tally.files.as_slice() {
            [file] if tally.work.changes_files() => Some(file_name(file)),
            _ => None,
        };
        let phrase = match &named {
            Some(_) => format!("{} ", tally.work.verb(tally.running)),
            None => tally.work.phrase(tally.count, tally.running),
        };
        if index > 0 {
            // Only the verb lowers: a server's name keeps its case.
            text.push_str(", ");
            let mut chars = phrase.chars();
            if let Some(first) = chars.next() {
                text.extend(first.to_lowercase());
                text.push_str(chars.as_str());
            }
        } else {
            text.push_str(&phrase);
        }
        if let Some(name) = named {
            mark(&mut text, &name, SummaryMark::File);
        }
        if tally.added > 0 {
            text.push(' ');
            mark(&mut text, &format!("+{}", tally.added), SummaryMark::Added);
        }
        if tally.removed > 0 {
            text.push(' ');
            mark(
                &mut text,
                &format!("\u{2212}{}", tally.removed),
                SummaryMark::Removed,
            );
        }
    }
    let more: usize = kinds
        .iter()
        .skip(SUMMARY_KINDS)
        .map(|tally| tally.count)
        .sum();
    if more > 0 {
        text.push_str(&format!(" and {more} more"));
    }
    (text, marks)
}

/// A path's last component (`src/notes.md` → `notes.md`).
fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map_or_else(
        || path.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// A group's calls, in order.
pub(crate) fn group_tools(members: &[Block]) -> impl Iterator<Item = &ToolBlock> {
    members.iter().filter_map(|block| match &block.body {
        Body::Tool(tool) => Some(tool),
        _ => None,
    })
}

/// Whether a call failed: its state, or its own test report behind a pipe
/// that exited 0 — the call's bullet reads the same.
pub(crate) fn call_failed(tool: &ToolBlock) -> bool {
    matches!(tool.state, ToolState::Failed(_))
        || (tool.state == ToolState::Ok
            && test_elbow(tool).is_some_and(|elbow| elbow.lead == theme::words::FAILED))
}

/// The calls a shut group shows under its summary while it works, drawn
/// whole (a suite's bar, a command's live line): its newest running call —
/// or, in an agents' card, every agent still running, since they run side
/// by side. Settled, a shut group shows nothing under it.
pub(crate) fn group_live(members: &[Block]) -> Vec<&Block> {
    let running = members.iter().filter(
        |block| matches!(&block.body, Body::Tool(tool) if tool.state == ToolState::Running),
    );
    let agents = group_tools(members)
        .next()
        .is_some_and(super::agents::is_agent);
    if agents {
        running.collect()
    } else {
        running.last().into_iter().collect()
    }
}

/// Whether an opened call that is not a command echoes its input: only
/// where the call line could not already show it whole (`INPUT_ECHO_CHARS`,
/// or a description stood in for it), or where the input is all there is to
/// disclose (a call still running). An open command always shows its
/// `$ cmd`.
pub(crate) fn shows_input(tool: &ToolBlock) -> bool {
    !tool.summary.is_empty()
        && (tool.title.is_some()
            || tool.summary.contains(['\n', '\r'])
            || tool.summary.chars().count() > theme::INPUT_ECHO_CHARS
            || (tool.output.is_none()
                && tool.structured_preview.is_none()
                && tool.diffs.is_empty()))
}

/// A failed call's own message, where it kept no output to show instead.
pub(crate) fn failure_message(tool: &ToolBlock) -> Option<&str> {
    match &tool.state {
        ToolState::Failed(message)
            if disclosed_output(tool).is_none() && !message.trim().is_empty() =>
        {
            Some(message.as_str())
        }
        _ => None,
    }
}

/// The word a settled edit's result says (`applied`, `The file … has been
/// updated`): its diff shows what changed, so the word is never printed.
fn edit_result(tool: &ToolBlock) -> Option<&str> {
    (tool.state == ToolState::Ok && !tool.diffs.is_empty())
        .then_some(tool.result_line.as_deref())
        .flatten()
}

/// A disclosed call's output, unless it only repeats a settled edit's word
/// beside its diff: one fact, one place.
pub(super) fn disclosed_output(tool: &ToolBlock) -> Option<&ferrite_core::transcript::ToolOutput> {
    let output = tool.output.as_ref()?;
    (edit_result(tool) != Some(output.text.trim())).then_some(output)
}

/// A diff line's code: the unified-diff marker byte removed, and nothing
/// else — indentation is code.
pub(super) fn diff_body(line: &str) -> &str {
    match line.as_bytes().first() {
        Some(b'+' | b'-' | b' ') => &line[1..],
        _ => line,
    }
}

/// Hard lines in a block of output.
pub(super) fn output_lines(text: &str) -> usize {
    text.lines().count()
}

/// Whether output leaves the inline run for the bounded native viewport:
/// past `OUTPUT_INLINE_BYTES`. Line count alone does not move it — the
/// viewport owns its own selection, and output that a copy sweep across the
/// transcript can reach must stay inline.
pub(super) fn output_scrolls(text: &str) -> bool {
    text.len() > theme::OUTPUT_INLINE_BYTES
}

/// `512 B`, `41 KB`, `3.4 MB` — whole kilobytes, the way an image's caption
/// reads (`nav-jitter.png · 580×320 · 41 KB`).
pub(super) fn byte_size(bytes: usize) -> String {
    match bytes {
        bytes if bytes < 1024 => format!("{bytes} B"),
        bytes if bytes < 1024 * 1024 => format!("{} KB", (bytes as f64 / 1024.0).round() as u64),
        bytes => format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)),
    }
}

pub(crate) fn collect_output_text(block: BlockId, part: &str, text: &str, selection: &TextRuns) {
    if output_scrolls(text) {
        // Large output owns its selection in a separate native control.
        let _ = selection.output(block, part, text);
    } else {
        let _ = selection.line(block, text.to_owned(), Vec::new());
    }
}

/// One Block's copy fragments, in the order `render_block` registers them.
pub(crate) fn collect_block_text(
    block: &Block,
    expanded: bool,
    skills: &[ferrite_core::SessionCommand],
    selection: &TextRuns,
) {
    match &block.body {
        Body::Visual(visual) => crate::visual::collect_text(block.id, visual, selection),
        Body::WrapUp(wrap) => super::wrap_up::collect_text(block.id, wrap, selection),
        Body::Prompt(line) => match prompt_text(line, skills).0 {
            PromptText::Literal(words) => {
                if !words.is_empty() {
                    let _ = selection.line(block.id, words, Vec::new());
                }
            }
            PromptText::Markdown(source) => {
                let _ = selection.markdown(block.id, source);
            }
        },
        Body::Paragraph { spans } | Body::Heading { spans, .. } | Body::Bullet { spans } => {
            let (text, _) = inline(spans);
            let _ = selection.line(block.id, text, Vec::new());
        }
        Body::Thinking(thought) => {
            if !thought.trim().is_empty() {
                match reasoning_text(thought).1 {
                    None => {
                        let _ = selection.markdown(block.id, thought.trim().to_owned());
                    }
                    Some(details) if expanded => {
                        let _ = selection.markdown(block.id, details);
                    }
                    Some(_) => {}
                }
            }
        }
        Body::Notice(text) | Body::Meta(text) => {
            let _ = selection.line(block.id, text.clone(), Vec::new());
        }
        Body::TurnEnd(end) => {
            if let Some(stamp) = turn_stamp(end) {
                let _ = selection.line(block.id, stamp, Vec::new());
            }
        }
        Body::Code { source, .. } => {
            let _ = selection.line(block.id, source.clone(), Vec::new());
        }
        Body::Tool(tool) => collect_tool_text(block.id, tool, expanded, selection),
    }
}

/// A group's copy fragments, in the order `render_tool_group` registers
/// them: its summary, then the calls it draws — every call while open,
/// the working call while shut — each as its own row's would be.
pub(crate) fn collect_group_text(
    members: &[Block],
    expanded: bool,
    member_expanded: impl Fn(&ToolBlock) -> bool,
    selection: &TextRuns,
) {
    let Some(leader) = members.first() else {
        return;
    };
    let _ = selection.line(leader.id, group_summary(members).text, Vec::new());
    let drawn: Vec<&Block> = if expanded {
        members.iter().collect()
    } else {
        group_live(members)
    };
    for block in drawn {
        if let Body::Tool(tool) = &block.body {
            collect_tool_text(block.id, tool, member_expanded(tool), selection);
        }
    }
}

/// A call's copy fragments, in the order `render_tool` registers them: its
/// call line; a failure's tag; once open, a command's `$ cmd` and its
/// output or another call's details, then its diffs.
fn collect_tool_text(block: BlockId, tool: &ToolBlock, expanded: bool, selection: &TextRuns) {
    if super::agents::is_agent(tool) {
        let _ = selection.line(block, agent_line(tool).0, Vec::new());
        if let Some(verdict) = call_verdict(tool) {
            let _ = selection.line(block, verdict, Vec::new());
        }
        return;
    }
    match call_pieces(tool) {
        CallPieces::Whole(call) => {
            let _ = selection.markdown(block, call);
        }
        CallPieces::File { path, trail, .. } => {
            let _ = selection.markdown(block, path);
            if !trail.is_empty() {
                let _ = selection.markdown(block, trail);
            }
        }
    }
    if let Some(verdict) = call_verdict(tool) {
        let _ = selection.line(block, verdict, Vec::new());
    }
    if !expanded {
        return;
    }
    if tools::is_command_run(&tool.name) {
        let shell = super::output::shell(tool);
        collect_output_text(block, "command", &shell.command, selection);
        if let Some(output) = shell.output {
            collect_output_text(block, "result", output, selection);
        }
    } else {
        if shows_input(tool) {
            collect_output_text(block, "command", &tool.summary, selection);
        }
        if let Some(output) = disclosed_output(tool) {
            collect_output_text(block, "result", &output.text, selection);
        } else if let Some(message) = failure_message(tool) {
            collect_output_text(block, "result", message, selection);
        }
        if let Some(output) = tool.structured_preview.as_ref() {
            collect_output_text(block, "details", &output.text, selection);
        }
    }
    for diff in &tool.diffs {
        collect_diff_text(block, diff, selection);
    }
}

/// A diff's selectable code, in the order its rows register it.
fn collect_diff_text(block: BlockId, diff: &Diff, selection: &TextRuns) {
    let (cap, _) = hunk_rows(diff.hunks.iter().map(|hunk| hunk.lines.len()).sum());
    for row in diff_rows(diff, cap) {
        if let DiffRow::Line(side) = row {
            let _ = selection.line(block, side.body, Vec::new());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::{
        transcript::{Input, Transcript},
        SessionEvent,
    };

    fn read(transcript: &mut Transcript, id: &str, is_error: bool) {
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: id.into(),
            name: "Read".into(),
            input: serde_json::json!({ "file_path": id }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: id.into(),
            output: "x".into(),
            is_error,
            result: ferrite_core::ToolResult::Opaque,
        }));
    }

    /// A call row reads in the UI face: only its literal argument is a
    /// code span (mono); a description is words.
    #[test]
    fn a_call_row_is_words_but_its_command() {
        let mut transcript = Transcript::default();
        for (id, input) in [
            ("bare", serde_json::json!({"command": "cargo test"})),
            (
                "described",
                serde_json::json!({"command": "sleep 15", "description": "Wait a while"}),
            ),
        ] {
            transcript.apply(Input::Event(SessionEvent::ToolStarted {
                id: id.into(),
                name: "Bash".into(),
                input,
            }));
        }
        let rows: Vec<String> = group_tools(transcript.blocks())
            .map(call_markdown)
            .collect();
        assert_eq!(rows, ["Running `cargo test`", "Wait a while"]);
    }

    /// The working line carries a call in its description where the
    /// provider wrote one, else a verb and a short target.
    #[test]
    fn the_working_line_reads_a_description_or_a_short_target() {
        let mut transcript = Transcript::default();
        let start = |transcript: &mut Transcript, id: &str, name: &str, input| {
            transcript.apply(Input::Event(SessionEvent::ToolStarted {
                id: id.into(),
                name: name.into(),
                input,
            }));
        };
        start(
            &mut transcript,
            "sleep",
            "Bash",
            serde_json::json!({"command": "sleep 15 && echo done", "description": "Waiting 15 seconds then printing done"}),
        );
        start(
            &mut transcript,
            "build",
            "commandExecution",
            serde_json::json!({"command": "cd crates/ui && cargo build --release"}),
        );
        start(
            &mut transcript,
            "read",
            "Read",
            serde_json::json!({"file_path": "/repo/crates/ui/src/main.rs"}),
        );
        let words: Vec<String> = group_tools(transcript.blocks())
            .map(working_words)
            .collect();
        assert_eq!(
            words,
            [
                "Waiting 15 seconds then printing done",
                "Running cargo build",
                "Reading main.rs"
            ]
        );
    }

    /// The provider's caption on a group's newest call stands for Ferrite's
    /// tally, and the group's failures are still said after it; a caption
    /// on an older call does not speak for the calls after it.
    #[test]
    fn a_provider_caption_reads_with_the_failure_suffix() {
        let mut transcript = Transcript::default();
        read(&mut transcript, "a", true);
        read(&mut transcript, "b", false);
        transcript.apply(Input::Event(SessionEvent::ToolSummary {
            ids: vec!["a".into(), "b".into()],
            summary: "Read the two configs".into(),
        }));
        let summary = group_summary(transcript.blocks());
        assert_eq!(summary.text, "Read the two configs (1 failed)");
        assert!(summary.marks.is_empty());

        read(&mut transcript, "c", false);
        assert_eq!(
            group_summary(transcript.blocks()).text,
            "Read 3 files (1 failed)"
        );
    }

    /// A denied call, and one cut off by a stopped turn, are stopped, not
    /// failed: the group says so and the row's tag is no red `Failed`.
    #[test]
    fn a_group_counts_what_the_operator_stopped() {
        let mut transcript = Transcript::default();
        read(&mut transcript, "a", false);
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "b".into(),
            name: "Bash".into(),
            input: serde_json::json!({"command": "rm -rf build"}),
        }));
        transcript.apply(Input::Answered {
            allowed: false,
            tool_name: "Bash".into(),
        });
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "b".into(),
            output: "The user doesn't want to proceed".into(),
            is_error: true,
            result: ferrite_core::ToolResult::Opaque,
        }));
        let stopped = group_tools(transcript.blocks()).last().unwrap().clone();
        assert_eq!(stopped.state, ToolState::Stopped);
        assert_eq!(call_verdict(&stopped), None, "stopped is not failed");
        assert_eq!(call_result(&stopped).as_deref(), Some("stopped"));
        assert!(group_summary(transcript.blocks())
            .text
            .ends_with("(1 stopped)"));
    }
}
