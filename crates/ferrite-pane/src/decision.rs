//! The Decision: approvals and questions, answerable with one key from
//! anywhere. Owned by WP-F.
//!
//! A Decision is the transcript's tail row, drawn as a box (SPEC §5.9):
//! the `shield` head (a question's `?`), the command well, the options and,
//! under the box on the Pane that holds the keyboard, a dim hint line.
//! Every approval offers exactly three options — `1 Allow`, `2 Allow
//! <pattern> for this thread`, `3 Deny, and tell <Provider> what to do
//! instead` — and the first is pre-selected: the cursor's row `SELECTED`.
//!
//! Presentation and pure rules only — no cockpit state. The cockpit's
//! answer paths (`cockpit::decisions`) read the rows, the patterns and the
//! hint from here, so what the row shows is what the keys do.

use std::borrow::Cow;
use std::ops::Range;
use std::path::Path;

use ferrite_core::store::Provider;
use ferrite_core::{Decision, DecisionKind, ThreadId};
use gpui::prelude::*;
use gpui::{actions, div, px, rgb, rgba, AnyElement, App, Div, ElementId, SharedString, Stateful};

use crate::components;
use crate::pointer::{Pointer, PointerPressed};
use crate::theme;

actions!(
    decision,
    [
        /// ↓ on a Decision: the cursor moves to the next option.
        SelectNext,
        /// ↑ on a Decision: the cursor moves to the previous option.
        SelectPrevious,
        /// ⏎ on a Decision: pick the option under the cursor.
        Confirm,
        /// esc on a Decision: close an open note, else deny (dismiss a
        /// question).
        Dismiss,
        /// ⇥ on a Decision: a note sent with the answer.
        Amend,
    ]
);

/// The keys `PickOption1..4` bind: rows past the fourth are picked by
/// pointer or the cursor.
pub const DIGIT_KEYS: usize = 4;

/// What option 3 tells the model: the refusal, and that words follow.
pub const DENY_AND_STEER: &str = "The operator denied this and will say what to do instead.";

/// What a dismissed question tells the model.
pub const QUESTION_SKIPPED: &str = "The operator skipped this question.";

/// The three answers every approval offers, in their order on the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ApprovalChoice {
    /// `1. Allow`: run this call.
    Allow,
    /// `2. Allow <pattern> for this thread`: the provider's standing answer
    /// when it offered one, else Allow plus a Ferrite thread rule.
    AllowForThread,
    /// `3. Deny, and tell <Provider> what to do instead`: refuse, and the
    /// keyboard lands in the Pane's Composer to steer.
    DenyAndSteer,
}

impl ApprovalChoice {
    pub(crate) const ALL: [Self; 3] = [Self::Allow, Self::AllowForThread, Self::DenyAndSteer];

    /// The choice row `at` (0-based) offers.
    pub fn from_index(at: usize) -> Option<Self> {
        Self::ALL.get(at).copied()
    }

    pub fn index(self) -> usize {
        match self {
            Self::Allow => 0,
            Self::AllowForThread => 1,
            Self::DenyAndSteer => 2,
        }
    }

    /// The digit that picks it.
    pub(crate) fn key(self) -> &'static str {
        ["1", "2", "3"][self.index()]
    }

    /// Its one word on the wall (`1 allow` `2 always` `3 deny`).
    pub(crate) fn word(self) -> &'static str {
        ["allow", "always", "deny"][self.index()]
    }
}

// ------------------------------------------------------------- the words

/// The tool's name as the transcript prints it: Codex's command item reads
/// `Bash` like Claude's, its patch item `Edit`. (The shared
/// `ferrite_core::transcript::display_tool_name` is core-transcript's; the
/// Decision head keeps its own two-entry table so it never depends on that
/// table's other rows.)
pub fn tool_word(tool_name: &str) -> &str {
    match tool_name {
        "commandExecution" => "Bash",
        "fileChange" => "Edit",
        other => other,
    }
}

/// A provider's name as prose says it (`tell Codex what to do`).
pub fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "Claude",
        Provider::Codex => "Codex",
    }
}

/// A provider's lowercase word on a head (`· codex ·`).
pub fn provider_word(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "claude",
        Provider::Codex => "codex",
    }
}

/// What the agent may do while it asks, as the head's last detail: Codex's
/// sandbox (`read-only sandbox`, `workspace-write sandbox`, `full access`;
/// Codex's own default is read-only), Claude's permission mode (`default`,
/// `accept edits`, `plan`).
pub fn context_label(
    provider: Provider,
    permission_mode: Option<&str>,
    codex_sandbox: Option<&str>,
) -> SharedString {
    match provider {
        Provider::Codex => match codex_sandbox {
            None | Some("read-only") => "read-only sandbox".into(),
            Some("workspace-write") => "workspace-write sandbox".into(),
            Some("danger-full-access") => "full access".into(),
            Some(other) => format!("{other} sandbox").into(),
        },
        Provider::Claude => permission_mode
            .and_then(theme::mode_word)
            .unwrap_or_else(|| "default".into()),
    }
}

/// An approval head's lead: `<Tool> needs approval`, the tool as the
/// transcript names it.
pub fn approval_lead(decision: &Decision) -> SharedString {
    match tool_word(&decision.tool_name) {
        "" => theme::words::APPROVAL.into(),
        tool => format!("{tool} needs approval").into(),
    }
}

/// A head's details after its lead, `·`-joined (`codex · read-only
/// sandbox`); `None` when there are none.
pub fn head_detail<'a>(parts: impl IntoIterator<Item = &'a str>) -> Option<SharedString> {
    let parts: Vec<&str> = parts.into_iter().filter(|part| !part.is_empty()).collect();
    (!parts.is_empty()).then(|| parts.join(" \u{b7} ").into())
}

/// The head's kind word for what is not an approval.
pub fn kind_word(decision: &Decision) -> &'static str {
    match &decision.kind {
        DecisionKind::Approval => theme::words::APPROVAL,
        DecisionKind::Questions(_) => theme::words::QUESTION,
        DecisionKind::Form { .. } => "input needed",
        DecisionKind::External { .. } => "finish in browser",
        DecisionKind::Unsupported { .. } => "can't answer here",
    }
}

/// A provider's `" (Recommended)"` suffix, stripped for display only: the
/// answer goes back by index, so core still sends the original label.
pub fn split_recommended(label: &str) -> (&str, bool) {
    const SUFFIX: &str = " (recommended)";
    let cut = label.len().checked_sub(SUFFIX.len());
    match cut {
        Some(cut) if label.is_char_boundary(cut) && label[cut..].eq_ignore_ascii_case(SUFFIX) => {
            (label[..cut].trim_end(), true)
        }
        _ => (label, false),
    }
}

