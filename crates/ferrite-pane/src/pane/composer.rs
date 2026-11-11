//! Solo's box Composer: its shelf and queue strip, the placeholder, the
//! footer's chips and the send button.

use super::*;
use ferrite_core::engine::{Delivery, QueueItem};

/// The name a model shows under, from any spelling the wire uses:
/// `claude-sonnet-4-5` → `Sonnet 4.5`, `gpt-5.4-mini` → `GPT-5.4 Mini`.
/// Public because every chip and row must spell a model exactly one way —
/// one grooming, never two; the catalog's own display names win where a
/// Session announced them (see `providers::models::label`).
#[cfg(any(test, feature = "test-support"))]
pub fn model_label(model: &str) -> SharedString {
    SharedString::from(ferrite_core::providers::models::display_name(model))
}

/// The model chip's face (T3's model trigger, Claude's `Opus 5.5 Medium`):
/// what serves the Thread and how hard it thinks.
pub struct ModelChip {
    /// The provider's logomark and brand ink (`provider::mark`).
    pub mark: (&'static str, u32),
    pub model: SharedString,
    /// The effort in force; `None` for a model that takes none.
    pub effort: Option<SharedString>,
    /// The Session is starting: the name is the provider's default until it
    /// says which model serves, and shimmers as the working line does
    /// (Codex's skeleton chip).
    pub loading: bool,
    /// The picker the chip opens is open.
    pub open: bool,
    /// The chip button's hover group: the effort lifts while it is hovered.
    pub group: SharedString,
}

/// The model chip's face: the provider's mark at `MODEL_PICKER_MARK`, then
/// the model in `INK_STRONG` at `W_LABEL`, cut first in a narrow Pane, then
/// the effort at `W_BODY`, `FOOTER_WORD_GAP` after it and never cut. The
/// effort is `INK_SUBTLE`, lifting to `INK_STRONG` while the chip is
/// hovered or its picker is open (T3's suffix).
pub fn model_effort_chip(chip: ModelChip) -> Div {
    let ModelChip {
        mark: (path, ink),
        model,
        effort,
        loading,
        open,
        group,
    } = chip;
    let model = if loading {
        components::shimmer(model, INK_STRONG)
    } else {
        model.into_any_element()
    };
    let mark_selector = format!("{group}-mark");
    let effort = effort.map(|effort| {
        div()
            .flex_shrink_0()
            .font_weight(theme::W_BODY)
            .text_color(rgb(if open { INK_STRONG } else { INK_SUBTLE }))
            .group_hover(group.clone(), |style| style.text_color(rgb(INK_STRONG)))
            .child(effort)
    });
    div()
        .flex()
        .flex_shrink(1.)
        .min_w_0()
        .items_center()
        .gap(px(theme::SPACE_1_5))
        .child(
            div()
                .debug_selector(move || mark_selector.clone())
                .flex_shrink_0()
                .child(icons::icon(path, theme::MODEL_PICKER_MARK, ink)),
        )
        .child(
            chip_face(model, INK_STRONG)
                .font_weight(theme::W_LABEL)
                .gap(px(theme::FOOTER_WORD_GAP))
                .children(effort),
        )
}

/// The permission mode's word under the box: `INK_SUBTLE`, or
/// `MODE_ATTENTION_INK` for `Manual` (`theme::mode_is_manual`): the one
/// place it is amber, its menu row neutral (Claude's).
pub fn mode_chip(label: SharedString, mode: &str) -> Div {
    let ink = if theme::mode_is_manual(mode) {
        theme::MODE_ATTENTION_INK
    } else {
        INK_SUBTLE
    };
    chip_face(label, ink)
}

/// A footer word's face: the label, cut at its end in a narrow Pane. No
/// chevron and no mark: the words are the controls (Claude's footer); the
/// pointer's fill says they press.
pub(super) fn chip_face(label: impl IntoElement, ink: u32) -> Div {
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
        .child(icon(icons::PLUS, theme::COMPOSER_ATTACH_ICON, INK_STRONG))
}

/// Which of the Composer's primary actions stand (T3's): Solo's stop while
/// a turn runs, its send once something is typed, both while a turn runs
/// with text (stop first). The `compact` Composer's one circle always
/// stands and always sends, into a running turn too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrimaryActions {
    pub stop: bool,
    pub send: bool,
}

