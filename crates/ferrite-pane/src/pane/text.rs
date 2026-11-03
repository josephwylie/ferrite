//! Logical copy fragments, collected without allocating GPUI elements.
//!
//! Keep disclosure/order rules aligned with the row renderer. The native
//! rendering tests compare both projections, including hidden tool details.
use super::*;

/// What a call whose result never came says after its call line.
pub(super) const NO_RESULT: &str = "no result";

/// How a turn's end reads in the transcript, if it says anything: a
/// completed turn `Worked for 5m 22s` once it took `TURN_STAMP_MIN_MS` (a
/// quicker one, or one whose time was never observed, says nothing); an
/// interrupted or failed one always, as core words it (`failed · 0.1s ·
/// <message>`).
pub(crate) fn turn_stamp(end: &ferrite_core::transcript::TurnEnd) -> Option<String> {
    if !end.completed() {
        return Some(end.text());
    }
    let elapsed = end.elapsed_ms.filter(|ms| *ms >= theme::TURN_STAMP_MIN_MS)?;
    Some(format!(
        "Worked for {}",
        ferrite_core::progress::settled_duration_label(std::time::Duration::from_millis(elapsed))
    ))
}

/// What kind of work a call did, for its call line's verb and its group's
/// summary. Classified by the tool alone: a command is a command
/// whatever it ran — `cat` is not read as a read without knowing what the
/// shell made of it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum WorkKind {
    Command,
    Read,
    Edit,
    Write,
    Search,
    List,
    Fetch,
    WebSearch,
    Agent,
    /// An MCP call, by its server (`mcp__github__get_issue` → `github`).
    Mcp(String),
    Other(String),
}

impl WorkKind {
    fn of(tool: &ToolBlock) -> Self {
        match tool.name.as_str() {
            "Bash" | "commandExecution" | "BashOutput" => Self::Command,
            "Read" | "NotebookRead" | "read_file" | "imageView" => Self::Read,
            "Edit" | "MultiEdit" | "NotebookEdit" | "fileChange" => Self::Edit,
            "Write" => Self::Write,
            "Task" | "Agent" => Self::Agent,
            "Grep" | "Glob" => Self::Search,
            "LS" => Self::List,
            "WebFetch" => Self::Fetch,
            "WebSearch" | "webSearch" => Self::WebSearch,
            name => match name
                .strip_prefix("mcp__")
                .and_then(|rest| rest.split_once("__"))
            {
                Some((server, _)) if !server.is_empty() => Self::Mcp(server.to_owned()),
                _ => Self::Other(ferrite_core::transcript::display_tool_name(name).to_owned()),
            },
        }
    }

    /// `Ran 3 commands`, `Reading 1 file`, `Called github 2 times`: the
    /// verb in the present tense while one of its calls runs.
    fn phrase(&self, count: usize, running: bool) -> String {
        let counted = |active: &str, done: &str, one: &str, many: &str| {
            let verb = if running { active } else { done };
            format!("{verb} {count} {}", if count == 1 { one } else { many })
        };
        match self {
            Self::Command => counted("Running", "Ran", "command", "commands"),
            Self::Read => counted("Reading", "Read", "file", "files"),
            Self::Edit => counted("Editing", "Edited", "file", "files"),
            Self::Write => counted("Writing", "Wrote", "file", "files"),
            Self::Search => counted("Searching for", "Searched for", "pattern", "patterns"),
            Self::List => counted("Listing", "Listed", "directory", "directories"),
            Self::Fetch => counted("Fetching", "Fetched", "page", "pages"),
            Self::WebSearch => counted("Running", "Ran", "web search", "web searches"),
            Self::Agent => counted("Running", "Ran", "agent", "agents"),
            Self::Mcp(server) | Self::Other(server) => {
                let verb = match (self, running) {
                    (Self::Mcp(_), true) => "Calling",
                    (Self::Mcp(_), false) => "Called",
                    (_, true) => "Using",
                    (_, false) => "Used",
                };
                match count {
                    1 => format!("{verb} {server}"),
                    count => format!("{verb} {server} {count} times"),
                }
            }
        }
    }

