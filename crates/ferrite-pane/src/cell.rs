//! A Pane's cell as the window mounts it: what its draw reads of the
//! window (`CellInputs`, read through a `Source` the window owns) and what
//! a press in it asks of the window (`PaneEvent`, naming the Pane), with
//! the keys the menus its controls open hang from.

pub mod footer;

use ferrite_core::activity::Subject;
use ferrite_core::delegate::DelegateId;
use ferrite_core::roster::PaneIdentity;
use ferrite_core::server::ServerId;
use ferrite_core::{MachineId, SessionControl, ThreadId, ThreadKey};
use ferrite_ui::components::{FloatPlace, MenuDoor};
use gpui::{AnyElement, AnyView, App, Entity, FocusHandle, Pixels, Point, SharedString, Window};

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
    pub fn band(&mut self, chip: pane::BandChip) -> Option<MenuDoor> {
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
