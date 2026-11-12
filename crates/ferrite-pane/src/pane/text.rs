//! Logical copy fragments, collected without allocating GPUI elements.
//!
//! Keep disclosure/order rules aligned with the row renderer. The native
//! rendering tests compare both projections, including hidden tool details.
use super::*;
use ferrite_core::transcript::tools::{self, ToolKind};

/// How a turn's end reads in the transcript, if it says anything: an
/// interrupted or failed one, as core words it (`failed · 0.1s ·
/// <message>`). A completed turn says nothing at its end: its fold, at its
/// top, says how long it worked (`transcript::turns`).
pub(crate) fn turn_stamp(end: &ferrite_core::transcript::TurnEnd) -> Option<String> {
    (!end.completed()).then(|| end.text())
}

/// What a running turn's live row says of its newest call (T3's
/// `liveWorkEntryLabel`): a command by its program alone, as its state
/// reads (`Running sed`, `Ran sed`, `Failed sed`, `Stopped sed`); any other
/// call as the working words name it (`Reading main.rs`).
pub(crate) fn live_words(tool: &ToolBlock) -> String {
    if Work::of(tool).kind != ToolKind::Command {
        return working_words(tool);
    }
    let verb = match tool.state {
        ToolState::Running => "Running",
        ToolState::Ok | ToolState::Unavailable => "Ran",
        ToolState::Failed(_) => "Failed",
        ToolState::Stopped => "Stopped",
    };
    match program_name(without_cd(&tool.argument())) {
        Some(program) => format!("{verb} {program}"),
        None => verb.to_owned(),
    }
}

/// A command line's program (T3's `commandProgramName`): its first word
/// past any `NAME=value` settings, a path's last part (`/usr/bin/sed` →
/// `sed`).
fn program_name(command: &str) -> Option<&str> {
    let word = command
        .split_whitespace()
        .find(|word| !(word.contains('=') && !word.starts_with('=')))?;
    let word = word.trim_matches(|c| matches!(c, '"' | '\'' | '('));
    let program = word.rsplit('/').next().unwrap_or(word);
    (!program.is_empty()).then_some(program)
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
            ToolKind::Findings => tense("Reporting findings", "Reported findings"),
            ToolKind::Mcp(_)
            | ToolKind::Todo
            | ToolKind::Question
            | ToolKind::Visual
            | ToolKind::Other => self.name,
        }
    }

}

/// A work row's label as it reads and copies (T3's `workEntryDisplayLabel`),
/// in two runs: the verb in the app's words (`Read `, its space carried when
/// an argument follows; empty for a command, whose own text is its label,
/// and for an edit, whose path is) and the argument as written. Nothing
/// trails it: what came of a call is in its open body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallLine {
    pub verb: String,
    pub argument: String,
}