pub fn primary_actions(running: bool, empty: bool, compact: bool) -> PrimaryActions {
    if compact {
        return PrimaryActions {
            stop: false,
            send: true,
        };
    }
    PrimaryActions {
        stop: running,
        send: !empty,
    }
}

/// The send circle (`COMPOSER_SEND` round, growing to
/// `MOTION_SEND_HOVER_SCALE` under the pointer; the `compact` Composer's,
/// SPEC §5.9, `SEND_CIRCLE_SM` with its glyph `SEND_GLYPH_SM`). With
/// something to send it is lit, T3's: `SEND_LIT` with an `ON_SEND_LIT`
/// arrow under a `SEND_LIT_HIGHLIGHT` top edge; with nothing (`empty`, the
/// compact one's) `paint::SEND` with the arrow in `INK_SUBTLE`, still
/// pressable. While a turn runs (`steer`) the arrow morphs into the steer
/// glyph (`send_glyph`). The tooltip names the key.
pub fn send_button(
    steer: bool,
    empty: bool,
    compact: bool,
    cx: &gpui::App,
) -> gpui::component::button::Button {
    let (size, glyph) = if compact {
        (theme::SEND_CIRCLE_SM, theme::SEND_GLYPH_SM)
    } else {
        (theme::COMPOSER_SEND, theme::COMPOSER_SEND_ICON)
    };
    let lit = !empty;
    let (rest, hover, press, ink): (gpui::Hsla, gpui::Hsla, gpui::Hsla, u32) = if lit {
        (
            rgb(theme::SEND_LIT).into(),
            rgb(theme::SEND_LIT_HOVER).into(),
            rgb(theme::SEND_LIT_HOVER).into(),
            theme::ON_SEND_LIT,
        )
    } else {
        (
            theme::paint::SEND.into(),
            theme::paint::SEND.into(),
            theme::paint::PRESS.into(),
            INK_SUBTLE,
        )
    };
    let lift = if compact {
        1.0
    } else {
        theme::MOTION_SEND_HOVER_SCALE
    };
    components::lifting_button("composer-send", rest, hover, press, rgb(ink).into(), lift, cx)
        .debug_selector(|| "composer-send".into())
        .relative()
        .p_0()
        .size(px(size))
        .flex_shrink_0()
        .rounded_full()
        .map(|button| {
            let verb = if steer { "Steer" } else { "Send" };
            button.tip_with_key(verb, Some(components::spell_chord("enter")))
        })
        .when(lit, |button| {
            button.child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded_full()
                    .border_t_1()
                    .border_color(rgba(theme::SEND_LIT_HIGHLIGHT)),
            )
        })
        .child(send_glyph(steer, glyph, ink))
}

/// Stop (T3's interrupt): a `COMPOSER_STOP` circle in `STOP`, `STOP_HOVER`
/// under the pointer, its white rounded square; the tooltip says
/// `Interrupt` and names the key.
pub fn stop_button(cx: &gpui::App) -> gpui::component::button::Button {
    components::faded_button(
        "composer-stop",
        rgba(theme::STOP).into(),
        rgba(theme::STOP_HOVER).into(),
        rgba(theme::STOP_HOVER).into(),
        rgb(theme::ON_SEND_LIT).into(),
        cx,
    )
    .debug_selector(|| "composer-stop".into())
    .p_0()
    .size(px(theme::COMPOSER_STOP))
    .flex_shrink_0()
    .rounded_full()
    .tip_with_key("Interrupt", components::bound_chord("cockpit::Interrupt"))
    .child(icon(icons::STOP, theme::COMPOSER_STOP_ICON, theme::ON_SEND_LIT))
}

/// The send circle's glyph, morphing between `arrow-up` and the steer
/// glyph when a turn starts or ends under it (`motion::settled`,
/// `FADE_QUICK`): the old glyph fades and shrinks to
/// `MOTION_SEND_GLYPH_FROM` as the new one fades and grows into its place,
/// both centred in the circle. First paint lands on its state. The arrow
/// is T3's, stroked `SEND_ARROW_STROKE`.
fn send_glyph(steer: bool, extent: f32, ink: u32) -> impl IntoElement {
    ferrite_ui::motion::settled(
        "send-glyph",
        steer,
        ferrite_ui::motion::FADE_QUICK,
        move |t| {
            let face =
                |path: &'static str, shown: f32| {
                    let scale = theme::MOTION_SEND_GLYPH_FROM
                        + (1.0 - theme::MOTION_SEND_GLYPH_FROM) * shown;
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .opacity(shown)
                        .child(
                            icons::icon_stroked(path, extent, theme::SEND_ARROW_STROKE, ink)
                                .with_transformation(gpui::Transformation::scale(gpui::size(
                                    scale, scale,
                                ))),
                        )
                };
            div()
                .relative()
                .size(px(extent))
                .child(face(icons::ARROW_UP, 1.0 - t))
                .child(face(icons::CORNER_UP_RIGHT, t))
        },
    )
}