    /// One call's verb, in the present tense while it runs: `Ran`, `Read`,
    /// `Edited`; a tool with no verb of its own is named (`Agent`, `github`,
    /// `TodoWrite`).
    fn verb(&self, running: bool) -> &str {
        let tense = |active, done| if running { active } else { done };
        match self {
            Self::Command => tense("Running", "Ran"),
            Self::Read => tense("Reading", "Read"),
            Self::Edit => tense("Editing", "Edited"),
            Self::Write => tense("Writing", "Wrote"),
            Self::Search => tense("Searching for", "Searched for"),
            Self::List => tense("Listing", "Listed"),
            Self::Fetch => tense("Fetching", "Fetched"),
            Self::WebSearch => tense("Searching the web for", "Searched the web for"),
            Self::Agent => "Agent",
            Self::Mcp(name) | Self::Other(name) => name,
        }
    }

    /// Whether a call of this kind takes a literal argument (a command, a
    /// path, a pattern, a URL), set in Geist Mono; an agent's task or a
    /// web query is words, set as the line is.
    fn literal(&self) -> bool {
        !matches!(
            self,
            Self::WebSearch | Self::Agent | Self::Mcp(_) | Self::Other(_)
        )
    }
}

/// A call line as it reads and copies, in three runs: the verb in the app's
/// words (`Ran `, its space carried when an argument follows), the argument
/// as written (`cargo test -p ferrite-core`, a path; empty when the call has
/// none) — `literal` when it is a command, a path or a pattern — and what
/// came of it after its seam (` · 954 passed`), if anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallLine {
    pub verb: String,
    pub argument: String,
    pub literal: bool,
    pub result: Option<String>,
}

/// The seam before a call line's result.
pub(crate) const RESULT_SEAM: &str = " \u{b7} ";

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
/// failed call's verdict (`call_verdict`), which stands apart from it.
pub(crate) fn call_markdown(tool: &ToolBlock) -> String {
    let mut line = call_line(tool);
    if call_failed(tool) {
        line.result = None;
    }
    line.markdown(argument_is_path(tool))
}

/// A failed call's verdict after its call line (` · failed`, ` · failed ·
/// 357 passed; 2 failed`): its own run, so its lead word alone wears the
/// failure ink and it stays in view when a long command is cut.
pub(crate) fn call_verdict(tool: &ToolBlock) -> Option<String> {
    call_failed(tool).then(|| call_line(tool).result).flatten()
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

pub(crate) fn call_line(tool: &ToolBlock) -> CallLine {
    let argument = tool.argument().into_owned();
    let kind = WorkKind::of(tool);
    let mut verb = kind.verb(tool.state == ToolState::Running).to_owned();
    if !argument.is_empty() {
        verb.push(' ');
    }
    CallLine {
        verb,
        argument,
        literal: kind.literal(),
        result: call_result(tool).map(|result| format!("{RESULT_SEAM}{result}")),
    }
}

/// What came of a call, said once after its call line: a test run's verdict
/// (`954 passed`, `failed · 357 passed; 2 failed`), a command's first line,
/// a read's `412 lines`, `failed`, `no result`. A running suite says nothing
/// here (its bar does), nor does a settled edit (its diff does).
fn call_result(tool: &ToolBlock) -> Option<String> {
    match &tool.state {
        ToolState::Running => tool
            .result_line
            .clone()
            .filter(|_| tool.progress.is_none()),
        ToolState::Unavailable => Some(NO_RESULT.to_owned()),
        ToolState::Failed(_) => Some(
            test_elbow(tool).map_or_else(|| theme::words::FAILED.to_owned(), |elbow| elbow.line()),
        ),
        ToolState::Ok => match test_elbow(tool) {
            Some(elbow) if elbow.lead == theme::words::FAILED || elbow.rest.is_empty() => {
                Some(elbow.line())
            }
            Some(elbow) => Some(elbow.rest),
            None => match command_fold(tool) {
                Some((first, _)) => Some(first),
                None => tool.result_line.clone().filter(|_| tool.diffs.is_empty()),
            },
        },
    }
}

/// A failed call's own words, on a line under its call line: its result
/// line, else the first line of its error. `None` for a test run (its
/// verdict and preview say it) or a failure that said nothing.
pub(crate) fn call_failure(tool: &ToolBlock) -> Option<String> {
    if !matches!(tool.state, ToolState::Failed(_)) || test_elbow(tool).is_some() {
        return None;
    }
    tool.result_line
        .clone()
        .or_else(|| failed_excerpt(tool).map(str::to_owned))
        .filter(|detail| !detail.trim().is_empty())
}

/// A group's summary line, as it reads and copies: what its calls did by
/// kind, in the order the kinds first appear, the first phrase capitalised
/// (`Ran 12 commands, read 1 file`) — an edit or a write of one file names
/// it (`edited notes.md`), and either says what its diffs changed (`+5 −1`)
/// — then what went wrong (`· 2 failed`, `· 1 without a result`). `marks`
/// are the ranges that wear their own ink: a named file, the counts, the
/// failure count.
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
    Failed,
}

