//! How each provider tool presents: what kind of work it is, where its row
//! goes among its neighbours, the name it shows, the one line its row
//! shows, and what its result summarizes to. One table ([`policy`]) for
//! every surface: a tool name is never matched anywhere else.

/// What kind of work a call is, for the words its row uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind<'a> {
    Command,
    Read,
    Edit,
    Write,
    Search,
    List,
    Fetch,
    WebSearch,
    /// A subagent: it has a transcript of its own.
    Agent,
    /// A checklist tool. The plan it acknowledges is `Progress.plan`.
    Todo,
    /// A question or plan approval put to the operator.
    Question,
    /// Ferrite's own `show_visual`.
    Visual,
    /// An MCP call, by its server (`mcp__github__get_issue` → `github`).
    Mcp(&'a str),
    /// Anything else: shown by its name, state and details.
    Other,
}

/// Where a call's row goes among the rows beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grouping {
    /// Folds into a group with the routine calls beside it.
    RollsUp,
    /// Folds only with the agent calls beside it, into Claude's `Ran 2
    /// agents` card; a lone agent keeps a row of its own. Either way the
    /// routine group before it ends.
    Agents,
    /// Stands alone: the group before it ends, another starts after it.
    StandsAlone,
    /// Only state: something else draws what it did (the checklist), so it
    /// has no row unless it failed ([`super::ToolBlock::draws_row`]).
    StateOnly,
}

/// How one tool presents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolPolicy<'a> {
    pub kind: ToolKind<'a>,
    pub grouping: Grouping,
    /// The name its row shows, the way Claude Code prints it: a file edit
    /// is `Update` (Claude's `Edit` and `MultiEdit`, Codex's `fileChange`),
    /// a Codex shell call is `Bash`; every other name stands as sent.
    pub display: &'a str,
}

/// The name every bridge gives a call that starts a subagent: Claude's own
/// (`Agent`, `Task` before it), and what the Codex bridge calls its spawns.
pub const AGENT: &str = "Agent";

/// The tool presentation table. Decided by the name alone, never by a
/// result still to come; an unknown name is `Other`, rolling up.
pub fn policy(name: &str) -> ToolPolicy<'_> {
    use Grouping::*;
    use ToolKind::*;
    let (kind, grouping) = match name {
        // Before `mcp__`: Claude calls the visual `mcp__ferrite__show_visual`.
        name if crate::visual::is_tool(name) => (Visual, StandsAlone),
        name if crate::questions::is_question_tool(name) || name == "ExitPlanMode" => {
            (Question, StandsAlone)
        }
        "Bash" | "commandExecution" | "BashOutput" => (Command, RollsUp),
        "Read" | "NotebookRead" | "read_file" | "imageView" => (Read, RollsUp),
        "Edit" | "MultiEdit" | "NotebookEdit" | "fileChange" => (Edit, RollsUp),
        "Write" => (Write, RollsUp),
        "Grep" | "Glob" => (Search, RollsUp),
        "LS" => (List, RollsUp),
        "WebFetch" => (Fetch, RollsUp),
        "WebSearch" | "webSearch" => (WebSearch, RollsUp),
        "Task" | AGENT => (Agent, Agents),
        // `TaskStop` and `TaskOutput` are background-task tools, not the
        // checklist's: they stay ordinary rows.
        "TodoWrite" | "TaskCreate" | "TaskUpdate" | "TaskGet" | "TaskList" => (Todo, StateOnly),
        // Before `mcp__`: Ferrite's own plan tool is the checklist's too.
        name if crate::update_plan::is_tool(name) => (Todo, StateOnly),
        name => match name
            .strip_prefix("mcp__")
            .and_then(|rest| rest.split_once("__"))
        {
            Some((server, _)) if !server.is_empty() => (Mcp(server), RollsUp),
            _ => (Other, RollsUp),
        },
    };
    let display = match name {
        "Edit" | "MultiEdit" | "fileChange" => "Update",
        "commandExecution" => "Bash",
        other => other,
    };
    ToolPolicy {
        kind,
        grouping,
        display,
    }
}