impl CallLine {
    /// The label as one Markdown run, so it selects and copies as the one
    /// line it reads: words, a path (`path`) a link that opens it
    /// (`rich::PATH_SCHEME`).
    pub(crate) fn markdown(&self, path: bool) -> String {
        let mut source = escape_words(&self.verb);
        if path {
            source.push_str(&path_link(&self.argument, &self.argument));
        } else {
            source.push_str(&escape_words(&self.argument));
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

/// A call's label as its one Markdown run (`CallLine::markdown`). A call on
/// a file is drawn in pieces instead (`call_pieces`).
pub(crate) fn call_markdown(tool: &ToolBlock) -> String {
    call_line(tool).markdown(argument_is_path(tool))
}

/// A call's label as the row draws it. A call on a file (`Read src/x.rs`)
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
    },
}

pub(crate) fn call_pieces(tool: &ToolBlock) -> CallPieces {
    if !argument_is_path(tool) {
        return CallPieces::Whole(call_markdown(tool));
    }
    let line = call_line(tool);
    CallPieces::File {
        path: path_link(&line.argument, &line.argument),
        verb: line.verb,
    }
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


/// A call's label: a command's own text less a leading `cd <dir> &&` (the
/// open body keeps it), an edit's or a write's path, any other call its
/// verb and argument (`Read src/x.rs`).
pub(crate) fn call_line(tool: &ToolBlock) -> CallLine {
    let work = Work::of(tool);
    let argument = tool.argument();
    let (verb, argument) = match work.kind {
        ToolKind::Command => (String::new(), without_cd(&argument).to_owned()),
        ToolKind::Edit | ToolKind::Write => (String::new(), argument.into_owned()),
        _ => {
            let mut verb = work.verb(tool.state == ToolState::Running).to_owned();
            if !argument.is_empty() {
                verb.push(' ');
            }
            (verb, argument.into_owned())
        }
    };
    CallLine { verb, argument }
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


/// An agent row's first line (`agents::render_agent`): its task (the
/// provider's description), or `Agent` with none.
pub(crate) fn agent_title(tool: &ToolBlock) -> String {
    let task = tool.argument();
    if task.is_empty() {
        "Agent".into()
    } else {
        task.into_owned()
    }
}

/// An agent row's second line (T3's): what it is doing while it runs (its
/// progress, carried as the call's result line: `AgentFaces::apply`), else
/// its standing.
pub(crate) fn agent_detail(tool: &ToolBlock) -> String {
    let standing = super::agents::Standing::of(tool);
    tool.result_line
        .as_deref()
        .map(str::trim)
        .filter(|summary| standing == super::agents::Standing::Running && !summary.is_empty())
        .map_or_else(|| standing.word().to_owned(), str::to_owned)
}

/// A subagent card's title: `3 subagents`.
pub(crate) fn agent_card_title(tools: &[&ToolBlock]) -> String {
    format!("{} subagents", tools.len())
}

/// How a card's agents stand (T3's `subagentDisplay`): `1 working · 2
/// done`, counted in that order, then failed and stopped.
pub(crate) fn agent_card_status(tools: &[&ToolBlock]) -> String {
    use super::agents::Standing::*;
    [(Running, "working"), (Done, "done"), (Failed, "failed"), (Stopped, "stopped")]
        .into_iter()
        .filter_map(|(standing, word)| {
            let count = tools
                .iter()
                .filter(|tool| super::agents::Standing::of(tool) == standing)
                .count();
            (count > 0).then(|| format!("{count} {word}"))
        })
        .collect::<Vec<_>>()
        .join(" \u{b7} ")
}


/// A group's summary line, as it reads and copies (T3 Code's
/// `summarizeToolGroup`): a group of thoughts alone is `Thought (×3)`;
/// otherwise its thoughts are left out and its calls are counted by what
/// they did. MCP calls lead by their integration (`Used github
/// integration`), then at most two actions are named, the weightier first
/// chosen (`Action::priority`) and the chosen put in the order they first
/// appear, and the rest are `performed N other actions`. Joined with `and`,
/// or an Oxford comma past two: `Ran 4 commands and changed 10 files`.
/// Always in the past tense, and never a failure: a failed call stands
/// apart from any group, and a failure tints its row's icon.
pub(crate) fn group_summary(members: &[Block]) -> String {
    let tools: Vec<&ToolBlock> = group_tools(members).collect();
    if tools.is_empty() {
        return match members.len() {
            1 => "Thought".to_owned(),
            count => format!("Thought (\u{d7}{count})"),
        };
    }
    let mut integrations: Vec<&str> = Vec::new();
    let mut actions: Vec<(Action, Vec<&ToolBlock>)> = Vec::new();
    for tool in tools.iter().copied() {
        match Action::of(tool) {
            None => {
                let ToolKind::Mcp(server) = tools::policy(&tool.name).kind else {
                    continue;
                };
                if !integrations.contains(&server) {
                    integrations.push(server);
                }
            }
            Some(action) => match actions.iter_mut().find(|(seen, _)| *seen == action) {
                Some((_, calls)) => calls.push(tool),
                None => actions.push((action, vec![tool])),
            },
        }
    }
    let mut chosen: Vec<usize> = (0..actions.len()).collect();
    chosen.sort_by_key(|at| (actions[*at].0.priority(), *at));
    chosen.truncate(2);
    chosen.sort_unstable();
    let mut labels = Vec::new();
    if !integrations.is_empty() {
        let noun = if integrations.len() == 1 {
            "integration"
        } else {
            "integrations"
        };
        labels.push(format!("Used {} {noun}", sentence_list(&integrations)));
    }
    let integrated = tools.len() - actions.iter().map(|(_, calls)| calls.len()).sum::<usize>();
    let named: usize = chosen.iter().map(|at| actions[*at].1.len()).sum();
    labels.extend(
        chosen
            .iter()
            .map(|at| actions[*at].0.label(&actions[*at].1)),
    );
    let rest = tools.len() - integrated - named;
    if rest > 0 {
        labels.push(format!(
            "Performed {rest} other {}",
            if rest == 1 { "action" } else { "actions" }
        ));
    }
    let labels: Vec<String> = labels
        .into_iter()
        .enumerate()
        .map(|(index, label)| {
            if index == 0 {
                return label;
            }
            let mut chars = label.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_lowercase().chain(chars).collect()
            })
        })
        .collect();
    sentence_list(&labels)
}

/// `a`, `a and b`, `a, b, and c`.
fn sentence_list(items: &[impl AsRef<str>]) -> String {
    match items {
        [] => String::new(),
        [one] => one.as_ref().to_owned(),
        [first, second] => format!("{} and {}", first.as_ref(), second.as_ref()),
        [init @ .., last] => {
            let init: Vec<&str> = init.iter().map(AsRef::as_ref).collect();
            format!("{}, and {}", init.join(", "), last.as_ref())
        }
    }
}

/// What a call counts toward in its group's summary (T3's
/// `ToolGroupAction`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Command,
    Edit,
    Agent,
    Read,
    CodeSearch,
    WebSearch,
    Browser,
    Other,
}

