//! Logical copy fragments, collected without allocating GPUI elements.
//!
//! Keep disclosure/order rules aligned with the row renderer. The native
//! rendering tests compare both projections, including hidden tool details.
use super::*;

/// A call line as it reads: `Name(args)`, the name as Claude Code prints it
/// (`Update`, `Bash`), a shell command out of its `bash -lc '…'` wrapper.
pub(super) fn tool_label(tool: &ToolBlock) -> String {
    let name = ferrite_core::transcript::display_tool_name(&tool.name);
    if tool.summary.is_empty() {
        name.to_string()
    } else {
        format!("{name}({})", tool_summary_line(tool))
    }
}

/// The call line's argument. It never lays out hard line breaks: the
/// original command stays in the ToolBlock and becomes selectable in the
/// disclosed details.
fn tool_summary_line(tool: &ToolBlock) -> std::borrow::Cow<'_, str> {
    // A command reads as the command (`Bash(cargo test)`), as Claude Code
    // prints it, whatever purpose the model wrote beside it (Claude's Bash
    // `description`: "Run cargo test to identify failing tests").
    let summary = if ferrite_core::docview::is_command_run(&tool.name) && !tool.summary.is_empty() {
        ferrite_core::providers::shell::unwrap_shell(&tool.summary)
    } else {
        std::borrow::Cow::Borrowed(tool.title.as_deref().unwrap_or(&tool.summary))
    };
    if summary.contains(['\n', '\r']) {
        let first = summary
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("");
        format!("{} \u{2026}", first.trim()).into()
    } else {
        summary
    }
}

/// Where a path argument sits in its call line, in cells: `(start, len)` —
/// after the name and its `(`. `None` when the argument is not a path.
pub(super) fn path_argument(tool: &ToolBlock) -> Option<(usize, usize)> {
    if !argument_is_path(tool) {
        return None;
    }
    let name = ferrite_core::transcript::display_tool_name(&tool.name)
        .chars()
        .count();
    Some((name + 1, tool.summary.chars().count()))
}

/// What a call whose result never came says under it.
pub(super) const NO_RESULT: &str = "no result";

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
            || (tool.output.is_none() && tool.structured_result.is_none() && tool.diffs.is_empty()))
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
/// `diff_shown` is whether a call's diff is drawn (a later edit's folds
/// behind `+ show diff`); `wide` lays diffs side by side.
pub(crate) fn collect_block_text(
    block: &Block,
    expanded: bool,
    diff_shown: bool,
    wide: bool,
    selection: &TextRuns,
) {
    match &block.body {
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
            let text = end.text();
            if end.completed() {
                let _ = selection.line(block.id, text, Vec::new());
            } else {
                let (head, message) = turn_end_runs(&text, turn_end_message(end));
                let _ = selection.line(block.id, head, Vec::new());
                if let Some(message) = message {
                    let _ = selection.line(block.id, message, Vec::new());
                }
            }
        }
        Body::Code { source, .. } => {
            let _ = selection.line(block.id, source.clone(), Vec::new());
        }
        Body::Tool(tool) => {
            collect_tool_text(block.id, tool, expanded, diff_shown, wide, selection)
        }
    }
}

/// A call's copy fragments, in the order `render_tool` registers them: its
/// call line; what hangs on its elbow (a test run's verdict and preview, a
/// command's first line, a failure and its fold, a result); its disclosed
/// details; its diffs while they are drawn.
fn collect_tool_text(
    block: BlockId,
    tool: &ToolBlock,
    expanded: bool,
    diff_shown: bool,
    wide: bool,
    selection: &TextRuns,
) {
    let line = |text: String| {
        let _ = selection.line(block, text, Vec::new());
    };
    line(tool_label(tool));
    let command = ferrite_core::docview::is_command_run(&tool.name);
    match &tool.state {
        ToolState::Running => {
            if tool.progress.is_none() {
                if let Some(result) = &tool.result_line {
                    line(result.clone());
                }
            }
            if expanded {
                if shows_input(tool) {
                    collect_output_text(block, "command", &tool.summary, selection);
                }
                if let Some(output) = disclosed_output(tool) {
                    collect_output_text(block, "result", &output.text, selection);
                }
                if let Some(output) = tool.structured_output() {
                    collect_output_text(block, "details", &output.text, selection);
                }
            }
        }
        ToolState::Unavailable => {}
        ToolState::Failed(_) => {
            if let Some(elbow) = test_elbow(tool) {
                line(elbow.line());
                if !elbow.preview.is_empty() {
                    line(elbow.preview.join("\n"));
                }
                if elbow.hidden > 0 && expanded {
                    collect_fold(block, tool, test_rest(tool, &elbow), selection);
                }
            } else {
                line(failed_head());
                if let Some(detail) = tool
                    .result_line
                    .clone()
                    .or_else(|| failed_excerpt(tool).map(str::to_owned))
                {
                    line(detail);
                }
                if let Some((shown, hidden)) = output_fold(tool) {
                    line(shown);
                    if hidden > 0 && expanded {
                        collect_fold(block, tool, failed_rest(tool, hidden), selection);
                    }
                }
            }
        }
        ToolState::Ok => {
            if let Some(elbow) = test_elbow(tool) {
                line(elbow.line());
            } else if let Some((first, rest)) = command_fold(tool) {
                line(first);
                if expanded {
                    collect_fold(block, tool, rest.join("\n"), selection);
                }
            } else if tool.diffs.is_empty() {
                if let Some(result) = &tool.result_line {
                    line(result.clone());
                }
            }
            if expanded && !command {
                if shows_input(tool) {
                    collect_output_text(block, "command", &tool.summary, selection);
                }
                if let Some(output) = disclosed_output(tool) {
                    collect_output_text(block, "result", &output.text, selection);
                }
                if let Some(output) = tool.structured_output() {
                    collect_output_text(block, "details", &output.text, selection);
                }
            }
        }
    }
    if diff_shown {
        for diff in &tool.diffs {
            collect_diff_text(block, diff, wide, selection);
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

/// A diff's selectable code, in the order the rows register it
/// (`DiffRow::selectable`).
fn collect_diff_text(block: BlockId, diff: &Diff, wide: bool, selection: &TextRuns) {
    let (cap, _) = hunk_rows(diff.hunks.iter().map(|hunk| hunk.lines.len()).sum());
    for row in preview_rows(diff, cap, wide) {
        for side in row.selectable() {
            let _ = selection.line(block, side.body.clone(), Vec::new());
        }
    }
}
