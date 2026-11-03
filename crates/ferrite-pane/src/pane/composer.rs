//! Solo's box Composer: its shelf and queue strip, the placeholder, the
//! footer's chips and the send button.

use super::*;

/// The name a model shows under, from any spelling the wire uses:
/// `claude-sonnet-4-5` → `Sonnet 4.5`, `gpt-5.4-mini` → `GPT-5.4 Mini`.
/// Public because every chip and row must spell a model exactly one way —
/// one grooming, never two; the catalog's own display names win where a
/// Session announced them (see `providers::models::label`).
#[cfg(any(test, feature = "test-support"))]
pub fn model_label(model: &str) -> SharedString {
    SharedString::from(ferrite_core::providers::models::display_name(model))
}

/// The model's word under the box (SPEC §5.5, Claude's footer): the model
/// as the picker names it, plain, in `INK_SUBTLE`. Render-only; the cockpit
/// gives it its button and its menu.
pub fn model_chip(label: SharedString) -> Div {
    chip_face(label, INK_SUBTLE)
}

/// The effort's word under the box: the level in force, sentence case.
pub fn effort_chip(label: SharedString) -> Div {
    chip_face(label, INK_SUBTLE)
}

/// The permission mode's word under the box: `INK_SUBTLE` in the provider's
/// default mode, else in `RISKY_INK` — a mode that changes what runs
/// without asking is worth seeing (`theme::mode_is_default`).
pub fn mode_chip(label: SharedString, mode: &str) -> Div {
    let ink = if theme::mode_is_default(mode) {
        INK_SUBTLE
    } else {
        theme::RISKY_INK
    };
    chip_face(label, ink)
}

/// A footer word's face: the label, cut at its end in a narrow Pane. No
/// chevron and no mark: the words are the controls (Claude's footer); the
/// pointer's fill says they press.
pub(super) fn chip_face(label: SharedString, ink: u32) -> Div {
    div()
        .flex()
        .flex_shrink(1.)
        .min_w_0()
        .items_center()
        .whitespace_nowrap()
        .font_family(theme::FONT_UI)
        .font_weight(theme::W_BODY)
        .text_size(px(theme::FS_META))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(ink))
        .child(div().min_w_0().truncate().child(label))
}

/// A footer word's button: `FOOTER_CHIP_H` high at `R_CONTROL`,
/// `FOOTER_CHIP_PAD_X` at each side, no ground at rest, `paint::RAISED`
/// under the pointer. The cockpit gives it its face and its menu. It may
/// narrow past its label, which then cuts (`composer_footer`).
pub fn toolbar_chip(
    id: impl Into<gpui::ElementId>,
    cx: &gpui::App,
) -> gpui::component::button::Button {
    chip_button(id, cx)
        .p_0()
        .h(px(theme::FOOTER_CHIP_H))
        .px(px(theme::FOOTER_CHIP_PAD_X))
        .min_w_0()
        .flex_shrink(1.)
        .rounded(px(theme::R_CONTROL))
}

/// The toolbar's `+`: a `COMPOSER_ATTACH` square that attaches files.
pub fn attach_button(cx: &gpui::App) -> gpui::component::button::Button {
    chip_button("composer-attach", cx)
        .debug_selector(|| "composer-attach".into())
        .p_0()
        .size(px(theme::COMPOSER_ATTACH))
        .flex_shrink_0()
        .rounded(px(theme::R_CONTROL))
        .tip("Attach files")
        .child(icon(icons::PLUS, theme::COMPOSER_ATTACH_ICON, ICON_INK))
}

/// Whether the send circle shows stop: Solo's (not `compact`), while the
/// turn runs, with nothing in the box. The cockpit draws the circle from
/// it and the working line drops its `esc to interrupt` by it, so the two
/// never disagree about whether a stop is in view.
pub fn send_is_stop(running: bool, empty: bool, compact: bool) -> bool {
    !compact && running && empty
}

