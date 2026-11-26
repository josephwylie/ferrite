//! A Pane's cell, as its part (`parts::PartView`) draws it: the
//! click-to-focus, drop and drag plumbing around `pane::render_pane` (or
//! `render_draft`), and the controls wired into it. It draws from its Pane,
//! the client's model, and `CellInputs`: what it reads of the window, read
//! through `Source` on the root (`CockpitView::cell_inputs`).

use super::details::content::Content;
use super::subagents::Cards;
use super::*;
use ferrite_core::activity::Subject;
use ferrite_ui::components::FloatPlace;

/// What a cell asks the window: its Pane's inputs, at the window's size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CellAsk {
    pub(crate) pane: PaneIdentity,
    pub(crate) viewport: gpui::Size<Pixels>,
}

/// What a Pane's cell draws of the window rather than of its Pane.
pub(crate) struct CellInputs {
    /// The Pane's place on the roster this frame: its elements' ids and
    /// selectors say it. A press names the Pane by `identity`.
    pub(crate) index: usize,
    pub(crate) identity: PaneIdentity,
    pub(crate) focused: bool,
    /// The Pane's machine, while it is lost (ADR 0026).
    pub(crate) lost: Option<MachineId>,
    pub(crate) sheet_up: bool,
    /// Native files hover this Pane, while their drag is live.
    pub(crate) drop_target: bool,
    /// The Composer's `/` or `@` menu, while it is up over this Pane.
    pub(crate) composer_menu: Option<Entity<composer_menu::ComposerMenuView>>,
    /// Where the controls a popup hangs from record their bounds.
    pub(crate) triggers: popups::Triggers,
    /// What a Main Composer's controls read; none on a Subagent's tab.
    pub(crate) controls: Option<ControlsInputs>,
    pub(crate) kind: KindInputs,
}

/// What the box's controls read of the window.
pub(crate) struct ControlsInputs {
    /// The `+` menu's focus, while it is up.
    pub(crate) attach_focus: Option<FocusHandle>,
    /// Where the `+` menu stands: over the Composer once it is laid out,
    /// else over the window's bar.
    pub(crate) attach_place: FloatPlace,
    /// The plan-usage ring's popover is held open.
    pub(crate) usage_popover_held: bool,
    /// The model picker is up from this Pane's chip.
    pub(crate) picker_up: bool,
    /// The operator's effort for the Pane's provider (Settings).
    pub(crate) effort_default: Option<String>,
}

/// A draft's or a Thread's own reads.
pub(crate) enum KindInputs {
    Draft(DraftInputs),
    /// A Thread's, once the model has it.
    Thread(Option<ThreadInputs>),
}

/// What a draft's band and hero read of the window.
pub(crate) struct DraftInputs {
    /// Its Project's title, `Project` until one is known.
    pub(crate) project: SharedString,
    /// The environment chip, while there is more than one environment to
    /// choose or a copy to wait on.
    pub(crate) environment: Option<(pane::ChipMark, SharedString)>,
    /// Where it runs as a word, while no chip says: a machine's name.
    pub(crate) machine: Option<String>,
    /// The copy it waits on: whether that still runs.
    pub(crate) copy_live: Option<bool>,
    /// The mode a new Thread starts in (Settings).
    pub(crate) mode: String,
}

/// What a Thread's cell reads of the window.
pub(crate) struct ThreadInputs {
    /// The Pane is the window's plane, not drawn by the Factory.
    pub(crate) plane: bool,
    /// The permission menu is up from its chip.
    pub(crate) mode_menu_up: bool,
    /// Its Project's and branch's words, once its facts are known.
    pub(crate) facts: Option<(Option<SharedString>, Option<SharedString>)>,
    /// The Decision cards on its plane.
    pub(crate) decisions: Option<Cards>,
    /// Its details card stands beside the transcript when there is room
    /// (`ThreadSurfaces::details_inline_open`).
    pub(crate) details_inline_open: bool,
    /// The slot's width as the board laid it out, for a test to read.
    #[cfg(test)]
    pub(crate) cell_width: f32,
}

/// What the details card's sections read of the window, asked only while
/// the card stands: the Thread, the Subject its Pane shows, and how many
/// pages of `Previous agents` it shows.
pub(crate) type DetailsRead<'a> = dyn Fn(ThreadKey, &Subject, usize, &gpui::App) -> Option<Content> + 'a;

