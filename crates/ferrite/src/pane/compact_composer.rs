//! The compact Composer (SPEC §5.9 item 5): a Group Pane's input box.
//!
//! Solo's Composer (`box_composer`) carries the queued row and the
//! toolbar; a Group Pane's is input only. One 38px box,
//! `COMPACT_COMPOSER_INSET` in from the card's sides and foot, at `R_BLOCK`
//! on `FIELD_COMPACT`, inside a 1px ring just outside it (`COMPACT_RING`,
//! `COMPACT_RING_FOCUSED` on the focused Pane). The input is Geist 13; its
//! placeholder is the state's (`placeholder`), or on an unfocused Pane its
//! flat facts (`1 attachment · 2 queued`) when it has any. The focused Pane
//! alone shows the 24px send circle (`send_button`). No chips: the mode, model and effort
//! live in Solo and in a Decision's context word.

use super::*;

/// One board Pane's Composer: the shelf and the working line over the box
/// (`composer_stack`), as Solo's, then the box.
pub(super) fn compact_composer(
    view: &PaneView,
    transcript: Option<&Transcript>,
    stack: ComposerStack,
) -> Div {
    let ComposerStack {
        decision,
        queued,
        empty,
        files,
        attachments,
        background,
        menu,
        draft,
        draft_error,
        suggestion,
        working,
        focused,
        drop_target,
        toolbar,
        ..
    } = stack;
    // Only the focused cell's Composer is live (C4): the others say what
    // their shelf and queue hold in place of the state's words.
    let live = focused || drop_target;
    let facts = flat_facts_text(files, queued.len());
    let says_facts = !live && !facts.is_empty();
    let ghost = if !says_facts {
        placeholder(decision.is_some(), draft, transcript, suggestion)
    } else {
        Ghost {
            head: facts.into(),
            accept: false,
        }
    };
    let namespace = view.text_namespace();
    let line = div()
        .debug_selector(move || {
            if focused {
                "focused-prompt-editor"
            } else {
                "prompt-editor"
            }
            .into()
        })
        .relative()
        .flex_1()
        .min_w_0()
        .line_height(px(theme::COMPOSER_ROW_H))
        .child(view.composer.clone())
        .when(says_facts, |line| {
            line.debug_selector(|| "composer-flat-facts".into())
        })
        .when(empty, |line| line.child(ghost_row(ghost, 0.)));
    let input = div()
        .debug_selector(move || {
            format!(
                "composer-{}-{namespace}",
                if live { "live" } else { "flat" }
            )
        })
        .flex()
        .flex_1()
        .min_w_0()
        .child(line);
    let ring = if drop_target {
        theme::DROP_EDGE
    } else if focused {
        theme::COMPACT_RING_FOCUSED
    } else {
        theme::COMPACT_RING
    };
    let geometry = view.geometry.clone();
    let field = div()
        .debug_selector(|| "composer-block".into())
        .when(drop_target, |field| {
            field.debug_selector(|| "composer-drop-target".into())
        })
        .when(!live, |field| {
            field.debug_selector(|| "composer-flat".into())
        })
        .relative()
        .flex()
        .items_center()
        .gap(px(theme::SPACE_2))
        .min_h(px(theme::COMPACT_COMPOSER_H))
        .min_w_0()
        .mx(px(theme::COMPACT_COMPOSER_INSET))
        .py(px((theme::COMPACT_COMPOSER_H - theme::COMPOSER_ROW_H) / 2.0))
        .pl(px(theme::COMPACT_COMPOSER_PAD_L))
        .pr(px(theme::COMPACT_COMPOSER_PAD_R))
        .rounded(px(theme::R_BLOCK))
        .bg(theme::paint::FIELD_COMPACT)
        .font_family(theme::FONT_UI)
        .font_weight(theme::W_BODY)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(TEXT))
        .when(decision.is_some(), |field| field.key_context("Decision"))
        .child(outer_ring(ring))
        .child(input)
        // The send circle, wired in the cockpit: the focused Pane's only.
        // It rides a row-high slot (overflowing it), so focus never
        // changes the box's height.
        .children(toolbar.filter(|_| focused).map(|send| {
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .h(px(theme::COMPOSER_ROW_H))
                .child(send)
        }));
    let mut field = components::on_bounds(field, move |bounds, _, _| {
        geometry.set(PaneGeometry {
            composer: Some(bounds),
            ..geometry.get()
        })
    });
    // The `/` or `@` popover hangs above the box, deferred so it escapes
    // the Pane's clip, at most `SLASH_MENU_W` wide.
    if let Some(menu) = menu {
        field = field.child(deferred(
            div()
                .absolute()
                .bottom(relative(1.))
                .left_0()
                .right_0()
                .mb(px(theme::FLOAT_OFFSET))
                .child(div().max_w(px(theme::SLASH_MENU_W)).child(menu)),
        ));
    }
    let (attachments, background) = if live {
        (attachments, background)
    } else {
        (None, None)
    };
    composer_stack(attachments, background, working, false)
        // A draft's failed bootstrap says why over the box.
        .children(draft_error.map(|error| {
            div()
                .flex_shrink_0()
                .min_w_0()
                .px(px(theme::COMPACT_COMPOSER_INSET))
                .pb(px(theme::SHELF_GAP))
                .whitespace_normal()
                .text_color(rgb(BLOCKED))
                .child(error)
        }))
        .pb(px(theme::COMPACT_COMPOSER_INSET))
        .child(field)
}

/// The box's 1px ring in `ink`, just outside it (the mockup's `box-shadow:
/// 0 0 0 1px`): it takes no layout, so the box keeps its 38.
fn outer_ring(ink: u32) -> Div {
    let out = px(-1.0);
    div()
        .absolute()
        .top(out)
        .left(out)
        .right(out)
        .bottom(out)
        .rounded(px(theme::R_BLOCK + 1.0))
        .border_1()
        .border_color(rgba(ink))
}