/// The send circle (`COMPOSER_SEND` round; the `compact` Composer's, SPEC
/// §5.9, `SEND_CIRCLE_SM` with its glyph `SEND_GLYPH_SM`, spaced by its
/// box's gap). Three faces, as Claude's and Codex's: with something to send,
/// lit — `PRIMARY` with an `ON_PRIMARY` `arrow-up`; with nothing (`empty`),
/// at rest — `paint::SEND` with the arrow in `INK_SUBTLE` (still pressable:
/// an empty ↵ has its own acts); while a turn runs with nothing typed
/// (`stop`), `paint::SEND` with `stop` in `INK_STRONG`. The ground changes
/// at once with the keystroke that changes it; the glyph morphs
/// (`send_glyph`). The tooltip names the key.
pub fn send_button(
    stop: bool,
    empty: bool,
    compact: bool,
    cx: &gpui::App,
) -> gpui::component::button::Button {
    let (size, glyph) = if compact {
        (theme::SEND_CIRCLE_SM, theme::SEND_GLYPH_SM)
    } else {
        (theme::COMPOSER_SEND, theme::COMPOSER_SEND_ICON)
    };
    let lit = !stop && !empty;
    let (rest, hover, press, ink): (gpui::Hsla, gpui::Hsla, gpui::Hsla, u32) = if lit {
        (
            rgb(theme::PRIMARY).into(),
            rgb(INK).into(),
            rgb(INK).into(),
            theme::ON_PRIMARY,
        )
    } else {
        (
            theme::paint::SEND.into(),
            theme::paint::SEND.into(),
            theme::paint::PRESS.into(),
            if stop { INK_STRONG } else { INK_SUBTLE },
        )
    };
    components::faded_button("composer-send", rest, hover, press, rgb(ink).into(), cx)
        .debug_selector(move || {
            if stop {
                "composer-stop"
            } else {
                "composer-send"
            }
            .into()
        })
        .p_0()
        .size(px(size))
        .flex_shrink_0()
        .rounded_full()
        .map(|button| {
            if stop {
                button.tip_with_key("Stop", components::bound_chord("cockpit::Interrupt"))
            } else {
                button.tip_with_key("Send", Some(components::spell_chord("enter")))
            }
        })
        .child(send_glyph(stop, glyph, ink))
}

/// The send circle's glyph, morphing between `arrow-up` and `stop` when
/// the state flips under it (`motion::settled`, `FADE_QUICK`): the old
/// glyph fades and shrinks to `MOTION_SEND_GLYPH_FROM` as the new one fades
/// and grows into its place, both centred in the circle. First paint lands
/// on its state.
fn send_glyph(stop: bool, extent: f32, ink: u32) -> impl IntoElement {
    ferrite_ui::motion::settled(
        "send-glyph",
        stop,
        ferrite_ui::motion::FADE_QUICK,
        move |t| {
            let face = |path: &'static str, shown: f32| {
                let scale = theme::MOTION_SEND_GLYPH_FROM
                    + (1.0 - theme::MOTION_SEND_GLYPH_FROM) * shown;
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .opacity(shown)
                    .child(icon(path, extent, ink).with_transformation(
                        gpui::Transformation::scale(gpui::size(scale, scale)),
                    ))
            };
            div()
                .relative()
                .size(px(extent))
                .child(face(icons::ARROW_UP, 1.0 - t))
                .child(face(icons::STOP, t))
        },
    )
}

/// The Composer's controls, wired in the cockpit: Solo's `+` and send
/// circle on the box's row and its footer under the box; a board Pane's
/// send circle alone.
pub struct ComposerControls {
    pub attach: Option<AnyElement>,
    pub send: AnyElement,
    pub footer: Option<AnyElement>,
}

/// The Solo box's footer (`COMPOSER_FOOTER_H`, under the box, Claude's):
/// `lead` at the left — a draft's Project and Workspace — and at the right
/// the model, the effort and the permission mode as plain words, then the
/// context ring. In a narrow box what matters least gives way: the model's
/// word cuts first (the picker says it whole). The rest never give way to
/// it, the mode — the permission in force, which matters for safety —
/// least of all; a long one cuts at its own `TOOLBAR_LEAD_MAX_W`. Narrower
/// still, the words clip at the row's start.
pub fn composer_footer(
    lead: Vec<AnyElement>,
    model: Option<AnyElement>,
    hold: Vec<AnyElement>,
    ring: Option<AnyElement>,
) -> Div {
    let give = |chip: AnyElement| {
        div()
            .min_w_0()
            .flex_shrink(1.)
            .overflow_hidden()
            .child(chip)
    };
    let held = |chip: AnyElement| {
        div()
            .flex_shrink_0()
            .max_w(px(theme::TOOLBAR_LEAD_MAX_W))
            .overflow_hidden()
            .child(chip)
    };
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .items_center()
        .gap(px(theme::COMPOSER_TOOLBAR_GAP))
        .children(lead.into_iter().map(held))
        .child(div().flex_1().min_w_0())
        .children(model.map(give))
        .children(hold.into_iter().map(held))
        .children(ring.map(|ring| {
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .pl(px(theme::SPACE_1_5))
                .child(ring)
        }))
}