impl Action {
    /// `None` for an integration's call (an MCP server's), which the
    /// summary names by its integration instead.
    fn of(tool: &ToolBlock) -> Option<Self> {
        let policy = tools::policy(&tool.name);
        Some(match policy.kind {
            ToolKind::Command => Self::Command,
            ToolKind::Edit | ToolKind::Write => Self::Edit,
            ToolKind::Agent => Self::Agent,
            ToolKind::Read | ToolKind::List => Self::Read,
            ToolKind::Search => Self::CodeSearch,
            ToolKind::Fetch | ToolKind::WebSearch => Self::WebSearch,
            _ if policy.icon == tools::ToolIcon::Browser => Self::Browser,
            ToolKind::Mcp(server) if server != "ferrite" => return None,
            _ => Self::Other,
        })
    }

    /// Which actions a summary names first: what changed things, then what
    /// read them, then the rest.
    fn priority(self) -> u8 {
        match self {
            Self::Command | Self::Edit | Self::Agent => 0,
            Self::Read | Self::CodeSearch | Self::WebSearch | Self::Browser => 1,
            Self::Other => 2,
        }
    }

    fn label(self, calls: &[&ToolBlock]) -> String {
        let counted = |count: usize, verb: &str, one: &str, many: &str| {
            format!("{verb} {count} {}", if count == 1 { one } else { many })
        };
        let times = |count: usize, verb: &str| counted(count, verb, "time", "times");
        let count = calls.len();
        match self {
            Self::Command => counted(count, "Ran", "command", "commands"),
            Self::Edit => counted(changed_files(calls), "Changed", "file", "files"),
            Self::Agent => counted(count, "Ran", "agent", "agents"),
            Self::Read => counted(count, "Read", "file", "files"),
            Self::CodeSearch => times(count, "Searched code"),
            Self::WebSearch => times(count, "Searched the web"),
            Self::Browser => times(count, "Used browser"),
            Self::Other => counted(count, "Used", "tool", "tools"),
        }
    }
}

