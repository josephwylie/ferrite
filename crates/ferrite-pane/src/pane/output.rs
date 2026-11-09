//! What a call printed: its open body (input well, exit status, output),
//! a test run's elbow and bar, and its diffs.

use super::*;

/// What a test run's verdict says (CT-14).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestElbow {
    /// `ok` or `failed`: the lead word.
    pub lead: &'static str,
    /// The words after the lead's seam: `38 passed`, `357 passed; 2 failed`.
    pub rest: String,
}

impl TestElbow {
    /// The elbow's one line: `ok · 38 passed`.
    pub fn line(&self) -> String {
        if self.rest.is_empty() {
            self.lead.to_string()
        } else {
            format!("{} \u{b7} {}", self.lead, self.rest)
        }
    }
}

/// A settled test run's verdict: `ok · <N> passed`, or `failed · <P>
/// passed; <F> failed` (its `test result:` lines, else counted). `None` for
/// a call that is not a settled test run.
pub fn test_elbow(tool: &ToolBlock) -> Option<TestElbow> {
    if !is_test_run(tool) {
        return None;
    }
    let exited = match &tool.state {
        ToolState::Ok => false,
        ToolState::Failed(_) => true,
        _ => return None,
    };
    let output = match tool.output.as_ref() {
        Some(output) => output.text.as_str(),
        None => match &tool.state {
            ToolState::Failed(message) => message.as_str(),
            _ => tool.result_line.as_deref().unwrap_or(""),
        },
    };
    let lines: Vec<&str> = output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    // Each test line's verdict (`ok`, `FAILED`).
    let tests: Vec<&str> = lines
        .iter()
        .filter_map(|line| ferrite_core::transcript::test_result_line(line))
        .map(|(_, verdict)| verdict)
        .collect();
    let results: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| line.trim_start().starts_with("test result:"))
        .collect();
    let counted = |verdict: &str| tests.iter().filter(|v| **v == verdict).count();
    let summary_of = |line: &str| -> Option<(usize, usize)> {
        let passed = passed_count(line)?;
        let failed = line
            .split(';')
            .find_map(|part| part.trim().strip_suffix(" failed")?.trim().parse().ok())
            .unwrap_or(0);
        Some((passed, failed))
    };
    // cargo prints one `test result:` per test binary (the crate's, each
    // integration suite's, the doc tests'): the run's tally is their sum —
    // unless the last one already accounts for every test the run announced
    // (`running 359 tests`, a workspace tally closing the output).
    let ignored_of = |line: &str| -> usize {
        line.split(';')
            .find_map(|part| part.trim().strip_suffix(" ignored")?.trim().parse().ok())
            .unwrap_or(0)
    };
    let announced: usize = lines
        .iter()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("running ")?
                .strip_suffix(" tests")
                .or_else(|| line.trim().strip_prefix("running ")?.strip_suffix(" test"))?
                .trim()
                .parse::<usize>()
                .ok()
        })
        .sum();
    let last = results
        .last()
        .and_then(|line| summary_of(line).map(|counts| (counts, ignored_of(line))));
    let totals = match last {
        Some(((passed, failed), ignored))
            if announced > 0 && passed + failed + ignored == announced =>
        {
            Some((passed, failed))
        }
        _ => results
            .iter()
            .filter_map(|line| summary_of(line))
            .reduce(|(p, f), (passed, failed)| (p + passed, f + failed)),
    };
    // A run is failed by its own report too: piped into `tail` or `grep`
    // the command exits 0 while its suite failed.
    let failed = exited
        || results
            .iter()
            .any(|line| line.trim_start().starts_with("test result: FAILED"))
        || tests.contains(&"FAILED");
    if !failed {
        let passed = totals
            .map(|(passed, _)| passed)
            .or_else(|| (!tests.is_empty()).then(|| counted("ok")))
            .or_else(|| tool.result_line.as_deref().and_then(passed_count));
        return Some(TestElbow {
            lead: "ok",
            rest: passed.map_or_else(String::new, |passed| format!("{passed} passed")),
        });
    }
    let (passed, failed_count) = totals.unwrap_or_else(|| (counted("ok"), counted("FAILED")));
    Some(TestElbow {
        lead: theme::words::FAILED,
        rest: format!("{passed} passed; {failed_count} failed"),
    })
}

/// What an open command holds (Claude's): the command as it ran (`$ cmd`,
/// out of its `bash -lc` wrapper, its `cd` kept), the code a failed run
/// exited with, and what it printed — its retained output, else a failure's
/// message, less a leading `Exit code N` line, which the status says.
pub(crate) struct Shell<'a> {
    pub command: std::borrow::Cow<'a, str>,
    pub output: Option<&'a str>,
    /// What the retained output left out.
    pub omitted: usize,
    /// The code a failed run said it exited with (Claude's `Exit code 1`
    /// line): its status. A success says nothing, as Claude's does.
    pub exit: Option<i64>,
    /// What it printed is the failure's own message, no code said (a hook's
    /// `Blocked: …`): it reads in the failure ink.
    pub failure: bool,
}

/// An open command's contents. See [`Shell`].
pub(crate) fn shell(tool: &ToolBlock) -> Shell<'_> {
    let retained = text::disclosed_output(tool);
    let printed = retained
        .map(|output| output.text.as_str())
        .or_else(|| text::failure_message(tool))
        .filter(|printed| !printed.trim().is_empty());
    let (code, output) = match printed.map(exit_line) {
        Some((code, rest)) => (code, Some(rest).filter(|rest| !rest.trim().is_empty())),
        None => (None, None),
    };
    Shell {
        command: ferrite_core::providers::shell::unwrap_shell(&tool.summary),
        output,
        omitted: retained.map_or(0, |output| output.omitted_bytes),
        exit: code.filter(|_| text::call_failed(tool)),
        failure: retained.is_none() && code.is_none() && output.is_some(),
    }
}