// --------------------------------------------------------------- Composer

/// The Composer stack's slice of `PaneState`, bundled so `composer_region`
/// stays readable as the states grow.
pub(super) struct ComposerStack<'a> {
    /// A board cell (C4): only the focused cell's Composer is live — its
    /// shelf and queue; the others keep their input band.
    pub(super) grid: bool,
    pub(super) decision: Option<&'a Decision>,
    /// Prompts held back while the turn runs, newest first: they pile up
    /// above the line, the latest on top.
    pub(super) queued: Vec<&'a str>,
    pub(super) empty: bool,
    /// How many files the draft holds: what the flat line counts while
    /// the shelf (`attachments`) waits for the live Composer.
    pub(super) files: usize,
    pub(super) attachments: Option<AnyElement>,
    /// Running background tasks as chips, hung at the right edge of the
    /// same shelf the pending files sit on.
    pub(super) background: Option<AnyElement>,
    pub(super) menu: Option<AnyElement>,
    /// A draft's Composer: nothing exists to steer yet.
    pub(super) draft: bool,
    pub(super) draft_error: Option<SharedString>,
    /// The follow-up predicted for this Thread's last response, if one has
    /// landed. The idle line shows it verbatim and Tab accepts it.
    pub(super) suggestion: Option<&'a str>,
    /// Whether this Pane holds the keyboard: the line's debug name, and on
    /// a board whether its Composer is live.
    pub(super) focused: bool,
    /// Native files hover the Pane: the band's edge is `DROP_EDGE`,
    /// saying where they will land.
    pub(super) drop_target: bool,
    /// A docked Decision sits flush on top. The band is a band either way
    /// (no box to join); kept so the levels hand the Composer one shape.
    #[allow(dead_code)]
    pub(super) joined: bool,
    /// The Composer's controls, wired in the cockpit: Solo's `+`, send and
    /// footer; on a board, the focused Pane's send circle.
    pub(super) toolbar: Option<ComposerControls>,
    /// The queued row's close button, wired to ⌫'s act in the cockpit.
    pub(super) unqueue: Option<AnyElement>,
}

/// The Composer stack (theme WP-D): a board cell's compact box
/// (`compact_composer`), Solo's box (`box_composer`, SPEC §5.5). The shelf
/// stands above either: background tasks, and on a board
/// the pending files (Solo's ride inside its box, over the text). The Pane lays the stack out `flex_shrink_0` below
/// the body, so the transcript gives way. The Decision row is **not**
/// here: it is the transcript's tail. While a Decision pends the input
/// carries the `Decision` key context, so its keys answer with the
/// keyboard in the Composer (#23).
pub(super) fn composer_region(view: &PaneView, transcript: Option<&Transcript>, stack: ComposerStack) -> Div {
    if stack.grid {
        compact_composer::compact_composer(view, transcript, stack)
    } else {
        box_composer(view, transcript, stack)
    }
}