/// The Composer's controls, wired in the cockpit: Solo's `+` and send
/// circle on the box's row, its footer under the box and, on an empty
/// Thread or a draft, where the work runs on a strip over it (`env`,
/// `env_strip`); a board Pane's send circle alone. Either's queue strip
/// carries `queue`, each queued message's action buttons
/// (`queued_send_now`, `queued_edit`, `queued_close`), in the queue's
/// listing order.
pub struct ComposerControls {
    pub attach: Option<AnyElement>,
    /// Stop, while a turn runs (`primary_actions`), before send.
    pub stop: Option<AnyElement>,
    /// The send circle; none while the box has nothing to send.
    pub send: Option<AnyElement>,
    pub footer: Option<AnyElement>,
    pub env: Option<AnyElement>,
    pub queue: Vec<Vec<AnyElement>>,
}

/// Where an empty Thread's or a draft's work runs, Codex's strip over the
/// box: `ENV_STRIP_H` of `paint::ENV_STRIP`, `ENV_STRIP_INSET` in from the
/// box's sides, its top corners `R_FLOAT`, standing on the box's top edge;
/// its chips (a draft's pickers, `band_chip`; an empty Thread's words,
/// `env_word`) from its left.
pub fn env_strip(chips: Vec<AnyElement>) -> Div {
    div()
        .debug_selector(|| "composer-env".into())
        .flex()
        .items_center()
        .gap(px(theme::ENV_CHIP_GAP))
        .min_w_0()
        .h(px(theme::ENV_STRIP_H))
        .mx(px(theme::ENV_STRIP_INSET))
        .px(px(theme::ENV_STRIP_PAD_X))
        .rounded_t(px(theme::R_FLOAT))
        .bg(theme::paint::ENV_STRIP)
        .overflow_hidden()
        .children(chips.into_iter().map(|chip| {
            div()
                .flex_shrink(1.)
                .min_w_0()
                .max_w(px(theme::TOOLBAR_LEAD_MAX_W))
                .overflow_hidden()
                .child(chip)
        }))
}

/// An env strip chip's face: its mark in `INK_SUBTLE`, then its words in
/// `INK_STRONG`, cut at their end.
pub(crate) fn env_face(mark: &'static str, label: SharedString) -> Div {
    div()
        .flex()
        .flex_shrink(1.)
        .min_w_0()
        .items_center()
        .gap(px(theme::ENV_MARK_GAP))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .child(icon(mark, theme::ENV_MARK, INK_STRONG)),
        )
        .child(chip_face(label, INK_STRONG))
}

/// An env strip word that is not a control (an empty Thread's Project and
/// branch): a chip's face at a chip's inset, so it lines up with a draft's
/// pickers.
pub fn env_word(mark: &'static str, label: SharedString) -> Div {
    div()
        .flex()
        .items_center()
        .min_w_0()
        .h(px(theme::FOOTER_CHIP_H))
        .px(px(theme::FOOTER_CHIP_PAD_X))
        .child(env_face(mark, label))
}

/// The Solo box's footer (`COMPOSER_FOOTER_H`, under the box, Claude's):
/// at the right the model and effort, then the permission mode as plain
/// words, then the plan-usage ring (where the work runs stands over the
/// box, `env_strip`). In a narrow box what matters least gives way: the
/// model's word cuts first (the picker says it whole). The rest never give
/// way to it, the mode — the permission in force, which matters for
/// safety — least of all; a long one cuts at its own `TOOLBAR_LEAD_MAX_W`.
/// Narrower still, the words clip at the row's start.
pub fn composer_footer(
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
        .gap(px(theme::FOOTER_CHIP_GAP))
        .child(div().flex_1().min_w_0())
        .children(model.map(give))
        .children(hold.into_iter().map(held))
        .children(ring.map(|ring| div().flex().flex_shrink_0().items_center().child(ring)))
}

