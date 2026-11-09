//! A turn's wrap-up box (HD58, `ferrite_core::wrap_up`): the agent's recap,
//! the question it waits on, and at most two next steps, each a button
//! that sends its own words as the next prompt. It sits on the content
//! column as a hairline card, the way Claude's does.
use super::*;
use ferrite_core::wrap_up::WrapUp;

/// What the box offers under its words.
pub(crate) enum Face {
    /// The next steps as the row's owner wired them (a press sends the
    /// words).
    Steps(Vec<AnyElement>),
    /// A step was used, or the Thread has moved on: no buttons, and the
    /// words that were sent, if they were one of the steps.
    Spent(Option<String>),
}

/// The box, as Claude draws it (`claude/parity/wrapup-*.png`): no ground,
/// a `CARD_RING` hairline at `R_CARD`, the recap in `INK_SUBTLE`, the
/// question in `INK_STRONG` and the steps under them, `SPACE_4` between
/// each. Spent, it shrinks to its words (`WRAP_UP_SPENT_MAX_W`) and the
/// step that was sent stands under the question, muted. The recap and the
/// ask are selectable text.
pub(crate) fn render_wrap_up(
    block: BlockId,
    wrap: &WrapUp,
    row_cx: &RowCx,
    face: Face,
) -> AnyElement {
    let selection = row_cx.selection;
    let spent = matches!(face, Face::Spent(_));
    let card = div()
        .debug_selector(move || if spent { "wrap-up-spent" } else { "wrap-up" }.into())
        .flex()
        .flex_col()
        .gap(px(theme::SPACE_4))
        .min_w_0()
        .p(px(theme::SPACE_4))
        .rounded(px(theme::R_CARD))
        .border_1()
        .border_color(theme::paint::CARD_RING)
        .text_color(rgb(INK_STRONG))
        .when(spent, |card| card.max_w(px(theme::WRAP_UP_SPENT_MAX_W)))
        .when(!spent, |card| card.w_full())
        .when(!wrap.recap.is_empty(), |card| {
            card.child(
                div()
                    .text_color(rgb(INK_SUBTLE))
                    .child(selection.line(block, wrap.recap.clone(), Vec::new())),
            )
        })
        .children(wrap.ask.as_ref().map(|ask| {
            let chosen = match &face {
                Face::Spent(Some(words)) => Some(words.clone()),
                _ => None,
            };
            div()
                .debug_selector(|| "wrap-up-ask".into())
                .flex()
                .flex_col()
                .gap(px(theme::SPACE_2))
                .child(selection.line(block, ask.clone(), Vec::new()))
                .children(chosen.map(|words| {
                    div()
                        .debug_selector(|| "wrap-up-chosen".into())
                        .text_color(rgb(INK_SUBTLE))
                        .child(SharedString::from(words))
                }))
        }))
        .children(match face {
            Face::Steps(steps) if !steps.is_empty() => Some(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(theme::SPACE_3))
                    .children(steps),
            ),
            _ => None,
        });
    div()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .child(card)
        .into_any_element()
}

/// Its copy fragments, in the order `render_wrap_up` registers them.
pub(crate) fn collect_text(block: BlockId, wrap: &WrapUp, selection: &TextRuns) {
    if !wrap.recap.is_empty() {
        let _ = selection.line(block, wrap.recap.clone(), Vec::new());
    }
    if let Some(ask) = &wrap.ask {
        let _ = selection.line(block, ask.clone(), Vec::new());
    }
}
