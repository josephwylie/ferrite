//! A Pane's cell, drawn by a view of its own (`CellView`) the window mounts
//! (ADR 0012): the click-to-focus, drop and drag plumbing around
//! `pane::render_pane` (or `render_draft`), and the controls wired into it.
//! It draws from its Pane, the client's model, and `CellInputs`: what it
//! reads of the window, through a `Source` the window owns
//! (`CockpitView::cell_inputs`). What a press asks of the window is a
//! `PaneEvent` naming the Pane, which its view emits and the window hears
//! (`CockpitView::on_pane_event`). Nothing here names the window, so the
//! cell builds and tests without it.

pub mod footer;
mod hero;

use ferrite_core::activity::Subject;
use ferrite_core::delegate::DelegateId;
use ferrite_core::roster::PaneIdentity;
use ferrite_core::server::ServerId;
use ferrite_client::cockpit::Cockpit;
use ferrite_core::{MachineId, SessionControl, ThreadId, ThreadKey};
use ferrite_ui::components::{FloatPlace, MenuDoor};
use ferrite_ui::source::Source;
use gpui::prelude::*;
use gpui::{
    div, AnyElement, AnyView, App, Context, Div, Entity, FocusHandle, Focusable, MouseButton,
    MouseDownEvent, Pixels, Point, SharedString, WeakEntity, Window,
};

use crate::composer::Composer;
use crate::details::content::Content;
use crate::pane::{self, PaneView};

/// What a press in a Pane's cell asks of the window, for the Pane it names.
#[derive(Clone, Debug)]
pub struct PaneEvent {
    pub pane: PaneIdentity,
    pub ask: PaneAsk,
}

/// What a cell's press asks: each with what the press saw, which the root
/// acts on.
#[derive(Clone, Debug)]
pub enum PaneAsk {
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
pub enum DetailsOpen {
    PullRequest(u64),
    Changes,
    SideChat(u64),
}

/// A queued prompt's three acts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueuedAct {
    SendNow,
    Edit,
    Remove,
}

/// What a cell asks the window: its Pane's inputs, at the window's size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellAsk {
    pub pane: PaneIdentity,
    pub viewport: gpui::Size<Pixels>,
}

/// What a Pane's cell draws of the window rather than of its Pane.
pub struct CellInputs {
    /// The Pane's place on the roster this frame: its elements' ids and
    /// selectors say it. A press names the Pane by `identity`.
    pub index: usize,
    pub identity: PaneIdentity,
    /// The Pane itself, as the window holds it under `identity`.
    pub view: Entity<PaneView>,
    pub focused: bool,
    /// The Pane's machine, while it is lost (ADR 0026).
    pub lost: Option<MachineId>,
    pub sheet_up: bool,
    /// Native files hover this Pane, while their drag is live.
    pub drop_target: bool,
    /// The Composer's `/` or `@` menu, while it is up over this Pane.
    pub composer_menu: Option<AnyView>,
    pub doors: Doors,
    /// What a Main Composer's controls read; none on a Subagent's tab.
    pub controls: Option<ControlsInputs>,
    pub kind: KindInputs,
}

/// What the box's controls read of the window.
pub struct ControlsInputs {
    /// The `+` menu's focus, while it is up.
    pub attach_focus: Option<FocusHandle>,
    /// Where the `+` menu stands: over the Composer once it is laid out,
    /// else over the window's bar.
    pub attach_place: FloatPlace,
    /// The plan-usage ring's popover is held open.
    pub usage_popover_held: bool,
    /// The operator's effort for the Pane's provider (Settings).
    pub effort_default: Option<String>,
}

/// The menus the cell's controls open, each as its door (ADR 0043):
/// whether it is up from that control, and where the control's bounds go
/// for it to hang from. Each control drawn takes its own.
#[derive(Default)]
pub struct Doors {
    /// A draft's band chips': Project, Workspace, where it runs.
    pub bands: Vec<(pane::BandChip, MenuDoor)>,
    /// An empty draft's hero word's: its Project menu.
    pub hero: Option<MenuDoor>,
    /// A Main Composer's model chip's: the model picker.
    pub model: Option<MenuDoor>,
    /// A Thread's mode chip's: the permission menu.
    pub mode: Option<MenuDoor>,
}