// --------------------------------------------------------------- Composer

/// The Composer stack's slice of `PaneState`, bundled so `composer_region`
/// stays readable as the states grow.
pub(super) struct ComposerStack<'a> {
    /// A board cell (C4): only the focused cell's Composer is live — its
    /// shelf and queue; the others keep their input band.
    pub(super) grid: bool,
    pub(super) decision: Option<&'a Decision>,
    /// Prompts not yet picked up, newest first (`ThreadView::queue`): the
    /// strip shows the latest and counts the rest.
    pub(super) queued: &'a [QueueItem],
    pub(super) empty: bool,
    /// How many files the draft holds: what the flat line counts while
    /// the shelf (`attachments`) waits for the live Composer.
    pub(super) files: usize,
    pub(super) attachments: Option<AnyElement>,
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
    /// footer; on a board, the focused Pane's send circle. Both carry the
    /// queue strip's actions.
    pub(super) toolbar: Option<ComposerControls>,
}

/// The Composer stack (theme WP-D): a board cell's compact box
/// (`compact_composer`), Solo's box (`box_composer`, SPEC §5.5). On a
/// board the shelf of pending files stands above it (Solo's ride inside
/// its box, over the text). Running background work has no shelf: it is
/// in its own rows, the rail and the Background tasks pane, as Claude's. The Pane lays the stack out `flex_shrink_0` below
/// the body, so the transcript gives way. The Decision row is **not**
/// here: it is the transcript's tail. While a Decision pends the input
/// carries the `Decision` key context, so its keys answer with the
/// keyboard in the Composer (#23).
pub(super) fn composer_region(
    view: &PaneView,
    transcript: Option<&Transcript>,
    stack: ComposerStack,
) -> Div {
    if stack.grid {
        compact_composer::compact_composer(view, transcript, stack)
    } else {
        box_composer(view, transcript, stack)
    }
}