/// A leading `Exit code N` line (how Claude's Bash reports a non-zero exit)
/// and what follows it; the text whole, with no code, otherwise.
fn exit_line(printed: &str) -> (Option<i64>, &str) {
    let trimmed = printed.trim_start();
    let (first, rest) = trimmed.split_once('\n').unwrap_or((trimmed, ""));
    match first
        .trim()
        .strip_prefix("Exit code ")
        .and_then(|code| code.trim().parse().ok())
    {
        Some(code) => (Some(code), rest),
        None => (None, printed),
    }
}

/// An open call's body (Claude's, measured on claude/failed-open.png and
/// tools-expanded.png), the same in a group's card or a call's own: no
/// box of its own, no label. Its input in a quiet well — a command's `$ cmd`
/// (the `$` in `INK_SUBTLE`), another call's input where its line could not
/// show it whole — in `INK_STRONG`; then a failed command's `Exit code 1` in
/// `BLOCKED`, in the row's words; then what it printed, plain `INK_SUBTLE`
/// mono (a failure's own message in `BLOCKED`), its structured result, what
/// was not kept. Each part `SPACE_3` under the last, the printed text
/// `SPACE_0_5` more. Mono on the block grid. `None` when a call has nothing
/// to show.
pub(super) fn call_body(
    block: BlockId,
    tool: &ToolBlock,
    grid: Grid,
    selection: &TextRuns,
) -> Option<Div> {
    if tool.state == ToolState::Ok && !tool.findings.is_empty() {
        return Some(findings_body(block, &tool.findings, grid, selection));
    }
    let cells = grid.block();
    let mut input = None;
    let mut exit = None;
    let mut printed: Vec<AnyElement> = Vec::new();
    let mut part = |name: &str, text: &str, omitted: usize, ink: u32| {
        printed.push(
            output_block(block, name, text, selection)
                .text_color(rgb(ink))
                .into_any_element(),
        );
        if omitted > 0 {
            printed.push(omitted_line(omitted).into_any_element());
        }
    };
    if ferrite_core::transcript::tools::is_command_run(&tool.name) {
        let shell = shell(tool);
        input = Some(
            div()
                .flex()
                .items_start()
                .w_full()
                .min_w_0()
                .child(
                    div()
                        .flex_shrink_0()
                        .w(px(2. * cells.cell()))
                        .text_color(rgb(INK_SUBTLE))
                        .child("$"),
                )
                .child(
                    output_block(block, "command", &shell.command, selection)
                        .flex_1()
                        .text_color(rgb(INK_STRONG)),
                ),
        );
        exit = shell.exit;
        if let Some(output) = shell.output {
            let ink = if shell.failure { BLOCKED } else { INK_SUBTLE };
            part("result", output, shell.omitted, ink);
        }
    } else {
        if text::shows_input(tool) {
            input = Some(
                output_block(block, "command", &tool.summary, selection)
                    .text_color(rgb(INK_STRONG)),
            );
        }
        if let Some(output) = text::disclosed_output(tool) {
            part("result", &output.text, output.omitted_bytes, INK_SUBTLE);
        } else if let Some(message) = text::failure_message(tool) {
            part("result", message, 0, BLOCKED);
        }
        if let Some(structured) = tool.structured_preview.as_ref() {
            part(
                "details",
                &structured.text,
                structured.omitted_bytes,
                INK_SUBTLE,
            );
        }
    }
    if input.is_none() && exit.is_none() && printed.is_empty() {
        return None;
    }
    let well = input.map(|input| {
        div()
            .debug_selector(|| "tool-input".into())
            .mx(px(-theme::SPACE_0_5))
            .px(px(theme::SPACE_3))
            .py(px(theme::BLOCK_PAD_Y))
            .rounded(px(theme::R_CONTROL))
            .border_1()
            .border_color(theme::paint::CARD_RING)
            .child(input)
    });
    let status = exit.map(|code| {
        div()
            .debug_selector(|| "tool-exit".into())
            .font_family(theme::FONT_UI)
            .text_size(px(grid.activity().size))
            .line_height(px(grid.line))
            .text_color(rgb(BLOCKED))
            .child(SharedString::from(format!("Exit code {code}")))
    });
    let printed = (!printed.is_empty()).then(|| {
        div()
            .debug_selector(|| "tool-output".into())
            .flex()
            .flex_col()
            .min_w_0()
            .mt(px(theme::SPACE_0_5))
            .children(printed)
    });
    Some(
        on_cells(div())
            .flex()
            .flex_col()
            .gap(px(theme::SPACE_3))
            .w_full()
            .min_w_0()
            .text_size(px(cells.size))
            .line_height(px(cells.line))
            .children(well)
            .children(status)
            .children(printed),
    )
}

/// A running suite's line under its call (CT-25): `running 357 tests`, its
/// bar, and `212/357` in tabular `INK_SUBTLE`.
pub(super) fn running_tests(
    call: &str,
    progress: ferrite_core::transcript::TestProgress,
    grid: Grid,
) -> Div {
    let small = grid.activity();
    let line = content_row(grid)
        .debug_selector(|| "tool-test-progress".into())
        .items_center()
        .text_size(px(small.size))
        .text_color(rgb(INK_SUBTLE));
    // The runner's total unheard (`TestProgress::known`): the count alone,
    // where `212/357` would stand, and no bar to fill against nothing.
    if !progress.known() {
        return line
            .gap(px(theme::SPACE_2))
            .child(
                div()
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .child("running tests"),
            )
            .child(components::tabular(
                div()
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .child(SharedString::from(progress.done.to_string())),
            ));
    }
    let total = progress.total.max(1);
    let fraction = (progress.done as f32 / total as f32).clamp(0., 1.);
    line.child(
        div()
            .flex_shrink_0()
            .whitespace_nowrap()
            .child(SharedString::from(format!(
                "running {} tests",
                progress.total
            ))),
    )
    .child(TestBar {
        id: SharedString::from(format!("test-bar-{call}")),
        fraction,
        cell: small.cell(),
    })
    .child(components::tabular(
        div()
            .flex_shrink_0()
            .whitespace_nowrap()
            .child(SharedString::from(format!(
                "{}/{}",
                progress.done, progress.total
            ))),
    ))
}

