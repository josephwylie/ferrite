//! A turn's wrap-up box (HD58, `ferrite_core::wrap_up`): the agent's recap,
//! the question it waits on, and at most two next steps, each a button
//! that sends its own words as the next prompt. It sits on the content
//! column as a raised card, the way a group's card does.
use super::*;
use ferrite_core::wrap_up::WrapUp;

/// The box. `steps` are its next steps as the row's owner wired them (a
/// press sends the words); the recap and the ask are selectable text.
pub(crate) fn render_wrap_up(
    block: BlockId,
    wrap: &WrapUp,
    row_cx: &RowCx,
    steps: Vec<AnyElement>,
) -> AnyElement {
    let selection = row_cx.selection;
    let card = div()
        .debug_selector(|| "wrap-up".into())
        .flex()
        .flex_col()
        .gap(px(theme::SPACE_2))
        .w_full()
        .min_w_0()
        .px(px(theme::SPACE_3))
        .py(px(theme::SPACE_2))
        .rounded(px(theme::R_CARD))
        .bg(theme::paint::RAISED)
        .text_color(rgb(INK_STRONG))
        .when(!wrap.recap.is_empty(), |card| {
            card.child(selection.line(block, wrap.recap.clone(), Vec::new()))
        })
        .children(wrap.ask.as_ref().map(|ask| {
            div()
                .debug_selector(|| "wrap-up-ask".into())
                .font_weight(theme::W_STRONG)
                .child(selection.line(block, ask.clone(), Vec::new()))
        }))
        .when(!steps.is_empty(), |card| {
            card.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(theme::SPACE_2))
                    .children(steps),
            )
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
