//! Logical copy fragments, collected without allocating GPUI elements.
//!
//! Keep disclosure/order rules aligned with the row renderer. The native
//! rendering tests compare both projections, including hidden tool details.
use super::*;

pub(super) fn tool_label(tool: &ToolBlock) -> String {
    if tool.summary.is_empty() {
        tool.name.clone()
    } else {
        format!("{}({})", tool.name, tool_summary_line(tool))
    }
}

pub(super) fn activity_label(activity: &ToolActivity<'_>) -> String {
    let unavailable = activity
        .blocks
        .iter()
        .filter(
            |block| matches!(&block.body, Body::Tool(tool) if tool.state == ToolState::Unavailable),
        )
        .count();
    if unavailable > 0 {
        format!(
            "{} tool calls · {unavailable} without results",
            activity.blocks.len()
        )
    } else {
        activity.summary()
    }
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

/// A disclosed call's output, unless it only repeats the word its trail
/// already says (`applied`, beside the diff card): one fact, one place.
pub(super) fn disclosed_output(tool: &ToolBlock) -> Option<&ferrite_core::transcript::ToolOutput> {
    let output = tool.output.as_ref()?;
    (trail_result(tool) != Some(output.text.trim())).then_some(output)
}

pub(super) fn redundant_test_result(tool: &ToolBlock) -> bool {
    tool.state == ToolState::Ok
        && is_test_run(tool)
        && tool.result_line.as_deref().and_then(passed_count).is_some()
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

/// `512 B`, `1.2 KB`, `3.4 MB`.
pub(super) fn byte_size(bytes: usize) -> String {
    match bytes {
        bytes if bytes < 1024 => format!("{bytes} B"),
        bytes if bytes < 1024 * 1024 => format!("{:.1} KB", bytes as f64 / 1024.0),
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

pub(crate) fn collect_block_text(block: &Block, expanded: bool, signal: u32, selection: &TextRuns) {
    match &block.body {
        Body::Prompt(line) => {
            let (text, _) = ferrite_core::prompt_files::split(line.clone());
            if !text.is_empty() {
                let _ = selection.line(block.id, text, Vec::new());
            }
        }
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
        Body::Notice(text) => {
            let text = super::notice_text(text, super::notice_docked(signal));
            let _ = selection.line(block.id, text.to_owned(), Vec::new());
        }
        Body::Meta(text) => {
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
        Body::Tool(tool) => collect_tool_text(block.id, tool, expanded, false, selection),
    }
}

fn collect_tool_text(
    block: BlockId,
    tool: &ToolBlock,
    expanded: bool,
    in_group: bool,
    selection: &TextRuns,
) {
    let _ = selection.line(block, tool_label(tool), Vec::new());
    let applied = trail_result(tool);
    if let Some(applied) = applied {
        let _ = selection.line(block, applied.to_owned(), Vec::new());
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
    } else {
        if !redundant_test_result(tool)
            && applied.is_none()
            && (!in_group || matches!(tool.state, ToolState::Failed(_)))
        {
            if let Some(line) = &tool.result_line {
                let _ = selection.line(block, line.clone(), Vec::new());
            }
        }
        if let Some(excerpt) = failed_excerpt(tool) {
            let _ = selection.line(block, failed_head(), Vec::new());
            let _ = selection.line(block, excerpt.to_owned(), Vec::new());
        }
    }
    if expanded || !in_group {
        for diff in &tool.diffs {
            let (cap, _) = hunk_rows(diff.hunks.iter().map(|hunk| hunk.lines.len()).sum());
            for line in diff.hunks.iter().flat_map(|hunk| &hunk.lines).take(cap) {
                let _ = selection.line(block, diff_body(line).to_owned(), Vec::new());
            }
        }
    }
}

pub(crate) fn collect_activity_text(
    activity: ToolActivity<'_>,
    expanded: bool,
    state: impl Fn(&DisclosureId) -> DisclosureState,
    selection: &TextRuns,
) {
    let _ = selection.line(activity.blocks[0].id, activity_label(&activity), Vec::new());
    for block in activity.blocks {
        let Body::Tool(tool) = &block.body else {
            continue;
        };
        if expanded || matches!(tool.state, ToolState::Failed(_)) {
            collect_tool_text(
                block.id,
                tool,
                state(&DisclosureId::Tool(tool.call.clone())) == DisclosureState::Expanded,
                true,
                selection,
            );
        }
    }
}