impl Doors {
    /// Band chip `chip`'s door.
    pub(crate) fn band(&mut self, chip: pane::BandChip) -> Option<MenuDoor> {
        let at = self.bands.iter().position(|(band, _)| *band == chip)?;
        Some(self.bands.swap_remove(at).1)
    }
}

/// A draft's or a Thread's own reads.
pub enum KindInputs {
    Draft(DraftInputs),
    /// A Thread's, once the model has it.
    Thread(Option<ThreadInputs>),
}

/// What a draft's band and hero read of the window.
pub struct DraftInputs {
    /// Its Project's title, `Project` until one is known.
    pub project: SharedString,
    /// The environment chip, while there is more than one environment to
    /// choose or a copy to wait on.
    pub environment: Option<(pane::ChipMark, SharedString)>,
    /// Where it runs as a word, while no chip says: a machine's name.
    pub machine: Option<String>,
    /// The copy it waits on: whether that still runs.
    pub copy_live: Option<bool>,
    /// The mode a new Thread starts in (Settings).
    pub mode: String,
}

/// What a Thread's cell reads of the window.
pub struct ThreadInputs {
    /// The Pane is the window's plane, not drawn by the Factory.
    pub plane: bool,
    /// Its Project's and branch's words, once its facts are known.
    pub facts: Option<(Option<SharedString>, Option<SharedString>)>,
    /// The Decision cards on its plane, as the window draws them.
    pub decisions: Option<DecisionsDraw>,
    /// Its details card stands beside the transcript when there is room
    /// (`ThreadSurfaces::details_inline_open`).
    pub details_inline_open: bool,
    /// The details card's menu is up, choosing what the card shows.
    pub details_menu_up: bool,
}

/// The window's draw of a Thread's Decision cards, run where the cell
/// stacks them.
pub type DecisionsDraw = Box<dyn FnOnce(&mut Window, &mut App) -> Option<AnyElement>>;

/// A board slot on the move: its head is the handle. Dropped on another
/// slot of the same board it swaps with it (the centre) or splits its
/// slot (an edge). `leaf` is the slot's leaf in the board's tree.
#[derive(Clone, Copy, Debug)]
pub struct PaneDrag {
    pub leaf: ThreadId,
}

/// The key the start hero's project word records its bounds under, as a
/// float trigger (`draft_hero`).
pub const HERO_TRIGGER: &str = "start-hero-project";

/// The key a draft's band chip records its bounds under, for the menu it
/// opens.
pub fn band_trigger(chip: pane::BandChip) -> SharedString {
    format!("band-{chip:?}").into()
}

/// The key a model chip records its bounds under, where the picker hangs.
pub fn model_chip_trigger(pane: PaneIdentity) -> SharedString {
    format!("model-chip-{pane:?}").into()
}

/// The key a Thread's mode chip records its bounds under.
pub fn mode_chip_trigger(thread: ThreadKey) -> SharedString {
    format!("mode-chip-{}", thread.thread().get()).into()
}

/// What the details card asks of the window: its sections for the Thread,
/// the Subject its Pane shows, and how many pages of `Previous agents`.
pub type DetailsAsk = (ThreadKey, Subject, usize);

/// The view that draws one Pane's cell, from what it reads of the window.
pub struct CellView {
    pane: PaneIdentity,
    /// The client's model, which the cell reads as it draws.
    model: Entity<Cockpit>,
    /// What the cell draws of the window (`CockpitView::cell_inputs`).
    inputs: Source<CellInputs, CellAsk>,
    /// The details card's sections (`CockpitView::details_content`), read
    /// only while the card stands.
    details: Source<Content, DetailsAsk>,
}

impl CellView {
    /// The cell of Pane `pane`, reading the window through `inputs` and
    /// the details card's sections through `details`.
    pub fn new(
        pane: PaneIdentity,
        model: Entity<Cockpit>,
        inputs: Source<CellInputs, CellAsk>,
        details: Source<Content, DetailsAsk>,
    ) -> Self {
        Self {
            pane,
            model,
            inputs,
            details,
        }
    }
}

impl Render for CellView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // This part's loops are laid out afresh for the loops overlay.
        ferrite_ui::loops_overlay::begin(window, cx.entity_id());
        let ask = CellAsk {
            pane: self.pane,
            viewport: window.viewport_size(),
        };
        let Some(inputs) = self.inputs.read(&ask, cx) else {
            return div().into_any_element();
        };
        let asks = PaneAsks::new(cx.entity().downgrade(), self.pane);
        let details = |thread, subject: &Subject, pages, cx: &gpui::App| {
            self.details.read(&(thread, subject.clone(), pages), cx)
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(cell(inputs, &self.model, &details, &asks, window, cx))
            .into_any_element()
    }
}