/// The approval hint, every word `TEXT_DIM`.
pub const APPROVAL_HINTS: [&str; 4] = [
    "\u{2191}\u{2193} select",
    "1\u{2013}3 pick",
    "esc deny",
    "tab amend",
];

/// A question's hint: its digits reach `keys` rows.
pub fn question_hints(keys: usize) -> Vec<String> {
    let pick = match keys {
        0 | 1 => "1 pick".to_string(),
        n => format!("1\u{2013}{n} pick"),
    };
    vec![
        "\u{2191}\u{2193} select".into(),
        pick,
        "esc dismiss".into(),
        "tab amend".into(),
    ]
}

/// A hint line's words, `·`-joined with a space either side.
pub fn hint_text<S: AsRef<str>>(parts: &[S]) -> String {
    parts
        .iter()
        .map(AsRef::as_ref)
        .collect::<Vec<_>>()
        .join(" \u{b7} ")
}

// ------------------------------------------------------------ the command

/// A login-shell wrapper taken off (`/bin/zsh -lc "…"`, `bash -c '…'`), so
/// a Codex command reads as the operator would type it. Anything that is
/// not exactly one quoted argument to a known shell comes back unchanged.
/// A wrapper inside a wrapper comes off too: 0.160 asks to escalate
/// `/bin/zsh -lc "/bin/zsh -lc 'printf …'"`.
pub fn unwrap_shell(raw: &str) -> Cow<'_, str> {
    let mut command = unwrap_shell_once(raw);
    for _ in 0..3 {
        match unwrap_shell_once(&command) {
            Cow::Owned(inner) => command = Cow::Owned(inner),
            Cow::Borrowed(_) => break,
        }
    }
    command
}

fn unwrap_shell_once(raw: &str) -> Cow<'_, str> {
    let line = raw.trim();
    for flag in [" -lc ", " -c "] {
        let Some(at) = line.find(flag) else {
            continue;
        };
        if !known_shell(&line[..at]) {
            continue;
        }
        let quoted = line[at + flag.len()..].trim_start();
        let inner = if let Some(body) = quoted
            .strip_prefix('\'')
            .and_then(|rest| rest.strip_suffix('\''))
        {
            single_quoted(body)
        } else if let Some(body) = quoted
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
        {
            double_quoted(body)
        } else {
            None
        };
        if let Some(inner) = inner {
            return Cow::Owned(inner);
        }
    }
    Cow::Borrowed(raw)
}

/// A shell program a login wrapper names (`/bin/zsh`, `bash`).
fn known_shell(program: &str) -> bool {
    !program.contains(char::is_whitespace)
        && program
            .rsplit('/')
            .next()
            .is_some_and(|name| matches!(name, "sh" | "bash" | "zsh" | "fish" | "dash"))
}

/// A single-quoted shell word's text: the quote juggling `'"'"'` and
/// `'\''` rejoins as `'`; any other `'` means it was not one word.
fn single_quoted(body: &str) -> Option<String> {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(at) = rest.find('\'') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        rest = tail
            .strip_prefix("'\"'\"'")
            .or_else(|| tail.strip_prefix("'\\''"))?;
        out.push('\'');
    }
    out.push_str(rest);
    Some(out)
}

/// A double-quoted shell word's text: `\"` `\\` `\$` `` \` `` unescape; an
/// unescaped `"` means it was not one word.
fn double_quoted(body: &str) -> Option<String> {
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some(next @ ('"' | '\\' | '$' | '`')) => out.push(next),
                Some('\n') => {}
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            },
            '"' => return None,
            c => out.push(c),
        }
    }
    Some(out)
}

/// The shell command an approval would run — Claude's `Bash` as sent,
/// Codex's `commandExecution` with its login-shell wrapper taken off — or
/// `None` when the call is not a shell command.
pub fn shell_source(decision: &Decision) -> Option<Cow<'_, str>> {
    let command = decision.input.get("command")?.as_str()?;
    match decision.tool_name.as_str() {
        "Bash" => Some(Cow::Borrowed(command)),
        "commandExecution" => Some(unwrap_shell(command)),
        _ => None,
    }
}

/// Whether an approval's prose says only what its command band already
/// shows — the description, trimmed (a Codex description is the wrapped
/// command itself), equals the command or sits inside its first line. Such
/// prose is dropped: the subject is printed once, and a reason stays.
pub fn prose_repeats_command(description: &str, command: Option<&str>) -> bool {
    let description = description.trim();
    let Some(command) = command else {
        return false;
    };
    if description.is_empty() {
        return false;
    }
    let unwrapped = unwrap_shell(description);
    let unwrapped = unwrapped.trim();
    let first = command.lines().next().unwrap_or_default().trim();
    command.trim() == description
        || command.trim() == unwrapped
        || first.contains(description)
        || first.contains(unwrapped)
}

/// What an approval's prose says beyond its head and its well — or `None`
/// when that is nothing. Claude composes the prose from the request's
/// parts (`cargo test · Bash · This command requires approval`,
/// `src/stats.rs · Edit`): a part that names the tool, repeats the command
/// or the edited path, or only says approval is required goes; a real
/// reason (a blocked path, the provider's own why) stays.
pub fn prose_beyond_subject(decision: &Decision, command: Option<&str>) -> Option<String> {
    let description = decision.description.trim();
    if description.is_empty() || prose_repeats_command(description, command) {
        return None;
    }
    let tool = decision.tool_name.to_lowercase();
    let word = tool_word(&decision.tool_name).to_lowercase();
    let path = edit_path(decision);
    let said = |part: &str| -> bool {
        let lower = part.to_lowercase();
        lower == tool
            || lower == word
            || prose_repeats_command(part, command)
            || path.is_some_and(|path| path == part || path.ends_with(&format!("/{part}")))
            || lower.ends_with("requires approval")
            || lower.ends_with("needs approval")
    };
    let rest: Vec<&str> = description
        .split(" \u{b7} ")
        .map(str::trim)
        .filter(|part| !part.is_empty() && !said(part))
        .collect();
    (!rest.is_empty()).then(|| rest.join(" \u{b7} "))
}

// ------------------------------------------------------------ the patterns

/// A shell command's leading words, as a rule names them: up to two, and
/// never an option (`cargo test --workspace` → `cargo test`).
pub fn command_prefix(command: &str) -> Vec<&str> {
    command
        .split_whitespace()
        .take_while(|word| !word.starts_with('-'))
        .take(2)
        .collect()
}

/// One plain command: no list, pipe, redirection, substitution or second
/// line. Only such a command can be allowed by a Ferrite thread rule — a
/// prefix says nothing about what follows a `&&`.
fn simple_command(command: &str) -> bool {
    !command.contains(['\n', ';', '&', '|', '<', '>', '`']) && !command.contains("$(")
}

/// The tools whose input names one file.
const EDIT_TOOLS: [&str; 5] = ["Edit", "Write", "MultiEdit", "NotebookEdit", "fileChange"];