/// The Solo Composer (SPEC §5.5): a box `COMPOSER_OUTSET` past the reading
/// column at each side — `paint::FIELD` at `R_COMPOSER`, its `FIELD_RING`
/// laid outside it — holding the pending files, the input (no `❯`) and the
/// toolbar the cockpit wires; the queue strip stands over it as its own
/// card, the footer under it. Files dragged over it light its edge and say
/// `Drop files to attach` over its contents.
fn box_composer(view: &PaneView, transcript: Option<&Transcript>, stack: ComposerStack) -> Div {
    let ComposerStack {
        grid: _,
        decision,
        queued,
        empty,
        files: _,
        attachments,
        menu,
        draft,
        draft_error,
        suggestion,
        focused,
        drop_target,
        joined: _,
        toolbar,
    } = stack;
    let geometry = view.geometry.clone();
    let namespace = view.text_namespace();
    let ring = |ink: u32| components::outer_ring(theme::R_COMPOSER, rgba(ink));
    // T3's `inset-shadow-2xs`: a hairline of light along the box's top.
    let highlight = div()
        .absolute()
        .inset_0()
        .rounded(px(theme::R_COMPOSER))
        .border_t_1()
        .border_color(rgba(theme::COMPOSER_TOP_HIGHLIGHT));
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
        .rounded(px(theme::R_COMPOSER))
        .bg(theme::paint::FIELD)
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(INK_STRONG))
        .when(decision.is_some(), |field| field.key_context("Decision"))
        .child(ring(theme::paint::FIELD_RING.rgba()))
        .child(highlight);
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
            .rounded(px(theme::R_COMPOSER))
            .bg(theme::paint::FIELD)
            .text_color(rgb(INK_STRONG))
            .child(icon(icons::PLUS, theme::COMPOSER_ATTACH_ICON, INK_STRONG))
            .child("Drop files to attach")
    });
    if drop_target {
        field = field.child(ring(theme::DROP_EDGE));
    }
    if let Some(error) = draft_error {
        field = field.child(ferrite_ui::motion::fade_in(
            SharedString::from(format!("composer-notice-{namespace}")),
            div()
                .flex_shrink_0()
                .min_w_0()
                .px(px(theme::COMPOSER_INPUT_PAD_X))
                .pt(px(theme::COMPOSER_INPUT_PAD_B))
                .whitespace_normal()
                .text_color(rgb(BLOCKED))
                .child(error),
            ferrite_ui::motion::FADE_QUICK,
            1.0,
        ));
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
    let ghost = placeholder(decision.is_some(), draft, transcript, suggestion);
    line = line.child(LineGhost {
        composer: view.composer.clone(),
        ghost,
        empty,
    });
    // One row, as Claude's: the `+`, the line that grows, the send circle,
    // the buttons standing at the row's foot as the line grows over them.
    let (attach, primary, footer, env, actions) = match toolbar {
        Some(controls) => (
            controls.attach,
            primary_slot(controls.stop, controls.send),
            controls.footer,
            controls.env,
            controls.queue,
        ),
        None => (None, None, None, None, Vec::new()),
    };
    let queue = queue_card(queued, &namespace, actions);
    let slot = |control: AnyElement| {
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(theme::COMPOSER_ATTACH))
            .child(control)
    };
    let live_key = namespace.clone();
    let live = div()
        .debug_selector(move || format!("composer-live-{live_key}"))
        .flex()
        .items_start()
        .flex_1()
        .min_h(px(theme::COMPOSER_ROW_H))
        .min_w_0()
        .child(line);
    // The hero's box (a draft's, an empty Thread's: the ones that say where
    // their work runs) is Codex's home composer: the line on its own,
    // `COMPOSER_HERO_INPUT_H` at least, over a toolbar row holding the `+`
    // at its left and the send circle at its right. Every other box is
    // Claude's one row.
    if env.is_some() {
        field = field
            .child(
                div()
                    .debug_selector(|| "composer-row".into())
                    .flex()
                    .min_w_0()
                    .min_h(px(theme::COMPOSER_HERO_INPUT_H))
                    .px(px(theme::COMPOSER_INPUT_PAD_X))
                    .pt(px(if attached {
                        theme::ATTACH_ROW_GAP
                    } else {
                        theme::COMPOSER_INPUT_PAD_T
                    }))
                    .pb(px(theme::COMPOSER_INPUT_PAD_B))
                    // The line fills the room, its text at the top, so
                    // typing grows the box upward from its fixed foot.
                    .child(live.items_stretch()),
            )
            .child(
                div()
                    .debug_selector(|| "composer-toolbar".into())
                    .flex()
                    .items_center()
                    .h(px(theme::COMPOSER_TOOLBAR_H))
                    .px(px(theme::COMPOSER_TOOLBAR_PAD_X))
                    .children(attach.map(slot))
                    .child(div().flex_1())
                    .children(primary.map(slot)),
            );
    } else {
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
                .child(live.py(px(theme::COMPOSER_TEXT_PAD_Y)))
                .children(primary.map(slot)),
        );
    }
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
    composer_stack(None, true)
        .children(queue.map(|queue| {
            in_column(theme::COMPOSER_INSET, true, queue).pb(px(theme::QUEUE_CARD_GAP))
        }))
        // Where the work runs, on the box's top edge (Codex's).
        .children(env.map(|env| in_column(theme::COMPOSER_INSET, true, env)))
        .child(in_column(theme::COMPOSER_INSET, true, field))
        // The model, effort and mode under the box, as plain words, the
        // last one's ink ending where the column's text does.
        .child(in_column(
            theme::COMPOSER_INSET,
            true,
            div()
                .debug_selector(|| "composer-footer".into())
                .flex()
                .items_center()
                .min_w_0()
                .h(px(theme::COMPOSER_FOOTER_H))
                .px(px(theme::COMPOSER_OUTSET - theme::FOOTER_CHIP_PAD_X))
                .children(footer),
        ))
}

/// Stop then send, `PRIMARY_ACTIONS_GAP` apart (T3's `gap-2`), or none.
fn primary_slot(stop: Option<AnyElement>, send: Option<AnyElement>) -> Option<AnyElement> {
    if stop.is_none() && send.is_none() {
        return None;
    }
    Some(
        div()
            .flex()
            .items_center()
            .gap(px(theme::PRIMARY_ACTIONS_GAP))
            .children(stop)
            .children(send)
            .into_any_element(),
    )
}

/// What stands over either Composer: `SHELF_GAP` over the box it belongs
/// to, the shelf of pending files. The shelf keeps to its box's inset
/// (`boxed`: Solo's). The working line is the transcript's last row
/// (`working_tail`), not the stack's.
pub(super) fn composer_stack(attachments: Option<AnyElement>, boxed: bool) -> Div {
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
        .when_some(attachments, |stack, attachments| {
            let shelf = div()
                .debug_selector(|| "composer-shelf".into())
                .flex()
                .items_end()
                .min_w_0()
                .child(div().flex_1().min_w_0().child(attachments));
            stack.child(in_column(inset, boxed, shelf).pb(px(theme::SHELF_GAP)))
        })
}

