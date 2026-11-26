//! A Pane's cell, as its part (`parts::PartView`) draws it, in the part's
//! own context: the click-to-focus, drop and drag plumbing around
//! `pane::render_pane` (or `render_draft`), and the controls wired into it.
//! It draws from its Pane, the client's model, and `CellInputs`: what it
//! reads of the window, read through `Source` on the root
//! (`CockpitView::cell_inputs`). What a press asks of the window is a
//! `PaneEvent` naming the Pane, which its part emits and the root hears
//! (`CockpitView::on_pane_event`).

use super::details::content::Content;
use super::parts::PartView;
use super::*;
use ferrite_pane::cell::{CellInputs, KindInputs, PaneAsk, PaneDrag, PaneEvent};
use ferrite_core::activity::Subject;
use gpui::{App, WeakEntity};

/// Where a cell's presses go: its part, which emits each as a `PaneEvent`
/// naming the Pane.
#[derive(Clone)]
pub(crate) struct PaneAsks {
    part: WeakEntity<PartView>,
    pane: PaneIdentity,
}

impl PaneAsks {
    pub(crate) fn new(part: WeakEntity<PartView>, pane: PaneIdentity) -> Self {
        Self { part, pane }
    }

    /// Ask the window `ask`, for this Pane.
    pub(crate) fn ask(&self, ask: PaneAsk, cx: &mut App) {
        let event = PaneEvent {
            pane: self.pane,
            ask,
        };
        let _ = self.part.update(cx, |_, cx| cx.emit(event));
    }

    /// A listener that asks what `ask` makes of its event, if anything.
    pub(crate) fn on<E: ?Sized + 'static>(
        &self,
        ask: impl Fn(&E, &mut Window, &mut App) -> Option<PaneAsk> + 'static,
    ) -> impl Fn(&E, &mut Window, &mut App) + 'static {
        let asks = self.clone();
        move |event, window, cx| {
            if let Some(asked) = ask(event, window, cx) {
                asks.ask(asked, cx);
            }
        }
    }

    /// A press that stops where it is and asks `ask`.
    pub(crate) fn press<E: ?Sized + 'static>(&self, ask: PaneAsk) -> impl Fn(&E, &mut Window, &mut App) + 'static {
        self.on(move |_: &E, _, cx| {
            cx.stop_propagation();
            Some(ask.clone())
        })
    }
}

/// What the details card's sections read of the window, asked only while
/// the card stands: the Thread, the Subject its Pane shows, and how many
/// pages of `Previous agents` it shows.
pub(crate) type DetailsRead<'a> = dyn Fn(ThreadKey, &Subject, usize, &gpui::App) -> Option<Content> + 'a;

/// The Pane's cell. While its machine is lost it is dimmed under a cover
/// that takes every press until the machine is back (ADR 0026).
pub(super) fn cell(
    inputs: CellInputs,
    model: &Entity<Cockpit>,
    details: &DetailsRead,
    asks: &PaneAsks,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let lost = inputs.lost;
    let pane = inputs.view.clone();
    let cell = live_cell(inputs, &pane, model, details, asks, window, cx);
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
    asks: &PaneAsks,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    #[cfg(test)]
    drawn::built_pane(inputs.identity);
    let sheet_up = inputs.sheet_up;
    let content = content(inputs, pane, model, details, asks, window, cx);
    if sheet_up {
        return content;
    }
    if !pane.read(cx).is_main() {
        return content;
    }
    let composer = pane.read(cx).composer.clone();
    let (hovered, dropped) = (asks.clone(), asks.clone());
    ferrite_pane::prompt_drop::target(
        content,
        composer,
        move |_, cx| dropped.ask(PaneAsk::FilesDropped, cx),
        move |over: bool, _: &mut Window, cx: &mut App| hovered.ask(PaneAsk::FilesOver(over), cx),
    )
}

fn content(
    inputs: CellInputs,
    pane: &Entity<PaneView>,
    model: &Entity<Cockpit>,
    details: &DetailsRead,
    asks: &PaneAsks,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let CellInputs {
        index,
        identity,
        focused,
        sheet_up,
        drop_target,
        composer_menu,
        mut doors,
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
        .on_mouse_down(MouseButton::Left, asks.on(|_: &MouseDownEvent, _, _| Some(PaneAsk::Pressed)))
        .on_mouse_down(
            MouseButton::Right,
            asks.on(move |event: &MouseDownEvent, window, cx| {
                // A draft has no context menu.
                identity.thread()?;
                cx.stop_propagation();
                let copied = gpui::base::TextSelection::selected_text(window, cx)
                    .trim_end_matches('\n')
                    .to_string();
                Some(PaneAsk::ContextMenu {
                    at: event.position,
                    copied: (!copied.is_empty()).then_some(copied),
                })
            }),
        );
    let cell = slot_drop_target(cell, asks);
    let menu = composer_menu.map(IntoElement::into_any_element);
    let thread_inputs = match kind {
        // A draft Pane (#29): the Composer and its menu instead of a
        // transcript — nothing in core exists to read yet.
        KindInputs::Draft(draft) => {
            // A Pane under the sheets shows no hero: the veil dims what is
            // under it, and the hero's words, as wide as the board and
            // centred on it, would read past the edge of any card narrower
            // than them.
            let start = doors
                .hero
                .take()
                .and_then(|door| empty_board::draft_hero(&draft.project, pane, door, asks, cx))
                .filter(|_| !sheet_up);
            let toolbar = controls.as_ref().map(|controls| {
                composer_footer::toolbar(
                    index,
                    identity,
                    controls,
                    &mut doors,
                    composer_footer::Kind::Draft(&draft),
                    pane,
                    model,
                    asks,
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
    let Some(mut inputs) = thread_inputs else {
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
    let activity_decisions = inputs.decisions.take().and_then(|draw| draw(window, cx));
    let details_card = details::card(
        thread,
        pane,
        (inputs.details_inline_open, inputs.details_menu_up),
        details,
        asks,
        cx,
    );
    let toolbar = controls.as_ref().map(|controls| {
        composer_footer::toolbar(
            index,
            identity,
            controls,
            &mut doors,
            composer_footer::Kind::Thread(thread, &inputs),
            pane,
            model,
            asks,
            cx,
        )
    });
    let resume_offer = resume_offer(thread, pane, model, asks, cx);
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
        drawn::Drawn {
            focused,
            unread: facts.unread,
            drop_target: facts.drop_target,
            editing: facts.editing,
            reduce_motion: facts.reduce_motion,
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
    thread: ThreadKey,
    pane: &Entity<PaneView>,
    model: &Entity<Cockpit>,
    asks: &PaneAsks,
    cx: &mut App,
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
    .on_mouse_down(MouseButton::Left, asks.press(PaneAsk::StartFresh));
    Some(pane::resume_offer(key, line.into(), action))
}

/// The cell as a drop target for a board slot dragged by its head: the
/// moves over it ask for the preview, the release for the drop.
fn slot_drop_target(cell: Div, asks: &PaneAsks) -> Div {
    cell.on_drag_move(asks.on(|event: &gpui::DragMoveEvent<PaneDrag>, _, cx| {
        Some(PaneAsk::SlotDragged {
            source: event.drag(cx).leaf,
            at: event.event.position,
            bounds: event.bounds,
        })
    }))
    .on_drop(asks.on(|drag: &PaneDrag, _, _| Some(PaneAsk::SlotDropped { source: drag.leaf })))
}