fn edit_path(decision: &Decision) -> Option<&str> {
    ["file_path", "path", "notebook_path"]
        .iter()
        .find_map(|key| decision.input.get(*key)?.as_str())
        .filter(|path| !path.is_empty())
}

/// A path as the Thread reads it: inside its workspace, relative.
fn relative<'a>(path: &'a str, workspace: Option<&Path>) -> Cow<'a, str> {
    workspace
        .and_then(|root| Path::new(path).strip_prefix(root).ok())
        .map(|inside| Cow::Owned(inside.to_string_lossy().into_owned()))
        .unwrap_or(Cow::Borrowed(path))
}

/// The pattern Ferrite derives for option 2 when the provider offered no
/// standing answer: a shell command's first two words and ` *`
/// (`gh issue *`), an edit's directory and `/**`, else the tool's name.
pub fn thread_pattern(decision: &Decision, workspace: Option<&Path>) -> String {
    let tool = tool_word(&decision.tool_name);
    if let Some(command) = shell_source(decision) {
        let prefix = command_prefix(&command);
        if !prefix.is_empty() {
            return format!("{} *", prefix.join(" "));
        }
    } else if EDIT_TOOLS.contains(&decision.tool_name.as_str()) {
        if let Some(path) = edit_path(decision) {
            let path = relative(path, workspace);
            let dir = Path::new(path.as_ref())
                .parent()
                .map(|dir| dir.to_string_lossy().into_owned())
                .unwrap_or_default();
            return if dir.is_empty() {
                "./**".into()
            } else {
                format!("{dir}/**")
            };
        }
    }
    tool.to_string()
}

/// The rule a standing answer adopts, in the same words a Ferrite rule
/// uses: Claude's `Bash(gh issue:*)` and Codex's execpolicy prefix
/// `["gh", "issue"]` both read `gh issue *`; a wrapped Codex prefix reads
/// as its one command; a network grant as its host. `None` for an answer
/// that is not a rule (a session grant, a mode switch).
pub fn standing_pattern(value: &serde_json::Value) -> Option<String> {
    if value["behavior"].as_str() == Some("allow") {
        let rules = value["rules"]
            .as_array()?
            .iter()
            .map(|rule| {
                let tool = rule["toolName"].as_str()?;
                Some(match rule["ruleContent"].as_str() {
                    Some(content) if tool == "Bash" => content
                        .strip_suffix(":*")
                        .map(|prefix| format!("{prefix} *"))
                        .unwrap_or_else(|| content.to_string()),
                    Some(content) => content.to_string(),
                    None => tool.to_string(),
                })
            })
            .collect::<Option<Vec<_>>>()?;
        return (!rules.is_empty()).then(|| rules.join(", "));
    }
    if let Some(parts) = value
        .get("acceptWithExecpolicyAmendment")
        .and_then(|amendment| amendment["execpolicy_amendment"].as_array())
    {
        let argv: Vec<&str> = parts
            .iter()
            .map(serde_json::Value::as_str)
            .collect::<Option<_>>()?;
        return match argv.as_slice() {
            [] => None,
            [shell, "-lc" | "-c", command] if known_shell(shell) => Some((*command).to_string()),
            words => Some(format!("{} *", words.join(" "))),
        };
    }
    if let Some(policy) = value
        .get("applyNetworkPolicyAmendment")
        .map(|amendment| &amendment["network_policy_amendment"])
    {
        if policy["action"].as_str() == Some("allow") {
            return policy["host"]
                .as_str()
                .map(str::trim)
                .filter(|host| !host.is_empty())
                .map(str::to_string);
        }
    }
    None
}

/// Whether a provider's standing answer ends with this Thread's Session —
/// what option 2's `for this thread` promises. Codex's execpolicy and
/// network amendments are written to the operator's own rules
/// (`~/.codex/rules/default.rules`, seen live on 0.160) and Claude's
/// `localSettings`/`projectSettings`/`userSettings` rules to settings
/// files: they outlive the Thread, so option 2 keeps a Ferrite thread rule
/// instead. A session grant (`acceptForSession`, Claude's `session`
/// destination) and an answer that names no scope qualify.
fn thread_scoped(value: &serde_json::Value) -> bool {
    if value.get("acceptWithExecpolicyAmendment").is_some()
        || value.get("applyNetworkPolicyAmendment").is_some()
    {
        return false;
    }
    match value.get("destination").and_then(serde_json::Value::as_str) {
        Some(destination) => destination == "session",
        None => true,
    }
}

/// The provider's standing answer option 2 adopts — a rule-shaped one
/// before a session grant — and its pattern when it has one. Only an
/// answer scoped to this Thread (`thread_scoped`).
pub fn standing_choice(decision: &Decision) -> Option<(usize, Option<String>)> {
    let standing: Vec<usize> = decision
        .suggestions
        .iter()
        .enumerate()
        .filter(|(_, choice)| choice.standing && thread_scoped(&choice.value))
        .map(|(at, _)| at)
        .collect();
    standing
        .iter()
        .find_map(|&at| standing_pattern(&decision.suggestions[at].value).map(|p| (at, Some(p))))
        .or_else(|| standing.first().map(|&at| (at, None)))
}

/// Option 2's pattern: the standing answer's rule, else Ferrite's own.
pub fn option_two_pattern(decision: &Decision, workspace: Option<&Path>) -> String {
    standing_choice(decision)
        .and_then(|(_, pattern)| pattern)
        .unwrap_or_else(|| thread_pattern(decision, workspace))
}

/// A Ferrite thread rule (option 2 with no standing answer): later
/// approvals in `thread` that `pattern` covers are allowed without asking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadRule {
    pub thread: ThreadId,
    /// The tool as the transcript names it (`Bash` for both providers).
    pub tool: String,
    pub pattern: String,
}

impl ThreadRule {
    pub fn new(thread: ThreadId, decision: &Decision, workspace: Option<&Path>) -> Self {
        Self {
            thread,
            tool: tool_word(&decision.tool_name).to_string(),
            pattern: thread_pattern(decision, workspace),
        }
    }

    /// Whether this rule answers `decision` in `thread`: the same tool, an
    /// approval, and — for a shell pattern — a plain command starting with
    /// the pattern's words; for a directory pattern, a file under it; a
    /// tool-wide pattern covers the tool.
    pub fn covers(&self, thread: ThreadId, decision: &Decision, workspace: Option<&Path>) -> bool {
        if thread != self.thread
            || self.pattern.is_empty()
            || !matches!(decision.kind, DecisionKind::Approval)
            || tool_word(&decision.tool_name) != self.tool
        {
            return false;
        }
        if let Some(prefix) = self.pattern.strip_suffix(" *") {
            let Some(command) = shell_source(decision) else {
                return false;
            };
            let want: Vec<&str> = prefix.split_whitespace().collect();
            let words: Vec<&str> = command.split_whitespace().take(want.len()).collect();
            return !want.is_empty() && words == want && simple_command(&command);
        }
        if let Some(dir) = self.pattern.strip_suffix("/**") {
            let Some(path) = edit_path(decision) else {
                return false;
            };
            let path = relative(path, workspace);
            let path = Path::new(path.as_ref());
            // A `..` anywhere could climb out of the directory.
            let climbs = path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir));
            return !climbs
                && if dir == "." {
                    !path.is_absolute()
                } else {
                    path.starts_with(dir)
                };
        }
        self.pattern == self.tool
    }
}