/// How many files a group's edits changed: the distinct paths their diffs
/// (or, with none, their arguments) name, and one for each edit that names
/// none.
fn changed_files(edits: &[&ToolBlock]) -> usize {
    let mut paths = std::collections::BTreeSet::new();
    let mut unnamed = 0;
    for edit in edits {
        let named: Vec<&str> = if edit.diffs.is_empty() {
            [edit.summary.as_str()]
                .into_iter()
                .filter(|path| !path.is_empty() && !path.contains(char::is_whitespace))
                .collect()
        } else {
            edit.diffs.iter().map(|diff| diff.path.as_str()).collect()
        };
        if named.is_empty() {
            unnamed += 1;
        }
        paths.extend(named);
    }
    paths.len() + unnamed
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
/// whole (a suite's bar, a command's live line): its newest running call.
/// A subagent card shut shows its trigger alone (T3's); settled, a shut
/// group shows nothing under it.
pub(crate) fn group_live(members: &[Block]) -> Vec<&Block> {
    if is_agent_card(members) {
        return Vec::new();
    }
    members
        .iter()
        .filter(|block| matches!(&block.body, Body::Tool(tool) if tool.state == ToolState::Running))
        .last()
        .into_iter()
        .collect()
}

/// Whether a group is agents started together: T3's subagent card
/// (`agents::render_agent_card`).
pub(crate) fn is_agent_card(members: &[Block]) -> bool {
    group_tools(members)
        .next()
        .is_some_and(super::agents::is_agent)
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
        // A thought's one-line preview is its label, not text; open, its
        // whole body is.
        Body::Thinking(thought) => {
            if expanded && !thought.trim().is_empty() {
                let _ = selection.markdown(block.id, thought.trim().to_owned());
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
        // A divider, not the conversation: a sweep reads across it.
        Body::Compaction(_) => {}
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
    member_expanded: impl Fn(&Block) -> bool,
    selection: &TextRuns,
) {
    let Some(leader) = members.first() else {
        return;
    };
    if is_agent_card(members) {
        let tools: Vec<_> = group_tools(members).collect();
        let _ = selection.line(leader.id, agent_card_title(&tools), Vec::new());
        let _ = selection.line(leader.id, agent_card_status(&tools), Vec::new());
    } else {
        let _ = selection.line(leader.id, group_summary(members), Vec::new());
    }
    let drawn: Vec<&Block> = if expanded {
        members.iter().collect()
    } else {
        group_live(members)
    };
    for block in drawn {
        collect_block_text(block, member_expanded(block), &[], selection);
    }
}

/// A call's copy fragments, in the order `render_tool` registers them: its
/// call line; a failure's tag; once open, a command's `$ cmd` and its
/// output or another call's details, then its diffs.
fn collect_tool_text(block: BlockId, tool: &ToolBlock, expanded: bool, selection: &TextRuns) {
    if super::agents::is_agent(tool) {
        let _ = selection.line(block, agent_title(tool), Vec::new());
        let _ = selection.line(block, agent_detail(tool), Vec::new());
        return;
    }
    match call_pieces(tool) {
        CallPieces::Whole(call) => {
            let _ = selection.markdown(block, call);
        }
        CallPieces::File { path, .. } => {
            let _ = selection.markdown(block, path);
        }
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

    /// The live row names a command by its program alone, in its state's
    /// verb, whatever the model wrote beside it.
    #[test]
    fn the_live_row_names_a_command_by_its_program() {
        let mut transcript = Transcript::default();
        let command = |transcript: &mut Transcript, id: &str, command: &str| {
            transcript.apply(Input::Event(SessionEvent::ToolStarted {
                id: id.into(),
                name: "Bash".into(),
                input: serde_json::json!({ "command": command, "description": "Edit the file" }),
            }));
        };
        command(&mut transcript, "a", "cd crates/x && sed -i '' 's/a/b/' lib.rs");
        command(&mut transcript, "b", "RUST_LOG=debug /usr/bin/cargo test");
        let words = |transcript: &Transcript| -> Vec<String> {
            transcript
                .blocks()
                .iter()
                .filter_map(|block| match &block.body {
                    Body::Tool(tool) => Some(live_words(tool)),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(words(&transcript), ["Running sed", "Running cargo"]);
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "a".into(),
            output: String::new(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "b".into(),
            output: "boom".into(),
            is_error: true,
            result: ferrite_core::ToolResult::Opaque,
        }));
        assert_eq!(words(&transcript), ["Ran sed", "Failed cargo"]);
    }

    /// A command's row reads its command (T3's); the provider's
    /// description is the working line's.
    #[test]
    fn a_command_row_reads_its_command_not_its_description() {
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
        assert_eq!(rows, ["cargo test", "sleep 15"]);
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

    fn call(transcript: &mut Transcript, id: &str, name: &str, input: serde_json::Value) {
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: id.into(),
            name: name.into(),
            input,
        }));
    }

    fn settle(transcript: &mut Transcript, id: &str) {
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: id.into(),
            output: "ok".into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        }));
    }

    fn summary(build: impl FnOnce(&mut Transcript)) -> String {
        let mut transcript = Transcript::default();
        build(&mut transcript);
        group_summary(transcript.blocks())
    }

    fn command(transcript: &mut Transcript, id: &str) {
        call(transcript, id, "Bash", serde_json::json!({ "command": "ls" }));
        settle(transcript, id);
    }

    fn edit(transcript: &mut Transcript, id: &str, path: &str) {
        call(transcript, id, "Edit", serde_json::json!({ "file_path": path }));
        settle(transcript, id);
    }

    /// T3's grammar: two actions named with `and`, an edit counted by the
    /// distinct files it changed, singular at one.
    #[test]
    fn a_group_summary_names_two_actions_in_the_past_tense() {
        assert_eq!(
            summary(|t| {
                for at in 0..4 {
                    command(t, &format!("c{at}"));
                }
                for at in 0..12 {
                    edit(t, &format!("e{at}"), &format!("src/{}.rs", at % 10));
                }
            }),
            "Ran 4 commands and changed 10 files"
        );
        assert_eq!(summary(|t| command(t, "a")), "Ran 1 command");
        assert_eq!(
            summary(|t| {
                edit(t, "a", "src/a.rs");
                edit(t, "b", "src/a.rs");
            }),
            "Changed 1 file"
        );
        // Still running, still the past tense; a failure says nothing.
        assert_eq!(
            summary(|t| {
                command(t, "a");
                call(t, "b", "Bash", serde_json::json!({ "command": "make" }));
                read(t, "c", true);
            }),
            "Ran 2 commands and read 1 file"
        );
    }

    /// The weightier actions are chosen, then put in the order they first
    /// appeared; the rest are other actions, after an Oxford comma.
    #[test]
    fn a_group_summary_counts_what_it_does_not_name() {
        assert_eq!(
            summary(|t| {
                read(t, "r", false);
                call(t, "g", "Grep", serde_json::json!({ "pattern": "x" }));
                command(t, "c");
                edit(t, "e", "a.rs");
                call(t, "o", "SomethingNew", serde_json::json!({}));
            }),
            "Ran 1 command, changed 1 file, and performed 3 other actions"
        );
        assert_eq!(
            summary(|t| {
                call(t, "a", "mcp__github__get_issue", serde_json::json!({}));
                call(t, "b", "mcp__linear__get_issue", serde_json::json!({}));
                command(t, "c");
            }),
            "Used github and linear integrations and ran 1 command"
        );
    }

    /// Thoughts alone are counted as thoughts; beside calls they are not
    /// counted at all.
    #[test]
    fn a_group_of_thoughts_reads_thought() {
        let thought = |t: &mut Transcript, text: &str| {
            t.apply(Input::Event(SessionEvent::ThinkingDelta { text: text.into() }));
            t.apply(Input::Event(SessionEvent::ContentBoundary));
        };
        assert_eq!(
            summary(|t| {
                thought(t, "one");
                thought(t, "two");
                thought(t, "three");
            }),
            "Thought (\u{d7}3)"
        );
        assert_eq!(
            summary(|t| {
                thought(t, "one");
                command(t, "a");
            }),
            "Ran 1 command"
        );
    }
}