/// [`ToolPolicy::display`] for `name`.
pub fn display_tool_name(name: &str) -> &str {
    policy(name).display
}

/// Whether a tool runs a command, so its summary is the command itself: the
/// transcript echoes it as `$ command` when the call is disclosed. Every
/// other tool's summary is a path, and a path is not a test result.
pub fn is_command_run(name: &str) -> bool {
    matches!(name, "Bash" | "commandExecution")
}

/// A call's subject cut to what a one-line status has room for (the
/// working line's `Reading main.rs`): a path to its file name, a command to
/// its head (`cargo build --release` → `cargo build`); anything else whole.
pub fn short_subject<'a>(kind: ToolKind, subject: &'a str) -> &'a str {
    match kind {
        ToolKind::Command => super::command_head(subject),
        ToolKind::Read | ToolKind::Edit | ToolKind::Write | ToolKind::List => subject
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .filter(|name| !name.is_empty())
            .unwrap_or(subject),
        _ => subject,
    }
}

/// `1 line` / `412 lines`.
fn counted(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// What a read or a search found, as Claude Code's own rows summarize it:
/// a Read's `412 lines` (its result's `file.numLines`, else the lines it
/// printed); a Grep's `6 matches in 2 files` (content mode), `2 files`
/// (files-with-matches) or `6 matches` (count). `None` for any other tool,
/// which keeps its first output line.
pub(super) fn result_summary(
    name: &str,
    structured: Option<&serde_json::Value>,
    output: &str,
) -> Option<String> {
    let number = |value: &serde_json::Value, key: &str| value.get(key).and_then(|v| v.as_u64());
    match name {
        "Read" | "read_file" => {
            let lines = structured
                .and_then(|value| value.get("file"))
                .and_then(|file| number(file, "numLines"))
                .or_else(|| {
                    // Whitespace alone says nothing worth a row.
                    let count = output.lines().count() as u64;
                    (count > 0 && !output.trim().is_empty()).then_some(count)
                })?;
            Some(counted(lines, "line", "lines"))
        }
        "Grep" => {
            let value = structured?;
            let filenames = value
                .get("filenames")
                .and_then(|names| names.as_array())
                .map(|names| names.len() as u64);
            let content = value.get("content").and_then(|content| content.as_str());
            let files = number(value, "numFiles")
                .filter(|files| *files > 0)
                .or(filenames.filter(|files| *files > 0))
                .or_else(|| {
                    // Content lines are `path:line:text`; distinct paths
                    // are the files that matched.
                    let paths: std::collections::BTreeSet<&str> = content?
                        .lines()
                        .filter_map(|line| line.split_once(':').map(|(path, _)| path))
                        .collect();
                    (!paths.is_empty()).then_some(paths.len() as u64)
                })
                .unwrap_or(0);
            match value.get("mode").and_then(|mode| mode.as_str()) {
                Some("files_with_matches") => Some(counted(files, "file", "files")),
                Some("count") => {
                    let matches = number(value, "numMatches").or_else(|| {
                        content.map(|content| {
                            content
                                .lines()
                                .filter_map(|line| line.rsplit_once(':'))
                                .filter_map(|(_, count)| count.trim().parse::<u64>().ok())
                                .sum()
                        })
                    })?;
                    Some(counted(matches, "match", "matches"))
                }
                Some("content") | None => {
                    let matches = number(value, "numMatches")
                        .or_else(|| number(value, "numLines"))
                        .or_else(|| {
                            content.map(|content| {
                                content
                                    .lines()
                                    .filter(|line| !line.trim().is_empty())
                                    .count() as u64
                            })
                        })?;
                    Some(format!(
                        "{} in {}",
                        counted(matches, "match", "matches"),
                        counted(files.max(u64::from(matches > 0)), "file", "files")
                    ))
                }
                Some(_) => None,
            }
        }
        _ => None,
    }
}

/// The one line a collapsed tool row shows. Tool inputs are the vendor's own
/// schema, so this reads the few keys that name a subject and gives up
/// quietly on anything else rather than guessing. A search names what it
/// looks for, then where: `Grep(status_line, crates/ferrite/src)`.
pub(super) fn tool_summary(name: &str, input: &serde_json::Value) -> String {
    if matches!(name, "Grep" | "Glob") {
        if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
            return match input.get("path").and_then(|v| v.as_str()) {
                Some(path) if !path.is_empty() => format!("{pattern}, {path}"),
                _ => pattern.to_string(),
            };
        }
    }
    tool_subject(input)
}