/// The Pane's cell. While its machine is lost it is dimmed under a cover
/// that takes every press until the machine is back (ADR 0026).
pub(super) fn cell(
    inputs: CellInputs,
    pane: &Entity<PaneView>,
    model: &Entity<Cockpit>,
    details: &DetailsRead,
    window: &mut Window,
    cx: &mut Context<CockpitView>,
) -> Div {
    let lost = inputs.lost;
    let cell = live_cell(inputs, pane, model, details, window, cx);
    match lost {
        Some(machine) => machines::held(cell, format!("machine-lost-{}", machine.get())),
        None => cell,
    }
}

fn live_cell(
    inputs: CellInputs,
    pane: &Entity<PaneView>,
    model: &Entity<Cockpit>,
    details: &DetailsRead,
    window: &mut Window,
    cx: &mut Context<CockpitView>,
) -> Div {
    #[cfg(test)]
    drawn::built_pane(inputs.identity);
    let (identity, sheet_up) = (inputs.identity, inputs.sheet_up);
    let content = content(inputs, pane, model, details, window, cx);
    if sheet_up {
        return content;
    }
    if !pane.read(cx).is_main() {
        return content;
    }
    let composer = pane.read(cx).composer.clone();
    let view = cx.entity().downgrade();
    let hovered = view.clone();
    let hover = move |over: bool, _: &mut Window, cx: &mut gpui::App| {
        let _ = hovered.update(cx, |view, cx| {
            let next = match (over, view.file_drop_over) {
                (true, _) => Some(identity),
                (false, Some(current)) if current == identity => None,
                (false, current) => current,
            };
            if view.file_drop_over != next {
                view.file_drop_over = next;
                cx.notify();
            }
        });
    };
    ferrite_pane::prompt_drop::target(
        content,
        composer,
        move |_, cx| {
            let _ = view.update(cx, |view, cx| {
                view.file_drop_over = None;
                if let Some(index) = view.index_of(identity) {
                    view.focus_pane(index, cx);
                    view.panes[index].update_draft(cx, |draft| draft.band_focus = None);
                    view.close_composer_menu(cx);
                    cx.notify();
                }
            });
        },
        hover,
    )
}