impl gpui::EventEmitter<PaneEvent> for CellView {}

/// Where a cell's presses go: its view, which emits each as a `PaneEvent`
/// naming the Pane.
#[derive(Clone)]
pub(crate) struct PaneAsks {
    part: WeakEntity<CellView>,
    pane: PaneIdentity,
}

impl PaneAsks {
    fn new(part: WeakEntity<CellView>, pane: PaneIdentity) -> Self {
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
fn cell(
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
        Some(machine) => held(cell, format!("machine-lost-{}", machine.get())),
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
    #[cfg(any(test, feature = "test-support"))]
    testing::built_pane(inputs.identity);
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
    crate::prompt_drop::target(
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
                .and_then(|door| hero::draft_hero(&draft.project, pane, door, asks, cx))
                .filter(|_| !sheet_up);
            let toolbar = controls.as_ref().map(|controls| {
                footer::toolbar(
                    index,
                    identity,
                    controls,
                    &mut doors,
                    footer::Kind::Draft(&draft),
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
    let details_card = crate::details::card(
        thread,
        pane,
        (inputs.details_inline_open, inputs.details_menu_up),
        details,
        asks,
        cx,
    );
    let toolbar = controls.as_ref().map(|controls| {
        footer::toolbar(
            index,
            identity,
            controls,
            &mut doors,
            footer::Kind::Thread(thread, &inputs),
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
    #[cfg(any(test, feature = "test-support"))]
    testing::record_pane(
        identity,
        testing::Drawn {
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
        details_lane_width: crate::details::lane_width(&pane.read(cx).details, inputs.details_inline_open),
        toolbar,
        resume_offer,
        // An empty Thread wears the draft's start card until its first
        // prompt.
        start: if pane.read(cx).is_main() && open.transcript().blocks().is_empty() {
            hero::thread_hero(inputs.facts.as_ref()).filter(|_| !sheet_up)
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

/// What a lost machine still shows (ADR 0026): `element` dimmed under a
/// cover, `id`, that takes every press until the machine is back.
fn held(element: Div, id: impl Into<SharedString>) -> Div {
    let id: SharedString = id.into();
    let selector = id.clone();
    element
        .relative()
        .opacity(ferrite_ui::theme::MACHINE_LOST_OPACITY)
        .child(
            div()
                .id(gpui::ElementId::Name(id))
                .debug_selector(move || selector.to_string())
                .absolute()
                .inset_0()
                .occlude(),
        )
}

/// What each Pane's cell last drew and how often each was built: the
/// window's stale-chrome tests read what is on screen, not the model, and
/// its frame budgets count the work each cell did.
#[cfg(any(test, feature = "test-support"))]
pub mod testing {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use ferrite_core::roster::PaneIdentity;

    /// What a Pane's cell drew of itself.
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct Drawn {
        pub focused: bool,
        pub unread: bool,
        pub drop_target: bool,
        pub editing: bool,
        pub reduce_motion: bool,
        /// The working clock its lines read (`12s`).
        pub clock: Option<String>,
    }

    thread_local! {
        static PANES: RefCell<HashMap<PaneIdentity, Drawn>> = RefCell::new(HashMap::new());
        static PANE_RENDERS: RefCell<HashMap<PaneIdentity, usize>> = RefCell::new(HashMap::new());
    }

    pub(crate) fn built_pane(identity: PaneIdentity) {
        PANE_RENDERS.with(|renders| *renders.borrow_mut().entry(identity).or_default() += 1);
    }

    pub(crate) fn record_pane(identity: PaneIdentity, drawn: Drawn) {
        PANES.with(|panes| panes.borrow_mut().insert(identity, drawn));
    }

    /// Pane `identity`'s cell as last drawn, once it has been.
    pub fn drawn(identity: PaneIdentity) -> Option<Drawn> {
        PANES.with(|panes| panes.borrow().get(&identity).cloned())
    }

    /// How many times this Pane's cell has been built on this thread.
    pub fn pane_renders(identity: PaneIdentity) -> usize {
        PANE_RENDERS.with(|renders| renders.borrow().get(&identity).copied().unwrap_or(0))
    }
}