/// A running test's bar (the prototype's `.bar`): `TEST_BAR_CELLS` wide and
/// `TEST_BAR_H` tall on a `paint::LINE` track, a cell's margin each side,
/// its `RUNNING` fill at sub-pixel precision easing to each new count over
/// `TEST_BAR_EASE_MS`.
#[derive(IntoElement)]
struct TestBar {
    id: SharedString,
    fraction: f32,
    cell: f32,
}

impl gpui::RenderOnce for TestBar {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let spec =
            ferrite_ui::motion::MotionSpec::new(theme::TEST_BAR_EASE_MS, ferrite_ui::motion::EASE);
        let now = cx.background_executor().now();
        let reduced = ferrite_ui::motion::reduced_motion(cx);
        let target = self.fraction;
        let (shown, running) = window.with_global_id(self.id.clone().into(), |id, window| {
            window.with_element_state(id, |state: Option<(f32, ferrite_ui::motion::Tween)>, _| {
                let (at, tween) = match state {
                    Some((at, tween)) if at == target => (at, tween),
                    Some((_, tween)) => (
                        target,
                        ferrite_ui::motion::Tween::retarget(
                            Some(tween),
                            target,
                            target,
                            spec,
                            now,
                            reduced,
                        ),
                    ),
                    None => (
                        target,
                        ferrite_ui::motion::Tween::new(target, target, spec, now),
                    ),
                };
                let value = tween.value(now, reduced);
                ((value, tween.running(now, reduced)), (at, tween))
            })
        });
        if running {
            window.request_animation_frame();
        }
        let width = theme::TEST_BAR_CELLS * self.cell;
        div()
            .debug_selector(|| "test-bar".into())
            .relative()
            .flex_shrink_0()
            .mx(px(self.cell))
            .w(px(width))
            .h(px(theme::TEST_BAR_H))
            .rounded(px(theme::TEST_BAR_R))
            .overflow_hidden()
            .bg(theme::paint::LINE)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(width * shown.clamp(0., 1.)))
                    .bg(rgb(RUNNING)),
            )
    }
}

/// A result's verdict words in their state ink, the rest in the line's
/// `INK_SUBTLE` (CT-15): a line's lead `ok` (`RUNNING`) or `failed`
/// (`BLOCKED`); a test line's trailing `... ok` / `... FAILED`; and the
/// verdict of a `test result:` line. `24 passed`, `357 passed; 2 failed`
/// and every other `passed` or `failed` stay muted.
pub(super) fn result_highlights(text: &str) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    let ink = |range: std::ops::Range<usize>, ink: u32| {
        (
            range,
            HighlightStyle {
                color: Some(rgb(ink).into()),
                ..Default::default()
            },
        )
    };
    let verdict_ink = |word: &str| match word {
        "ok" => Some(RUNNING),
        "FAILED" | "failed" => Some(BLOCKED),
        _ => None,
    };
    let mut highlights = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let indent = body.len() - body.trim_start().len();
        let trimmed = &body[indent..];
        // The lead state word: `ok · 38 passed`, `failed · …`.
        for word in ["ok", "failed"] {
            if trimmed == word
                || trimmed.starts_with(&format!("{word} "))
                || trimmed.starts_with(&format!("{word}\u{b7}"))
            {
                let start = offset + indent;
                highlights.push(ink(
                    start..start + word.len(),
                    verdict_ink(word).unwrap_or(INK_SUBTLE),
                ));
            }
        }
        // A test line's trailing verdict.
        if let Some((_, verdict)) = trimmed.rsplit_once(" ... ") {
            let verdict = verdict.trim_end();
            if let Some(color) = verdict_ink(verdict).filter(|_| verdict != "failed") {
                let start = offset + indent + trimmed.trim_end().len() - verdict.len();
                highlights.push(ink(start..start + verdict.len(), color));
            }
        }
        // `test result: FAILED. 37 passed; …`.
        if let Some(rest) = trimmed.strip_prefix("test result: ") {
            for word in ["ok", "FAILED"] {
                if rest.starts_with(word) {
                    let start = offset + indent + "test result: ".len();
                    highlights.push(ink(
                        start..start + word.len(),
                        verdict_ink(word).unwrap_or(INK_SUBTLE),
                    ));
                }
            }
        }
        offset += line.len();
    }
    highlights.sort_by_key(|(range, _)| range.start);
    highlights.dedup_by(|a, b| a.0 == b.0);
    highlights
}

/// A disclosed block of text in a call's open body — its input, output, a
/// structured result. Text draws inline, keeping its whitespace; text past
/// `OUTPUT_INLINE_BYTES` scrolls in one bounded, selectable native control
/// `OUTPUT_MAX_LINES` high, and `… +N lines` under it says how much is out
/// of view.
pub(crate) fn output_block(block: BlockId, part: &str, text: &str, selection: &TextRuns) -> Div {
    let rows = div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .text_color(rgb(INK_SUBTLE));
    if text::output_scrolls(text) {
        let hidden = text::output_lines(text).saturating_sub(theme::OUTPUT_MAX_LINES);
        // Text still out of view fades into the ground at the box's foot
        // (Codex's shell card), and `… +N lines` says how much.
        let scroller = div()
            .relative()
            .child(selection.output(block, part, text))
            .when(hidden > 0, |scroller| {
                scroller.child(
                    div()
                        .debug_selector(|| "output-fade".into())
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .child(components::edge_fade(rgb(theme::CARD).into(), true)),
                )
            });
        return rows.child(scroller).when(hidden > 0, |rows| {
            rows.child(SharedString::from(format!("\u{2026} +{hidden} lines")))
        });
    }
    rows.child(selection.line(block, text.to_string(), result_highlights(text)))
}

// ------------------------------------------------------------------ diffs

/// One side of a diff row: its line number, its kind, its code and the
/// changed tokens' range in it (a removed line paired with an added one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DiffSide {
    pub kind: DiffKind,
    pub number: u32,
    pub body: String,
    pub words: Option<std::ops::Range<usize>>,
}

/// A unified diff as rows: a hunk's header, or one line numbered by the
/// file it is in (the old file's for a removed line, the new file's else).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DiffRow {
    /// `@@ -208,7 +208,11 @@ fn thread_row`: drawn only where the hunk
    /// names a section.
    Hunk(String),
    Line(DiffSide),
}