/// The Solo Composer (SPEC §5.5): a box `COMPOSER_INSET` inside the card's
/// sides and foot — `paint::FIELD` at `R_FLOAT`, its `FIELD_RING` laid
/// outside it — holding the queued row, the pending files, the input
/// (Geist, no `❯`) and the toolbar the cockpit wires. Files dragged over
/// it light its edge and say `Drop files to attach` over its contents.
fn box_composer(view: &PaneView, transcript: Option<&Transcript>, stack: ComposerStack) -> Div {
    let ComposerStack {
        grid: _,
        decision,
        queued,
        empty,
        files: _,
        attachments,
        background,
        menu,
        draft,
        draft_error,
        suggestion,
        focused,
        drop_target,
        joined: _,
        toolbar,
        unqueue,
    } = stack;
    let geometry = view.geometry.clone();
    let namespace = view.text_namespace();
    let ring = |ink: u32| components::outer_ring(theme::R_FLOAT, rgba(ink));
    let field = div()
        .debug_selector(|| "composer-block".into())
        .when(drop_target, |field| {
            field.debug_selector(|| "composer-drop-target".into())
        })
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .min_w_0()
        .rounded(px(theme::R_FLOAT))
        .bg(theme::paint::FIELD)
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(INK))
        .when(decision.is_some(), |field| field.key_context("Decision"))
        .child(ring(theme::paint::FIELD_RING.rgba()));
    let mut field = components::on_bounds(field, move |bounds, _, _| {
        geometry.set(PaneGeometry {
            composer: Some(bounds),
            ..geometry.get()
        })
    });
    // Files hovering the Pane: the box's edge in `DROP_EDGE`, and (last,
    // so it paints over the contents) one line that says what letting go
    // does.
    let drop_hint = drop_target.then(|| {
            div()
                .debug_selector(|| "composer-drop-hint".into())
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(theme::SPACE_2))
                .rounded(px(theme::R_FLOAT))
                .bg(theme::paint::FIELD)
                .text_color(rgb(INK))
                .child(icon(icons::PLUS, theme::COMPOSER_ATTACH_ICON, INK))
                .child("Drop files to attach")
    });
    if drop_target {
        field = field.child(ring(theme::DROP_EDGE));
    }
    if let Some(row) = queued_row(&queued, &namespace, unqueue) {
        field = field.child(row);
    }
    if let Some(error) = draft_error {
        field = field.child(
            div()
                .flex_shrink_0()
                .min_w_0()
                .px(px(theme::COMPOSER_INPUT_PAD_X))
                .pt(px(theme::COMPOSER_INPUT_PAD_B))
                .whitespace_normal()
                .text_color(rgb(BLOCKED))
                .child(error),
        );
    }
    // The pending files ride inside the box, over the text they go with
    // (as Claude's and Codex's do), the text `ATTACH_ROW_GAP` under them.
    let attached = attachments.is_some();
    if let Some(attachments) = attachments {
        field = field.child(
            div()
                .debug_selector(|| "composer-attachments".into())
                .flex()
                .flex_shrink_0()
                .w_full()
                .min_w_0()
                .pt(px(theme::COMPOSER_INPUT_PAD_T))
                .px(px(theme::COMPOSER_INPUT_PAD_X))
                // A row's slot, as the shelf gives them: the chips take the
                // width and cut their names only when it runs out.
                .child(div().flex_1().min_w_0().child(attachments)),
        );
    }
    // The one line that grows: `COMPOSER_ROW_H` per visual row. The
    // placeholder overlays its first row while the line is empty.
    let mut line = div()
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
        .font_family(theme::FONT_UI)
        .font_weight(theme::W_BODY)
        .text_size(px(theme::FS_COMPOSER))
        .line_height(px(theme::COMPOSER_ROW_H))
        .text_color(rgb(INK_STRONG))
        .child(view.composer.clone());
    if empty {
        let ghost = placeholder(decision.is_some(), draft, transcript, suggestion);
        line = line.child(ghost_row(ghost, 0.));
    }
    // One row, as Claude's: the `+`, the line that grows, the send circle,
    // the buttons standing at the row's foot as the line grows over them.
    let (attach, send, footer) = match toolbar {
        Some(controls) => (controls.attach, Some(controls.send), controls.footer),
        None => (None, None, None),
    };
    let slot = |control: AnyElement| {
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(theme::COMPOSER_ATTACH))
            .child(control)
    };
    field = field.child(
        div()
            .debug_selector(|| "composer-row".into())
            .flex()
            .items_end()
            .gap(px(theme::COMPOSER_ROW_GAP))
            .min_w_0()
            .p(px(theme::COMPOSER_ROW_PAD))
            .when(attached, |row| row.pt(px(theme::ATTACH_ROW_GAP)))
            .children(attach.map(slot))
            .child(
                div()
                    .debug_selector(move || format!("composer-live-{namespace}"))
                    .flex()
                    .items_start()
                    .flex_1()
                    .min_h(px(theme::COMPOSER_ROW_H))
                    .min_w_0()
                    .py(px(theme::COMPOSER_TEXT_PAD_Y))
                    .child(line),
            )
            .children(send.map(slot)),
    );
    field = field.children(drop_hint);
    // The `/` or `@` popover paints above the stack — deferred, so it
    // escapes the Pane's clip and draws over the transcript (#24) — from
    // the box's left edge, at most `SLASH_MENU_W` wide.
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
    composer_stack(None, background, true)
        .child(in_column(theme::COMPOSER_INSET, true, field))
        // The model, effort and mode under the box, as plain words.
        .child(in_column(
            theme::COMPOSER_INSET,
            true,
            div()
                .debug_selector(|| "composer-footer".into())
                .flex()
                .items_center()
                .min_w_0()
                .h(px(theme::COMPOSER_FOOTER_H))
                .children(footer),
        ))
        .pb(px(theme::COMPOSER_FOOTER_PAD_B))
}