// --------------------------------------------------------------- the rows

/// One approval option: its choice, the digit that picks it, its words
/// (with the pattern's byte range, drawn as inline code) and whether
/// picking it can do anything.
#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalRow {
    pub choice: ApprovalChoice,
    pub key: SharedString,
    pub label: SharedString,
    pub code: Option<Range<usize>>,
    pub enabled: bool,
}

impl ApprovalRow {
    /// Its one word on the wall.
    pub fn word(&self) -> &'static str {
        self.choice.word()
    }
}

/// An approval's three options, keyed `1` `2` `3` — the same list drives
/// the row, the digit keys, the cursor, the wall and the tests.
pub fn approval_rows(
    decision: &Decision,
    provider: Option<Provider>,
    workspace: Option<&Path>,
) -> Vec<ApprovalRow> {
    let policy = &decision.policy;
    let allows = policy.allow && !policy.interaction_required;
    let standing = standing_choice(decision).is_some();
    let pattern = option_two_pattern(decision, workspace);
    let lead = "Allow ";
    let agent = provider.map_or("the agent", provider_name);
    ApprovalChoice::ALL
        .into_iter()
        .map(|choice| {
            let (label, code, enabled) = match choice {
                ApprovalChoice::Allow => ("Allow".to_string(), None, allows),
                ApprovalChoice::AllowForThread => (
                    format!("{lead}{pattern} for this thread"),
                    Some(lead.len()..lead.len() + pattern.len()),
                    standing || allows,
                ),
                ApprovalChoice::DenyAndSteer => (
                    format!("Deny, and tell {agent} what to do instead"),
                    None,
                    policy.deny,
                ),
            };
            ApprovalRow {
                choice,
                key: choice.key().into(),
                label: label.into(),
                code,
                enabled,
            }
        })
        .collect()
}

/// Where the cursor starts: the first option that can act.
pub fn first_enabled(rows: &[ApprovalRow]) -> usize {
    rows.iter().position(|row| row.enabled).unwrap_or(0)
}

/// The cursor one step from `at` (wrapping), over the options that can act.
pub fn step_cursor(enabled: &[bool], at: usize, step: isize) -> usize {
    let count = enabled.len();
    if count == 0 {
        return 0;
    }
    let mut next = at.min(count - 1);
    for _ in 0..count {
        next = (next as isize + step).rem_euclid(count as isize) as usize;
        if enabled[next] {
            return next;
        }
    }
    at.min(count - 1)
}

// ------------------------------------------------------------------ frame

/// The Decision box (SPEC §5.9 item 3): `DECISION` ground at `R_BLOCK`
/// inside a 1px `DECISION_RING`, padded `DECISION_PAD`, its sections
/// `DECISION_GAP` apart, in Geist 13/20, across the transcript column. It
/// keeps the `question-island` selector every kind has always answered to.
pub fn card(serial: u64, children: impl IntoIterator<Item = AnyElement>) -> Stateful<Div> {
    div()
        .id(("question-island", serial as usize))
        .debug_selector(|| "question-island".into())
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(px(theme::DECISION_GAP))
        .w_full()
        .min_w_0()
        .cursor_default()
        // The ring is inset: its pixel is the padding's first.
        .p(px(theme::DECISION_PAD - 1.0))
        .border_1()
        .border_color(rgba(theme::DECISION_RING))
        .rounded(px(theme::R_BLOCK))
        .bg(theme::paint::DECISION)
        .font_family(theme::FONT_UI)
        .font_weight(theme::W_BODY)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(theme::TEXT))
        .children(children)
}

/// The box, then the hint line under it (`DECISION_HINT_GAP` below): the
/// keys, said only where the keyboard is.
pub fn with_hint(card: Stateful<Div>, hint: Option<Div>) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(px(theme::DECISION_HINT_GAP))
        .w_full()
        .min_w_0()
        .child(card)
        .children(hint)
}

/// A section of the box: the box's gap spaces it.
pub fn section(element: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .w_full()
        .min_w_0()
        .child(element)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    Shield,
    Question,
}

/// The approval head (`Bash needs approval   codex · read-only sandbox`):
/// the `shield` in `ATTENTION`, the lead 13 at `W_LABEL` in `TEXT_STRONG`,
/// and at the right the context in 12 `TEXT_DIM`, then a status. One line;
/// a long context ends in an ellipsis.
pub fn head(
    lead: impl Into<SharedString>,
    detail: Option<SharedString>,
    status: Option<AnyElement>,
) -> Div {
    head_line(Mark::Shield, lead.into(), detail, status)
}

/// A question's head: a `?` in `ATTENTION` where the shield stands, the
/// question in the lead's type, wrapping.
pub fn question_head(
    text: impl Into<SharedString>,
    detail: Option<SharedString>,
    status: Option<AnyElement>,
) -> Div {
    head_line(Mark::Question, text.into(), detail, status)
}

fn head_line(
    mark: Mark,
    lead: SharedString,
    detail: Option<SharedString>,
    status: Option<AnyElement>,
) -> Div {
    let question = mark == Mark::Question;
    let glyph = match mark {
        Mark::Shield => crate::icons::icon(
            crate::icons::SHIELD,
            theme::DECISION_HEAD_ICON,
            theme::ATTENTION,
        )
        .into_any_element(),
        Mark::Question => components::glyph("?", theme::ATTENTION).into_any_element(),
    };
    div()
        .debug_selector(|| "decision-head".into())
        .flex()
        .flex_shrink_0()
        .items_start()
        .gap(px(theme::SPACE_2))
        .w_full()
        .min_w_0()
        .min_h(px(theme::LH_UI))
        .child(
            div()
                .debug_selector(|| "decision-mark".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .w(px(theme::DECISION_HEAD_ICON))
                .h(px(theme::LH_UI))
                .child(glyph),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_weight(theme::W_LABEL)
                .text_color(rgb(theme::TEXT_STRONG))
                .when(!question, |line| line.truncate())
                .child(lead),
        )
        .children(detail.map(|detail| {
            div()
                .flex_shrink(1.)
                .min_w_0()
                .truncate()
                .text_size(px(theme::FS_META))
                .text_color(rgb(theme::TEXT_DIM))
                .child(detail)
        }))
        .children(status.map(|status| div().flex_shrink_0().child(status)))
}

/// A head's right-hand status (`answer when ready`, `work continues`).
pub fn status(text: impl Into<SharedString>) -> Div {
    div()
        .debug_selector(|| "decision-status".into())
        .flex_shrink_0()
        .text_color(rgb(theme::TEXT_DIM))
        .child(text.into())
}

/// An answer in flight: a still `RUNNING` dot and `sending`, `TEXT_DIM`.
/// Nothing on it moves — a Decision is static (rule 2.10.4).
pub fn sending() -> Div {
    div()
        .debug_selector(|| "decision-status".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::CH))
        .text_color(rgb(theme::TEXT_DIM))
        .child(components::status_dot(theme::RUNNING))
        .child(theme::words::SENDING)
}