/// A line's tokens for word marks (CT-22): identifier runs
/// (`[A-Za-z0-9_]+`, so `0x7fbf95` is one), whitespace runs, and every
/// other character alone.
fn diff_tokens(line: &str) -> Vec<std::ops::Range<usize>> {
    let mut tokens = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        let word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
        let mut end = start + ch.len_utf8();
        if word(ch) {
            while let Some(&(at, next)) = chars.peek() {
                if !word(next) {
                    break;
                }
                end = at + next.len_utf8();
                chars.next();
            }
        } else if ch.is_whitespace() {
            while let Some(&(at, next)) = chars.peek() {
                if !next.is_whitespace() {
                    break;
                }
                end = at + next.len_utf8();
                chars.next();
            }
        }
        tokens.push(start..end);
    }
    tokens
}

/// The changed tokens between a removed line and the added line it became:
/// what is left of each once their common leading and trailing characters
/// go, snapped to whole tokens — the tokens wholly inside what changed, or,
/// where no whole token is (`0x7fbf95` → `0x93cf8c` past their common `0x`;
/// `live` past `if l`), the tokens it touches — and trimmed of whitespace
/// at its edges. `None` when nothing is common (the whole line changed) or
/// nothing differs.
pub(super) fn changed_words(
    old: &str,
    new: &str,
) -> Option<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    // Common leading and trailing characters, in bytes, never splitting a
    // character and never overlapping.
    let prefix = old
        .char_indices()
        .zip(new.chars())
        .take_while(|((_, x), y)| x == y)
        .last()
        .map_or(0, |((at, ch), _)| at + ch.len_utf8());
    let suffix = old[prefix..]
        .chars()
        .rev()
        .zip(new[prefix..].chars().rev())
        .take_while(|(x, y)| x == y)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    if prefix == 0 && suffix == 0 {
        return None;
    }
    let span = |line: &str| -> std::ops::Range<usize> {
        let tokens = diff_tokens(line);
        let (start, end) = (prefix, line.len() - suffix);
        let solid = |token: &&std::ops::Range<usize>| !line[(*token).clone()].trim().is_empty();
        // The tokens wholly inside the change, else those it touches.
        let inside: Vec<&std::ops::Range<usize>> = tokens
            .iter()
            .filter(|token| token.start >= start && token.end <= end)
            .filter(solid)
            .collect();
        let picked = if inside.is_empty() {
            tokens
                .iter()
                .filter(|token| token.start < end && token.end > start)
                .filter(solid)
                .collect()
        } else {
            inside
        };
        match (picked.first(), picked.last()) {
            (Some(first), Some(last)) => first.start..last.end,
            _ => start.min(line.len())..start.min(line.len()),
        }
    };
    let (old_range, new_range) = (span(old), span(new));
    if old_range.is_empty() && new_range.is_empty() {
        return None;
    }
    Some((old_range, new_range))
}

/// How alike two changed lines are for pairing them (their word marks): the
/// tokens their ends share (common leading plus trailing tokens, whitespace
/// aside) over the longer line's tokens. Lines that begin or end alike —
/// `if let Some(…) {` and `if live {` — pair; lines that share only words in
/// the middle do not.
fn line_affinity(old: &str, new: &str) -> f32 {
    let words = |line: &str| -> Vec<String> {
        diff_tokens(line)
            .into_iter()
            .map(|range| line[range].to_string())
            .filter(|token| !token.trim().is_empty())
            .collect()
    };
    let (a, b) = (words(old), words(new));
    if a.is_empty() || b.is_empty() {
        return 0.;
    }
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    (prefix + suffix) as f32 / a.len().max(b.len()) as f32
}

/// Which removed and added lines of one change pair up for their word
/// marks: the alignment that keeps both sides in order and
/// maximizes the pairs' affinity (`line_affinity`), unpaired lines on rows
/// of their own. `(Some(i), Some(j))` pairs; `(Some(i), None)` and `(None,
/// Some(j))` stand alone.
fn pair_lines(removed: &[&str], added: &[&str]) -> Vec<(Option<usize>, Option<usize>)> {
    let (n, m) = (removed.len(), added.len());
    let affinity: Vec<Vec<f32>> = removed
        .iter()
        .map(|old| added.iter().map(|new| line_affinity(old, new)).collect())
        .collect();
    // best[i][j]: the most affinity pairing removed[i..] with added[j..].
    let mut best = vec![vec![0f32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let pair = if affinity[i][j] > 0. {
                affinity[i][j] + best[i + 1][j + 1]
            } else {
                f32::MIN
            };
            best[i][j] = pair.max(best[i + 1][j]).max(best[i][j + 1]);
        }
    }
    let mut rows = Vec::with_capacity(n.max(m));
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        let pair =
            affinity[i][j] > 0. && (affinity[i][j] + best[i + 1][j + 1] - best[i][j]).abs() < 1e-6;
        if pair {
            rows.push((Some(i), Some(j)));
            i += 1;
            j += 1;
        } else if (best[i][j + 1] - best[i][j]).abs() < 1e-6 && best[i][j + 1] >= best[i + 1][j] {
            rows.push((None, Some(j)));
            j += 1;
        } else {
            rows.push((Some(i), None));
            i += 1;
        }
    }
    rows.extend((i..n).map(|i| (Some(i), None)));
    rows.extend((j..m).map(|j| (None, Some(j))));
    rows
}

thread_local! {
    /// Sections found by reading a post-edit file, once per file and hunk.
    static FILE_SECTIONS: std::cell::RefCell<HashMap<(String, u32), Option<String>>> =
        std::cell::RefCell::new(HashMap::new());
}