/// What stands over either Composer: `SHELF_GAP` over the box it belongs
/// to, the shelf — pending files at left, background chips at right; the
/// chips give way first, cutting their labels. The shelf keeps to its box's
/// inset (`boxed`: Solo's). The working line is the transcript's last row
/// (`working_tail`), not the stack's.
pub(super) fn composer_stack(
    attachments: Option<AnyElement>,
    background: Option<AnyElement>,
    boxed: bool,
) -> Div {
    let inset = if boxed {
        theme::COMPOSER_INSET
    } else {
        theme::COMPACT_COMPOSER_INSET
    };
    div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .min_w_0()
        .when(attachments.is_some() || background.is_some(), |stack| {
            let shelf = div()
                .debug_selector(|| "composer-shelf".into())
                .flex()
                .items_end()
                .gap(px(theme::SPACE_2))
                .min_w_0()
                .when_some(attachments, |shelf, attachments| {
                    shelf.child(div().flex_1().min_w_0().child(attachments))
                })
                .when_some(background, |shelf, chips| {
                    shelf.child(div().ml_auto().min_w_0().max_w_full().child(chips))
                });
            stack.child(in_column(inset, boxed, shelf).pb(px(theme::SHELF_GAP)))
        })
}

/// One row of the Composer stack, `inset` in from the card's sides — and in
/// Solo (`column`) at most the reading column wide (`TX_COLUMN_MAX_W`),
/// centred as the transcript is, so the box, the shelf and the working line
/// stand under the messages they follow (as Claude's and Codex's do). A
/// Group Pane's rows keep the card's whole width.
pub(super) fn in_column(inset: f32, column: bool, row: impl IntoElement) -> Div {
    div()
        .flex()
        .justify_center()
        .min_w_0()
        .px(px(inset))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .when(column, |row| row.max_w(px(theme::TX_COLUMN_MAX_W)))
                .child(row),
        )
}

/// The Solo box's queued row (`QUEUED_ROW_H`, a `FIELD_RING` rule under it):
/// `Queued`, the top of the pile on one cut line — the prompt ↑ takes back
/// and ⌫ drops — `+N` for the rest, and the close button that drops it.
/// The keys act as ever; they are not printed. None with nothing queued.
pub(super) fn queued_row(queued: &[&str], namespace: &str, close: Option<AnyElement>) -> Option<Div> {
    let latest = queued.first()?;
    let more = queued.len() - 1;
    let selector = format!("composer-queue-{namespace}");
    Some(
        div()
            .debug_selector(move || selector.clone())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(theme::QUEUED_GAP))
            .h(px(theme::QUEUED_ROW_H + 1.))
            .min_w_0()
            .pl(px(theme::QUEUED_PAD_L))
            .pr(px(theme::QUEUED_PAD_R))
            .border_b_1()
            .border_color(theme::paint::FIELD_RING)
            .whitespace_nowrap()
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(theme::FS_META))
                    .font_weight(theme::W_LABEL)
                    .text_color(rgb(INK_SUBTLE))
                    .child("Queued"),
            )
            .child(
                div()
                    .debug_selector(|| "queued-0".into())
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(INK))
                    .child(SharedString::from(queued_preview(latest))),
            )
            .when(more > 0, |row| {
                row.child(components::tabular(
                    div()
                        .debug_selector(|| "queued-more".into())
                        .flex_shrink_0()
                        .text_color(rgb(INK_SUBTLE))
                        .child(format!("+{more}")),
                ))
            })
            .children(close),
    )
}