fn tool_subject(input: &serde_json::Value) -> String {
    // A patch (Codex's `fileChange`) names its files in `changes`: the
    // first, and how many more.
    if let Some(changes) = input.get("changes").and_then(|v| v.as_array()) {
        let paths: Vec<&str> = changes
            .iter()
            .filter_map(|change| change.get("path")?.as_str())
            .collect();
        match paths.as_slice() {
            [] => {}
            [one] => return (*one).to_string(),
            [first, rest @ ..] => return format!("{first} +{}", rest.len()),
        }
    }
    for key in [
        "command",
        "file_path",
        "path",
        "pattern",
        "url",
        "subject",
        "query",
        "description",
        "prompt",
    ] {
        if let Some(value) = input.get(key).and_then(|v| v.as_str()) {
            return value.to_string();
        }
    }
    if let Some(args) = input.get("arguments").filter(|value| value.is_object()) {
        return tool_subject(args);
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_subject_keeps_what_a_status_line_needs() {
        use ToolKind::*;
        assert_eq!(
            short_subject(Read, "/repo/crates/ui/src/main.rs"),
            "main.rs"
        );
        assert_eq!(short_subject(List, "crates/ui/"), "ui");
        assert_eq!(
            short_subject(Command, "cargo build --release"),
            "cargo build"
        );
        assert_eq!(short_subject(Search, "fn main, src"), "fn main, src");
    }

    #[test]
    fn every_tool_has_one_policy() {
        use Grouping::*;
        use ToolKind::*;
        for (name, kind, grouping) in [
            ("Bash", Command, RollsUp),
            ("commandExecution", Command, RollsUp),
            ("BashOutput", Command, RollsUp),
            ("Read", Read, RollsUp),
            ("imageView", Read, RollsUp),
            ("MultiEdit", Edit, RollsUp),
            ("fileChange", Edit, RollsUp),
            ("Write", Write, RollsUp),
            ("Glob", Search, RollsUp),
            ("LS", List, RollsUp),
            ("WebFetch", Fetch, RollsUp),
            ("webSearch", WebSearch, RollsUp),
            ("Task", Agent, Agents),
            ("Agent", Agent, Agents),
            ("mcp__ferrite__show_visual", Visual, StandsAlone),
            ("ferrite.show_visual", Visual, StandsAlone),
            ("AskUserQuestion", Question, StandsAlone),
            ("ExitPlanMode", Question, StandsAlone),
            ("TodoWrite", Todo, StateOnly),
            ("TaskCreate", Todo, StateOnly),
            ("TaskUpdate", Todo, StateOnly),
            ("TaskList", Todo, StateOnly),
            ("mcp__ferrite__update_plan", Todo, StateOnly),
            ("TaskStop", Other, RollsUp),
            ("mcp__github__get_issue", Mcp("github"), RollsUp),
            ("mcp____odd", Other, RollsUp),
            ("SomethingNew", Other, RollsUp),
        ] {
            let policy = policy(name);
            assert_eq!((policy.kind, policy.grouping), (kind, grouping), "{name}");
        }
    }
}