/// A hunk's section where its own context named none (a `structuredPatch`
/// hunk deep in an item): git's default funcname rule over the post-edit
/// file — the nearest line above the hunk that starts with a letter, `_` or
/// `$` — read once per file and hunk. `None` for a path that is not an
/// absolute file on disk.
fn file_section(path: &str, hunk: &ferrite_core::Hunk) -> Option<String> {
    if !std::path::Path::new(path).is_absolute() || hunk.new_start <= 1 {
        return None;
    }
    FILE_SECTIONS.with(|cache| {
        cache
            .borrow_mut()
            .entry((path.to_string(), hunk.new_start))
            .or_insert_with(|| {
                let text = std::fs::read_to_string(path).ok()?;
                text.lines()
                    .take(hunk.new_start.saturating_sub(1) as usize)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .find(|line| ferrite_core::transcript::funcname_line(line))
                    .and_then(ferrite_core::transcript::section_head)
            })
            .clone()
    })
}

/// A diff's rows, at most `cap` code lines (`hunk_rows`): a header only
/// where the hunk names a section; each change's removed lines, then its
/// added lines, paired by likeness (`pair_lines`) and their changed tokens
/// marked.
pub(super) fn diff_rows(diff: &Diff, cap: usize) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    let mut drawn = 0usize;
    for hunk in &diff.hunks {
        if drawn == cap {
            break;
        }
        if let Some(section) = hunk
            .section
            .clone()
            .or_else(|| file_section(&diff.path, hunk))
        {
            rows.push(DiffRow::Hunk(format!(
                "@@ -{},{} +{},{} @@ {section}",
                hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
            )));
        }
        let (mut old, mut new) = (hunk.old_start, hunk.new_start);
        let lines: Vec<&str> = hunk
            .lines
            .iter()
            .take(cap - drawn)
            .map(String::as_str)
            .collect();
        drawn += lines.len();
        let mut at = 0;
        while at < lines.len() {
            match DiffKind::of(lines[at]) {
                DiffKind::Context => {
                    rows.push(DiffRow::Line(DiffSide {
                        kind: DiffKind::Context,
                        number: new,
                        body: text::diff_body(lines[at]).to_owned(),
                        words: None,
                    }));
                    old += 1;
                    new += 1;
                    at += 1;
                }
                _ => {
                    // A change: its removed run, then its added run, paired
                    // by likeness for the word marks.
                    let removed: Vec<&str> = lines[at..]
                        .iter()
                        .take_while(|line| DiffKind::of(line) == DiffKind::Removed)
                        .map(|line| text::diff_body(line))
                        .collect();
                    at += removed.len();
                    let added: Vec<&str> = lines[at..]
                        .iter()
                        .take_while(|line| DiffKind::of(line) == DiffKind::Added)
                        .map(|line| text::diff_body(line))
                        .collect();
                    at += added.len();
                    let mut left: Vec<DiffSide> = removed
                        .iter()
                        .enumerate()
                        .map(|(index, body)| DiffSide {
                            kind: DiffKind::Removed,
                            number: old + index as u32,
                            body: (*body).to_owned(),
                            words: None,
                        })
                        .collect();
                    let mut right: Vec<DiffSide> = added
                        .iter()
                        .enumerate()
                        .map(|(index, body)| DiffSide {
                            kind: DiffKind::Added,
                            number: new + index as u32,
                            body: (*body).to_owned(),
                            words: None,
                        })
                        .collect();
                    for pair in pair_lines(&removed, &added) {
                        if let (Some(i), Some(j)) = pair {
                            if let Some((was, became)) = changed_words(removed[i], added[j]) {
                                left[i].words = Some(was);
                                right[j].words = Some(became);
                            }
                        }
                    }
                    old += removed.len() as u32;
                    new += added.len() as u32;
                    rows.extend(left.into_iter().chain(right).map(DiffRow::Line));
                }
            }
        }
    }
    rows
}

/// One styled run of a line.
type Highlight = (std::ops::Range<usize>, HighlightStyle);
/// A changed-token span and the deeper hue it is washed in.
type WordWash = (std::ops::Range<usize>, gpui::Hsla);

/// A diff line's syntax colours, and its changed tokens' wash: the range
/// and the deeper hue `CellCut` paints under them as the prototype's inline
/// `.wr` / `.wa` span boxes (the text's own content box, not the row).
fn diff_highlights(side: &DiffSide, language: Option<&str>) -> (Vec<Highlight>, Option<WordWash>) {
    let syntax = language
        .map(|language| {
            let tokens = ferrite_core::transcript::highlight_tokens(Some(language), &side.body);
            code(&side.body, Some(&tokens))
        })
        .unwrap_or_default();
    let wash = match side.kind {
        DiffKind::Added => Some((theme::DIFF_ADDED_WORD, theme::DIFF_ADDED_WORD_ALPHA)),
        DiffKind::Removed => Some((theme::DIFF_REMOVED_WORD, theme::DIFF_REMOVED_WORD_ALPHA)),
        DiffKind::Context => None,
    };
    let words = side
        .words
        .clone()
        .filter(|words| !words.is_empty())
        .zip(wash)
        .map(|(words, (wash, alpha))| {
            let wash: gpui::Hsla = rgb(wash >> 8).into();
            (words, wash.opacity(alpha))
        });
    let syntax = match &words {
        // The changed tokens are a span of their own: the runs break at
        // its edges, as the prototype's `.wa` / `.wr` breaks its line.
        Some((words, _)) => syntax
            .into_iter()
            .flat_map(|(range, style)| {
                let mut cuts = vec![range.start];
                cuts.extend(
                    [words.start, words.end]
                        .into_iter()
                        .filter(|cut| range.start < *cut && *cut < range.end),
                );
                cuts.push(range.end);
                cuts.windows(2)
                    .map(|cut| (cut[0]..cut[1], style))
                    .collect::<Vec<_>>()
            })
            .collect(),
        None => syntax,
    };
    (syntax, words)
}