/// The queued row's close button: a `QUEUED_CLOSE` square at `R_CONTROL`, the
/// `close` icon in `INK_SUBTLE`, `paint::RAISED` under the pointer. The
/// cockpit wires its click.
pub fn queued_close(cx: &gpui::App) -> gpui::component::button::Button {
    chip_button("queued-close", cx)
        .debug_selector(|| "queued-close".into())
        .p_0()
        .size(px(theme::QUEUED_CLOSE))
        .flex_shrink_0()
        .rounded(px(theme::R_CONTROL))
        .tip("Remove queued message")
        .child(icon(icons::CLOSE, theme::QUEUED_CLOSE_ICON, ICON_INK))
}

/// A quiet control's button: no ground at rest, `paint::HOVER` under the
/// pointer over the one 150ms blend, `paint::PRESS` pressed at once.
fn chip_button(id: impl Into<gpui::ElementId>, cx: &gpui::App) -> gpui::component::button::Button {
    components::faded_button(
        id,
        gpui::rgba(theme::TRANSPARENT).into(),
        theme::paint::HOVER.into(),
        theme::paint::PRESS.into(),
        rgb(INK_SUBTLE).into(),
        cx,
    )
}

/// The idle line's placeholder: a couple of words in `PLACEHOLDER_INK` after
/// the caret cell and a space, cut at their end only where even they do not
/// fit. The keys live behind `?` (ADR 0014), not on the line.
///
/// A predicted follow-up is already in the operator's voice and already
/// filtered, so it is shown verbatim — a draft of their next prompt, not a
/// description of one — with `⇥ accept`, which is always kept whole: an
/// accept key nobody knows about is the same as no accept key, so the
/// prediction's own words give way to it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Ghost {
    pub(super) head: SharedString,
    /// A verbatim prediction, followed by its accept key.
    pub(super) accept: bool,
}

impl Ghost {
    fn says(head: &'static str) -> Self {
        Self {
            head: head.into(),
            accept: false,
        }
    }

    /// The whole line, as the tests read it.
    #[cfg(test)]
    pub(super) fn text(&self) -> String {
        if self.accept {
            format!("{} \u{b7} {ACCEPT_HINT}", self.head)
        } else {
            self.head.to_string()
        }
    }
}

/// What follows a verbatim prediction.
const ACCEPT_HINT: &str = "\u{21e5} accept";

/// The placeholder for this line: a draft's first prompt, a Decision's
/// reply, a dead Session's revival, a landed prediction, a follow-up queued
/// behind a running turn, or steering a live Thread.
pub(super) fn placeholder(
    pending: bool,
    draft: bool,
    transcript: Option<&Transcript>,
    suggestion: Option<&str>,
) -> Ghost {
    if draft {
        return Ghost::says("Describe a task or ask a question");
    }
    let busy = transcript.is_some_and(|transcript| transcript.status() == Status::Streaming);
    match followup::suggest(pending, transcript, suggestion) {
        // A docked Decision owns the block above this line; the line itself
        // still steers, so it says so (rule 2.8.1: one input line).
        Followup::Decision => Ghost::says("Answer above"),
        Followup::Revive => Ghost::says("Revive and continue"),
        Followup::Suggested(text) => Ghost {
            head: SharedString::from(text),
            accept: true,
        },
        Followup::Steer if busy => Ghost::says("Queue a follow-up"),
        Followup::Steer => Ghost::says("Steer this thread"),
    }
}

/// `flat_facts`' words: empty when there is nothing to say.
pub fn flat_facts_text(files: usize, queued: usize) -> String {
    let mut facts = Vec::new();
    if files > 0 {
        facts.push(format!(
            "{files} attachment{}",
            if files == 1 { "" } else { "s" }
        ));
    }
    if queued > 0 {
        facts.push(format!("{queued} queued"));
    }
    facts.join(" \u{b7} ")
}