/// One question of several: `W_STRONG` `TEXT_STRONG` on the grid, on the
/// prose measure.
pub fn question_text(text: impl Into<SharedString>) -> Div {
    div()
        .w_full()
        .min_w_0()
        .max_w(px(theme::MEASURE_CH * theme::CH))
        .flex_shrink_0()
        .font_weight(theme::W_STRONG)
        .text_color(rgb(theme::TEXT_STRONG))
        .child(text.into())
}

/// What an approval asks, in prose — a reason the command does not say:
/// `TEXT` on the grid, on the prose measure.
pub fn prose(text: impl Into<SharedString>) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .max_w(px(theme::MEASURE_CH * theme::CH))
        .text_color(rgb(theme::TEXT))
        .child(text.into())
}

/// A UI aside (`choose any`, a form field's hint).
pub fn note(text: impl Into<SharedString>) -> Div {
    div()
        .flex_shrink_0()
        .text_color(rgb(theme::TEXT_DIM))
        .child(text.into())
}

/// The command well (SPEC §5.9): the exact input in Geist Mono 12.5
/// `TEXT_STRONG` on `SUNKEN` at `R_TAB`, padded `DECISION_WELL_PAD_X` /
/// `_Y`. A shell command reads `$ gh issue close 212`: the `$ ` is
/// `TEXT_DIM` and outside the selectable text, so a copy gives the command
/// alone. A long input scrolls inside it.
pub fn well(prompt: bool, child: impl IntoElement) -> Div {
    div()
        .flex()
        .items_start()
        .w_full()
        .min_w_0()
        .flex_shrink_1()
        .min_h(px(theme::DECISION_WELL_MIN_H))
        .overflow_hidden()
        .rounded(px(theme::R_TAB))
        .bg(theme::paint::SUNKEN)
        .px(px(theme::DECISION_WELL_PAD_X))
        .py(px(theme::DECISION_WELL_PAD_Y))
        .font_family(theme::FONT_CODE)
        .text_size(px(theme::FS_LABEL))
        .text_color(rgb(theme::TEXT_STRONG))
        .when(prompt, |well| {
            well.child(
                div()
                    .debug_selector(|| "approval-prompt".into())
                    .flex_shrink_0()
                    .text_color(rgb(theme::TEXT_DIM))
                    .child("$\u{a0}"),
            )
        })
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .max_h_full()
                .child(child),
        )
}

// ------------------------------------------------------------------- rows

/// What one option row shows.
#[derive(Default)]
pub struct Row {
    /// The key that picks it, dim before the label; `None` leaves no key.
    pub key: Option<SharedString>,
    pub label: SharedString,
    /// A byte range of `label` drawn in Geist Mono (a pattern).
    pub code: Option<Range<usize>>,
    pub description: Option<SharedString>,
    pub recommended: bool,
    /// The row under the cursor: `SELECTED`, its label `TEXT_STRONG`.
    pub cursor: bool,
    /// A question's pick (a multi-select's checked option): a trailing
    /// strong check.
    pub picked: bool,
    pub enabled: bool,
}

/// One option row (SPEC §5.9) on `gpui_base::Button` (tab stop,
/// Enter/Space, focus ring): `DECISION_OPTION_H` at `R_TAB`, padded
/// `DECISION_OPTION_PAD_X`; the key in Geist Mono 12 `TEXT_DIM` in a
/// one-cell column, `DECISION_OPTION_GAP` before the label in Geist 13
/// `TEXT` (`TEXT_STRONG` under the cursor, `TEXT_DIM` disabled), a pattern
/// in Geist Mono 12. The cursor's row is `SELECTED` (selector
/// `decision-cursor`); under the pointer `RAISED`. No `❯`, no keycaps.
/// The key and the trailing word stand on the label's first line, as a
/// list's number does, when a description (or a wrap) adds lines under it.
pub fn option_row(id: impl Into<ElementId>, row: Row) -> gpui_base::Button {
    let id = id.into();
    let hover = crate::pointer::hover_key(&id);
    let ink = match (row.enabled, row.cursor) {
        (false, _) => theme::TEXT_DIM,
        (true, true) => theme::TEXT_STRONG,
        (true, false) => theme::TEXT,
    };
    let accessibility = SharedString::from(format!(
        "{}{}",
        row.label,
        if row.picked { ", selected" } else { "" }
    ));
    let mono = |text: SharedString| {
        div()
            .flex_shrink_0()
            .font_family(theme::FONT_CODE)
            .text_size(px(theme::FS_META))
            .child(text)
    };
    // The label's runs: a pattern in the code face between plain words.
    let label = row.label.clone();
    let code = row
        .code
        .clone()
        .filter(|code| !code.is_empty() && code.end <= label.len());
    let mut words = div()
        .flex()
        .items_center()
        .flex_wrap()
        .min_w_0()
        .whitespace_nowrap();
    match code {
        Some(code) => {
            let part = |range: Range<usize>| SharedString::from(label[range].to_string());
            words = words
                .child(part(0..code.start))
                .child(mono(part(code.clone())))
                .child(part(code.end..label.len()));
        }
        None => words = words.child(label),
    }
    let keyed = row.key.is_some();
    let key = row.key.filter(|_| row.enabled);
    let trailing = || {
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(theme::LH_UI))
            .pl(px(theme::CH))
    };
    let cursor = row.enabled && row.cursor;
    gpui_base::Button::new(id)
        .tab_stop(row.enabled)
        .disabled(!row.enabled)
        .accessibility_label(accessibility)
        .w_full()
        .min_w_0()
        .min_h(px(theme::DECISION_OPTION_H))
        .flex_shrink_0()
        .justify_start()
        .items_start()
        .gap(px(theme::DECISION_OPTION_GAP))
        .px(px(theme::DECISION_OPTION_PAD_X))
        .py(px((theme::DECISION_OPTION_H - theme::LH_UI) / 2.0))
        .rounded(px(theme::R_TAB))
        .font_family(theme::FONT_UI)
        .font_weight(theme::W_BODY)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(ink))
        // The cursor's fill is a keyboard change: at once.
        .map(|button| match (row.enabled, row.cursor) {
            (_, true) => button.bg(theme::paint::SELECTED).cursor_pointer(),
            (false, false) => button.cursor_default(),
            (true, false) => button.hover_raised(hover).press_raised(),
        })
        .focus_visible(components::control_focus)
        // The cursor's row answers to `decision-cursor` (the caller names
        // the button itself).
        .when(cursor, |button| {
            button.relative().child(
                div()
                    .absolute()
                    .inset_0()
                    .debug_selector(|| "decision-cursor".into()),
            )
        })
        .children(keyed.then(|| {
            div()
                .flex_shrink_0()
                .w(px(theme::FS_META * theme::CODE_ADVANCE))
                .font_family(theme::FONT_CODE)
                .text_size(px(theme::FS_META))
                .text_color(rgb(theme::TEXT_DIM))
                .children(key)
        }))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(words)
                .children(
                    row.description
                        .filter(|text| !text.is_empty())
                        .map(|text| div().text_color(rgb(theme::TEXT_DIM)).child(text)),
                ),
        )
        .when(row.recommended, |button| {
            button.child(
                trailing()
                    .text_color(rgb(theme::TEXT_DIM))
                    .child("recommended"),
            )
        })
        .when(row.picked, |button| {
            button.child(trailing().child(components::glyph("\u{2713}", theme::TEXT_STRONG)))
        })
}