/// A diff under its call (SPEC §5.4; the mockup's `.diff`): one hairline
/// well (`diff_well`) on the content column — a header naming the file in the
/// app's words with its size (`+4 −1`) hard right over a 1px `paint::CARD_RING`,
/// then the unified diff set `TX_BLOCK_STEP` under the transcript. A
/// `INK_SUBTLE` hunk header opens a hunk that names a section (none
/// otherwise); each row is `[number][sign][code]`: the number
/// right-aligned in `DIFF_NUMBER_CELLS` with `DIFF_NUMBER_PAD_CELLS` after
/// it, in `TEXT_FAINT` or the add/delete number tint; the sign
/// (`DIFF_SIGN_CELLS`) in its hue; the code in full syntax colour with its
/// indentation intact, exactly one line tall, cut with `…` at the box's
/// edge. Removed and added rows wear their soft washes the box's whole
/// width, the changed tokens a deeper one.
///
/// The code cells route through the overlay — their lines copy honestly and
/// whole, however they are cut; numbers, signs and headers are chrome and
/// never do (#27). Without a selection (`None`) nothing registers.
///
/// The diff draws at most `HUNK_MAX_ROWS` lines and then names what it
/// left out.
pub(super) fn render_diff(
    selection: Option<(&TextRuns, BlockId)>,
    diff: &Diff,
    grid: Grid,
) -> impl IntoElement {
    let content = grid;
    let grid = grid.block();
    let pad = theme::CODE_PAD_CELLS * grid.cell();
    let total: usize = diff.hunks.iter().map(|hunk| hunk.lines.len()).sum();
    let (cap, omitted) = hunk_rows(total);
    let cells = diff_number_cells(diff_max_number(diff, cap));
    let number_w = cells * grid.cell();
    let sign_w = theme::DIFF_SIGN_CELLS * grid.cell();
    let language = ferrite_core::transcript::language_for_path(std::path::Path::new(&diff.path));
    let number = |number: u32, ink: u32| {
        components::tabular(
            div()
                .flex_shrink_0()
                .w(px(number_w))
                .pr(px(theme::DIFF_NUMBER_PAD_CELLS * grid.cell()))
                .text_right()
                .whitespace_nowrap()
                .text_color(rgb(ink))
                .child(SharedString::from(number.to_string())),
        )
    };
    // A line under the code column's edge: a hunk header, the omission.
    let note = |selector: &'static str, text: String| {
        div()
            .debug_selector(move || selector.into())
            .w_full()
            .min_w_0()
            .h(px(grid.line))
            .pl(px(pad + number_w + sign_w))
            .pr(px(pad))
            .truncate()
            .text_color(rgb(INK_SUBTLE))
            .child(SharedString::from(text))
    };
    let mut lines = div()
        .debug_selector(|| "tool-diff".into())
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .py(px(theme::BLOCK_PAD_Y))
        .text_color(rgb(INK_STRONG));
    for row in diff_rows(diff, cap) {
        lines = lines.child(match &row {
            DiffRow::Hunk(header) => note("diff-hunk", header.clone()),
            DiffRow::Line(side) => {
                let DiffPaint {
                    sign,
                    sign_color,
                    number_color,
                    code_color,
                    wash,
                } = side.kind.paint();
                let (highlights, words) = diff_highlights(side, language);
                let runs = highlights.iter().map(|(range, _)| range.clone()).collect();
                let cut = CellCut::new(
                    side.body.clone(),
                    words,
                    rgb(code_color).into(),
                    grid.cell(),
                )
                .runs(runs);
                let code = match selection {
                    Some((selection, block)) => selection
                        .line(block, side.body.clone(), highlights)
                        .into_any_element(),
                    None => StyledText::new(SharedString::from(side.body.clone()))
                        .with_highlights(highlights)
                        .into_any_element(),
                };
                div()
                    .flex()
                    .items_start()
                    .w_full()
                    .min_w_0()
                    .h(px(grid.line))
                    .px(px(pad))
                    .overflow_hidden()
                    .when_some(wash, |row, wash| row.bg(rgba(wash)))
                    .child(number(side.number, number_color))
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(sign_w))
                            .whitespace_nowrap()
                            .text_color(rgb(sign_color))
                            .child(sign),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_color(rgb(code_color))
                            .child(cut.child(code)),
                    )
            }
        });
    }
    // What the cap left out, on the code column — never a silent truncation.
    if omitted > 0 {
        lines = lines.child(note("diff-omitted", format!("\u{2026} +{omitted} lines")));
    }
    let name = std::path::Path::new(&diff.path)
        .file_name()
        .map_or(diff.path.clone(), |name| {
            name.to_string_lossy().into_owned()
        });
    let small = content.activity();
    let header = div()
        .debug_selector(|| "diff-header".into())
        .flex()
        .items_center()
        .w_full()
        .min_w_0()
        .px(px(pad))
        .py(px(theme::BLOCK_PAD_Y))
        .border_b_1()
        .border_color(theme::paint::CARD_RING)
        .font_family(theme::FONT_UI)
        .text_size(px(small.size))
        .text_color(rgb(INK_STRONG))
        .child(div().min_w_0().truncate().child(SharedString::from(name)))
        .child(div().flex_1())
        .child(
            div()
                .flex_shrink_0()
                .pl(px(theme::SPACE_3))
                .font_family(theme::FONT_CODE)
                .child(trail_diff_stat(diff.added, diff.removed)),
        );
    content_row(content).pt(px(theme::BLOCK_MARGIN_T)).child(
        on_cells(diff_well(grid))
            .debug_selector(|| "diff-box".into())
            .py(px(0.))
            .overflow_hidden()
            .child(header)
            .child(lines),
    )
}

/// A one-line code run cut at its column's edge as the prototype's
/// `text-overflow: ellipsis` cuts it: the whole cells that leave room for
/// `…`, then `…` in the cell's own ink (the browser draws the ellipsis in
/// the block's colour, whatever token it hides). gpui's own truncation
/// trims trailing punctuation before its `…` (`clone…` where the browser
/// draws `clone(…`), so a diff row draws its text whole and this clips it;
/// the text itself — what a selection copies — is never cut.
///
/// It also paints the changed tokens' wash (`words`) the way the browser
/// paints an inline span's background: over the face's content box —
/// ascent plus descent, each rounded, a whole-pixel half-leading above —
/// not the row (17px inside a 20px row at 13px), so washes on adjacent
/// rows stay apart. A wash that runs past the cut keeps washing, unbroken
/// under the `…`, to the column's edge.
pub(crate) struct CellCut {
    text: String,
    words: Option<WordWash>,
    ink: gpui::Hsla,
    cell: f32,
    /// Where the run's styled runs break, in cells: each run is laid out
    /// from the last one's width rounded up to the layout unit (gpui's
    /// text paint, vendor/gpui-pre), so a cell's x counts them.
    breaks: Vec<usize>,
    child: Option<AnyElement>,
}