/// The placeholder drawn on a clipped row one line high, `left` in (Solo's
/// box: at the text's start; a board's band: two cells, after the caret's
/// cell and a space). A prediction lets its own words
/// truncate before the accept hint, which never goes.
pub(super) fn ghost_row(ghost: Ghost, left: f32) -> Div {
    div()
        .debug_selector(|| "prompt-placeholder".into())
        .absolute()
        .left(px(left))
        .right_0()
        .top_0()
        .h(px(theme::COMPOSER_ROW_H))
        .flex()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_color(rgb(theme::PLACEHOLDER_INK))
        .child(
            div()
                .flex_shrink(1.)
                .min_w_0()
                .h(px(theme::COMPOSER_ROW_H))
                .truncate()
                .child(ghost.head),
        )
        .when(ghost.accept, |row| {
            // The placeholder is one muted run, its `·` too (the
            // prototype's `.ph-text`).
            row.child(
                div()
                    .debug_selector(|| "prompt-placeholder-hint".into())
                    .flex()
                    .flex_shrink_0()
                    .h(px(theme::COMPOSER_ROW_H))
                    .child(
                        components::cells("\u{b7}")
                            .px(px(theme::CH))
                            .w(px(3.0 * theme::CH)),
                    )
                    .child(components::cells(ACCEPT_HINT)),
            )
        })
}

/// #11: whether this Thread still offers adopting a CLI session — no
/// conversation yet (nothing in the transcript beyond Ferrite's own notices
/// and bookkeeping) and at rest. One predicate for every surface that opens
/// the door — the placeholder hint, the import picker's choice of the
/// Thread's place, and the pick that closes the blank Thread — so no two
/// can disagree.
pub fn offers_import(transcript: Option<&Transcript>) -> bool {
    transcript.is_some_and(Transcript::offers_import)
}

/// The mode chip's label (SPEC §5.5): the Session's own label for `mode`
/// when its list has one (`permission_modes`, already sentence case); else
/// the mode's word (`theme::mode_word`), sentence case; `Default` at the
/// default. Never a raw id.
pub fn permission_mode_label(
    mode: &str,
    choices: &[ferrite_core::PermissionModeChoice],
) -> SharedString {
    if let Some(choice) = choices.iter().find(|choice| choice.value == mode) {
        return choice.label.clone().into();
    }
    theme::mode_word(mode).map_or_else(|| "Default".into(), |word| sentence_case(&word).into())
}

/// The queue's share of the editor's pane-height budget: Solo's one
/// queued row and its hairline whenever anything is queued; nothing in a
/// board's compact box, which is input only.
pub(crate) fn composer_queue_height(_height: f32, grid: bool, count: usize) -> f32 {
    if grid || count == 0 {
        0.
    } else {
        theme::QUEUED_ROW_H + 1.
    }
}

/// The Composer's height less its editor rows and queue, the same at every
/// level, focused or not. Solo's box: the row's padding around the line
/// (its ring lies outside it), then the footer and the air under it. A board's compact box:
/// the air around its row and the inset under it. The shelf and the
/// working line stand above and are not part of it.
pub(super) fn composer_fixed_height(grid: bool) -> f32 {
    if grid {
        theme::COMPACT_COMPOSER_H - theme::COMPOSER_ROW_H + theme::COMPACT_COMPOSER_INSET
    } else {
        2.0 * (theme::COMPOSER_ROW_PAD + theme::COMPOSER_TEXT_PAD_Y)
            + theme::COMPOSER_FOOTER_H
            + theme::COMPOSER_FOOTER_PAD_B
    }
}

/// Leave the majority of a Pane available for its Thread context. Only the
/// viewport changes: the Composer keeps every character and scrolls to its
/// caret, then reveals more rows again when the Pane grows.
pub fn composer_row_limit(height: f32, grid: bool, queued: usize) -> usize {
    let fixed = composer_fixed_height(grid);
    let queue = composer_queue_height(height, grid, queued);
    ((height * theme::COMPOSER_MAX_PANE_FRACTION - fixed - queue) / theme::COMPOSER_ROW_H)
        .floor()
        .max(1.)
        .min(crate::composer::MAX_ROWS as f32) as usize
}

/// A held prompt as one line: the queued row is a single line and
/// gpui's truncation still breaks at `\n`, so a multi-line prompt would
/// stack its lines into the row and clip them both.
pub(super) fn queued_preview(held: &str) -> String {
    held.split_whitespace().collect::<Vec<_>>().join(" ")
}