/// The amend note under the options (⇥): `note` dim on the options' text
/// column, then the one-line field that holds the keyboard.
pub fn note_row(field: impl IntoElement) -> Div {
    div()
        .debug_selector(|| "decision-note".into())
        .flex()
        .flex_shrink_0()
        .items_start()
        .w_full()
        .min_w_0()
        .min_h(px(theme::LH_UI))
        .pl(px(theme::DECISION_OPTION_PAD_X))
        .child(
            div()
                .flex_shrink_0()
                .pr(px(theme::CH))
                .text_color(rgb(theme::TEXT_DIM))
                .child("note"),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .text_color(rgb(theme::TEXT))
                .child(field),
        )
}

// ---------------------------------------------------------------- footer

/// The one error line: the typed `✗` and `lead` in `BLOCKED` (only that
/// phrase carries the hue), then `· detail` in `TEXT_DIM`.
pub fn error_line(lead: &'static str, detail: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_start()
        .gap(px(theme::CH))
        .min_w_0()
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .h(px(theme::LH_UI))
                .child(components::glyph("\u{2717}", theme::BLOCKED)),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(rgb(theme::BLOCKED))
                .child(lead),
        )
        .child(
            div()
                .min_w_0()
                .text_color(rgb(theme::TEXT_DIM))
                .child(SharedString::from(format!("\u{b7} {}", detail.into()))),
        )
}

/// The hint line (`↑↓ select · 1–3 pick · esc deny · tab amend`): one
/// 12px `TEXT_DIM` run, its `·` seams the same ink, no keycaps.
pub fn hint_line<S: AsRef<str>>(parts: &[S]) -> Div {
    div()
        .debug_selector(|| "decision-hints".into())
        .flex_shrink_0()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_META))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(theme::TEXT_DIM))
        .child(SharedString::from(hint_text(parts)))
}

/// The answer row of a form: the hint line on the left, actions on the
/// right.
pub fn footer<S: AsRef<str>>(parts: &[S], actions: impl IntoIterator<Item = AnyElement>) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::SPACE_2))
        .min_w_0()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .children((!parts.is_empty()).then(|| hint_line(parts))),
        )
        .children(actions)
}

/// The completing action (`Send`, `Complete`): the primary, its label at
/// `W_BODY`. `enter` adds the key that sends — a mono `↵` at
/// `ON_PRIMARY` 70% — so the key lives on the button, not in the hints.
pub fn send_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    disabled: bool,
    enter: bool,
    cx: &App,
) -> gpui::component::button::Button {
    let ink = if disabled {
        theme::TEXT_DIM
    } else {
        theme::ON_PRIMARY
    };
    components::primary_button(id, disabled, cx)
        .h(px(theme::CONTROL_H))
        .px(px(theme::CONTROL_PAD_X))
        .child(
            components::text_ui()
                .flex()
                .items_center()
                .gap(px(theme::CH))
                .text_color(rgb(ink))
                .child(label.into())
                .when(enter, |label| {
                    label.child(
                        div()
                            .font_family(theme::FONT_CODE)
                            .map(|key| {
                                if disabled {
                                    key.text_color(rgb(theme::TEXT_DIM))
                                } else {
                                    key.text_color(rgba(theme::SEND_KEY_INK))
                                }
                            })
                            .child("\u{21b5}"),
                    )
                }),
        )
}

/// The declining action (`Skip`, `Cancel`): a ghost with no key.
pub fn skip_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &App,
) -> gpui::component::button::Button {
    components::ghost_button(id, label, cx).tab_stop(true)
}

// -------------------------------------------------------------------- L2