impl CellCut {
    fn new(text: String, words: Option<WordWash>, ink: gpui::Hsla, cell: f32) -> Self {
        Self {
            text,
            words,
            ink,
            cell,
            breaks: Vec::new(),
            child: None,
        }
    }

    /// The styled runs the cell's text is painted in (byte ranges).
    fn runs(mut self, runs: Vec<std::ops::Range<usize>>) -> Self {
        let cell_of = |at: usize| self.text[..at.min(self.text.len())].chars().count();
        let total = self.text.chars().count();
        let mut breaks: Vec<usize> = runs
            .iter()
            .flat_map(|run| [cell_of(run.start), cell_of(run.end)])
            .filter(|at| *at > 0 && *at < total)
            .collect();
        breaks.sort_unstable();
        breaks.dedup();
        self.breaks = breaks;
        self
    }

    /// The x of cell `at` from the run's start: its cells, each run before
    /// it rounded up to the layout unit.
    fn x_at(&self, at: usize) -> f32 {
        let unit = |width: f32| (width * 64.0).ceil() / 64.0;
        let mut x = 0.0;
        let mut from = 0;
        for &cut in self.breaks.iter().filter(|cut| **cut <= at) {
            x += unit((cut - from) as f32 * self.cell);
            from = cut;
        }
        x + (at - from) as f32 * self.cell
    }

    pub(crate) fn child(mut self, child: AnyElement) -> Self {
        self.child = Some(child);
        self
    }

    /// How many whole cells stay when `width` cannot hold the run: the
    /// cells that leave one for `…`. `None` when the whole run fits.
    fn kept(&self, width: f32) -> Option<usize> {
        let whole = self.text.chars().count() as f32 * self.cell;
        (whole > width + 0.01)
            .then(|| ((width - self.cell) / self.cell + 0.001).floor().max(0.) as usize)
    }

    /// The cells the wash covers, `[from, to)`, and whether it reaches the
    /// first hidden cell (then it runs on under the `…` to the edge).
    fn wash_cells(&self, kept: Option<usize>) -> Option<(usize, usize, bool)> {
        let (range, _) = self.words.as_ref()?;
        let cell_of = |at: usize| self.text[..at.min(self.text.len())].chars().count();
        let (from, to) = (cell_of(range.start), cell_of(range.end));
        match kept {
            Some(kept) if from > kept => None,
            Some(kept) => Some((from, to.min(kept), to > kept)),
            None => Some((from, to, false)),
        }
    }
}

/// The browser's inline content box in a line of `line_height`: its top
/// below the line's and its height — the face's ascent plus descent,
/// rounded to the pixel (17px at 13px), under a floored half-leading.
fn inline_content_box(window: &gpui::Window) -> (f32, f32) {
    let style = window.text_style();
    let size = style.font_size.to_pixels(window.rem_size());
    let line_height = f32::from(style.line_height_in_pixels(window.rem_size()));
    let text_system = window.text_system();
    let font = text_system.resolve_font(&style.font());
    let ascent = f32::from(text_system.ascent(font, size)).abs();
    let descent = f32::from(text_system.descent(font, size)).abs();
    let height = (ascent + descent).round().min(line_height);
    (((line_height - height) / 2.).floor().max(0.), height)
}

impl IntoElement for CellCut {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl gpui::Element for CellCut {
    /// The child's layout node: the cut and the wash measure from its
    /// unrounded box, where the run's glyphs stand.
    type RequestLayoutState = gpui::LayoutId;
    type PrepaintState = gpui::Bounds<gpui::Pixels>;

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) -> (gpui::LayoutId, gpui::LayoutId) {
        let child = self.child.get_or_insert_with(|| div().into_any_element());
        let layout_id = child.request_layout(window, cx);
        (layout_id, layout_id)
    }

    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<gpui::Pixels>,
        layout_id: &mut gpui::LayoutId,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) -> gpui::Bounds<gpui::Pixels> {
        let bounds = window.unsnapped_layout_bounds(*layout_id);
        if let Some(child) = self.child.as_mut() {
            child.prepaint(window, cx);
        }
        bounds
    }

    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<gpui::Pixels>,
        _: &mut gpui::LayoutId,
        bounds: &mut gpui::Bounds<gpui::Pixels>,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        let bounds = *bounds;
        let Some(mut child) = self.child.take() else {
            return;
        };
        // The run keeps its glyphs where they fall; the wash is a box, its
        // edges on the whole pixel the browser rounds a span's background
        // to (`components::css_px`).
        let kept = self.kept(f32::from(bounds.size.width));
        let left = f32::from(bounds.left());
        let cut = kept.map(|kept| px(left + self.x_at(kept)));
        if let (Some((from, to, runs_on)), Some((_, wash))) =
            (self.wash_cells(kept), self.words.as_ref())
        {
            let (top, height) = inline_content_box(window);
            let edge = |x: f32| px(ferrite_ui::components::css_px(x));
            let x0 = edge(left + self.x_at(from));
            let x1 = match (runs_on, cut) {
                (true, _) => edge(f32::from(bounds.right())),
                (false, Some(cut)) => edge(left + self.x_at(to)).min(edge(f32::from(cut))),
                (false, None) => edge(left + self.x_at(to)),
            };
            if x1 > x0 {
                let y0 = bounds.top() + px(top);
                window.paint_quad(gpui::fill(
                    gpui::Bounds::from_corners(
                        gpui::point(x0, y0),
                        gpui::point(x1, y0 + px(height)),
                    ),
                    *wash,
                ));
            }
        }
        let Some(cut) = cut else {
            child.paint(window, cx);
            return;
        };
        let shown = gpui::Bounds::from_corners(bounds.origin, gpui::point(cut, bounds.bottom()));
        window.with_content_mask(Some(gpui::ContentMask { bounds: shown }), |window| {
            child.paint(window, cx)
        });
        let style = window.text_style();
        let size = style.font_size.to_pixels(window.rem_size());
        let line_height = style.line_height_in_pixels(window.rem_size());
        let ellipsis = "\u{2026}";
        let run = gpui::TextRun {
            len: ellipsis.len(),
            font: style.font(),
            color: self.ink,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window
            .text_system()
            .shape_line(ellipsis.into(), size, &[run], None);
        let _ = line.paint(
            gpui::point(cut, bounds.top()),
            line_height,
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}

/// How many of a diff's rows the card draws, and how many are left for
/// the omission line to account for. Split out so the arithmetic the card
/// depends on is assertable without a window.
pub(super) fn hunk_rows(total: usize) -> (usize, usize) {
    let drawn = total.min(theme::HUNK_MAX_ROWS);
    (drawn, total - drawn)
}

/// The largest line number the diff will draw.
fn diff_max_number(diff: &Diff, cap: usize) -> usize {
    let mut drawn = 0;
    let mut max = 0usize;
    for hunk in &diff.hunks {
        let (mut old, mut new) = (hunk.old_start, hunk.new_start);
        for line in &hunk.lines {
            if drawn == cap {
                return max;
            }
            drawn += 1;
            match DiffKind::of(line) {
                DiffKind::Added => new += 1,
                DiffKind::Removed => old += 1,
                DiffKind::Context => {
                    old += 1;
                    new += 1;
                }
            }
            max = max
                .max(old.saturating_sub(1) as usize)
                .max(new.saturating_sub(1) as usize);
        }
    }
    max
}

/// The number column in cells: `DIFF_NUMBER_CELLS` (three digits and the
/// `DIFF_NUMBER_PAD_CELLS` after them), wider only for a longer number.
pub fn diff_number_cells(max: usize) -> f32 {
    let digits = max.max(1).ilog10() as f32 + 1.;
    theme::DIFF_NUMBER_CELLS.max(digits + theme::DIFF_NUMBER_PAD_CELLS)
}

/// What a unified-diff line is, read from its first byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffKind {
    Added,
    Removed,
    Context,
}