/// One row of the Composer stack, `inset` in from the card's sides — and in
/// Solo (`column`) at most `COMPOSER_MAX_W`, the reading column and
/// `COMPOSER_OUTSET` either side, centred as the transcript is, so the box,
/// the queue and the shelf frame the messages they follow (Claude's). A
/// Group Pane's rows keep the card's whole width.
pub(super) fn in_column(inset: f32, column: bool, row: impl IntoElement) -> Div {
    div().flex().justify_center().min_w_0().px(px(inset)).child(
        div()
            .flex_1()
            .min_w_0()
            .when(column, |row| row.max_w(px(theme::COMPOSER_MAX_W)))
            .child(row),
    )
}

/// The queue strip (Claude's): its own card over the box at `R_CARD` on
/// `paint::RAISED`, a `QUEUE_CARD_H` row for each queued message in the
/// order they go — `queued` lists the latest first, so the strip reads
/// it upwards — `N messages queued` leading the first. Each row is its
/// message on one cut line in `INK_SUBTLE` (`Steering` after one Codex is
/// already folding into the running turn) and its own `actions` (send
/// now, edit, remove), the cockpit's, aimed at it by its id: `actions[i]`
/// are `queued[i]`'s. Past `QUEUE_ROWS_SHOWN` rows the card scrolls. None
/// with nothing queued. Snaps in and out (motion-measured: Claude's strip
/// leaves in one frame).
pub(super) fn queue_card(
    queued: &[QueueItem],
    namespace: &str,
    actions: Vec<Vec<AnyElement>>,
) -> Option<gpui::Stateful<Div>> {
    if queued.is_empty() {
        return None;
    }
    let count = queued.len();
    let mut actions = actions.into_iter();
    let rows: Vec<_> = queued
        .iter()
        .enumerate()
        .map(|(index, item)| (index, item, actions.next().unwrap_or_default()))
        .collect();
    let rows = rows
        .into_iter()
        .rev()
        .enumerate()
        .map(|(shown, (index, item, acts))| {
            let steering = matches!(item.delivery, Delivery::Steering | Delivery::Folding).then(|| {
                div()
                    .debug_selector(move || format!("queued-steering-{index}"))
                    .flex_shrink_0()
                    .text_color(rgb(INK_SUBTLE))
                    .child("Steering")
            });
            let count_label = (shown == 0).then(|| {
                components::tabular(
                    div()
                        .debug_selector(move || format!("queued-count-{count}"))
                        .flex_shrink_0()
                        .font_weight(theme::W_LABEL)
                        .text_color(rgb(INK_STRONG))
                        .child(queued_count(count)),
                )
            });
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(theme::QUEUED_GAP))
                .h(px(theme::QUEUE_CARD_H))
                .min_w_0()
                .pl(px(theme::QUEUED_PAD_X))
                .pr(px(
                    theme::QUEUED_PAD_X - (theme::QUEUED_ACTION - theme::QUEUED_ICON) / 2.
                ))
                .children(count_label)
                .child(
                    div()
                        .debug_selector(move || format!("queued-{index}"))
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(INK_SUBTLE))
                        .child(SharedString::from(queued_preview(&item.text))),
                )
                .children(steering)
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(theme::QUEUED_ACTION_GAP))
                        .children(acts),
                )
        });
    let selector = format!("composer-queue-{namespace}");
    Some(
        div()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .flex()
            .flex_col()
            .flex_shrink_0()
            .min_w_0()
            .max_h(px(queue_rows_shown(count) as f32 * theme::QUEUE_CARD_H))
            .overflow_y_scroll()
            .rounded(px(theme::R_CARD))
            .bg(theme::paint::RAISED)
            .font_family(theme::FONT_UI)
            .text_size(px(theme::FS_UI))
            .line_height(px(theme::LH_UI))
            .whitespace_nowrap()
            .children(rows),
    )
}

/// How many of `count` queued rows the strip shows before it scrolls.
fn queue_rows_shown(count: usize) -> usize {
    count.min(theme::QUEUE_ROWS_SHOWN)
}