/// An L2 quick answer — `y allow` — pressable (the prototype's `.qa
/// button`): a 1px `paint::LINE2` edge, one cell inside, the key dim and
/// the verb in `TEXT`; under the pointer the hover face and `TEXT_STRONG`.
/// A narrow cell drops the verb and keeps the key (`verb` false): pairs go
/// whole, never cut. No keycaps.
#[cfg(any(test, feature = "test-support"))]
pub fn key_action(
    id: &'static str,
    key: &'static str,
    verb: &'static str,
    with_verb: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .group(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::CH))
        .px(px(theme::CH))
        .border_1()
        .border_color(theme::paint::LINE2)
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(theme::TEXT))
        .hover_raised(id)
        .press_row()
        .child(div().text_color(rgb(theme::TEXT_DIM)).child(key))
        .when(with_verb, |pair| {
            pair.child(
                div()
                    .group_hover(id, |style| style.text_color(rgb(theme::TEXT_STRONG)))
                    .child(verb),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::DecisionChoice;

    fn approval(
        tool: &str,
        input: serde_json::Value,
        suggestions: Vec<DecisionChoice>,
    ) -> Decision {
        Decision {
            delivery: Default::default(),
            kind: Default::default(),
            policy: Default::default(),
            id: "perm".into(),
            tool_use_id: "toolu".into(),
            tool_name: tool.into(),
            description: String::new(),
            input,
            suggestions,
        }
    }

    fn bash(command: &str) -> Decision {
        approval("Bash", serde_json::json!({ "command": command }), vec![])
    }

    fn choice(label: &str, value: serde_json::Value, standing: bool) -> DecisionChoice {
        DecisionChoice {
            label: label.into(),
            value,
            standing,
        }
    }

    fn labels(rows: &[ApprovalRow]) -> Vec<(String, String)> {
        rows.iter()
            .map(|row| (row.key.to_string(), row.label.to_string()))
            .collect()
    }

    /// D-3: every approval offers exactly three options, keyed 1–3, in the
    /// prototype's words; the pattern is drawn as inline code.
    #[test]
    fn an_approval_always_offers_the_three_options() {
        let rows = approval_rows(
            &bash("gh issue close 212 --reason \"not planned\""),
            Some(Provider::Codex),
            None,
        );
        assert_eq!(
            labels(&rows),
            [
                ("1".into(), "Allow".into()),
                ("2".into(), "Allow gh issue * for this thread".into()),
                ("3".into(), "Deny, and tell Codex what to do instead".into()),
            ]
        );
        let code = rows[1].code.clone().unwrap();
        assert_eq!(&rows[1].label[code], "gh issue *");
        assert!(rows.iter().all(|row| row.enabled));
        assert_eq!(
            rows.iter().map(ApprovalRow::word).collect::<Vec<_>>(),
            ["allow", "always", "deny"]
        );
        assert_eq!(
            labels(&approval_rows(&bash("ls"), Some(Provider::Claude), None))[2].1,
            "Deny, and tell Claude what to do instead"
        );
    }

    /// Option 2 names the provider's standing rule when it offered one
    /// that ends with the Thread, else Ferrite's own.
    #[test]
    fn option_two_names_the_standing_rule_before_its_own() {
        let mut claude = bash("gh issue close 212");
        claude.suggestions = vec![choice(
            "Allow Bash(gh issue:*) in local settings",
            serde_json::json!({
                "type": "addRules",
                "behavior": "allow",
                "destination": "localSettings",
                "rules": [{ "toolName": "Bash", "ruleContent": "gh issue:*" }],
            }),
            true,
        )];
        assert_eq!(option_two_pattern(&claude, None), "gh issue *");
        let mut codex = approval(
            "commandExecution",
            serde_json::json!({ "command": "/bin/zsh -lc 'cargo test --workspace'" }),
            vec![
                choice(
                    "Allow for this session",
                    serde_json::json!("acceptForSession"),
                    true,
                ),
                choice(
                    "Always allow cargo test",
                    serde_json::json!({ "acceptWithExecpolicyAmendment": {
                        "execpolicy_amendment": ["cargo", "test"] } }),
                    true,
                ),
            ],
        );
        // An amendment Codex writes to the operator's own rules outlives the
        // Thread: the session grant is what `for this thread` adopts.
        assert_eq!(standing_choice(&codex), Some((0, None)));
        codex.suggestions.remove(0);
        assert_eq!(
            standing_choice(&codex),
            None,
            "an amendment alone leaves option 2 to a Ferrite thread rule"
        );
        // Claude's rule for local settings outlives the Thread too; its
        // session-scoped twin is adopted.
        assert_eq!(standing_choice(&claude), None);
        claude.suggestions[0].value["destination"] = "session".into();
        assert_eq!(
            standing_choice(&claude),
            Some((0, Some("gh issue *".into())))
        );
        codex.suggestions = vec![choice(
            "Allow for this session",
            serde_json::json!("acceptForSession"),
            true,
        )];
        assert_eq!(standing_choice(&codex), Some((0, None)));
        // A session grant has no rule: Ferrite's own pattern names it.
        assert_eq!(option_two_pattern(&codex, None), "cargo test *");
        let wrapped = serde_json::json!({ "acceptWithExecpolicyAmendment": {
            "execpolicy_amendment": ["/bin/zsh", "-lc", "printf ok > ferrite-perm.txt"] } });
        assert_eq!(
            standing_pattern(&wrapped).as_deref(),
            Some("printf ok > ferrite-perm.txt")
        );
    }

    /// Ferrite's own pattern: a command's first two words, an edit's
    /// directory, else the tool.
    #[test]
    fn ferrite_derives_a_pattern_per_tool() {
        assert_eq!(
            thread_pattern(&bash("gh issue close 212"), None),
            "gh issue *"
        );
        assert_eq!(
            thread_pattern(&bash("cargo test --workspace"), None),
            "cargo test *"
        );
        assert_eq!(thread_pattern(&bash("ls -la"), None), "ls *");
        let edit = approval(
            "Edit",
            serde_json::json!({ "file_path": "/work/ferrite/crates/core/src/lib.rs" }),
            vec![],
        );
        assert_eq!(
            thread_pattern(&edit, Some(Path::new("/work/ferrite"))),
            "crates/core/src/**"
        );
        assert_eq!(
            thread_pattern(&edit, None),
            "/work/ferrite/crates/core/src/**"
        );
        let root = approval(
            "Write",
            serde_json::json!({ "file_path": "README.md" }),
            vec![],
        );
        assert_eq!(thread_pattern(&root, None), "./**");
        let fetch = approval(
            "WebFetch",
            serde_json::json!({ "url": "https://x" }),
            vec![],
        );
        assert_eq!(thread_pattern(&fetch, None), "WebFetch");
    }

    /// A thread rule allows only what its pattern covers, in its Thread,
    /// and never a compound command.
    #[test]
    fn a_thread_rule_covers_only_its_pattern() {
        let thread = ThreadId::new(7);
        let rule = ThreadRule::new(thread, &bash("gh issue close 212"), None);
        assert_eq!(rule.pattern, "gh issue *");
        assert!(rule.covers(thread, &bash("gh issue list --state open"), None));
        assert!(!rule.covers(ThreadId::new(8), &bash("gh issue list"), None));
        assert!(!rule.covers(thread, &bash("gh pr merge 3"), None));
        assert!(!rule.covers(thread, &bash("gh issue list && rm -rf /"), None));
        assert!(!rule.covers(thread, &bash("gh issue list | sh"), None));
        // Codex's command item is the same `Bash` tool.
        let codex = approval(
            "commandExecution",
            serde_json::json!({ "command": "/bin/zsh -lc 'gh issue view 9'" }),
            vec![],
        );
        assert!(rule.covers(thread, &codex, None));
        let edit = |path: &str| approval("Edit", serde_json::json!({ "file_path": path }), vec![]);
        let dir = ThreadRule::new(thread, &edit("src/ui/a.rs"), None);
        assert_eq!(dir.pattern, "src/ui/**");
        assert!(dir.covers(thread, &edit("src/ui/deep/b.rs"), None));
        assert!(!dir.covers(thread, &edit("src/other.rs"), None));
        assert!(!dir.covers(thread, &edit("src/ui/../../etc/passwd"), None));
        assert!(!dir.covers(thread, &bash("cat src/ui/a.rs"), None));
    }

    /// D-7: Codex's login-shell wrapper comes off, in either quoting, so
    /// both providers read `$ gh issue close 212 --reason "not planned"`.
    #[test]
    fn the_shell_wrapper_comes_off_codex_commands() {
        assert_eq!(
            unwrap_shell(r#"/bin/zsh -lc "gh issue close 212 --reason \"not planned\"""#),
            r#"gh issue close 212 --reason "not planned""#
        );
        assert_eq!(unwrap_shell("/bin/bash -c 'make -j'"), "make -j");
        assert_eq!(
            unwrap_shell(r#"/bin/zsh -lc 'printf '"'"'ok'"'"''"#),
            "printf 'ok'"
        );
        assert_eq!(unwrap_shell("python -c 'print(1)'"), "python -c 'print(1)'");
        assert_eq!(
            unwrap_shell("/bin/zsh -lc 'a' && 'b'"),
            "/bin/zsh -lc 'a' && 'b'"
        );
        assert_eq!(unwrap_shell("gh issue list"), "gh issue list");
    }

    /// D-7 against a captured Codex request (`codex` 0.149.1's
    /// `item/commandExecution/requestApproval`): the band holds the
    /// command alone, behind the shell prompt, and the description — the
    /// wrapped command itself — is never printed again as prose.
    #[test]
    fn a_captured_codex_request_reads_as_its_command() {
        let capture =
            include_str!("../../ferrite-core/tests/fixtures/codex-approval-allow-0.149.1.jsonl");
        let request: serde_json::Value = capture
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|line| line["method"] == "item/commandExecution/requestApproval")
            .expect("the capture holds the approval request");
        let params = request["params"].clone();
        let decision = Decision {
            delivery: Default::default(),
            kind: Default::default(),
            policy: Default::default(),
            id: "0".into(),
            tool_use_id: params["itemId"].as_str().unwrap().into(),
            tool_name: "commandExecution".into(),
            description: params["command"].as_str().unwrap().into(),
            input: params,
            suggestions: vec![],
        };
        assert!(crate::pane::shell_command(&decision));
        let command = shell_source(&decision).unwrap();
        assert_eq!(command, "printf 'ok' > ferrite-perm.txt");
        assert!(prose_repeats_command(&decision.description, Some(&command)));
        assert_eq!(approval_lead(&decision), "Bash needs approval");
        // Claude's Bash reads the same, untouched.
        let claude = bash(r#"gh issue close 212 --reason "not planned""#);
        assert!(crate::pane::shell_command(&claude));
        assert_eq!(
            shell_source(&claude).unwrap(),
            r#"gh issue close 212 --reason "not planned""#
        );
    }

    /// D-2: the head's words.
    #[test]
    fn the_head_names_tool_provider_and_context() {
        let decision = approval(
            "commandExecution",
            serde_json::json!({ "command": "/bin/zsh -lc 'gh issue close 212'" }),
            vec![],
        );
        let context = context_label(Provider::Codex, None, Some("read-only"));
        assert_eq!(
            format!(
                "{} \u{b7} {}",
                approval_lead(&decision),
                head_detail([provider_word(Provider::Codex), context.as_ref()]).unwrap()
            ),
            "Bash needs approval \u{b7} codex \u{b7} read-only sandbox"
        );
        assert_eq!(
            context_label(Provider::Codex, None, None),
            "read-only sandbox"
        );
        assert_eq!(
            context_label(Provider::Codex, None, Some("workspace-write")),
            "workspace-write sandbox"
        );
        assert_eq!(
            context_label(Provider::Codex, None, Some("danger-full-access")),
            "full access"
        );
        assert_eq!(context_label(Provider::Claude, None, None), "default");
        assert_eq!(
            context_label(Provider::Claude, Some("default"), None),
            "default"
        );
        assert_eq!(
            context_label(Provider::Claude, Some("acceptEdits"), None),
            "accept edits"
        );
        assert_eq!(context_label(Provider::Claude, Some("plan"), None), "plan");
    }

    /// D-5: the hint, exactly.
    #[test]
    fn the_hints_read_as_the_prototype() {
        assert_eq!(
            hint_text(&APPROVAL_HINTS),
            "\u{2191}\u{2193} select \u{b7} 1\u{2013}3 pick \u{b7} esc deny \u{b7} tab amend"
        );
        assert_eq!(
            hint_text(&question_hints(4)),
            "\u{2191}\u{2193} select \u{b7} 1\u{2013}4 pick \u{b7} esc dismiss \u{b7} tab amend"
        );
    }

    /// A forbidden plain Allow leaves its row unable to act, and the cursor
    /// starts and steps over the rows that can.
    #[test]
    fn the_cursor_skips_options_that_cannot_act() {
        let mut decision = bash("ls");
        decision.policy.allow = false;
        let rows = approval_rows(&decision, None, None);
        assert_eq!(
            rows.iter().map(|row| row.enabled).collect::<Vec<_>>(),
            [false, false, true]
        );
        assert_eq!(first_enabled(&rows), 2);
        decision.suggestions = vec![choice("Allow this session", serde_json::json!("x"), true)];
        let rows = approval_rows(&decision, None, None);
        // The standing answer still goes back as the provider's own choice.
        assert!(rows[1].enabled);
        assert_eq!(first_enabled(&rows), 1);
        let enabled = [true, true, true];
        assert_eq!(step_cursor(&enabled, 0, 1), 1);
        assert_eq!(step_cursor(&enabled, 2, 1), 0);
        assert_eq!(step_cursor(&enabled, 0, -1), 2);
        assert_eq!(step_cursor(&[true, false, true], 0, 1), 2);
    }

    /// The subject is printed once: prose that only restates the command
    /// goes, and a reason stays.
    #[test]
    fn prose_that_repeats_the_command_is_dropped() {
        let command = Some("gh issue close 212\n--comment done");
        assert!(prose_repeats_command("gh issue close 212", command));
        assert!(prose_repeats_command("  issue close 212 ", command));
        assert!(!prose_repeats_command(
            "Close the stale issue after the fix landed",
            command
        ));
        assert!(!prose_repeats_command("gh issue close 212", None));
        assert!(!prose_repeats_command("", command));
        assert!(prose_repeats_command(
            "/bin/zsh -lc 'gh issue close 212'",
            Some("gh issue close 212")
        ));
    }

    /// The head's kind words are the lexicon's.
    #[test]
    fn kind_words_come_from_the_lexicon() {
        let mut decision = bash("ls");
        assert_eq!(kind_word(&decision), theme::words::APPROVAL);
        decision.kind = DecisionKind::Questions(Vec::new());
        assert_eq!(kind_word(&decision), theme::words::QUESTION);
    }

    #[test]
    fn recommended_is_stripped_for_display_only() {
        assert_eq!(
            split_recommended("Keep the draft (Recommended)"),
            ("Keep the draft", true)
        );
        assert_eq!(split_recommended("keep (recommended)"), ("keep", true));
        assert_eq!(split_recommended("Rewrite"), ("Rewrite", false));
        assert_eq!(split_recommended("é"), ("é", false));
    }
}