fn content(
    inputs: CellInputs,
    pane: &Entity<PaneView>,
    model: &Entity<Cockpit>,
    details: &DetailsRead,
    window: &mut Window,
    cx: &mut Context<CockpitView>,
) -> Div {
    let CellInputs {
        index,
        identity,
        focused,
        sheet_up,
        drop_target,
        composer_menu,
        triggers,
        controls,
        kind,
        ..
    } = inputs;
    let cell = div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .track_focus(&pane.read(cx).controls_focus)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                view.pointer_down(index, event, window, cx)
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                let Some(thread) = view.panes.get(index).and_then(|pane| pane.thread()) else {
                    return;
                };
                cx.stop_propagation();
                let copied = gpui::base::TextSelection::selected_text(window, cx)
                    .trim_end_matches('\n')
                    .to_string();
                view.native_copy = (!copied.is_empty()).then_some(copied);
                view.focus_pane(index, cx);
                view.open_context_menu(MenuTarget::Pane(thread), event.position, cx);
            }),
        );
    let cell = CockpitView::slot_drop_target(cell, pane_leaf(identity), cx);
    let menu = composer_menu.map(IntoElement::into_any_element);
    let thread_inputs = match kind {
        // A draft Pane (#29): the Composer and its menu instead of a
        // transcript — nothing in core exists to read yet.
        KindInputs::Draft(draft) => {
            // A Pane under the sheets shows no hero: the veil dims what is
            // under it, and the hero's words, as wide as the board and
            // centred on it, would read past the edge of any card narrower
            // than them.
            let start = empty_board::draft_hero(&draft.project, pane, &triggers, cx)
                .filter(|_| !sheet_up);
            let toolbar = controls.as_ref().map(|controls| {
                composer_footer::toolbar(
                    index,
                    identity,
                    controls,
                    &triggers,
                    composer_footer::Kind::Draft(&draft),
                    pane,
                    model,
                    cx,
                )
            });
            // The parts above draw with `cx`; the Pane is read after.
            let view = pane.read(cx);
            let draft = view.draft().expect("a Pane is a Thread or a draft");
            return cell.child(pane::render_draft(
                view,
                pane::DraftState {
                    start,
                    attachments: Composer::attachments(&view.composer, &view.preview, cx),
                    menu,
                    composer_empty: view.composer.read(cx).is_empty(),
                    focused,
                    error: draft.error.as_ref(),
                    reduce_motion: cx.reduce_motion(),
                    drop_target,
                    toolbar,
                },
            ));
        }
        KindInputs::Thread(thread) => thread,
    };
    let thread = identity.thread().expect("a Thread's inputs are a Thread's Pane's");
    // One Pane is drawn at a time: this names the Thread the window
    // shows, for a driver to read.
    let cell = cell.debug_selector(move || format!("view-thread-{}", thread.thread().get()));
    // Over a link, the roster can list a Thread a frame before its facts
    // land: the cell stays an empty shell until they do.
    let Some(inputs) = thread_inputs else {
        return cell;
    };
    let retained_transcript = pane.read(cx).transcript().map(|transcript| {
        let document = transcript.read(cx).selection_document();
        gpui::base::TextSelectionDocumentOwner::new(document)
            .child(
                transcript.cached(
                    gpui::StyleRefinement::default()
                        .flex_1()
                        .min_h_0()
                        .min_w_0()
                        .w_full(),
                ),
            )
            .into_any_element()
    });
    let attachments = Composer::attachments(&pane.read(cx).composer, &pane.read(cx).preview, cx);
    // The docked Decision merges into the Composer when that Composer is a
    // live block with nothing floating between them (rule 2.8.1).
    let joins = pane.read(cx).is_main() && attachments.is_none();
    let activity_decisions = inputs
        .decisions
        .as_ref()
        .and_then(|cards| cards.render(window, cx));
    let details_card = details::card(thread, pane, inputs.details_inline_open, details, cx);
    let toolbar = controls.as_ref().map(|controls| {
        composer_footer::toolbar(
            index,
            identity,
            controls,
            &triggers,
            composer_footer::Kind::Thread(thread, &inputs),
            pane,
            model,
            cx,
        )
    });
    let resume_offer = resume_offer(index, thread, pane, model, cx);
    // The parts above draw with `cx`; the facts read the model after.
    let Some(open) = model.read(cx).thread(thread) else {
        return cell;
    };
    let facts = pane::PaneFacts {
        composer_empty: pane.read(cx).composer.read(cx).is_empty(),
        focused,
        unread: model
            .read(cx)
            .notifications(thread.machine())
            .attention(thread.thread()),
        reduce_motion: cx.reduce_motion(),
        editing: focused
            && window.is_window_active()
            && pane.read(cx).composer.read(cx).focus_handle(cx).is_focused(window),
        drop_target,
        plane: inputs.plane,
        decision_joined: joins && activity_decisions.is_some(),
        thread: open,
    };
    #[cfg(test)]
    drawn::record_pane(
        identity,
        drawn::Pane {
            focused,
            unread: facts.unread,
            drop_target: facts.drop_target,
            editing: facts.editing,
            reduce_motion: facts.reduce_motion,
            cell_width: inputs.cell_width,
            clock: open.turn().elapsed().map(ferrite_core::progress::live_seconds),
        },
    );
    let wiring = pane::PaneWiring {
        transcript: retained_transcript,
        attachments,
        menu,
        activity_decisions,
        details: details_card,
        details_lane_width: details::lane_width(&pane.read(cx).details, inputs.details_inline_open),
        toolbar,
        resume_offer,
        // An empty Thread wears the draft's start card until its first
        // prompt.
        start: if pane.read(cx).is_main() && open.transcript().blocks().is_empty() {
            empty_board::thread_hero(inputs.facts.as_ref()).filter(|_| !sheet_up)
        } else {
            None
        },
    };
    cell.child(pane::render_pane(pane.read(cx), facts, wiring))
}

/// The offer to go on in a fresh conversation, while the Thread's provider
/// has none to resume. It says what the way on costs.
fn resume_offer(
    index: usize,
    thread: ThreadKey,
    pane: &Entity<PaneView>,
    model: &Entity<Cockpit>,
    cx: &mut Context<CockpitView>,
) -> Option<AnyElement> {
    if !pane.read(cx).is_main() {
        return None;
    }
    model.read(cx).thread(thread).filter(|open| open.resume_lost())?;
    let key = thread.thread().get();
    let line = "A fresh conversation starts from the transcript's latest turns, sent as \
                context with your next prompt. It will not remember the rest.";
    let action = ferrite_ui::components::secondary_button(
        SharedString::from(format!("resume-fresh-{key}")),
        None,
        "Start a fresh conversation",
        cx,
    )
    .debug_selector(move || format!("resume-fresh-{key}"))
    .on_mouse_down(
        MouseButton::Left,
        cx.listener(move |view, _: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            view.start_fresh_conversation(index, thread, window, cx);
        }),
    );
    Some(pane::resume_offer(key, line.into(), action))
}