/// A diff row's colours: the sign column, the code, and the row's wash.
#[derive(Debug, PartialEq, Eq)]
pub struct DiffPaint {
    pub sign: &'static str,
    pub sign_color: u32,
    /// The line number's ink: the sign's hue dimmed, structure on a
    /// context row.
    pub number_color: u32,
    pub code_color: u32,
    pub wash: Option<u32>,
}

impl DiffKind {
    pub fn of(line: &str) -> Self {
        match line.chars().next() {
            Some('+') => Self::Added,
            Some('-') => Self::Removed,
            _ => Self::Context,
        }
    }

    /// Both CLIs sign a diff in ASCII, `+` and `-`. The sign carries the
    /// hue (`DIFF_*_SIGN`); the code keeps body ink under its syntax
    /// colours on the row's wash; a context line is the same code with no
    /// wash.
    pub fn paint(self) -> DiffPaint {
        match self {
            Self::Added => DiffPaint {
                sign: "+",
                sign_color: theme::DIFF_ADDED_SIGN,
                number_color: theme::DIFF_ADDED_NUMBER,
                code_color: SYN_PLAIN,
                wash: Some(DIFF_ADDED_WASH),
            },
            Self::Removed => DiffPaint {
                sign: "-",
                sign_color: theme::DIFF_REMOVED_SIGN,
                number_color: theme::DIFF_REMOVED_NUMBER,
                code_color: SYN_PLAIN,
                wash: Some(DIFF_REMOVED_WASH),
            },
            Self::Context => DiffPaint {
                sign: "",
                sign_color: INK_SUBTLE,
                number_color: TEXT_FAINT,
                code_color: theme::SYN_PLAIN,
                wash: None,
            },
        }
    }
}

/// A review's findings (`ReportFindings`, HD70), grouped by file in the
/// order first reported: the file, then each finding's line, its summary,
/// its verdict when verified, and its failure scenario beneath in muted ink.
fn findings_body(
    block: BlockId,
    findings: &[ferrite_core::transcript::tools::Finding],
    grid: Grid,
    selection: &TextRuns,
) -> Div {
    let mut files: Vec<(&str, Vec<&ferrite_core::transcript::tools::Finding>)> = Vec::new();
    for finding in findings {
        match files.iter_mut().find(|(file, _)| *file == finding.file) {
            Some((_, group)) => group.push(finding),
            None => files.push((&finding.file, vec![finding])),
        }
    }
    let cells = grid.block();
    div()
        .debug_selector(|| "findings".into())
        .flex()
        .flex_col()
        .gap(px(theme::SPACE_3))
        .w_full()
        .min_w_0()
        .font_family(theme::FONT_UI)
        .text_size(px(cells.size))
        .line_height(px(cells.line))
        .children(files.into_iter().map(|(file, group)| {
            div()
                .flex()
                .flex_col()
                .gap(px(theme::SPACE_1))
                .child(
                    div()
                        .text_color(rgb(INK_STRONG))
                        .child(selection.line(block, file.to_string(), Vec::new())),
                )
                .children(group.into_iter().map(|finding| {
                    let place = finding.line.map(|line| format!("line {line}")).unwrap_or_default();
                    let verdict = finding.verdict.as_deref().map(str::to_lowercase);
                    let head = [Some(place), Some(finding.summary.clone()), verdict]
                        .into_iter()
                        .flatten()
                        .filter(|part| !part.is_empty())
                        .collect::<Vec<_>>()
                        .join(" \u{b7} ");
                    div()
                        .debug_selector(|| "finding".into())
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_color(rgb(INK_STRONG))
                                .child(selection.line(block, head, Vec::new())),
                        )
                        .when(!finding.failure_scenario.is_empty(), |row| {
                            row.child(
                                div().text_color(rgb(INK_SUBTLE)).child(
                                    selection.line(block, finding.failure_scenario.clone(), Vec::new()),
                                ),
                            )
                        })
                }))
        }))
}
