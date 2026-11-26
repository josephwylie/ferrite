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
use ferrite_core::activity::Subject;
use ferrite_core::delegate::DelegateId;
use ferrite_core::server::ServerId;
use ferrite_core::SessionControl;
use ferrite_ui::components::{FloatPlace, MenuDoor};
use gpui::{App, WeakEntity};

/// What a press in a Pane's cell asks of the window, for the Pane it names.
#[derive(Clone, Debug)]
pub(crate) struct PaneEvent {
    pub(crate) pane: PaneIdentity,
    pub(crate) ask: PaneAsk,
}

/// What a cell's press asks: each with what the press saw, which the root
/// acts on.
#[derive(Clone, Debug)]
pub(crate) enum PaneAsk {
    /// A left press anywhere on the cell: the Pane takes the focus.
    Pressed,
    /// A right press on a Thread's cell, at `at`: its context menu, with
    /// the window's text selection as it stood, for the menu's Copy.
    ContextMenu { at: Point<Pixels>, copied: Option<String> },
    /// Native files dragged onto the Pane's Composer (`true`) or off it.
    FilesOver(bool),
    /// Native files dropped on the Pane's Composer, which staged them.
    FilesDropped,
    /// A board slot (`source`, its leaf) dragged over this Pane's, the
    /// pointer at `at`; `bounds` the cell's.
    SlotDragged {
        source: ThreadId,
        at: Point<Pixels>,
        bounds: gpui::Bounds<Pixels>,
    },
    /// A dragged slot (`source`) let go over this Pane's.
    SlotDropped { source: ThreadId },
    /// The resume offer's Start a fresh conversation.
    StartFresh,
    /// A draft's band chip, or its hero's Project word (`from_hero`): its
    /// menu.
    BandMenu { chip: pane::BandChip, from_hero: bool },
    /// A queued prompt's act, the prompt named by its id.
    Queued { id: String, act: QueuedAct },
    /// The send circle: Enter's act.
    Send,
    /// Stop: Esc's act.
    Stop,
    /// The `+` menu opens (`true`) or shuts.
    AttachMenu(bool),
    /// The `+` menu's row `at`: files, a folder, or `/`.
    AttachPick(usize),
    /// A Connectors row: its Session control, run on the Session of
    /// `generation`, or none, which opens the Panel.
    Connector {
        generation: u64,
        act: Option<SessionControl>,
    },
    /// A plugin's command, started on the line as a chip.
    PluginCommand(String),
    /// The permission mode chip, on the Session of `generation`.
    ModeMenu { generation: u64 },
    /// The model chip: the picker opens, or shuts.
    ModelPicker,
    /// The details card's context menu, at `at`.
    DetailsMenu { at: Point<Pixels> },
    /// A press outside the details card standing over the transcript,
    /// neither on its opener nor in its menu: the card shuts.
    CloseDetails,
    /// A details row: what it opens in the Panel.
    Surface(DetailsOpen),
    /// The details card's Create PR.
    CreatePr,
    /// The watched PR's eye: stop watching `url`.
    StopWatching { url: String },
    /// Lineage's `Previous agents` fold.
    PreviousAgents,
    /// Lineage's `Show more` previous agents.
    MorePreviousAgents,
    /// A Lineage row for a Subject in this Pane: its parent's selects it
    /// (`parent`), an agent's opens it.
    Subject { subject: Subject, parent: bool },
    /// A Lineage row for a Thread elsewhere: the Delegate's, or the
    /// Thread that delegated this one (`delegator`).
    Elsewhere {
        server: ServerId,
        thread: ThreadId,
        delegator: bool,
    },
    /// An open Delegate's stop.
    CancelDelegate(DelegateId),
}

/// What a details row opens in the Panel: the pull request, the Changes,
/// or a side chat by its tab's id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DetailsOpen {
    PullRequest(u64),
    Changes,
    SideChat(u64),
}

/// A queued prompt's three acts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueuedAct {
    SendNow,
    Edit,
    Remove,
}

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
    /// The Pane itself, as the window holds it under `identity`.
    pub(crate) view: Entity<PaneView>,
    pub(crate) focused: bool,
    /// The Pane's machine, while it is lost (ADR 0026).
    pub(crate) lost: Option<MachineId>,
    pub(crate) sheet_up: bool,
    /// Native files hover this Pane, while their drag is live.
    pub(crate) drop_target: bool,
    /// The Composer's `/` or `@` menu, while it is up over this Pane.
    pub(crate) composer_menu: Option<gpui::AnyView>,
    pub(crate) doors: Doors,
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
    /// The operator's effort for the Pane's provider (Settings).
    pub(crate) effort_default: Option<String>,
}

/// The menus the cell's controls open, each as its door (ADR 0043):
/// whether it is up from that control, and where the control's bounds go
/// for it to hang from. Each control drawn takes its own.
#[derive(Default)]
pub(crate) struct Doors {
    /// A draft's band chips': Project, Workspace, where it runs.
    pub(crate) bands: Vec<(pane::BandChip, MenuDoor)>,
    /// An empty draft's hero word's: its Project menu.
    pub(crate) hero: Option<MenuDoor>,
    /// A Main Composer's model chip's: the model picker.
    pub(crate) model: Option<MenuDoor>,
    /// A Thread's mode chip's: the permission menu.
    pub(crate) mode: Option<MenuDoor>,
}

impl Doors {
    /// Band chip `chip`'s door.
    pub(crate) fn band(&mut self, chip: pane::BandChip) -> Option<MenuDoor> {
        let at = self.bands.iter().position(|(band, _)| *band == chip)?;
        Some(self.bands.swap_remove(at).1)
    }
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
    /// Its Project's and branch's words, once its facts are known.
    pub(crate) facts: Option<(Option<SharedString>, Option<SharedString>)>,
    /// The Decision cards on its plane, as the window draws them.
    pub(crate) decisions: Option<DecisionsDraw>,
    /// Its details card stands beside the transcript when there is room
    /// (`ThreadSurfaces::details_inline_open`).
    pub(crate) details_inline_open: bool,
    /// The details card's menu is up, choosing what the card shows.
    pub(crate) details_menu_up: bool,
}

/// The window's draw of a Thread's Decision cards, run where the cell
/// stacks them.
pub(crate) type DecisionsDraw = Box<dyn FnOnce(&mut Window, &mut App) -> Option<AnyElement>>;

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