/// Whether a queued prompt can still be edited or removed: not while the
/// provider has it unacknowledged (core can't cancel it by id yet), nor
/// once Codex is steering it into the running turn.
pub fn queued_takes_back(item: &QueueItem) -> bool {
    !matches!(item.delivery, Delivery::Steering | Delivery::Sending)
}

/// Whether a queued prompt can be sent now: not while the provider has it
/// unacknowledged, nor from a Server too old to say where it stands; one
/// already steering only while its turn runs (stopping that turn sends
/// what it has not yet taken).
pub fn queued_sends_now(item: &QueueItem, running: bool) -> bool {
    match item.delivery {
        Delivery::Holding | Delivery::Queued | Delivery::Folding => true,
        Delivery::Steering => running,
        Delivery::Sending | Delivery::Unknown => false,
    }
}

/// The strip's count: `1 message queued`, `3 messages queued`.
pub fn queued_count(count: usize) -> String {
    format!(
        "{count} message{} queued",
        if count == 1 { "" } else { "s" }
    )
}

/// A row's send now (`corner-down-left`): its message goes next
/// (`Cockpit::send_queued_now`). `index` is its place in the queue's
/// listing, as `queue_card`'s `actions` are.
pub fn queued_send_now(index: usize, cx: &gpui::App) -> gpui::component::button::Button {
    queued_action(
        "queued-send-now",
        index,
        icons::CORNER_DOWN_LEFT,
        "Send now",
        cx,
    )
}

/// A row's edit (`pencil`): its message back into the Composer, ahead of
/// any draft — ↑'s act on the latest.
pub fn queued_edit(index: usize, cx: &gpui::App) -> gpui::component::button::Button {
    queued_action("queued-edit", index, icons::PENCIL, "Edit", cx)
}

/// A row's remove (`close`): its message dropped — ⌫'s act on the latest.
pub fn queued_close(index: usize, cx: &gpui::App) -> gpui::component::button::Button {
    queued_action("queued-close", index, icons::CLOSE, "Remove", cx)
}

/// One of a row's actions: a `QUEUED_ACTION` square at `R_CONTROL`, its
/// icon in `INK_STRONG`, `paint::HOVER` under the pointer; selected as
/// `{id}-{index}`. The cockpit wires its click.
fn queued_action(
    id: &'static str,
    index: usize,
    path: &'static str,
    tip: &'static str,
    cx: &gpui::App,
) -> gpui::component::button::Button {
    chip_button((id, index), cx)
        .debug_selector(move || format!("{id}-{index}"))
        .p_0()
        .size(px(theme::QUEUED_ACTION))
        .flex_shrink_0()
        .rounded(px(theme::R_CONTROL))
        .tip(tip)
        .child(icon(path, theme::QUEUED_ICON, INK_STRONG))
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
/// description of one — with its `⇥ Tab` key cap, always kept whole: an
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
            format!("{} {ACCEPT_HINT}", self.head)
        } else {
            self.head.to_string()
        }
    }
}

/// The key cap after a verbatim prediction (T3's outline `Kbd`).
const ACCEPT_HINT: &str = "\u{21e5} Tab";

/// The line's words in every ordinary state (T3's one placeholder): it
/// names the three triggers.
const IDLE_PLACEHOLDER: &str = "Ask anything. @ files, $ skills, / commands";
/// Both Enter and ⌘Enter bind `Submit`, which while a turn runs joins that
/// turn (Claude folds it in at the next tool round, Codex steers it), so
/// the hint names the one thing a key does.
const RUNNING_PLACEHOLDER: &str = "\u{21b5} to steer this turn";

