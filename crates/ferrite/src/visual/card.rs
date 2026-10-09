//! A Visual row's look, in the transcript's terminal grammar (`theme.rs`
//! WP-A, `pane::render_tool`): a call line, what hangs under it on the `└`
//! elbow, then the page itself on the content column.
//!
//! ```text
//! ●  Visual  Token usage, last 30 days                      drawing
//!    └ Daily cost with a 7-day average, by model
//!    ┌──────────────────────────────────────────────────────────┐
//!    │  the live page                                           │
//!    └──────────────────────────────────────────────────────────┘
//!    open in browser · copy html
//! ```
//!
//! The gutter mark says where the visual is: the braille spinner while the
//! agent draws it and Ferrite checks it, a muted `●` once shown, a red one if
//! it failed (with `└ failed · why`). The page sits in a 1px `LINE` frame, the
//! only edge a terminal row gets (theme rule 2: square, flat, no card). The
//! title and caption are text, selectable and copied like any row's
//! ([`collect_text`] registers exactly what [`render`] draws).

use std::rc::Rc;

use ferrite_core::transcript::BlockId;
use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, App, Div, Entity, SharedString, Window};

use super::element::{WebView, PLACEHOLDER_HEIGHT};
use super::{Visual, VisualStatus};
use crate::components;
use crate::pane::{self, Grid};
use crate::select::TextRuns;
use crate::theme::{self, BLOCKED, RUNNING, TEXT, TEXT_FAINT, TEXT_MUTED, TEXT_STRONG};

/// The call line's name for every visual.
pub(crate) const NAME: &str = "Visual";
/// The agent is still writing it.
pub(crate) const DRAWING: &str = "drawing";
/// Written; Ferrite is rendering the screenshot the agent checks.
pub(crate) const CHECKING: &str = "checking";
/// A visual a later one replaced.
pub(crate) const REPLACED: &str = "replaced by the next version below";
/// Narrower than this (logical px of the content column), a page is laid out
/// too cramped to read: the row shows its words and the fallbacks instead.
pub(crate) const MIN_LIVE_WIDTH: f32 = 320.0;

/// What the row shows under its words.
pub(crate) enum Stage {
    /// The page, live (or its last frame, when far off-screen).
    Live(Entity<WebView>),
    /// Chromium is starting, or the agent hasn't written any HTML yet: a
    /// quiet box at the page's usual height, so nothing jumps when it lands.
    Waiting,
    /// The Pane is too narrow for the page.
    Narrow,
    /// No engine here: why.
    Unavailable(SharedString),
    /// A later visual replaced this one (the agent fixed it): folded away.
    Replaced,
    /// Nothing under the words (a failed call: its elbow says why).
    Hidden,
}

/// The row's two actions.
pub(crate) struct Actions {
    pub open_in_browser: Rc<dyn Fn(&mut Window, &mut App)>,
    pub copy_html: Rc<dyn Fn(&mut Window, &mut App)>,
}

/// The status word on the call line's right, if the status has one.
fn status_word(status: &VisualStatus) -> Option<&'static str> {
    match status {
        VisualStatus::Drawing => Some(DRAWING),
        VisualStatus::Checking => Some(CHECKING),
        VisualStatus::Shown => None,
        // The elbow under it says `failed · why`: one place per fact.
        VisualStatus::Failed(_) => None,
    }
}

/// The call line's text: `Visual  <title>`.
fn call_text(visual: &Visual) -> String {
    let title = visual.title.trim();
    if title.is_empty() {
        NAME.to_string()
    } else {
        format!("{NAME}  {title}")
    }
}

/// The caption, when it says something.
fn caption(visual: &Visual) -> Option<String> {
    visual
        .caption
        .as_deref()
        .map(str::trim)
        .filter(|caption| !caption.is_empty())
        .map(str::to_owned)
}

/// The failure's words (`failed · why`), when it failed.
fn failure(visual: &Visual) -> Option<String> {
    match &visual.status {
        VisualStatus::Failed(why) if !why.trim().is_empty() => {
            Some(format!("{} \u{b7} {}", theme::words::FAILED, why.trim()))
        }
        VisualStatus::Failed(_) => Some(theme::words::FAILED.to_string()),
        _ => None,
    }
}

/// Register the row's selectable text, in the order [`render`] draws it:
/// the call line, the caption, the failure.
pub(crate) fn collect_text(block: BlockId, visual: &Visual, selection: &TextRuns) {
    let _ = selection.line(block, call_text(visual), Vec::new());
    if let Some(caption) = caption(visual) {
        let _ = selection.line(block, caption, Vec::new());
    }
    if let Some(failure) = failure(visual) {
        let _ = selection.line(block, failure, Vec::new());
    }
}

