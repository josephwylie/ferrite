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
            "{} tool calls · {unavailable} results unavailable",
            activity.blocks.len()
        )
    } else {
        activity.summary()
    }
}

pub(super) fn redundant_test_result(tool: &ToolBlock) -> bool {
    tool.state == ToolState::Ok
        && is_test_run(tool)
        && tool.result_line.as_deref().and_then(passed_count).is_some()
}

pub(super) fn diff_body(line: &str) -> &str {
    match DiffKind::of(line) {
        DiffKind::Added | DiffKind::Removed => line[1..].trim_start(),
        DiffKind::Context => line.trim_start(),
    }
}

pub(crate) fn collect_output_text(block: BlockId, part: &str, text: &str, selection: &TextRuns) {
    if text.len() > 8 * 1024 {
        // Large output owns its selection in a separate native control.
        let _ = selection.output(block, part, text);
    } else {
        let _ = selection.line(block, text.to_owned(), Vec::new());
    }
}

pub(crate) fn collect_block_text(block: &Block, expanded: bool, selection: &TextRuns) {
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
        Body::Notice(text) | Body::Meta(text) => {
            let _ = selection.line(block.id, text.clone(), Vec::new());
        }
        Body::TurnEnd(end) => {
            let _ = selection.line(block.id, end.text(), Vec::new());
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
    if expanded {
        if !tool.summary.is_empty() {
            collect_output_text(block, "command", &tool.summary, selection);
        }
        if let Some(output) = &tool.output {
            collect_output_text(block, "result", &output.text, selection);
        }
        if let Some(output) = tool.structured_output() {
            collect_output_text(block, "details", &output.text, selection);
        }
    } else {
        if !redundant_test_result(tool) && (!in_group || matches!(tool.state, ToolState::Failed(_)))
        {
            if let Some(line) = &tool.result_line {
                let _ = selection.line(block, line.clone(), Vec::new());
            }
        }
        if let ToolState::Failed(message) = &tool.state {
            let first = message
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("");
            if !first.is_empty() && tool.result_line.as_deref() != Some(first) {
                let _ = selection.line(block, first.to_owned(), Vec::new());
            }
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