impl GroupSummary {
    #[cfg(test)]
    pub(crate) fn failed(&self) -> Option<std::ops::Range<usize>> {
        self.marks
            .iter()
            .find(|(_, mark)| *mark == SummaryMark::Failed)
            .map(|(range, _)| range.clone())
    }
}

/// One kind's share of a group: its calls, whether one runs, the files it
/// touched (an edit's or a write's) and what their diffs added and removed.
struct KindTally {
    kind: WorkKind,
    count: usize,
    running: bool,
    files: Vec<String>,
    added: usize,
    removed: usize,
}

pub(crate) fn group_summary(members: &[Block]) -> GroupSummary {
    let mut kinds: Vec<KindTally> = Vec::new();
    let (mut failed, mut unanswered) = (0, 0);
    for tool in group_tools(members) {
        let kind = WorkKind::of(tool);
        let running = tool.state == ToolState::Running;
        let at = match kinds.iter().position(|tally| tally.kind == kind) {
            Some(at) => at,
            None => {
                kinds.push(KindTally {
                    kind,
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
        tally.running |= running;
        if matches!(tally.kind, WorkKind::Edit | WorkKind::Write) {
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
        if call_failed(tool) {
            failed += 1;
        }
        if tool.state == ToolState::Unavailable {
            unanswered += 1;
        }
    }
    let mut text = String::new();
    let mut marks = Vec::new();
    let mut mark = |text: &mut String, words: &str, kind: SummaryMark| {
        let start = text.len();
        text.push_str(words);
        marks.push((start..text.len(), kind));
    };
    for (index, tally) in kinds.iter().enumerate() {
        // One file edited or written is named, as the app's summary does.
        let named = match (&tally.kind, tally.files.as_slice()) {
            (WorkKind::Edit | WorkKind::Write, [file]) => Some(file_name(file)),
            _ => None,
        };
        let phrase = match &named {
            Some(_) => format!("{} ", tally.kind.verb(tally.running)),
            None => tally.kind.phrase(tally.count, tally.running),
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
    if failed > 0 {
        text.push_str(" \u{b7} ");
        mark(
            &mut text,
            &format!("{failed} {}", theme::words::FAILED),
            SummaryMark::Failed,
        );
    }
    if unanswered > 0 {
        text.push_str(&format!(
            " \u{b7} {unanswered} without {}",
            if unanswered == 1 {
                "a result"
            } else {
                "results"
            }
        ));
    }
    GroupSummary { text, marks }
}

/// A path's last component (`src/notes.md` → `notes.md`).
fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map_or_else(|| path.to_owned(), |name| name.to_string_lossy().into_owned())
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

/// The call a shut group shows under its summary while it works: its
/// newest running call, drawn whole (a suite's bar, a command's live
/// line). Settled, a shut group shows nothing under it.
pub(crate) fn group_live(members: &[Block]) -> Option<&Block> {
    members
        .iter()
        .rev()
        .find(|block| matches!(&block.body, Body::Tool(tool) if tool.state == ToolState::Running))
}

/// Whether a disclosed call echoes its input: only where the call line
/// could not already show it whole (`INPUT_ECHO_CHARS`), or where the input
/// is all there is to disclose (a call still running). A command the call
/// line shows whole is never echoed as `$ command` under it: one fact, one
/// place.
pub(crate) fn shows_input(tool: &ToolBlock) -> bool {
    !tool.summary.is_empty()
        && (tool.title.is_some()
            || tool.summary.contains(['\n', '\r'])
            || tool.summary.chars().count() > theme::INPUT_ECHO_CHARS
            || (tool.output.is_none()
                && tool.structured_preview.is_none()
                && tool.diffs.is_empty()))
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
pub(crate) fn collect_block_text(block: &Block, expanded: bool, selection: &TextRuns) {
    match &block.body {
        Body::Visual(visual) => crate::visual::collect_text(block.id, visual, selection),
        Body::Prompt(line) => match prompt_text(line).0 {
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
        Body::Tool(tool) => {
            collect_tool_text(block.id, tool, expanded, selection)
        }
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
        group_live(members).into_iter().collect()
    };
    for block in drawn {
        if let Body::Tool(tool) = &block.body {
            collect_tool_text(block.id, tool, member_expanded(tool), selection);
        }
    }
}

/// A call's copy fragments, in the order `render_tool` registers them: its
/// call line; a failure's own words; its output box (a failure's
/// preview and open fold, an open command's output, its disclosed
/// details); its diffs while it is open.
fn collect_tool_text(
    block: BlockId,
    tool: &ToolBlock,
    expanded: bool,
    selection: &TextRuns,
) {
    let line = |text: String| {
        let _ = selection.line(block, text, Vec::new());
    };
    let _ = selection.markdown(block, call_markdown(tool));
    if let Some(verdict) = call_verdict(tool) {
        line(verdict);
    }
    if let Some(failure) = call_failure(tool) {
        line(failure);
    }
    let failed_test = test_elbow(tool).filter(|elbow| elbow.lead == theme::words::FAILED);
    if let Some(elbow) = &failed_test {
        if !elbow.preview.is_empty() {
            line(elbow.preview.join("\n"));
        }
        if elbow.hidden > 0 && expanded {
            collect_fold(block, tool, test_rest(tool, elbow), selection);
        }
    } else if matches!(tool.state, ToolState::Failed(_)) {
        if let Some((shown, hidden)) = output_fold(tool) {
            line(shown);
            if hidden > 0 && expanded {
                collect_fold(block, tool, failed_rest(tool, hidden), selection);
            }
        }
    } else if let Some((all, _)) = command_open(tool).filter(|_| expanded) {
        collect_fold(block, tool, all, selection);
    }
    let details = match tool.state {
        ToolState::Running => expanded,
        ToolState::Ok => expanded && !ferrite_core::docview::is_command_run(&tool.name),
        _ => false,
    };
    if details {
        if shows_input(tool) {
            collect_output_text(block, "command", &tool.summary, selection);
        }
        if let Some(output) = disclosed_output(tool) {
            collect_output_text(block, "result", &output.text, selection);
        }
        if let Some(output) = tool.structured_preview.as_ref() {
            collect_output_text(block, "details", &output.text, selection);
        }
    }
    if expanded {
        for diff in &tool.diffs {
            collect_diff_text(block, diff, selection);
        }
    }
}

/// An open fold's copy fragment (`fold_open`): the lines it held, or the
/// call's whole output in its native viewer when that is past the inline
/// cap.
fn collect_fold(block: BlockId, tool: &ToolBlock, rest: String, selection: &TextRuns) {
    let full = disclosed_output(tool).map_or("", |output| output.text.as_str());
    if output_scrolls(full) {
        let _ = selection.output(block, "result", full);
    } else {
        let _ = selection.line(block, rest, Vec::new());
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