/// The row: see the module docs.
pub(crate) fn render(
    block: BlockId,
    visual: &Visual,
    stage: Stage,
    actions: Actions,
    grid: Grid,
    selection: &TextRuns,
) -> AnyElement {
    let gutter = match &visual.status {
        VisualStatus::Drawing | VisualStatus::Checking => div()
            .flex_shrink_0()
            .w(px(grid.gutter()))
            .h(px(grid.line))
            .flex()
            .items_center()
            .child(components::braille_spinner(RUNNING))
            .debug_selector(|| "visual-spinner".into()),
        VisualStatus::Shown => pane::glyph_gutter(grid, pane::BULLET, TEXT_MUTED),
        VisualStatus::Failed(_) => pane::glyph_gutter(grid, pane::BULLET, BLOCKED),
    };
    let name_len = NAME.len();
    let call = div()
        .min_w_0()
        .flex_1()
        .truncate()
        .text_color(rgb(TEXT))
        .child(selection.line(
            block,
            call_text(visual),
            vec![(
                0..name_len,
                gpui::HighlightStyle {
                    color: Some(rgb(TEXT_STRONG).into()),
                    font_weight: Some(theme::W_LABEL),
                    ..Default::default()
                },
            )],
        ));
    let trail = status_word(&visual.status).map(|word| {
        div()
            .flex_shrink_0()
            .pl(px(2.0 * grid.cell()))
            .whitespace_nowrap()
            .text_color(rgb(theme::word_ink(word)))
            .debug_selector(|| "visual-status".into())
            .child(word)
    });
    let line = div()
        .flex()
        .items_start()
        .w_full()
        .min_w_0()
        .child(gutter)
        .child(call)
        .children(trail);

    let mut row = div()
        .id(SharedString::from(format!("visual-row-{}", visual.id)))
        .debug_selector(|| "visual-row".into())
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .child(line);
    if let Some(caption) = caption(visual) {
        row = row.child(
            pane::elbow_line(grid, TEXT_MUTED).child(
                div()
                    .flex_1()
                    .min_w_0()
                    .debug_selector(|| "visual-caption".into())
                    .child(selection.line(block, caption, Vec::new())),
            ),
        );
    }
    if let Some(failure) = failure(visual) {
        let lead = theme::words::FAILED.len();
        row = row.child(
            pane::elbow_line(grid, TEXT_MUTED).child(
                div().flex_1().min_w_0().child(selection.line(
                    block,
                    failure,
                    vec![(
                        0..lead,
                        gpui::HighlightStyle {
                            color: Some(rgb(BLOCKED).into()),
                            ..Default::default()
                        },
                    )],
                )),
            ),
        );
    }
    let replaced = matches!(stage, Stage::Replaced);
    let body = match stage {
        Stage::Live(web) => Some(frame(grid).child(web).into_any_element()),
        Stage::Waiting => Some(
            frame(grid)
                .child(div().w_full().h(px(PLACEHOLDER_HEIGHT)))
                .into_any_element(),
        ),
        Stage::Narrow => Some(note(grid, "widen the pane to see it")),
        Stage::Unavailable(why) => Some(note(grid, why)),
        Stage::Replaced => Some(note(grid, REPLACED)),
        Stage::Hidden => None,
    };
    row = row.children(body);
    if !replaced {
        row = row.child(action_line(&visual.id, actions, grid));
    }
    row.into_any_element()
}

/// The page's frame: on the content column, a half line under the words,
/// one `LINE` around it.
fn frame(grid: Grid) -> Div {
    div()
        .debug_selector(|| "visual-frame".into())
        .ml(px(grid.gutter()))
        .mt(px(grid.half()))
        .flex()
        .flex_col()
        .min_w_0()
        .border_1()
        .border_color(theme::paint::LINE)
        .overflow_hidden()
}

/// A line where the page would be, saying why it isn't (muted, on the
/// content column).
fn note(grid: Grid, words: impl Into<SharedString>) -> AnyElement {
    pane::elbow_line(grid, TEXT_MUTED)
        .debug_selector(|| "visual-note".into())
        .child(div().flex_1().min_w_0().child(words.into()))
        .into_any_element()
}

/// `open in browser · copy html`, muted, each brightening under the pointer.
fn action_line(id: &str, actions: Actions, grid: Grid) -> Div {
    let action = |key: &str, label: &'static str, run: Rc<dyn Fn(&mut Window, &mut App)>| {
        let id = SharedString::from(format!("visual-{key}-{id}"));
        div()
            .id(id.clone())
            .group(id.clone())
            .debug_selector(move || format!("visual-{key_owned}", key_owned = label.replace(' ', "-")))
            .flex_shrink_0()
            .whitespace_nowrap()
            .cursor_pointer()
            .text_color(rgb(TEXT_MUTED))
            .group_hover(id, |style| style.text_color(rgb(TEXT)))
            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                run(window, cx);
            })
            .child(label)
    };
    div()
        .flex()
        .items_center()
        .w_full()
        .min_w_0()
        .pl(px(grid.gutter()))
        .pt(px(grid.half()))
        .gap(px(grid.cell()))
        .child(action("open", "open in browser", actions.open_in_browser))
        .child(div().text_color(rgb(TEXT_FAINT)).child("\u{b7}"))
        .child(action("copy", "copy html", actions.copy_html))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visual(status: VisualStatus) -> Visual {
        Visual {
            id: "toolu_1".into(),
            title: "Token usage".into(),
            caption: Some("Daily cost, last 30 days".into()),
            html: "<p>hi</p>".into(),
            status,
            replace: false,
            replaced: false,
        }
    }

    #[test]
    fn the_status_word_says_only_what_is_in_progress_or_wrong() {
        assert_eq!(status_word(&VisualStatus::Drawing), Some("drawing"));
        assert_eq!(status_word(&VisualStatus::Checking), Some("checking"));
        assert_eq!(status_word(&VisualStatus::Shown), None);
        assert_eq!(status_word(&VisualStatus::Failed("x".into())), None, "its elbow says it");
    }

    #[test]
    fn the_words_are_the_call_line_caption_and_failure() {
        let v = visual(VisualStatus::Failed("render timed out".into()));
        assert_eq!(call_text(&v), "Visual  Token usage");
        assert_eq!(caption(&v).as_deref(), Some("Daily cost, last 30 days"));
        assert_eq!(failure(&v).as_deref(), Some("failed \u{b7} render timed out"));
        let untitled = Visual { title: "  ".into(), caption: Some(" ".into()), ..visual(VisualStatus::Shown) };
        assert_eq!(call_text(&untitled), "Visual");
        assert_eq!(caption(&untitled), None);
        assert_eq!(failure(&untitled), None);
    }
}