/// The placeholder for this line: while a turn runs, that ↵ steers it;
/// else the triggers, a draft's and an empty Thread's alike.
/// A landed prediction shows verbatim instead. A Decision or a dead
/// Session changes nothing here: the line still takes typing, and sending
/// revives.
pub(super) fn placeholder(
    pending: bool,
    draft: bool,
    transcript: Option<&Transcript>,
    suggestion: Option<&str>,
) -> Ghost {
    if draft || transcript.is_some_and(|transcript| transcript.blocks().is_empty()) {
        return Ghost::says(IDLE_PLACEHOLDER);
    }
    let busy = transcript.is_some_and(|transcript| transcript.status() == Status::Streaming);
    match followup::suggest(pending, transcript, suggestion) {
        Followup::Suggested(text) => Ghost {
            head: SharedString::from(text),
            accept: true,
        },
        Followup::Steer if busy => Ghost::says(RUNNING_PLACEHOLDER),
        Followup::Steer | Followup::Decision | Followup::Revive => Ghost::says(IDLE_PLACEHOLDER),
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

/// What the line says in grey over its text: the placeholder while it is
/// empty, and Claude's `Type to filter` after a lone `/` (the slash picker
/// is open on it), clear of the typed slash.
#[derive(IntoElement)]
struct LineGhost {
    composer: Entity<crate::composer::Composer>,
    ghost: Ghost,
    empty: bool,
}

/// The words after a lone `/`.
pub(super) const FILTER_HINT: &str = "Type to filter";

impl RenderOnce for LineGhost {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        if self.composer.read(cx).text() == "/" {
            let size = px(theme::FS_COMPOSER);
            let mut face = window.text_style();
            face.font_family = theme::FONT_UI.into();
            face.font_weight = theme::W_BODY;
            let slash = window
                .text_system()
                .shape_line("/".into(), size, &[face.to_run(1)], None)
                .width;
            return ghost_row(Ghost::says(FILTER_HINT), f32::from(slash) + theme::PLACEHOLDER_GAP)
                .into_any_element();
        }
        if self.empty {
            // Clear of the caret's bar, so it never touches `↵ to send`.
            return ghost_row(self.ghost, theme::PLACEHOLDER_GAP).into_any_element();
        }
        gpui::Empty.into_any_element()
    }
}

/// The placeholder drawn on a clipped row one line high, `left` in (Solo's
/// box: `PLACEHOLDER_GAP` past the caret; a board's band: two cells, after the caret's
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
            row.child(
                div()
                    .debug_selector(|| "prompt-placeholder-hint".into())
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .h(px(theme::COMPOSER_ROW_H))
                    .pl(px(theme::SPACE_2))
                    .child(outline_key_cap(ACCEPT_HINT)),
            )
        })
}

/// T3's outline key cap (`Kbd variant="outline"`): a physical key, so the
/// hint stays distinct beside the prediction's own words. 1px edges with a
/// 2px foot in `KEY_CAP_OUTLINE_EDGE`, no ground.
fn outline_key_cap(key: &'static str) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .min_w(px(theme::KEY_CAP_SOFT))
        .h(px(theme::KEY_CAP_SOFT))
        .px(px(theme::SPACE_1_5))
        .border_1()
        .border_b_2()
        .border_color(rgba(theme::KEY_CAP_OUTLINE_EDGE))
        .rounded(px(theme::KEY_CAP_SOFT_R))
        .text_size(px(theme::FS_BADGE))
        .font_weight(theme::W_LABEL)
        .text_color(rgb(INK_SUBTLE))
        .child(key)
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

/// The mode chip's and its menu's label (SPEC §5.5): the ask-first default
/// reads `Manual`, as Claude's app says it (the wire keeps `default`); else
/// the Session's own label for `mode` when its list has one
/// (`permission_modes`, already sentence case); else the mode's word
/// (`theme::mode_word`), sentence case. Never a raw id.
pub fn permission_mode_label(
    mode: &str,
    choices: &[ferrite_core::PermissionModeChoice],
) -> SharedString {
    if matches!(mode, "" | "default") {
        return "Manual".into();
    }
    if let Some(choice) = choices.iter().find(|choice| choice.value == mode) {
        return choice.label.clone().into();
    }
    theme::mode_word(mode).map_or_else(|| "Default".into(), |word| sentence_case(&word).into())
}

/// The queue's share of the editor's pane-height budget: Solo's strip —
/// the rows it shows — and its gap whenever anything is queued; nothing on
/// a board, whose compact box is input only.
pub(crate) fn composer_queue_height(_height: f32, grid: bool, count: usize) -> f32 {
    if grid || count == 0 {
        0.
    } else {
        queue_rows_shown(count) as f32 * theme::QUEUE_CARD_H + theme::QUEUE_CARD_GAP
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
        2.0 * (theme::COMPOSER_ROW_PAD + theme::COMPOSER_TEXT_PAD_Y) + theme::COMPOSER_FOOTER_H
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

/// A held prompt as one line: the queue strip is a single line and
/// gpui's truncation still breaks at `\n`, so a multi-line prompt would
/// stack its lines into the row and clip them both.
pub(super) fn queued_preview(held: &str) -> String {
    held.split_whitespace().collect::<Vec<_>>().join(" ")
}
