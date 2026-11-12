//! Retained, virtualized transcript rendering.
//!
//! Cockpit owns roster and cross-pane commands. This module owns the expensive
//! per-Subject row snapshot, native text, selection document and viewport.

mod message_menu;
mod find;
mod outputs;

/// The folders a changed-files card can open, by their keys
/// (`DisclosureId::Folder`).
pub fn changes_folders(
    changes: &[ferrite_core::transcript::outputs::Changed],
    root: Option<&std::path::Path>,
) -> Vec<String> {
    outputs::folders(&outputs::tree(changes, root))
}
mod reply;
#[cfg(any(test, feature = "test-support"))]
pub use find::FindState;
mod rows;
pub use rows::gated;
mod scroll;
mod turns;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use ferrite_core::{
    transcript::{Block, BlockId, Body, Status, ToolState},
    ThreadId,
};
use gpui::{
    base::ElementExt, div, list, prelude::*, px, AnyElement, App, Context, Entity, EventEmitter,
    FocusHandle, IntoElement, MouseButton, Render, SharedString, Window,
};

use self::{
    rows::{live_anchor, RowId, RowKind, RowShape, TranscriptRow, TranscriptRows},
    scroll::{EaseStep, EaseTarget},
};
pub use self::scroll::TranscriptScroll;
use crate::{
    file_links::TargetContext,
    pane::{self, DisclosureId, DisclosureState, Grid},
    preview::Preview,
    rich::TextCache,
    select::{RunText, TextRuns, TranscriptText},
};
use ferrite_ui::{pointer::Pointer, theme};
use std::{cell::Cell, rc::Rc};

/// A row another package appends after the transcript's last (the pending
/// Decision, built by `CockpitView::decision_tail`): `key` names its content
/// (a change re-renders it), `render` builds it.
/// What builds a `TranscriptTail`'s row.
pub(crate) type TailRender = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// A pinned prompt band's place: its turn's row and its offset.
type PinnedBand = Option<(usize, gpui::Pixels)>;

#[derive(Clone)]
pub struct TranscriptTail {
    pub key: u64,
    pub render: TailRender,
}

impl std::fmt::Debug for TranscriptTail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TranscriptTail")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

/// What the live row draws to open the running turn's trailing work: its
/// state, and what a press does (the transcript toggles the work's
/// disclosure). The transcript owns the state, so the row is handed it.
#[derive(Clone)]
pub(crate) struct LineDisclosure {
    pub open: bool,
    pub toggle: Rc<dyn Fn(&mut Window, &mut App)>,
}

pub use crate::pane::WorkingTail;

/// A keyboard step through the transcript (`TranscriptView::page`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Up,
    Down,
    Top,
    Bottom,
}

/// Where a scene (or a command) asks a transcript to scroll.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
pub enum ScrollTarget {
    /// The top of the rows.
    Top,
    /// `offset` pixels below the top of turn `turn`'s prompt band (0 is the
    /// first prompt in the window), its band pinned above.
    TurnBand { turn: usize, offset: f32 },
}

/// The paths a call names, read from the Thread's checkout: inside it,
/// relative (`Read(src/stats.rs)`, `Update(src/stats.rs)`), as Claude Code
/// prints them — Claude reports every
/// path absolute. A path outside the checkout stays whole.
fn relative_paths(input: &mut TranscriptInput) {
    let Some(root) = input.workspace.clone() else {
        return;
    };
    for block in &mut input.blocks {
        if let Body::Tool(tool) = &mut block.body {
            relative_call(tool, &root);
        }
    }
}

/// One call's path argument, relative inside `root`. The diffs keep their
/// whole paths: a hunk's section is read off the file on disk
/// (`pane::file_section`).
pub(crate) fn relative_call(tool: &mut ferrite_core::transcript::ToolBlock, root: &std::path::Path) {
    if !pane::argument_is_path(tool) {
        return;
    }
    if let Some(inside) =
        crate::file_links::inside_checkout(std::path::Path::new(&tool.summary), root)
            .filter(|rest| !rest.as_os_str().is_empty())
    {
        tool.summary = inside.to_string_lossy().into_owned();
    }
}

/// Owned input for one selected Subject. Cockpit clones only its retained L1
/// render window when this revision changes; rendering never borrows core.
pub struct TranscriptInput {
    pub thread: ThreadId,
    pub namespace: SharedString,
    pub content_revision: (u64, u64),
    pub display_revision: u64,
    pub blocks: Vec<Block>,
    pub signal_status: Option<Status>,
    pub focused: bool,
    pub reading_size: ferrite_core::settings::ReadingSize,
    pub selection_scope: gpui::base::TextSelectionScopeId,
    pub preview: Preview,
    pub expanded: HashSet<DisclosureId>,
    pub target: Option<DisclosureId>,
    pub disclosure_focus: FocusHandle,
    /// The running turn's header and live row (`pane::working_tail`): the
    /// header under its prompt, the live row after the last row.
    pub working: Option<WorkingTail>,
    /// The row after the last (the pending Decision).
    pub tail: Option<TranscriptTail>,
    /// The call a pending Decision gates: hidden while it waits.
    pub pending_call: Option<String>,
    /// The Thread's checkout, which path targets resolve against.
    pub workspace: Option<PathBuf>,
    /// The Pane stands alone (Solo): the first row takes the body's top
    /// padding, which board Panes do not have.
    pub solo: bool,
    /// Whether the model's thinking shows as `∴` rows (Settings'
    /// `show_thinking`, the palette's `show thinking`).
    pub thinking: bool,
    /// The subagents this transcript's calls started, by call: the blocks
    /// already carry their standing (`AgentFaces::apply`); a call with a
    /// face opens its agent (`TranscriptEvent::OpenAgent`).
    pub agents: pane::AgentFaces,
    /// Whether a sent prompt's Edit, Rewind and Fork can be asked now:
    /// `None` while its agent offers none (they are not drawn), `Err` with
    /// why not while they stand disabled.
    pub rewind: Option<Result<(), SharedString>>,
    /// Whether a selected passage can be quoted into a Composer: the Pane
    /// draws one (a subagent's tab does not), so a drag offers its Reply.
    pub quotes: bool,
    /// Who a Reply is to (`Reply to Claude`): the provider's name.
    pub reply_to: SharedString,
    /// The Session's commands and skills: a sent prompt's `/name` that
    /// names one is drawn blue, its description on hover.
    pub skills: std::sync::Arc<[ferrite_core::SessionCommand]>,
    /// A muted line standing under the first prompt, which no block says:
    /// `Model Haiku 5.5` in a subagent's drill-in.
    pub note: Option<SharedString>,
    /// How many prompts wait behind the running turn: Scroll to end names
    /// them (`Scroll to end · 2 queued`).
    pub queued: usize,
}

/// Everything a transcript's render depends on besides its blocks and its
/// disclosures, compared every root render before any block is cloned.
#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptKey {
    pub namespace: SharedString,
    pub content_revision: (u64, u64),
    pub display_revision: u64,
    pub focused: bool,
    pub signal_status: Option<Status>,
    pub reading_size: ferrite_core::settings::ReadingSize,
    pub working: Option<u64>,
    pub tail: Option<u64>,
    pub pending_call: Option<String>,
    pub workspace: Option<PathBuf>,
    pub solo: bool,
    pub thinking: bool,
    pub agents: pane::AgentFaces,
    pub rewind: Option<Result<(), SharedString>>,
    pub quotes: bool,
    /// How many skills the prompts can name: a late catalogue redraws them.
    pub skills: usize,
    pub note: Option<SharedString>,
    pub queued: usize,
}

impl TranscriptInput {
    pub(crate) fn key(&self) -> TranscriptKey {
        TranscriptKey {
            namespace: self.namespace.clone(),
            content_revision: self.content_revision,
            display_revision: self.display_revision,
            focused: self.focused,
            signal_status: self.signal_status,
            reading_size: self.reading_size,
            working: self.working.as_ref().map(|working| working.key),
            tail: self.tail.as_ref().map(|tail| tail.key),
            pending_call: self.pending_call.clone(),
            workspace: self.workspace.clone(),
            solo: self.solo,
            thinking: self.thinking,
            agents: self.agents.clone(),
            rewind: self.rewind.clone(),
            quotes: self.quotes,
            skills: self.skills.len(),
            note: self.note.clone(),
            queued: self.queued,
        }
    }

    /// What the rows are projected with.
    fn shape(&self) -> RowShape {
        let mut shape = RowShape {
            top_pad: if self.solo {
                theme::TX_PAD_T
            } else {
                theme::TX_PAD_T_BOARD
            },
            working: self.working.as_ref().and_then(|working| working.header_key()),
            live: self.working.as_ref().map(|working| working.key),
            live_open: false,
            open_folds: self
                .expanded
                .iter()
                .filter_map(|id| match id {
                    DisclosureId::Turn(prompt) => Some(*prompt),
                    _ => None,
                })
                .collect(),
            tail: self.tail.as_ref().map(|tail| tail.key),
            pending_call: self.pending_call.clone(),
            thinking: self.thinking,
            note: self.note.clone(),
        };
        shape.live_open =
            live_anchor(&self.blocks, &shape).is_some_and(|anchor| self.expanded.contains(&anchor));
        shape
    }
}

#[derive(Clone, Debug)]
pub enum TranscriptEvent {
    ToggleDisclosure(DisclosureId),
    /// ⌘-click on a path target: open it in a reader pane beside the
    /// Thread (`CockpitView::open_beside`).
    OpenReader {
        path: PathBuf,
        line: Option<u32>,
    },
    /// A press on an agent call's row: open the subagent it started.
    OpenAgent(String),
    /// A press on a sent prompt's Edit, Rewind or Fork, at `at`.
    Prompt {
        block: BlockId,
        action: PromptAction,
        at: gpui::Point<gpui::Pixels>,
    },
    /// A selection's `Cite` (or an answer's Reply row), its words empty: a
    /// Reply card in the Pane's tray (`Composer::reply`).
    Reply(crate::quote::Reply),
    /// An answer's Fork: a new Thread holding the conversation through
    /// the answer whose last block is `after`.
    Fork {
        after: BlockId,
    },
    /// A selection's `Start a side chat`: a new Thread from this point,
    /// the passage quoted in its Composer.
    SideChat(String),
    /// A background call's `View live output ▸`: the Background tasks
    /// pane.
    OpenBackground,
    /// A Thread link in prose (`[title](ferrite-thread:12)`) pressed: land
    /// on that Thread.
    OpenThread(ferrite_core::ThreadId),
    /// A changed-files card's `Open diff`, or a press on one of its files
    /// (as the turn named it): the Changes Surface, on that file.
    OpenChanges(Option<String>),
    /// A wrap-up's next step pressed: its words, sent as the next prompt.
    Send(String),
}

/// What a sent prompt's actions ask of its Thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptAction {
    /// Back to before it, its words back in the Composer.
    Edit,
    /// Back to before it, choosing whether its files come back too; its
    /// words come back to the Composer either way.
    Rewind,
    /// A new Thread from the conversation before it.
    Fork,
}

/// The press target over an agent call's whole row: it asks the cockpit to
/// open the agent (`TranscriptEvent::OpenAgent`), whose transcript then
/// takes the Pane's body.
fn agent_opener(call: &str, view: Entity<TranscriptView>) -> AnyElement {
    let opened = call.to_owned();
    div()
        .id(SharedString::from(format!("agent-open-{call}")))
        .debug_selector({
            let call = call.to_owned();
            move || format!("agent-open-{call}")
        })
        .absolute()
        .inset_0()
        .cursor_pointer()
        .tooltip(ferrite_ui::menu::tooltip("Open agent"))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            gpui::base::TextSelection::clear(window, cx);
            let opened = opened.clone();
            view.update(cx, |_, cx| cx.emit(TranscriptEvent::OpenAgent(opened)));
        })
        .into_any_element()
}

/// A background call's `View live output ▸`, live while its work runs:
/// pressed, it asks the cockpit for the Background tasks pane.
fn background_opener(tool: &ferrite_core::transcript::ToolBlock, view: Option<&Entity<TranscriptView>>) -> Option<AnyElement> {
    let view = view.filter(|_| tool.in_background())?.clone();
    Some(
        pane::background_link()
            .id(SharedString::from(format!("background-open-{}", tool.call)))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                gpui::base::TextSelection::clear(window, cx);
                view.update(cx, |_, cx| cx.emit(TranscriptEvent::OpenBackground));
            })
            .into_any_element(),
    )
}

/// A wrap-up's next steps as buttons, the recommended one first and
/// primary (Claude's): a press sends its words once
/// (`TranscriptView::use_step`).
fn wrap_up_steps(
    wrap: &ferrite_core::wrap_up::WrapUp,
    view: &Entity<TranscriptView>,
    cx: &App,
) -> Vec<AnyElement> {
    wrap.next
        .iter()
        .enumerate()
        .map(|(index, step)| {
            let id = SharedString::from(format!("wrap-up-step-{}-{index}", wrap.call));
            let button = if index == 0 {
                crate::decision::primary_action(id.clone(), step.clone(), cx)
            } else {
                crate::decision::secondary_action(id.clone(), step.clone(), false, false, cx)
            };
            let view = view.clone();
            let (call, words) = (wrap.call.clone(), step.clone());
            div()
                .debug_selector(move || id.to_string())
                .child(button.on_click(move |_, _, cx| {
                    view.update(cx, |view, cx| {
                        if view.use_step(&call, &words) {
                            cx.emit(TranscriptEvent::Send(words.clone()));
                            cx.notify();
                        }
                    });
                }))
                .into_any_element()
        })
        .collect()
}

/// A stable GPUI entity for one Pane Subject's heavy transcript subtree.
pub struct TranscriptView {
    input: TranscriptInput,
    rows: TranscriptRows,
    /// Rows appended at the tail while the operator watched, and when: they
    /// come in over their motion (`arrival_fade`). First paint, a history
    /// window growing at its head and a row scrolled back into view never
    /// do.
    arrivals: HashMap<RowId, Arrival>,
    /// When the streaming answer last grew: its new words fade in by the
    /// run (`rich::Markdown::streaming`), and this transcript asks for the
    /// frames until the last has landed.
    streamed_at: Option<Instant>,
    /// When the turn last ended: every streamed fade still running lands
    /// within `motion::STREAM_LAND` of it (Claude's fast finish).
    landed_at: Option<Instant>,
    /// When each turn's answer finished (its turn end's `completed_at`), by
    /// the turn's last prose block: the time among the answer's actions.
    answer_times: HashMap<BlockId, String>,
    scroll: TranscriptScroll,
    rich: TextCache,
    selection_source: TranscriptText,
    transcript_focus: FocusHandle,
    controls_end: FocusHandle,
    document: gpui::base::TextSelectionDocument,
    /// The pinned prompt band's height, as of the last layout: the next
    /// turn's band pushes it up by what it overlaps.
    pinned_h: Rc<Cell<gpui::Pixels>>,
    /// The pinned band this frame drew, to notice a layout that moves it.
    pinned_drawn: Rc<Cell<PinnedBand>>,
    /// The position a re-render was last asked for.
    pinned_asked: Rc<Cell<Option<PinnedBand>>>,
    /// Each row's laid-out height, as the list last measured it: the
    /// minimap places its ticks and its view band from these.
    heights: Rc<std::cell::RefCell<HashMap<RowId, f32>>>,
    /// Where each disclosure control was last laid out, which the tests
    /// aim at.
    /// The wrap-up step each box has had pressed, by the box's call, so a
    /// second press (a double-click) sends nothing before the sent prompt
    /// reaches the transcript and spends the box.
    used_steps: HashMap<String, String>,
    #[cfg(any(test, feature = "test-support"))]
    disclosure_bounds: Rc<std::cell::RefCell<HashMap<DisclosureId, gpui::Bounds<gpui::Pixels>>>>,
    /// What prose path targets resolve against: the checkout and the paths
    /// this window's calls named.
    targets: TargetContext,
    /// Room past the last row so a band jump can put its band at the top
    /// of a transcript shorter than that (a board Pane with its first prompt
    /// scrolled away, R2): see `Runway`.
    runway: Rc<Cell<Runway>>,
    /// A send was made: the prompt it adds is scrolled to the top, with the
    /// room below it for the answer (`scroll_to_new_prompt`).
    jump_to_new_prompt: bool,
    /// The prompt a send brought to the top, while the operator has not
    /// scrolled away since: its turn's first work lets it go, to the tail
    /// (`release_send_anchor`).
    send_anchor: Option<RowId>,
    /// Since when Scroll to end has been wanted, and in which namespace: it
    /// shows `SCROLL_END_SHOW_DELAY_MS` later (`scroll_to_end_pill`).
    scroll_end_wanted: Option<(SharedString, Instant)>,
    /// The agents' visuals in these rows: their live pages (ADR 0013).
    visuals: crate::visual::Shelf,
    /// The list's width as of the last layout (rows can't ask the list
    /// while it lays them out).
    list_w: Rc<Cell<f32>>,
    /// The list's height as of the last layout: an open group's list is
    /// capped at half of it.
    list_h: Rc<Cell<f32>>,
    /// Each open group's list scroll, by its group, kept while the
    /// transcript lives.
    group_lists: std::cell::RefCell<HashMap<DisclosureId, Rc<pane::GroupList>>>,
    /// The texts copy and find read, in row order, each with its row.
    texts: Vec<(usize, RunText)>,
    /// The find bar, while it is open (⌘F).
    find: Option<find::TranscriptFind>,
    /// The Cite button a finished selection offers (`reply`), and where
    /// the left press that may be selecting landed.
    reply: Option<reply::SelectionReply>,
    pressed_at: Option<gpui::Point<gpui::Pixels>>,
    /// A turn card's menu, while open (`outputs`).
    output_menu: Option<outputs::OutputMenu>,
    /// A message's or a link's menu, while open (`message_menu`).
    message_menu: Option<message_menu::MessageMenu>,
    /// The answers pinned as chapters (`message_menu`), by last block.
    pinned: HashSet<BlockId>,
}

/// The scroll room a band jump adds under the last row: the jump's row and
/// its offset into it (until the list has laid it out), the room, and how
/// tall the rows from the jump's row down were when it was sized — rows
/// arriving later use the room up, so the tail never floats above it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Runway {
    /// The offset into `index`'s row, while the room is being sized.
    jump: Option<f32>,
    index: usize,
    room: f32,
    below: f32,
}

impl EventEmitter<TranscriptEvent> for TranscriptView {}

impl TranscriptView {
    pub(crate) fn empty(
        thread: ThreadId,
        namespace: SharedString,
        preview: Preview,
        rich: TextCache,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(
            TranscriptInput {
                thread,
                namespace,
                content_revision: (0, 0),
                display_revision: 0,
                blocks: Vec::new(),
                signal_status: None,
                focused: false,
                reading_size: Default::default(),
                selection_scope: gpui::base::TextSelectionScopeId::new(),
                preview,
                expanded: HashSet::new(),
                target: None,
                disclosure_focus: cx.focus_handle(),
                working: None,
                tail: None,
                pending_call: None,
                workspace: None,
                solo: false,
                thinking: false,
                agents: Default::default(),
                rewind: None,
                quotes: false,
                reply_to: SharedString::default(),
                skills: Default::default(),
                note: None,
                queued: 0,
            },
            rich,
            TranscriptText::default(),
            cx,
        )
    }

    pub(crate) fn new(
        mut input: TranscriptInput,
        rich: TextCache,
        selection_source: TranscriptText,
        cx: &mut Context<Self>,
    ) -> Self {
        relative_paths(&mut input);
        let rows = TranscriptRows::new(
            &input.blocks,
            &input.shape(),
            Grid::pane(input.reading_size, input.solo).size,
        );
        let scroll = TranscriptScroll::new(rows.len());
        scroll.scroll_to_bottom();
        let scope = if input.focused {
            gpui::base::TextSelectionScopeId::default()
        } else {
            input.selection_scope
        };
        let targets = target_context(&input);
        let times = answer_times(&input.blocks);
        // Chromium came up (the first visual asked for it): draw the
        // visuals that were waiting.
        cx.observe_global::<crate::visual::engine::Visuals>(|_, cx| cx.notify())
            .detach();
        let weak = cx.entity().downgrade();
        let visuals = crate::visual::Shelf::new(Rc::new(move |id: &str, cx: &mut App| {
            let _ = weak.update(cx, |view: &mut Self, cx| view.visual_height_changed(id, cx));
        }));
        let mut view = Self {
            input,
            rows,
            arrivals: HashMap::new(),
            streamed_at: None,
            landed_at: None,
            answer_times: times,
            scroll,
            rich,
            selection_source,
            transcript_focus: cx.focus_handle(),
            // A tab stop, so the controls walk meets it at the end of the
            // transcript instead of wrapping round the window.
            controls_end: cx.focus_handle().tab_stop(true),
            document: gpui::base::TextSelectionDocument::new(scope, cx),
            pinned_h: Rc::new(Cell::new(gpui::px(0.))),
            pinned_drawn: Rc::new(Cell::new(None)),
            pinned_asked: Rc::new(Cell::new(None)),
            heights: Default::default(),
            used_steps: HashMap::new(),
            #[cfg(any(test, feature = "test-support"))]
            disclosure_bounds: Default::default(),
            targets,
            runway: Rc::default(),
            jump_to_new_prompt: false,
            send_anchor: None,
            scroll_end_wanted: None,
            visuals,
            list_w: Rc::default(),
            list_h: Rc::default(),
            group_lists: Default::default(),
            texts: Vec::new(),
            find: None,
            reply: None,
            pressed_at: None,
            output_menu: None,
            message_menu: None,
            pinned: HashSet::new(),
        };
        view.register_scope(cx);
        view.sync_members(cx);
        let owner = cx.entity_id();
        cx.on_release(move |view: &mut Self, cx| view.forget_namespace(owner, cx))
            .detach();
        view
    }

    /// Route ⌘-click on this transcript's path targets to
    /// `TranscriptEvent::OpenReader`, and its Thread links to `OpenThread`.
    fn register_scope(&self, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        crate::file_links::register_scope(
            self.input.namespace.clone(),
            cx.entity_id(),
            crate::file_links::ScopeRoute {
                reader: Rc::new({
                    let weak = weak.clone();
                    move |path, line, _window, cx| {
                        let _ = weak.update(cx, |_, cx| {
                            cx.emit(TranscriptEvent::OpenReader { path, line });
                        });
                    }
                }),
                thread: Rc::new(move |thread, _window, cx| {
                    let _ = weak.update(cx, |_, cx| {
                        cx.emit(TranscriptEvent::OpenThread(thread));
                    });
                }),
            },
            cx,
        );
    }

    /// Nothing under this transcript's namespace can be drawn again: the
    /// transcript was released, or its history regenerated under a new
    /// namespace. Release its ⌘-click route, its laid-out targets and its
    /// native text, unless another transcript has taken the namespace.
    /// A Subject switch keeps both transcripts and their namespaces.
    fn forget_namespace(&self, owner: gpui::EntityId, cx: &mut App) {
        if crate::file_links::release_scope(&self.input.namespace, owner, cx) {
            self.rich.forget_namespace(&self.input.namespace);
        }
    }

    /// Whether this view already shows `key` (the cockpit compares it every
    /// root render before cloning a block).
    pub fn matches(&self, key: &TranscriptKey) -> bool {
        &self.input.key() == key
    }

    /// Reconcile only when Cockpit's revision key changes. Ordinary Cockpit
    /// redraws compare that key without cloning rows or notifying this entity.
    pub fn sync(
        &mut self,
        mut input: TranscriptInput,
        selection_source: TranscriptText,
        cx: &mut Context<Self>,
    ) {
        relative_paths(&mut input);
        let before = self.input.key();
        let after = input.key();
        let namespace_changed = before.namespace != after.namespace;
        // An agent's standing is its call's: it changes the blocks.
        let content_changed = namespace_changed
            || before.content_revision != after.content_revision
            || before.agents != after.agents
            || before.skills != after.skills;
        let disclosure_changed =
            self.input.expanded != input.expanded || self.input.target != input.target;
        let reading_changed = before.reading_size != after.reading_size;
        // The answer above the working row drops its actions row while the
        // turn runs: its height changes when the turn starts or ends.
        let working_toggled = before.working.is_some() != after.working.is_some();
        let shape_changed = self.input.shape() != input.shape();
        let display_changed = before != after || disclosure_changed;
        let workspace_changed = before.workspace != after.workspace;
        if namespace_changed {
            self.forget_namespace(cx.entity_id(), cx);
        }
        self.input = input;
        self.selection_source = selection_source;
        if namespace_changed {
            self.register_scope(cx);
        }
        if content_changed || workspace_changed {
            self.targets = target_context(&self.input);
        }
        if content_changed {
            self.answer_times = answer_times(&self.input.blocks);
        }
        // The turn ended: what still fades lands quickly from here.
        if working_toggled && after.working.is_none() {
            self.landed_at = Some(cx.background_executor().now());
        }
        // The gap table is part of the rows: a reading-size change
        // re-projects them, re-spacing every row whose gap scales; a diff
        // opening or the top padding and tail changing re-project them too.
        if content_changed || reading_changed || shape_changed {
            // The newest content row: the working row and the Decision
            // stand after it, and do not move it.
            let tail = self
                .rows
                .rows()
                .iter()
                .rev()
                .find(|row| !matches!(row.kind(), RowKind::Working | RowKind::LiveTail | RowKind::Tail))
                .map(|row| row.id().clone());
            let delta =
                self.rows
                    .reconcile(&self.input.blocks, &self.input.shape(), self.grid().size);
            self.scroll.reconcile(&delta);
            if namespace_changed {
                self.send_anchor = None;
            }
            if std::mem::take(&mut self.jump_to_new_prompt) {
                self.land_new_prompt(cx);
            } else {
                self.release_send_anchor();
            }
            // The newest row grew in place while the turn runs: its new
            // words fade in by the run.
            if self.input.working.is_some()
                && !namespace_changed
                && self.newest_content().is_some_and(|row| {
                    Some(row.id()) == tail.as_ref() && matches!(row.kind(), RowKind::Answer { .. })
                })
            {
                self.streamed_at = Some(cx.background_executor().now());
            }
            self.note_arrivals(tail, cx);
            let live: HashSet<&str> = self
                .input
                .blocks
                .iter()
                .filter_map(|block| crate::visual::visual_of(block).map(|v| v.id.as_str()))
                .collect();
            self.visuals.retain(&|id| live.contains(id));
        }
        if content_changed || display_changed {
            if disclosure_changed || reading_changed || working_toggled {
                self.scroll.remeasure_all();
            }
            let scope = if self.input.focused {
                gpui::base::TextSelectionScopeId::default()
            } else {
                self.input.selection_scope
            };
            self.document.set_scope(scope, cx);
            if content_changed || disclosure_changed || shape_changed || workspace_changed {
                self.sync_members(cx);
            }
            cx.notify();
        }
    }

    /// Stamp the content rows that now follow what was the newest: appended
    /// live. The working row and the Decision after them are not content.
    fn note_arrivals(&mut self, tail: Option<RowId>, cx: &App) {
        let now = cx.background_executor().now();
        self.arrivals.retain(|_, arrival| arrival.moving(now));
        let Some(tail) = tail else {
            return;
        };
        let rows = self.rows.rows();
        let Some(at) = rows.iter().rposition(|row| *row.id() == tail) else {
            return;
        };
        for row in &rows[at + 1..] {
            if !matches!(row.kind(), RowKind::Working | RowKind::LiveTail | RowKind::Tail) {
                self.arrivals.insert(
                    row.id().clone(),
                    Arrival::new(arrival_fade(row.kind()), now),
                );
            }
        }
    }

    /// Whether a sent prompt is held at the top for its turn.
    #[cfg(any(test, feature = "test-support"))]
    pub fn send_anchored(&self) -> bool {
        self.send_anchor.is_some()
    }

    /// The room under the last row a band jump left (`Runway`).
    #[cfg(any(test, feature = "test-support"))]
    pub fn runway_room(&self) -> f32 {
        self.runway.get().room
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn arrivals(&self) -> usize {
        self.arrivals.len()
    }

    /// How far the sent bubbles still popping in are (`arriving`).
    #[cfg(any(test, feature = "test-support"))]
    pub fn bubbles_arriving(&self, cx: &App) -> Vec<f32> {
        let now = cx.background_executor().now();
        self.arrivals
            .values()
            .filter(|arrival| {
                arrival.spec == ferrite_ui::motion::BUBBLE_IN && arrival.moving(now)
            })
            .map(|arrival| {
                arrival.drawn.map_or(0., |drawn| {
                    arrival
                        .spec
                        .progress_at(now.saturating_duration_since(drawn))
                })
            })
            .collect()
    }

    /// How far along each row still on its way in is now (0 until a frame
    /// draws it), read without starting a clock.
    #[cfg(any(test, feature = "test-support"))]
    pub fn arriving(&self, cx: &App) -> Vec<f32> {
        let now = cx.background_executor().now();
        self.arrivals
            .values()
            .filter(|arrival| arrival.moving(now))
            .map(|arrival| {
                arrival.drawn.map_or(0., |drawn| {
                    arrival
                        .spec
                        .progress_at(now.saturating_duration_since(drawn))
                })
            })
            .collect()
    }

    /// How far a live-appended row is into its motion, while it moves. Its
    /// clock starts on the frame that first draws it: the frame after a
    /// send or a turn's end is the heaviest there is, and a clock started
    /// at the change would have spent most of the entrance before a pixel
    /// of it showed. A row that arrives whole plays its whole fade, the
    /// turn's end or not; only streamed words land early (`landed_at`).
    fn arrival(&mut self, row: &RowId, cx: &App) -> Option<f32> {
        if ferrite_ui::motion::reduced_motion(cx) {
            return None;
        }
        let now = cx.background_executor().now();
        let arrival = self.arrivals.get_mut(row)?;
        // The sent bubble pops where it lands, not on its way up: held at
        // its first frame, unstarted, until the send's ease is done.
        if self.scroll.easing() && self.send_anchor.as_ref() == Some(row) {
            *arrival = Arrival::new(arrival.spec, now);
            return Some(0.);
        }
        arrival.drawn(now)
    }

    /// A streamed fade's linear progress, landing with the turn's end.
    fn streamed_progress(&self, at: Instant, now: Instant) -> f32 {
        gpui::base::text::fade_progress(
            at,
            now,
            ferrite_ui::motion::STREAM_FADE.duration(),
            self.landing(),
        )
    }

    /// The turn's end, and how quickly what still fades lands from it.
    fn landing(&self) -> Option<(Instant, std::time::Duration)> {
        self.landed_at
            .map(|ended| (ended, ferrite_ui::motion::STREAM_LAND))
    }

    /// Whether the streaming answer's newest words still fade at `now`.
    fn streaming_fade(&self, now: Instant) -> bool {
        self.streamed_at
            .is_some_and(|at| self.streamed_progress(at, now) < 1.)
    }

    /// When the turn ended, while the newest answer's words still land.
    fn answer_landing(&self, cx: &App) -> Option<Instant> {
        let now = cx.background_executor().now();
        self.landed_at
            .filter(|_| self.input.working.is_none() && self.streaming_fade(now))
    }

    /// Whether a fade is still running (a row arriving, the streaming
    /// answer's newest words): the transcript asks for its next frame, from
    /// this one place.
    fn fading(&self, cx: &App) -> bool {
        if ferrite_ui::motion::reduced_motion(cx) {
            return false;
        }
        let now = cx.background_executor().now();
        self.scroll.easing()
            || self.streaming_fade(now)
            || self.arrivals.values().any(|arrival| arrival.moving(now))
    }

    /// The newest content row: the working row and the Decision stand
    /// after it.
    fn newest_content(&self) -> Option<&TranscriptRow> {
        self.rows
            .rows()
            .iter()
            .rev()
            .find(|row| !matches!(row.kind(), RowKind::Working | RowKind::LiveTail | RowKind::Tail))
            .map(|row| &**row)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn scroll(&self) -> &TranscriptScroll {
        &self.scroll
    }
    pub fn selection_document(&self) -> gpui::base::TextSelectionDocument {
        self.document.clone()
    }
    pub(crate) fn transcript_focus(&self) -> FocusHandle {
        self.transcript_focus.clone()
    }
    /// Enter or advance the mounted native controls using GPUI's rendered tab
    /// order. The non-tab-stop boundaries keep this walk inside one Subject.
    pub fn cycle_controls(
        &self,
        reverse: bool,
        from_edge: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        if from_edge {
            window.focus(
                if reverse {
                    &self.controls_end
                } else {
                    &self.transcript_focus
                },
                cx,
            );
        }
        loop {
            let before = window.focused(cx);
            if reverse {
                window.focus_prev(cx);
            } else {
                window.focus_next(cx);
            }
            let Some(focus) = window.focused(cx) else {
                return false;
            };
            if Some(&focus) == before.as_ref() || !self.transcript_focus.contains(&focus, window) {
                return false;
            }
            // The end sentinel: forward, the walk is past the last control;
            // backward, it wrapped round from the first.
            if focus == self.controls_end {
                return false;
            }
            if crate::rich::code_actions_focused(window) {
                return true;
            }
        }
    }

    pub(crate) fn tool_state(&self, call: impl Into<DisclosureId>) -> DisclosureState {
        if self.input.expanded.contains(&call.into()) {
            DisclosureState::Expanded
        } else {
            DisclosureState::Collapsed
        }
    }
    pub(crate) fn tool_targeted(&self, call: impl Into<DisclosureId>) -> bool {
        self.input.target.as_ref() == Some(&call.into())
    }
    fn clear_output_selection(&self, cx: &mut App) {
        self.rich.clear_output_selection(&self.input.namespace, cx)
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn is_following_tail(&self) -> bool {
        self.scroll.is_following_tail()
    }

    /// The next prompt to arrive scrolls to the top of the view, leaving
    /// the room below it for its answer (Claude's, on a send). The next
    /// content change settles it: a send adds its prompt at once.
    pub fn scroll_to_new_prompt(&mut self, cx: &mut Context<Self>) {
        self.jump_to_new_prompt = true;
        self.scroll_end_wanted = None;
        cx.notify();
    }

    /// The prompt a send just added rises to the top of the view,
    /// `TX_SEND_ANCHOR_OFFSET` below it, with the room under it for its
    /// answer (`Runway`): eased, or at once under reduced motion.
    fn land_new_prompt(&mut self, cx: &App) {
        let Some((index, row)) = self
            .rows
            .rows()
            .iter()
            .enumerate()
            .rfind(|(_, row)| row.kind() == RowKind::Prompt)
        else {
            return;
        };
        let into = (row.gap() - theme::TX_SEND_ANCHOR_OFFSET).max(0.);
        self.send_anchor = Some(row.id().clone());
        self.runway.set(Runway {
            jump: Some(into),
            index,
            ..self.runway.get()
        });
        if ferrite_ui::motion::reduced_motion(cx) {
            self.scroll.scroll_to(index, px(into));
        } else {
            self.scroll.ease_to(EaseTarget::Row(gpui::ListOffset {
                item_ix: index,
                offset_in_item: px(into),
            }));
        }
    }

    /// The sent prompt's turn has started work (a call, a group, its
    /// reasoning): the prompt lets go and the transcript follows the tail,
    /// at once, as T3's does. A prose-only answer keeps the prompt at the
    /// top. Rows that outgrew the room let it go too, where they stand.
    fn release_send_anchor(&mut self) {
        let Some(anchor) = &self.send_anchor else {
            return;
        };
        if !self.scroll.easing() && self.runway.get() == Runway::default() {
            self.send_anchor = None;
            return;
        }
        let rows = self.rows.rows();
        let Some(at) = rows.iter().position(|row| row.id() == anchor) else {
            self.send_anchor = None;
            return;
        };
        // The running turn's trailing work folds into the live row.
        let worked = rows[at + 1..].iter().any(|row| match row.kind() {
            RowKind::Activity | RowKind::Group | RowKind::Reasoning => true,
            RowKind::LiveTail => !row.blocks().is_empty(),
            _ => false,
        });
        if worked {
            self.send_anchor = None;
            self.runway.set(Runway::default());
            self.scroll.scroll_to_bottom();
        }
    }

    /// The operator scrolled (a wheel, a page, a jump): a programmatic
    /// scroll on its way stops where it stands, and a sent prompt is no
    /// longer held for its turn. The room under it stays, used up as rows
    /// arrive.
    fn operator_scrolled(&mut self) {
        self.scroll.cancel_ease();
        self.send_anchor = None;
        let runway = self.runway.get();
        self.runway.set(Runway {
            jump: None,
            ..runway
        });
    }

    pub fn scroll_to_bottom(&mut self, cx: &mut Context<Self>) {
        self.send_anchor = None;
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    /// PageUp and PageDown (a page, a line of the last kept in view), Home
    /// and End (the top, the live tail).
    pub fn page(&mut self, page: Page, cx: &mut Context<Self>) {
        match page {
            Page::Up | Page::Down => {
                self.operator_scrolled();
                self.runway.set(Runway::default());
                self.scroll
                    .scroll_page(page == Page::Down, px(self.grid().line));
                cx.notify();
            }
            Page::Top => self.scroll_to(ScrollTarget::Top, cx),
            Page::Bottom => self.scroll_to_bottom(cx),
        }
    }

    /// Scroll to `target` (a scene's Solo views): the top,
    /// the live tail, or a turn's band with its output scrolled under it.
    pub fn scroll_to(&mut self, target: ScrollTarget, cx: &mut Context<Self>) {
        self.send_anchor = None;
        match target {
            ScrollTarget::Top => {
                self.runway.set(Runway::default());
                self.scroll.scroll_to(0, px(0.))
            }
            ScrollTarget::TurnBand { turn, offset } => {
                let Some((index, row)) = self
                    .rows
                    .rows()
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| row.kind() == RowKind::Prompt)
                    .nth(turn)
                else {
                    return;
                };
                let into = row.gap() + offset;
                // A band brought to the very top gets the room to get
                // there (the room is sized once the list has laid the rows
                // out); any other offset is clamped at the end, as a
                // browser clamps a scroll past it.
                if offset == 0.0 {
                    self.runway.set(Runway {
                        jump: Some(into),
                        index,
                        ..self.runway.get()
                    });
                } else {
                    self.runway.set(Runway::default());
                }
                self.scroll.scroll_to(index, px(into));
            }
        }
        cx.notify();
    }

    /// The prompt the operator is reading: the pinned band's turn, else the
    /// last prompt in the window. What `copy prompt` and `resend prompt`
    /// act on.
    pub fn reading_prompt(&self) -> Option<String> {
        let pinned = self
            .pinned_prompt()
            .and_then(|(index, _)| self.rows.get(index))
            .and_then(|row| match &row.blocks().first()?.body {
                Body::Prompt(line) => Some(line.clone()),
                _ => None,
            });
        pinned.or_else(|| {
            self.input
                .blocks
                .iter()
                .rev()
                .find_map(|block| match &block.body {
                    Body::Prompt(line) => Some(line.clone()),
                    _ => None,
                })
        })
    }

    /// The scope this transcript's path targets register under (what
    /// `file_links::laid_out` finds them by): a headless driver points the
    /// mouse at one.
    #[cfg(feature = "visual-reference")]
    pub fn namespace(&self) -> SharedString {
        self.input.namespace.clone()
    }

    /// The hover key the newest answer's actions blend under: a headless
    /// driver rests the pointer on it.
    #[cfg(feature = "visual-reference")]
    pub fn newest_answer_hover(&self) -> Option<SharedString> {
        let row = self
            .rows
            .rows()
            .iter()
            .rev()
            .find(|row| row.kind() == RowKind::Answer { narration: false })?;
        Some(actions_key(&self.answer_id(answer_first(row.blocks()))))
    }

    /// An answer row's id, by its first block.
    fn answer_id(&self, first: BlockId) -> SharedString {
        SharedString::from(format!("answer-{}-{first:?}", self.input.namespace))
    }

    fn text_runs(&self) -> TextRuns {
        self.selection_source
            .clone()
            .overlay_scoped(
                self.input.thread,
                self.input.namespace.clone(),
                &self.input.blocks,
                self.rich.clone(),
            )
            .with_document(self.document.clone())
            .with_find(self.find_marks())
    }

    /// Synchronize all logical native text fragments without mounting rows.
    /// This runs outside a draw, including while the window is occluded.
    /// Never construct elements here: GPUI's fallback element arena retains
    /// them indefinitely. Only plain text wrappers may be made and dropped.
    fn sync_members(&mut self, cx: &mut Context<Self>) {
        let selection = self.text_runs();
        let mut rows = Vec::new();
        let (members, texts) = selection.capture_members(|| {
            for (index, row) in self.rows.rows().iter().enumerate() {
                selection.begin_row();
                self.collect_row_text(row, &selection);
                rows.resize(selection.captured(), index);
            }
        });
        self.document.sync_members(members, cx);
        self.texts = rows.into_iter().zip(texts).collect();
        if self.find.is_some() {
            self.refind(cx);
        }
    }

    /// An answer's Markdown with its path targets and issue refs linked
    /// (`file_links::link_targets`): rendered and copied from the same text.
    fn linked(&self, source: &str) -> String {
        crate::file_links::link_targets(source, &self.targets)
    }

    fn collect_row_text(&self, row: &TranscriptRow, selection: &TextRuns) {
        if matches!(
            row.kind(),
            RowKind::Tail | RowKind::Working | RowKind::LiveTail | RowKind::TurnFold | RowKind::Divider
        ) {
            return;
        }
        if let Some(source) = row.source() {
            let block = &row.blocks()[0];
            let _ = selection.answer(block.markdown_run.unwrap_or(block.id), self.linked(source));
        } else if row.kind() == RowKind::Group {
            pane::collect_group_text(
                row.blocks(),
                self.group_expanded(row),
                |block| self.tool_state(self.member_id(block)) == DisclosureState::Expanded,
                selection,
            );
        } else if let Some(block) = row.blocks().first() {
            let expanded = match &block.body {
                Body::Tool(tool) => self.tool_state(DisclosureId::Tool(tool.call.clone())),
                Body::Thinking(_) => self.tool_state(DisclosureId::Reasoning(block.id)),
                _ => DisclosureState::Collapsed,
            } == DisclosureState::Expanded;
            pane::collect_block_text(block, expanded, &self.input.skills, selection);
        }
    }

    /// Run only inside a test draw, where GPUI owns the temporary elements.
    /// Compare every logical fragment (including offscreen rows) against the
    /// actual renderer, so disclosure and copy projections cannot drift.
    #[cfg(test)]
    pub(crate) fn assert_text_projection(&self, cx: &mut Context<Self>) {
        let selection = self.text_runs();
        let _ = selection.capture_members(|| {
            for row in self.rows.rows() {
                selection.begin_row();
                self.collect_row_text(row, &selection);
            }
        });
        let logical = self.selection_source.registered(self.input.thread);
        let _ = selection.capture_members(|| {
            for row in self.rows.rows() {
                selection.begin_row();
                let _ = self.render_row(row, &selection, None, None, cx);
            }
        });
        assert_eq!(
            logical,
            self.selection_source.registered(self.input.thread),
            "logical copy fragments must match rendered row identities and text"
        );
    }

    /// What a row reads besides its block.
    fn row_cx<'a>(&'a self, selection: &'a TextRuns) -> pane::RowCx<'a> {
        pane::RowCx {
            selection,
            rich: &self.rich,
            preview: &self.input.preview,
            grid: self.grid(),
            scope: &self.input.namespace,
            skills: &self.input.skills,
        }
    }

    /// Take a wrap-up step as sent: false when the box is spent already (a
    /// step of it was pressed, or a later prompt followed it), so a repeat
    /// press sends nothing.
    fn use_step(&mut self, call: &str, words: &str) -> bool {
        if self.used_steps.contains_key(call) || self.wrap_up_moved_on(call) {
            return false;
        }
        self.used_steps.insert(call.to_owned(), words.to_owned());
        true
    }

    /// The blocks after the wrap-up of `call`.
    fn after_wrap_up(&self, call: &str) -> &[ferrite_core::transcript::Block] {
        let at = self.input.blocks.iter().rposition(
            |block| matches!(&block.body, Body::WrapUp(wrap) if wrap.call == call),
        );
        at.map_or(&[], |at| &self.input.blocks[at + 1..])
    }

    /// Whether the Thread has gone past the wrap-up of `call`: a prompt
    /// or another wrap-up came after it.
    fn wrap_up_moved_on(&self, call: &str) -> bool {
        self.after_wrap_up(call)
            .iter()
            .any(|block| matches!(block.body, Body::Prompt(_) | Body::WrapUp(_)))
    }

    /// What a wrap-up offers: its steps while it is live, else (spent) the
    /// step that was sent, when one of them was.
    fn wrap_up_face(
        &self,
        wrap: &ferrite_core::wrap_up::WrapUp,
        spent: bool,
        view: Option<&Entity<Self>>,
        cx: &App,
    ) -> pane::WrapUpFace {
        let used = self.used_steps.get(&wrap.call);
        if !spent && used.is_none() {
            return pane::WrapUpFace::Steps(
                view.map_or_else(Vec::new, |view| wrap_up_steps(wrap, view, cx)),
            );
        }
        let sent_after = || {
            self.after_wrap_up(&wrap.call)
                .iter()
                .find_map(|block| match &block.body {
                    Body::Prompt(words) => wrap.next.iter().find(|step| *step == words).cloned(),
                    _ => None,
                })
        };
        pane::WrapUpFace::Spent(used.cloned().or_else(sent_after))
    }

    /// The live row's press, when the running turn has trailing work: it
    /// opens that work's rows above it (`rows::live_anchor`).
    fn line_disclosure(&self, view: Entity<Self>) -> Option<LineDisclosure> {
        let call = live_anchor(&self.input.blocks, &self.input.shape())?;
        Some(LineDisclosure {
            open: self.tool_state(&call) == DisclosureState::Expanded,
            toggle: Rc::new(move |_, cx| {
                view.update(cx, |_, cx| {
                    cx.emit(TranscriptEvent::ToggleDisclosure(call.clone()));
                })
            }),
        })
    }

    /// A settled turn's fold (`pane::turn_fold`): a press toggles it
    /// (`DisclosureId::Turn`), opening the turn's work in place under it.
    fn render_fold(&self, fold: &turns::Fold, view: Option<Entity<Self>>) -> AnyElement {
        let id = DisclosureId::Turn(fold.prompt);
        let open = self.tool_state(&id) == DisclosureState::Expanded;
        let targeted = self
            .tool_targeted(&id)
            .then_some(&self.input.disclosure_focus);
        let toggle = view.map(|view| -> Rc<dyn Fn(&mut Window, &mut App)> {
            let id = id.clone();
            Rc::new(move |_, cx| {
                view.update(cx, |view, cx| {
                    view.clear_output_selection(cx);
                    cx.emit(TranscriptEvent::ToggleDisclosure(id.clone()));
                })
            })
        });
        pane::turn_fold(
            pane::FoldFace {
                key: SharedString::from(format!("turn-fold-{}-{id:?}", self.input.namespace)),
                label: &fold.label,
                started: fold.started.as_deref(),
                open,
                targeted,
            },
            toggle,
        )
    }

    fn render_row(
        &self,
        row: &TranscriptRow,
        selection: &TextRuns,
        view: Option<Entity<Self>>,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if row.kind() == RowKind::Working {
            return match &self.input.working {
                Some(working) => working.header(),
                None => div().into_any_element(),
            };
        }
        if row.kind() == RowKind::LiveTail {
            return match &self.input.working {
                Some(working) => {
                    let disclosure = view.and_then(|view| self.line_disclosure(view));
                    working.live(row.blocks(), disclosure)
                }
                None => div().into_any_element(),
            };
        }
        if let Some(fold) = row.fold() {
            return self.render_fold(fold, view);
        }
        if row.kind() == RowKind::Outputs {
            return self.render_outputs(row, cx);
        }
        if row.kind() == RowKind::Tail {
            return match (&self.input.tail, window) {
                (Some(tail), Some(window)) => div()
                    .debug_selector(|| "transcript-tail".into())
                    .w_full()
                    .min_w_0()
                    .flex_shrink_0()
                    .child((tail.render)(window, cx))
                    .into_any_element(),
                _ => div().into_any_element(),
            };
        }
        let blocks = row.blocks();
        if row.kind() == RowKind::Divider {
            return date_divider(&blocks[0]).into_any_element();
        }
        if let Some(Body::Compaction(compaction)) = blocks.first().map(|block| &block.body) {
            return compaction_divider(compaction).into_any_element();
        }
        if row.kind() == RowKind::Note {
            return note_line(row.source().unwrap_or_default()).into_any_element();
        }
        if let Some(source) = row.source() {
            let first = answer_first(blocks);
            // Prose on the column's edge, narration (more work follows it
            // in its turn) at the answer's full ink, as both reference apps
            // draw it: it is the content, not a footnote to the work. A
            // turn's answer carries its actions under it (narration is the
            // work's, and copies with it) once it is written: the answer
            // still streaming above the working row has none, so the
            // working line stands right under its words.
            let narration = row.kind() == RowKind::Answer { narration: true };
            let newest = self
                .newest_content()
                .is_some_and(|last| last.id() == row.id());
            let streaming = self.input.working.is_some() && newest;
            let landing = self.answer_landing(cx).filter(|_| newest);
            let id = self.answer_id(first);
            let shown = self.actions_shown(&id);
            let time = blocks
                .iter()
                .find_map(|block| self.answer_times.get(&block.id))
                .map(|at| {
                    message_time(SharedString::from(format!("{id}-time")), "answer-time", at)
                });
            let last = blocks.last().map_or(first, |block| block.id);
            let actions = (!narration && !streaming).then(|| {
                message_actions(shown)
                    .debug_selector(|| "answer-actions".into())
                    // The first icon's ink, not its box, on the prose's
                    // edge (one left edge).
                    .ml(px(
                        -(theme::MESSAGE_ACTION - theme::MESSAGE_ACTION_GLYPH) / 2.
                    ))
                    .child(self.copy_action(row.id(), source.to_owned()))
                    .children(view.as_ref().and_then(|view| {
                        let allowed = self.input.rewind.as_ref()?;
                        Some(self.message_door(
                            row.id(),
                            ("answer-fork", ferrite_ui::icons::BRANCH, "Fork from here"),
                            allowed,
                            view.clone(),
                            move |_, _, cx| cx.emit(TranscriptEvent::Fork { after: last }),
                        ))
                    }))
                    .children(view.as_ref().map(|view| {
                        let text = source.to_owned();
                        let row_id = row.id().clone();
                        self.message_door(
                            row.id(),
                            (
                                "answer-more",
                                ferrite_ui::icons::ELLIPSIS,
                                message_menu::MORE_LABEL,
                            ),
                            &Ok(()),
                            view.clone(),
                            move |view, at, cx| {
                                let menu = view.answer_menu(&row_id, last, text.clone(), at);
                                view.open_message_menu(menu, cx)
                            },
                        )
                    }))
                    .children(self.pinned.contains(&last).then(|| {
                        div()
                            .debug_selector(|| "answer-chapter".into())
                            .flex_shrink_0()
                            .text_color(gpui::rgb(theme::INK_SUBTLE))
                            .child("Chapter")
                    }))
                    .children(time)
            });
            let right_id = row.id().clone();
            let right_source = source.to_owned();
            return div()
                .id(id.clone())
                .flex()
                .items_start()
                .min_w_0()
                .w_full()
                .flex_shrink_0()
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        let menu = view.answer_menu(&right_id, last, right_source.clone(), event.position);
                        view.right_click_message(menu, window, cx);
                    }),
                )
                .when(actions.is_some(), |row| {
                    row.flex_col()
                        .group(MESSAGE_GROUP)
                        .on_hover(actions_listener(&id))
                })
                .child(
                    // The prose alone, its actions under it apart.
                    div()
                        .debug_selector(|| "transcript-answer".into())
                        .w_full()
                        .flex_1()
                        .min_w_0()
                        .child(
                            selection
                                .answer(first, self.linked(source))
                                .streaming(streaming && !ferrite_ui::motion::reduced_motion(cx))
                                .landing(landing),
                        ),
                )
                .children(actions.map(|actions| actions.mt(px(theme::MESSAGE_ACTIONS_GAP))))
                .into_any_element();
        }
        let Some(block) = blocks.first() else {
            return div().into_any_element();
        };
        if matches!(&block.body, Body::Thinking(text) if text.trim().is_empty()) {
            return div().into_any_element();
        }
        if let Some(visual) = crate::visual::visual_of(block) {
            return self.visuals.row(
                block.id,
                visual,
                self.content_width(),
                self.input.reading_size,
                selection,
                cx,
            );
        }
        let row_cx = self.row_cx(selection);
        if row.kind() == RowKind::Group {
            return self.render_group(row, &row_cx, view);
        }
        if let Body::Prompt(line) = &block.body {
            // Its send time, Edit, Rewind, Fork and `Copy`, under the
            // bubble's right edge. The three
            // that cut need the id it was sent as and an agent that can.
            let cuts = block
                .prompt_id
                .as_ref()
                .zip(self.input.rewind.as_ref())
                .zip(view.as_ref());
            let id = SharedString::from(format!("prompt-{}-{:?}", self.input.namespace, block.id));
            let time = block.sent_at.as_deref().map(|at| {
                message_time(
                    SharedString::from(format!("{id}-time")),
                    "prompt-sent-at",
                    at,
                )
            });
            let actions = message_actions(self.actions_shown(&id))
                .children(time)
                .children(cuts.into_iter().flat_map(|((_, allowed), view)| {
                    [PromptAction::Edit, PromptAction::Rewind, PromptAction::Fork].map(|action| {
                        self.prompt_action(row.id(), block.id, action, allowed, view.clone())
                    })
                }))
                // Its words, as the bubble shows them: the attachments'
                // trailer is chips, never copied text.
                .child(
                    self.copy_action(row.id(), ferrite_core::prompt_files::split(line.clone()).0),
                );
            let words = ferrite_core::prompt_files::split(line.clone()).0;
            let prompt_block = block.id;
            let cuts = cuts.map(|((_, allowed), _)| allowed.clone());
            let joins = row.joins();
            let face = pane::PromptFace {
                above: joins.above,
                below: joins.below,
                clamp: pane::prompt_clamps(line).then(|| {
                    let clamp = DisclosureId::Prompt(block.id);
                    let open = self.tool_state(&clamp) == DisclosureState::Expanded;
                    pane::PromptClamp {
                        open,
                        toggle: self.clamp_toggle(clamp, open, view.clone()),
                    }
                }),
            };
            return div()
                .id(id.clone())
                .group(MESSAGE_GROUP)
                .on_hover(actions_listener(&id))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |view, event: &gpui::MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        let menu = message_menu::MessageMenu {
                            target: message_menu::Target::Prompt {
                                block: prompt_block,
                                cuts: cuts.clone(),
                            },
                            plain: words.clone(),
                            markdown: words.clone(),
                            at: event.position,
                        };
                        view.right_click_message(menu, window, cx);
                    }),
                )
                .w_full()
                .min_w_0()
                .child(pane::prompt_row(
                    block,
                    line,
                    &row_cx,
                    face,
                    Some(actions.into_any_element()),
                ))
                .into_any_element();
        }
        match &block.body {
            Body::WrapUp(wrap) => {
                let spent = matches!(row.kind(), RowKind::WrapUp { spent: true });
                let face = self.wrap_up_face(wrap, spent, view.as_ref(), cx);
                pane::render_wrap_up(block.id, wrap, &row_cx, face)
            }
            _ => self.render_work(block, &row_cx, view.as_ref(), false),
        }
    }

    /// One block on its own row or in a group's list: a call
    /// (`render_call`), or any other block, a thought opening and shutting
    /// under the pointer and the keys.
    fn render_work(
        &self,
        block: &ferrite_core::transcript::Block,
        row_cx: &pane::RowCx,
        view: Option<&Entity<Self>>,
        grouped: bool,
    ) -> AnyElement {
        if let Body::Tool(tool) = &block.body {
            return self.render_call(block, tool, row_cx, view, grouped);
        }
        let call = matches!(&block.body, Body::Thinking(text) if !text.trim().is_empty())
            .then_some(DisclosureId::Reasoning(block.id));
        // Still being thought: the running turn's last block.
        let live = self.input.working.is_some()
            && self.input.blocks.last().is_some_and(|last| last.id == block.id);
        pane::render_block(
            block,
            row_cx,
            call.as_ref()
                .is_some_and(|call| self.tool_state(call) == DisclosureState::Expanded),
            live,
            call.as_ref().and_then(|call| {
                view.map(|view| self.disclosure_control(call, false, view.clone()))
            }),
        )
    }

    /// A row's width (the list's, less the row insets), once laid out.
    fn content_width(&self) -> Option<f32> {
        let width = self.list_w.get();
        (width > 0.).then_some(theme::tx_column(width, self.input.solo))
    }

    /// The transcript's grid: the reading size in Solo, a half step under
    /// it in a Group Pane (`theme::tx_size`).
    fn grid(&self) -> Grid {
        Grid::pane(self.input.reading_size, self.input.solo)
    }

    /// A visual's page changed height: the list re-measures its row (off
    /// screen too, so it never changes height under the reader).
    fn visual_height_changed(&mut self, id: &str, cx: &mut Context<Self>) {
        let index = self.rows.rows().iter().position(|row| {
            row.blocks()
                .first()
                .and_then(crate::visual::visual_of)
                .is_some_and(|visual| visual.id == id)
        });
        if let Some(index) = index {
            self.scroll.list_state().remeasure_items(index..index + 1);
            cx.notify();
        }
    }

    /// One call as its row draws it, alone or in a group: its disclosure
    /// (the whole line the pointer's toggle, its `›`, the keyboard's
    /// target, its folds) when it has details to open. `grouped`: it stands
    /// in a group's card, so its open body takes no card of its own.
    fn render_call(
        &self,
        stamped: &ferrite_core::transcript::Block,
        tool: &ferrite_core::transcript::ToolBlock,
        row_cx: &pane::RowCx,
        view: Option<&Entity<Self>>,
        grouped: bool,
    ) -> AnyElement {
        let block = stamped.id;
        if pane::is_agent(tool) {
            let face = self.input.agents.get(&tool.call);
            let open = view
                .filter(|_| face.is_some())
                .map(|view| agent_opener(&tool.call, view.clone()));
            let targeted = self
                .tool_targeted(DisclosureId::Tool(tool.call.clone()))
                .then_some(&self.input.disclosure_focus);
            return pane::render_agent(
                block,
                tool,
                row_cx,
                pane::AgentRowCx {
                    clock: face.and_then(|face| face.clock),
                    open,
                    targeted,
                    live: background_opener(tool, view),
                    ground: theme::tx_ground(self.input.solo),
                },
            );
        }
        let call = DisclosureId::Tool(tool.call.clone());
        let expanded = self.tool_state(&call) == DisclosureState::Expanded;
        let disclosure = view
            .filter(|_| pane::tool_has_details(tool))
            .map(|view| self.disclosure_control(&call, true, view.clone()));
        pane::render_tool(
            block,
            tool,
            row_cx,
            pane::ToolCx {
                expanded,
                grouped,
                disclosure,
                live: background_opener(tool, view),
                at: stamped.at.clone(),
            },
        )
    }

    /// The disclosure naming a group row: its first call's (what the live
    /// row opens, `rows::live_anchor`), or a group of thoughts alone, its
    /// first thought's.
    fn group_id(row: &TranscriptRow) -> Option<DisclosureId> {
        let blocks = row.blocks();
        blocks
            .iter()
            .find_map(|block| match &block.body {
                Body::Tool(tool) => Some(tool.call.clone()),
                _ => None,
            })
            .or_else(|| blocks.first().map(|block| format!("thoughts-{:?}", block.id)))
            .map(DisclosureId::Group)
    }

    fn group_expanded(&self, row: &TranscriptRow) -> bool {
        Self::group_id(row).is_some_and(|id| self.tool_state(id) == DisclosureState::Expanded)
    }

    /// A group row (`pane::render_tool_group`): its summary is the
    /// disclosure — the whole line under the pointer, its chevron always
    /// shown — and its calls, open, are each the row they would be alone.
    fn render_group(
        &self,
        row: &TranscriptRow,
        row_cx: &pane::RowCx,
        view: Option<Entity<Self>>,
    ) -> AnyElement {
        let Some(id) = Self::group_id(row) else {
            return div().into_any_element();
        };
        let expanded = self.group_expanded(row);
        let disclosure = view
            .clone()
            .map(|view| self.disclosure_control(&id, true, view));
        if pane::is_agent_card(row.blocks()) {
            let clock = pane::AgentClock::longest(
                pane::group_tools(row.blocks())
                    .filter_map(|tool| self.input.agents.get(&tool.call)?.clock),
            );
            let rows = if expanded {
                row.blocks()
                    .iter()
                    .map(|block| self.render_work(block, row_cx, view.as_ref(), true))
                    .collect()
            } else {
                Vec::new()
            };
            return pane::render_agent_card(
                row.blocks(),
                row_cx,
                pane::AgentCardCx {
                    disclosure,
                    open: expanded,
                    clock,
                    rows,
                    ground: theme::tx_ground(self.input.solo),
                },
            );
        }
        let opened = expanded
            && row
                .blocks()
                .iter()
                .any(|block| self.tool_state(self.member_id(block)) == DisclosureState::Expanded);
        let list = expanded.then(|| {
            let mut lists = self.group_lists.borrow_mut();
            lists.entry(id.clone()).or_default().clone()
        });
        let members = move || {
            let drawn: Vec<&ferrite_core::transcript::Block> = if expanded {
                row.blocks().iter().collect()
            } else {
                pane::group_live(row.blocks())
            };
            drawn
                .into_iter()
                .map(|block| self.render_work(block, row_cx, view.as_ref(), true))
                .collect()
        };
        pane::render_tool_group(
            row.blocks(),
            row_cx,
            pane::GroupCx {
                disclosure,
                members: Box::new(members),
                list: list.as_deref().map(|list| (list, opened)),
                view_h: self.list_h.get(),
                ground: theme::tx_ground(self.input.solo),
            },
        )
    }

    /// The disclosure a group member opens by: a call's, a thought's.
    fn member_id(&self, block: &ferrite_core::transcript::Block) -> DisclosureId {
        match &block.body {
            Body::Tool(tool) => DisclosureId::Tool(tool.call.clone()),
            _ => DisclosureId::Reasoning(block.id),
        }
    }

    /// Where `call`'s disclosure control was last laid out.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn disclosure_bounds(
        &self,
        call: &DisclosureId,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.disclosure_bounds.borrow().get(call).copied()
    }

    /// Every disclosure the rows draw, in order: the keyboard's stops. A
    /// turn's fold, a call with details, a long thought, a group — and,
    /// while a group is open, its calls; while shut, the call it shows
    /// working.
    pub fn disclosures(&self) -> Vec<DisclosureId> {
        let mut stops = Vec::new();
        let call = |stops: &mut Vec<DisclosureId>, block: &ferrite_core::transcript::Block| {
            match &block.body {
                Body::Tool(tool) => {
                    // An agent's row has no details: it is a stop while it
                    // opens its agent (↵), which the keyboard can stop too.
                    let stop = if pane::is_agent(tool) {
                        self.input.agents.get(&tool.call).is_some()
                    } else {
                        pane::tool_has_details(tool)
                    };
                    if stop {
                        stops.push(DisclosureId::Tool(tool.call.clone()));
                    }
                }
                Body::Thinking(text) if !text.trim().is_empty() => {
                    stops.push(DisclosureId::Reasoning(block.id));
                }
                _ => {}
            }
        };
        for row in self.rows.rows() {
            match row.kind() {
                // A fold opens and shuts from the keys too (↵, Space).
                RowKind::TurnFold => {
                    if let Some(fold) = row.fold() {
                        stops.push(DisclosureId::Turn(fold.prompt));
                    }
                }
                RowKind::Group => {
                    let Some(id) = Self::group_id(row) else {
                        continue;
                    };
                    stops.push(id);
                    if self.group_expanded(row) {
                        for block in row.blocks() {
                            call(&mut stops, block);
                        }
                    } else {
                        for block in pane::group_live(row.blocks()) {
                            call(&mut stops, block);
                        }
                    }
                }
                RowKind::Activity | RowKind::Reasoning => {
                    if let Some(block) = row.blocks().first() {
                        call(&mut stops, block);
                    }
                }
                _ => {}
            }
        }
        stops
    }

    /// A header's disclosure: the whole header toggles under the pointer,
    /// its chevron turning; `shown` keeps the chevron in view (a group's,
    /// whose line is only ever a disclosure) where a reasoning row's shows
    /// under the pointer.
    fn disclosure_control(
        &self,
        call: &DisclosureId,
        shown: bool,
        view: Entity<Self>,
    ) -> pane::Disclosure {
        let call = call.clone();
        let clicked = call.clone();
        let expanded = self.tool_state(&call) == DisclosureState::Expanded;
        let targeted = self.tool_targeted(&call);
        let control = pane::tool_disclosure_control(
            &call,
            expanded,
            targeted,
            &self.input.disclosure_focus,
            self.grid().gutter(),
        )
        // The disclosure overlay fills the rendered header. Keep the handler
        // on it so the chevron, label, and trailing row text share one target.
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            gpui::base::TextSelection::clear(window, cx);
            view.update(cx, |view, cx| {
                view.clear_output_selection(cx);
                cx.emit(TranscriptEvent::ToggleDisclosure(clicked.clone()));
            });
        });
        #[cfg(any(test, feature = "test-support"))]
        let control = {
            let sink = self.disclosure_bounds.clone();
            let measured = call.clone();
            control.on_children_prepainted(move |bounds, _, _| {
                // The hit box is the overlay's last child; a keyboard ring
                // may precede it.
                if let Some(bounds) = bounds.last() {
                    sink.borrow_mut().insert(measured.clone(), *bounds);
                }
            })
        };
        pane::Disclosure {
            overlay: Some(control.into_any_element()),
            chevron: Some(pane::disclosure_chevron(
                SharedString::from(format!("chevron-{call:?}")),
                expanded,
                targeted || shown,
            )),
            targeted,
            focus: None,
        }
    }

    /// A message's `Copy` (`components::copy_button`, the message action's
    /// door): `text` to the clipboard, a check for `COPIED_HOLD_MS` after.
    fn copy_action(&self, row: &RowId, text: String) -> AnyElement {
        let id = gpui::ElementId::from(SharedString::from(format!(
            "message-copy-{}-{row:?}",
            self.input.namespace
        )));
        // The hover blend draws through this cached view: ask for its next
        // frame.
        let t = ferrite_ui::motion::hover_t(&ferrite_ui::pointer::hover_key(&id));
        if t > 0. && t < 1. {
            crate::file_links::mark_fading();
        }
        div()
            .debug_selector(|| "message-copy".into())
            .flex_shrink_0()
            .child(ferrite_ui::components::copy_button(id, text))
            .into_any_element()
    }

    /// A long prompt's `Show full message` / `Show less` (T3's ghost-muted
    /// xs button): `INK_SUBTLE` meta type, `INK_STRONG` on the raised hover
    /// ground, toggling its clamp (`DisclosureId::Prompt`).
    fn clamp_toggle(&self, clamp: DisclosureId, open: bool, view: Option<Entity<Self>>) -> AnyElement {
        div()
            .id(SharedString::from(format!(
                "prompt-clamp-{}-{clamp:?}",
                self.input.namespace
            )))
            .debug_selector(|| "prompt-clamp-toggle".into())
            .flex()
            .items_center()
            .h(px(theme::MESSAGE_ACTION))
            .px(px(theme::MESSAGE_ACTION_PAD_X))
            .rounded(px(theme::R_CONTROL))
            .text_size(px(theme::FS_BADGE))
            .text_color(gpui::rgb(theme::INK_SUBTLE))
            .cursor_pointer()
            .hover(|style| style.bg(theme::paint::HOVER).text_color(gpui::rgb(theme::INK_STRONG)))
            .child(if open { "Show less" } else { "Show full message" })
            .when_some(view, |button, view| {
                button.on_click(move |_, _, cx| {
                    view.update(cx, |_, cx| {
                        cx.emit(TranscriptEvent::ToggleDisclosure(clamp.clone()))
                    })
                })
            })
            .into_any_element()
    }

    /// One of a sent prompt's Edit, Rewind and Fork, asking the cockpit
    /// for it (`TranscriptEvent::Prompt`).
    fn prompt_action(
        &self,
        row: &RowId,
        block: BlockId,
        action: PromptAction,
        allowed: &Result<(), SharedString>,
        view: Entity<Self>,
    ) -> AnyElement {
        let face = match action {
            PromptAction::Edit => ("message-edit", ferrite_ui::icons::PENCIL, "Edit"),
            PromptAction::Rewind => ("message-rewind", ferrite_ui::icons::BACK, "Rewind to here"),
            PromptAction::Fork => ("message-fork", ferrite_ui::icons::BRANCH, "Fork from here"),
        };
        self.message_door(row, face, allowed, view, move |_, at, cx| {
            cx.emit(TranscriptEvent::Prompt { block, action, at })
        })
    }

    /// A message action's door — its selector, glyph and label — doing
    /// `press` at the press's point, or, while it cannot be asked
    /// (`allowed`), its glyph faded and its tooltip saying why.
    fn message_door(
        &self,
        row: &RowId,
        (selector, glyph, label): (&'static str, &'static str, &'static str),
        allowed: &Result<(), SharedString>,
        view: Entity<Self>,
        press: impl Fn(&mut Self, gpui::Point<gpui::Pixels>, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let id = gpui::ElementId::from(SharedString::from(format!(
            "{selector}-{}-{row:?}",
            self.input.namespace
        )));
        let door = div()
            .id(id.clone())
            .debug_selector(move || selector.into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(theme::MESSAGE_ACTION))
            .rounded(px(theme::R_CONTROL));
        let face = |opacity| {
            ferrite_ui::icons::icon(glyph, theme::MESSAGE_ACTION_GLYPH, theme::INK_SUBTLE)
                .opacity(opacity)
        };
        let door = match allowed {
            Err(why) => door
                .tooltip(ferrite_ui::menu::tooltip(format!("{label} \u{b7} {why}")))
                .child(face(0.4)),
            Ok(()) => {
                let key = ferrite_ui::pointer::hover_key(&id);
                // The hover blend draws through this cached view: ask for
                // its next frame.
                let t = ferrite_ui::motion::hover_t(&key);
                if t > 0. && t < 1. {
                    crate::file_links::mark_fading();
                }
                use ferrite_ui::pointer::PointerPressed;
                door.hover_raised(key)
                    .press_raised()
                    .tooltip(ferrite_ui::menu::tooltip(label))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(move |event, _, cx| {
                        cx.stop_propagation();
                        let at = event.position();
                        view.update(cx, |view, cx| press(view, at, cx));
                    })
                    .child(face(1.))
            }
        };
        door.into_any_element()
    }

    /// The pinned prompt: its turn's bubble stuck to the top on a plane in
    /// the card's fill, the rows passing under it into the top fade. A
    /// click jumps back to its turn. Wheel gestures scroll the list.
    fn pinned_band(
        &self,
        index: usize,
        shift: gpui::Pixels,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let row = self.rows.get(index)?;
        let block = row.blocks().first()?;
        let Body::Prompt(line) = &block.body else {
            return None;
        };
        let selection = self.text_runs();
        let line_height = self.grid().line;
        let available = f32::from(self.scroll.list_state().viewport_bounds().size.height) / 4.;
        let lines = ((available / line_height) as usize).clamp(1, theme::PINNED_PROMPT_LINES);
        let bubble = pane::pinned_prompt(block, line, &self.row_cx(&selection), lines);
        let measured = self.pinned_h.clone();
        let goal = self.estimated_tops().0.get(index).copied().unwrap_or(0.);
        let weak = cx.entity().downgrade();
        let remask = weak.clone();
        let id = gpui::ElementId::from(SharedString::from(format!(
            "transcript-pinned-{}",
            self.input.namespace
        )));
        let key = ferrite_ui::pointer::hover_key(&id);
        let t = ferrite_ui::motion::hover_t(&key);
        if t > 0. && t < 1. {
            crate::file_links::mark_fading();
        }
        let bubble = bubble.id(id).cursor_pointer().bg(ferrite_ui::motion::mix(
            theme::paint::RAISED.into(),
            theme::paint::SELECTION_HOVER.into(),
            t,
        ));
        let blend = ferrite_ui::motion::hover_listener(key);
        let bubble = bubble
            .on_hover(blend)
            .tooltip(ferrite_ui::menu::tooltip("Back to this prompt"))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                cx.stop_propagation();
                let _ = weak.update(cx, |view, cx| view.jump_to_goal(goal, cx));
            });
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(shift)
                // The list masks its top by the band's height: a band that
                // changed height lays the list out again under it.
                .on_children_prepainted(move |bounds, window, cx| {
                    if let Some(bounds) = bounds.first() {
                        if (measured.get() - bounds.size.height).abs() > px(0.5) {
                            let remask = remask.clone();
                            window.defer(cx, move |_, cx| {
                                let _ = remask.update(cx, |_, cx| cx.notify());
                            });
                        }
                        measured.set(bounds.size.height);
                    }
                })
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "transcript-pinned-plane-{}",
                            self.input.namespace
                        )))
                        .block_mouse_except_scroll()
                        .cursor_default()
                        .flex()
                        .justify_center()
                        .w_full()
                        .px(px(theme::tx_inset_at(self.list_w.get(), self.input.solo)))
                        .py(px(theme::PINNED_PROMPT_PAD_Y))
                        .bg(gpui::rgb(theme::tx_ground(self.input.solo)))
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .w_full()
                                .max_w(px(theme::TX_COLUMN_MAX_W))
                                .child(bubble),
                        ),
                )
                .child(top_fade(self.input.solo))
                .into_any_element(),
        )
    }

    /// How far the rows run on under the transcript's foot, past the
    /// list's own foot room (its padding and runway, which hold no row).
    fn rows_below(&self) -> gpui::Pixels {
        let state = self.scroll.list_state();
        let below = state.max_offset_for_scrollbar().y + state.scroll_px_offset_for_scrollbar().y;
        below - px(theme::body_pad_b(self.input.solo) + self.runway.get().room)
    }

    /// How far a message's actions are shown: under the pointer, or (a
    /// scene's) held there.
    fn actions_shown(&self, message: &SharedString) -> f32 {
        ferrite_ui::motion::hover_t(&actions_key(message))
    }

    /// Scroll to end (T3 Code's): while the reader is scrolled away from
    /// the tail, a pill over the transcript's foot eases back to it. It
    /// shows `SCROLL_END_SHOW_DELAY_MS` after it is first wanted in this
    /// namespace (so a Subject switch never flashes it) and goes at once,
    /// with no fade; never while the find bar is open, nor while a
    /// programmatic scroll moves.
    fn scroll_to_end_pill(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        // Within a couple of lines of the end there is nothing to scroll to.
        let wanted = !self.scroll.is_following_tail()
            && !self.scroll.easing()
            && self.find.is_none()
            && self.rows_below() >= px(2. * self.grid().line);
        if !wanted {
            self.scroll_end_wanted = None;
            return None;
        }
        let now = cx.background_executor().now();
        let delay = std::time::Duration::from_millis(theme::SCROLL_END_SHOW_DELAY_MS);
        let since = match &self.scroll_end_wanted {
            Some((namespace, since)) if *namespace == self.input.namespace => *since,
            _ => {
                self.scroll_end_wanted = Some((self.input.namespace.clone(), now));
                cx.spawn(async move |view, cx| {
                    cx.background_executor().timer(delay).await;
                    let _ = view.update(cx, |_, cx| cx.notify());
                })
                .detach();
                now
            }
        };
        if now.saturating_duration_since(since) < delay {
            return None;
        }
        let id = gpui::ElementId::from(SharedString::from(format!(
            "transcript-scroll-end-{}",
            self.input.namespace
        )));
        let key = ferrite_ui::pointer::hover_key(&id);
        let t = ferrite_ui::motion::hover_t(&key);
        if t > 0. && t < 1. {
            crate::file_links::mark_fading();
        }
        let weak = cx.entity().downgrade();
        let queued = (self.input.queued > 0).then(|| {
            div()
                .debug_selector(|| "scroll-to-end-queued".into())
                .text_color(gpui::rgb(theme::INK_SUBTLE))
                .child(format!("· {} queued", self.input.queued))
        });
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(theme::SCROLL_END_INSET))
                .flex()
                .justify_center()
                .child(
                    div()
                        .id(id)
                        .debug_selector(|| "scroll-to-end".into())
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(theme::SCROLL_END_GAP))
                        .h(px(theme::SCROLL_END_H))
                        .px(px(theme::SCROLL_END_PAD_X))
                        .rounded_full()
                        .whitespace_nowrap()
                        .bg(theme::paint::CARD)
                        .border_1()
                        .border_color(ferrite_ui::motion::mix(
                            theme::SCROLL_END_BORDER.into(),
                            theme::SCROLL_END_BORDER_HOVER.into(),
                            t,
                        ))
                        .shadow(ferrite_ui::components::pill_shadow())
                        .font_family(theme::FONT_UI)
                        .text_size(px(theme::FS_BADGE))
                        .line_height(px(theme::LH_META))
                        .font_weight(theme::W_LABEL)
                        .text_color(gpui::rgb(theme::INK_STRONG))
                        .cursor_pointer()
                        .on_hover(ferrite_ui::motion::hover_listener(key))
                        // Down, not click: the press never takes focus from
                        // the Composer (the transcript root's own press would).
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            let _ = weak.update(cx, |view, cx| view.scroll_to_end(cx));
                        })
                        .child(ferrite_ui::icons::icon(
                            ferrite_ui::icons::CHEVRON_DOWN,
                            theme::SCROLL_END_GLYPH,
                            theme::INK_SUBTLE,
                        ))
                        .child("Scroll to end")
                        .children(queued),
                )
                .into_any_element(),
        )
    }

    /// Scroll to end's press: the pill goes at once, the room and the sent
    /// prompt's hold are let go, and the transcript eases to the tail (at
    /// once under reduced motion), following it from there.
    fn scroll_to_end(&mut self, cx: &mut Context<Self>) {
        self.scroll_end_wanted = None;
        self.send_anchor = None;
        self.runway.set(Runway::default());
        if ferrite_ui::motion::reduced_motion(cx) {
            self.scroll.scroll_to_bottom();
        } else {
            self.scroll.ease_to(EaseTarget::End);
        }
        cx.notify();
    }
}

/// A row appended live, on its way in: noted when it arrived, its clock
/// started by the first frame that draws it ([`Arrival::drawn`]).
#[derive(Clone, Copy, Debug)]
struct Arrival {
    spec: ferrite_ui::motion::MotionSpec,
    noted: Instant,
    drawn: Option<Instant>,
}

impl Arrival {
    fn new(spec: ferrite_ui::motion::MotionSpec, now: Instant) -> Self {
        Self {
            spec,
            noted: now,
            drawn: None,
        }
    }

    /// Whether it still moves at `now`: drawn, for its spec's length; not
    /// yet drawn (scrolled away, say), for as long from its arrival, after
    /// which it simply stands there when it is.
    fn moving(&self, now: Instant) -> bool {
        let since = self.drawn.unwrap_or(self.noted);
        now.saturating_duration_since(since) < self.spec.duration()
    }

    /// Drawn now: how far into its motion, while it moves. The first draw
    /// starts its clock.
    fn drawn(&mut self, now: Instant) -> Option<f32> {
        if self.drawn.is_none() && self.moving(now) {
            self.drawn = Some(now);
        }
        let drawn = self.drawn?;
        self.moving(now)
            .then(|| self.spec.progress_at(now.saturating_duration_since(drawn)))
    }
}

/// How a row appended live comes in: the operator's bubble pops in over
/// `BUBBLE_IN` (`motion::bubble_face`), what the agent streams fades in
/// over `STREAM_FADE`, opacity only.
fn arrival_fade(kind: RowKind) -> ferrite_ui::motion::MotionSpec {
    if kind == RowKind::Prompt {
        ferrite_ui::motion::BUBBLE_IN
    } else {
        ferrite_ui::motion::STREAM_FADE
    }
}

/// A transcript menu (a card's, an answer's), hung at the press that
/// opened it, kept inside the window, over everything after it.
fn hang_menu(id: &'static str, at: gpui::Point<gpui::Pixels>, shell: gpui::Div) -> AnyElement {
    gpui::deferred(
        gpui::anchored()
            .position(at)
            .snap_to_window_with_margin(px(theme::GRID_PAD))
            .child(ferrite_ui::motion::menu_in(
                id,
                shell,
                ferrite_ui::motion::Opens::Down,
            )),
    )
    .with_priority(2)
    .into_any_element()
}

/// The group a message row names: its actions show under the pointer
/// anywhere on it.
const MESSAGE_GROUP: &str = "transcript-message";

/// A message's actions (`MESSAGE_ACTION`s in a row), laid out at rest but
/// shown only under the pointer anywhere on the message, the latest
/// answer's too (Claude's: actions, not content, never stand at rest), at
/// `shown` (`actions_listener`).
fn message_actions(shown: f32) -> gpui::Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::SPACE_0_5))
        .h(px(theme::MESSAGE_ACTION))
        .opacity(shown)
}

/// The block an answer row is known by: its Markdown run's first.
fn answer_first(blocks: &[ferrite_core::transcript::Block]) -> BlockId {
    let first = blocks.first().expect("markdown row has a block");
    first.markdown_run.unwrap_or(first.id)
}

/// The hover key a message's actions blend under.
fn actions_key(message: &SharedString) -> SharedString {
    format!("actions:{message}").into()
}

/// The pointer on a message: its actions fade in and out over 200ms with
/// no wait (`motion::ACTIONS_IN`, T3's).
fn actions_listener(message: &SharedString) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    ferrite_ui::motion::hover_listener_with(
        actions_key(message),
        ferrite_ui::motion::ACTIONS_IN,
        ferrite_ui::motion::ACTIONS_OUT,
    )
}

/// A message's time among its actions, as T3's reads (`8:26 pm`,
/// `yesterday at 8:26 pm`, `11/9 8:26 pm`: `clock::day_aware_label`), its
/// day in a tooltip (`8:26 pm, 9th November 2026`); meta type in
/// `INK_SUBTLE`. Read as the row draws; the transcript is drawn again as
/// each minute turns, on the pulse clock's one timer, so it never goes
/// stale past midnight. A log from before stamps were kept shows the time
/// it printed (`7:31 pm`).
fn message_time(
    id: SharedString,
    selector: &'static str,
    stamp: &str,
) -> gpui::Stateful<gpui::Div> {
    let time = div()
        .id(id)
        .debug_selector(move || selector.into())
        .px(px(theme::SPACE_1))
        .text_size(px(theme::FS_BADGE))
        .line_height(px(theme::LH_META))
        .text_color(gpui::rgb(theme::INK_SUBTLE));
    let Some(at) = ferrite_core::clock::parse_local(stamp) else {
        // A log from before stamps were kept: the time it printed.
        return time.child(SharedString::from(stamp.to_owned()));
    };
    // The Cockpit's sweep redraws the transcript as the minute turns over
    // (`CockpitView::message_minute`), the only time the label can change.
    let now = ferrite_core::clock::now();
    time.child(SharedString::from(ferrite_core::clock::day_aware_label(
        at, now,
    )))
    .tooltip(ferrite_ui::menu::tooltip(
        ferrite_core::clock::day_aware_tooltip(at),
    ))
}

/// A long thread's date divider (`RowKind::Divider`) over the first prompt
/// of a new day: the day, centred and quiet in `INK_SUBTLE` at the meta
/// size, between `LINE` hairlines.
/// The owner's note under the first prompt (`TranscriptInput::note`): one
/// quiet line.
fn note_line(text: &str) -> gpui::Div {
    div()
        .debug_selector(|| "transcript-note".into())
        .w_full()
        .min_w_0()
        .h(px(theme::LH_META))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_META))
        .line_height(px(theme::LH_META))
        .text_color(gpui::rgb(theme::INK_SUBTLE))
        .child(SharedString::from(text.to_string()))
}

fn date_divider(prompt: &Block) -> gpui::Div {
    let day = prompt
        .sent_at
        .as_deref()
        .and_then(ferrite_core::clock::parse_local)
        .map(|at| ferrite_core::clock::day_heading(at, ferrite_core::clock::now()))
        .unwrap_or_default();
    divider(divider_line())
        .debug_selector(|| "date-divider".into())
        .text_color(gpui::rgb(theme::INK_SUBTLE))
        .child(div().flex_shrink_0().child(SharedString::from(day)))
        .child(divider_rule(divider_line()))
}

/// T3's line / label / line separator: 12px text `DIVIDER_GAP` from a
/// hairline either side in `rule`, `DIVIDER_PAD_Y` above and below. The
/// caller adds the label and the second rule.
fn divider(rule: gpui::Hsla) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(theme::DIVIDER_GAP))
        .w_full()
        .min_w_0()
        .py(px(theme::DIVIDER_PAD_Y))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_BADGE))
        .line_height(px(theme::LH_META))
        .child(divider_rule(rule))
}

/// A divider's hairline ink: `LINE` at 70% (T3's `bg-border/70`).
fn divider_line() -> gpui::Hsla {
    gpui::Hsla::from(theme::paint::LINE).opacity(0.7)
}

fn divider_rule(ink: gpui::Hsla) -> gpui::Div {
    div().flex_1().h(px(theme::HAIRLINE_W)).bg(ink)
}

/// A context compaction (`RowKind::Compaction`, T3's
/// `ContextCompactionTimelineRow`): `Minimize2 · Context compacted 367K →
/// 27.7K tokens` centred between hairlines, the arrow a 12px icon, in
/// `INK_SUBTLE`; shining (`Shine::Activity`) while it runs. A failure reads
/// in `BLOCKED` with `CircleAlert`, its reason after a `·` at 80%, and its
/// rules in `BLOCKED` at 30%.
pub(crate) fn compaction_divider(compaction: &ferrite_core::transcript::Compaction) -> gpui::Div {
    use ferrite_ui::{components, icons};
    let failed = compaction.report.failure.is_some();
    let ink = if failed { theme::BLOCKED } else { theme::INK_SUBTLE };
    let rule: gpui::Hsla = if failed {
        gpui::Hsla::from(gpui::rgb(theme::BLOCKED)).opacity(0.3)
    } else {
        divider_line()
    };
    let glyph = |path| icons::icon(path, theme::DIVIDER_ICON, ink).flex_shrink_0();
    let mut label = div()
        .flex()
        .min_w_0()
        .items_center()
        .gap(px(theme::DIVIDER_LABEL_GAP))
        .child(glyph(if failed { icons::CIRCLE_ALERT } else { icons::MINIMIZE }));
    let title = compaction.title();
    label = match compaction.counts() {
        Some((before, after)) => label
            .child(SharedString::from(format!("{title} {before}")))
            .child(glyph(icons::ARROW_RIGHT))
            .child(SharedString::from(format!("{after} tokens"))),
        None if compaction.running => label.child(components::shine(title, ink, components::Shine::Activity)),
        None => label.child(title),
    };
    if let Some(reason) = compaction.report.failure.as_deref().filter(|r| !r.is_empty()) {
        label = label.child(
            div()
                .min_w_0()
                .truncate()
                .opacity(0.8)
                .child(SharedString::from(format!("\u{b7} {reason}"))),
        );
    }
    divider(rule)
        .debug_selector(|| "compaction-divider".into())
        .text_color(gpui::rgb(ink))
        .child(label)
        .child(divider_rule(rule))
}

/// The top fade (`EDGE_FADE`): the ground (T3's stops) fading to nothing, over
/// what scrolls under the top of the transcript or its pinned prompt.
fn top_fade(solo: bool) -> gpui::Div {
    ferrite_ui::components::fade(
        gpui::rgb(theme::tx_ground(solo)).into(),
        false,
        theme::EDGE_FADE,
        &theme::TX_TOP_FADE_STOPS,
    )
}

/// The foot fade: the same fill rising from the transcript's foot while a
/// row runs on under it (the reader scrolled away from the tail, or the
/// tail grown past it), so a row cut there fades into the Composer rather
/// than ending on a hard cut.
fn bottom_fade(solo: bool) -> gpui::Div {
    ferrite_ui::components::edge_fade(gpui::rgb(theme::tx_ground(solo)).into(), true)
}

/// When each turn's answer finished, by the turn's last prose block: its
/// turn end's `completed_at` (`7:31 pm`), when core observed one.
fn answer_times(blocks: &[Block]) -> HashMap<BlockId, String> {
    let mut times = HashMap::new();
    let mut prose = None;
    for block in blocks {
        match &block.body {
            Body::Prompt(_) => prose = None,
            Body::Paragraph { .. }
            | Body::Heading { .. }
            | Body::Bullet { .. }
            | Body::Code { .. } => prose = Some(block.id),
            Body::TurnEnd(end) => {
                if let (Some(id), Some(at)) = (prose.take(), end.completed_at.clone()) {
                    times.insert(id, at);
                }
            }
            _ => {}
        }
    }
    times
}

/// What prose path targets resolve against: the checkout, and every path
/// this window's calls named (their arguments and their diffs).
fn target_context(input: &TranscriptInput) -> TargetContext {
    let mut known: Vec<String> = Vec::new();
    for block in &input.blocks {
        let Body::Tool(tool) = &block.body else {
            continue;
        };
        if pane::argument_is_path(tool) && !known.contains(&tool.summary) {
            known.push(tool.summary.clone());
        }
        for diff in &tool.diffs {
            if !known.contains(&diff.path) {
                known.push(diff.path.clone());
            }
        }
    }
    TargetContext {
        workspace: input.workspace.clone(),
        known,
    }
}

/// What a minimap tick marks: a prompt, a failure, a Decision, a pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mark {
    Prompt,
    Failure,
    Decision,
    Pass,
}

impl Mark {
    fn ink(self) -> u32 {
        match self {
            Self::Prompt => theme::INK_STRONG,
            Self::Failure => theme::BLOCKED,
            Self::Decision => theme::ATTENTION,
            Self::Pass => theme::RUNNING,
        }
    }

    /// What a row is worth a tick for, if anything: a prompt, a failed call
    /// or turn, a Decision (its record, or the pending one at the tail), a
    /// passing check.
    pub(crate) fn of(row: &TranscriptRow) -> Option<Self> {
        match row.kind() {
            RowKind::Prompt => return Some(Self::Prompt),
            RowKind::Tail => return Some(Self::Decision),
            RowKind::Working | RowKind::LiveTail => return None,
            _ => {}
        }
        let mut pass = false;
        for block in row.blocks() {
            match &block.body {
                Body::Tool(tool) => match &tool.state {
                    ToolState::Failed(_) => return Some(Self::Failure),
                    ToolState::Ok if ferrite_core::docview::is_test_run(tool) => pass = true,
                    _ => {}
                },
                Body::TurnEnd(end) => {
                    if matches!(end.outcome, ferrite_core::TurnOutcome::Error(_)) {
                        return Some(Self::Failure);
                    }
                }
                Body::Meta(_) => return Some(Self::Decision),
                _ => {}
            }
        }
        pass.then_some(Self::Pass)
    }
}

impl TranscriptView {
    /// The prompt band the top of the viewport reads under, and how far the
    /// next turn's band pushes it up: `None` while the band is in place (or
    /// no prompt heads the rows in view).
    fn pinned_prompt(&self) -> Option<(usize, gpui::Pixels)> {
        pinned_of(&self.rows, self.scroll.list_state(), self.pinned_h.get())
    }

    /// Every row's estimated top and the whole transcript's height, from the
    /// heights the list last measured (an unmeasured row counts as the
    /// measured average).
    fn estimated_tops(&self) -> (Vec<f32>, f32) {
        let heights = self.heights.borrow();
        let rows = self.rows.rows();
        let known: Vec<f32> = rows
            .iter()
            .filter_map(|row| heights.get(row.id()).copied())
            .collect();
        let fallback = if known.is_empty() {
            3.0 * self.grid().line
        } else {
            known.iter().sum::<f32>() / known.len() as f32
        };
        let mut tops = Vec::with_capacity(rows.len());
        let mut at = 0.0;
        for row in rows {
            tops.push(at);
            at += heights.get(row.id()).copied().unwrap_or(fallback);
        }
        (tops, at + theme::body_pad_b(self.input.solo))
    }

    /// Where the viewport's top is, in the estimated tops' space.
    fn scroll_position(&self, tops: &[f32], total: f32) -> f32 {
        let state = self.scroll.list_state();
        let view_h = f32::from(state.viewport_bounds().size.height);
        if self.scroll.is_following_tail() {
            return (total - view_h).max(0.);
        }
        let top = state.logical_scroll_top();
        tops.get(top.item_ix).copied().unwrap_or(total) + f32::from(top.offset_in_item)
    }

    /// Hold the viewport's top at `at` in the estimated tops' space.
    fn jump_to(&self, at: f32, tops: &[f32]) {
        if tops.is_empty() {
            return;
        }
        let index = tops.iter().rposition(|top| *top <= at).unwrap_or(0);
        self.scroll.scroll_to(index, px((at - tops[index]).max(0.)));
    }

    /// Scroll to `goal` (estimated tops' space) in one frame, stopping any
    /// programmatic scroll on its way. T3 eases its minimap's jumps; this
    /// one snaps, as a drag along the rail lands here on every move.
    fn jump_to_goal(&mut self, goal: f32, cx: &mut Context<Self>) {
        self.operator_scrolled();
        let (tops, total) = self.estimated_tops();
        self.jump_to(goal.clamp(0., total), &tops);
        cx.notify();
    }

    /// The minimap (the WP-A minimap tokens): a tick per prompt, failure,
    /// Decision and pass, the viewport as a translucent band while the
    /// transcript overflows, shown while the pointer is on the Pane. A click
    /// on a tick jumps to its row; anywhere else on the rail centres the
    /// view on that point.
    fn minimap(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let rows = self.rows.rows();
        if rows.is_empty() {
            return None;
        }
        let state = self.scroll.list_state().clone();
        let viewport = state.viewport_bounds();
        let view_h = f32::from(viewport.size.height);
        let (tops, total) = self.estimated_tops();
        if view_h <= 0. || total <= 0. {
            return None;
        }
        let overflows = total > view_h + 1.;
        let scrolled = self.scroll_position(&tops, total);
        let band_top = (scrolled / total).clamp(0., 1.);
        let band_h = (view_h / total).clamp(0., 1.);
        let entity = cx.entity().downgrade();
        let tops = Rc::new(tops);
        let ticks: Vec<_> = rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                let mark = Mark::of(row)?;
                let at = tops[index] / total;
                let entity = entity.clone();
                let goal = tops[index] + row.gap();
                Some(
                    div()
                        .id(SharedString::from(format!("minimap-tick-{index}")))
                        .debug_selector(move || format!("minimap-tick-{mark:?}"))
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(gpui::relative(at))
                        .h(px(theme::MINIMAP_TICK_H + 2. * theme::MINIMAP_TICK_INSET))
                        .mt(px(-theme::MINIMAP_TICK_INSET))
                        .flex()
                        .items_center()
                        .px(px(theme::MINIMAP_TICK_INSET))
                        .child(
                            div()
                                .w_full()
                                .h(px(theme::MINIMAP_TICK_H))
                                .rounded(px(theme::MINIMAP_TICK_R))
                                .bg(gpui::rgb(mark.ink())),
                        )
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            let _ = entity.update(cx, |view, cx| view.jump_to_goal(goal, cx));
                        }),
                )
            })
            .collect();
        let rail = SharedString::from(format!("transcript-minimap-{}", self.input.namespace));
        let band_key = SharedString::from(format!("{rail}-band"));
        let band_t = ferrite_ui::motion::hover_t(&band_key);
        let band_ink = ferrite_ui::motion::mix(
            gpui::rgba(theme::MINIMAP_BAND).into(),
            gpui::rgba(theme::MINIMAP_BAND_HOVER).into(),
            band_t,
        );
        if band_t > 0. && band_t < 1. {
            crate::file_links::mark_fading();
        }
        let blend = ferrite_ui::motion::hover_listener(band_key);
        let hover_entity = entity.clone();
        let jump_entity = entity;
        Some(
            div()
                .id(rail)
                .debug_selector(|| "transcript-minimap".into())
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .w(px(theme::MINIMAP_W))
                .cursor_pointer()
                .opacity(0.)
                .group_hover(pane::PANE_GROUP, |style| style.opacity(1.))
                .on_hover(move |over, window, cx| {
                    blend(over, window, cx);
                    let _ = hover_entity.update(cx, |_, cx| cx.notify());
                })
                .when(overflows, |rail| {
                    rail.child(
                        div()
                            .id("minimap-view")
                            .absolute()
                            .left(px(theme::MINIMAP_BAND_INSET))
                            .right(px(theme::MINIMAP_BAND_INSET))
                            .top(gpui::relative(band_top))
                            .h(gpui::relative(band_h))
                            .min_h(px(theme::MINIMAP_BAND_MIN_H))
                            .rounded(px(theme::MINIMAP_BAND_R))
                            .bg(band_ink),
                    )
                })
                .children(ticks)
                .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                    cx.stop_propagation();
                    let viewport = state.viewport_bounds();
                    let height = f32::from(viewport.size.height).max(1.);
                    let fraction =
                        (f32::from(event.position.y - viewport.top()) / height).clamp(0., 1.);
                    let goal = (fraction * total - view_h / 2.).max(0.);
                    let _ = jump_entity.update(cx, |view, cx| view.jump_to_goal(goal, cx));
                })
                .into_any_element(),
        )
    }
}

/// The prompt band the top of the viewport reads under (see
/// `TranscriptView::pinned_prompt`), from the list's last layout: `None`
/// before the list has laid out, while the band is in place, or when no
/// prompt heads the rows in view.
fn pinned_of(
    rows: &TranscriptRows,
    state: &gpui::ListState,
    pinned_h: gpui::Pixels,
) -> Option<(usize, gpui::Pixels)> {
    let rows = rows.rows();
    let viewport = state.viewport_bounds();
    if rows.is_empty() || viewport.size.height <= px(0.) {
        return None;
    }
    let top = state.logical_scroll_top();
    // Past the last row is the tail's sentinel before layout resolves it.
    if top.item_ix >= rows.len() {
        return None;
    }
    let prompt = rows[..=top.item_ix]
        .iter()
        .rposition(|row| row.kind() == RowKind::Prompt)?;
    if prompt == top.item_ix && top.offset_in_item <= px(rows[prompt].gap()) {
        return None;
    }
    let mut shift = px(0.);
    if let Some(next) = (top.item_ix + 1..rows.len()).find(|ix| rows[*ix].kind() == RowKind::Prompt)
    {
        if let Some(bounds) = state.bounds_for_item(next) {
            let band_top = bounds.top() + px(rows[next].gap()) - viewport.top();
            if band_top < pinned_h {
                shift = band_top - pinned_h;
            }
        }
    }
    Some((prompt, shift))
}

impl Render for TranscriptView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The working row's spinner and shimmer are loops: the overlay
        // draws them, so their steps rebuild nothing of this transcript.
        ferrite_ui::loops_overlay::begin(window, cx.entity_id());
        self.document.begin_viewport_update(cx);
        crate::file_links::begin_layout(&self.input.namespace, cx);
        // The answers' file links are this transcript's path targets.
        self.rich.set_path_scope(self.input.namespace.clone());
        self.settle_found(window);
        if self.fading(cx) {
            window.request_animation_frame();
        }
        let rows = self.rows.clone();
        let selection = self.text_runs();
        let view = cx.entity();
        // The pinned band reads the last layout, before a tail follow
        // resets the list to its end sentinel for the next one.
        let pinned = self.pinned_prompt();
        self.pinned_drawn.set(pinned);
        if self.scroll.is_following_tail() {
            self.scroll.scroll_to_bottom();
        }
        let grid = self.grid();
        let solo = self.input.solo;
        let row_inset = theme::tx_inset_at(self.list_w.get(), solo);
        let list = list(
            self.scroll.list_state().clone(),
            move |index, window, cx| {
                let Some(row) = rows.get(index).cloned() else {
                    return div().into_any_element();
                };
                let (element, arrival) = view.update(cx, |view, cx| {
                    selection.begin_row();
                    let element =
                        view.render_row(&row, &selection, Some(cx.entity()), Some(window), cx);
                    // A path underline mid-fade: draw again next frame.
                    if crate::file_links::take_fading() {
                        cx.notify();
                    }
                    (element, view.arrival(row.id(), cx))
                });
                let bubble = row.kind() == RowKind::Prompt;
                // Every row is wrapped: a list item is laid out as its own
                // root, where a bare row's `w_full` has no parent width to
                // resolve against and shrinks to its text. The wrapper holds
                // the transcript column's inset (`tx_inset`), centres the
                // reading column (`TX_COLUMN_MAX_W` at most), and holds the
                // row's own gap above it (the first row's is the body's top
                // padding: the list's own top padding flickers mid-scroll).
                let gap = row.gap();
                let wrapper = div().w_full().pt(px(gap)).px(px(row_inset));
                // A row appended live comes in where it stands (its frames
                // come from the transcript's render, `fading`): the sent
                // bubble pops from its right edge, the rest fade.
                let column = div()
                    .w_full()
                    .max_w(px(theme::TX_COLUMN_MAX_W))
                    .mx_auto()
                    .child(element);
                match arrival {
                    Some(t) if bubble => wrapper.child(ferrite_ui::motion::bubble_face(column, t)),
                    Some(t) => wrapper.opacity(t).child(column),
                    None => wrapper.child(column),
                }
                .into_any_element()
            },
        )
        // The bottom padding is the list's own: it counts in the scroll
        // extent and the tail follow. A band jump's room rides it.
        .pb(px(
            theme::body_pad_b(self.input.solo) + self.runway.get().room
        ))
        .size_full()
        .min_h_0();
        let scroll = self.scroll.clone();
        let gaps = self.rows.clone();
        let heights = self.heights.clone();
        let weak = cx.entity().downgrade();
        let pinned_drawn = self.pinned_drawn.clone();
        let pinned_asked = self.pinned_asked.clone();
        let pinned_h = self.pinned_h.clone();
        let runway = self.runway.clone();
        let shelf = self.visuals.clone();
        let list_w = self.list_w.clone();
        let list_h = self.list_h.clone();
        let reading = self.input.reading_size;
        let inset = row_inset;
        let pad_b = theme::body_pad_b(solo);
        let list = div()
            .on_children_prepainted(move |_, window, cx| {
                let anchored = scroll.did_layout();
                // A programmatic scroll on its way moves one frame more (the
                // render asked for the frame: `fading`); landed, the list lays
                // out once more at its target. A scrollbar drag took it: the
                // room stays, the prompt is no longer held.
                let foot = px(pad_b + runway.get().room);
                match scroll.step_ease(cx.background_executor().now(), foot) {
                    EaseStep::Idle | EaseStep::Moving => {}
                    EaseStep::Landed => {
                        let weak = weak.clone();
                        window.defer(cx, move |_, cx| {
                            let _ = weak.update(cx, |_, cx| cx.notify());
                        });
                    }
                    EaseStep::Cancelled => {
                        let weak = weak.clone();
                        window.defer(cx, move |_, cx| {
                            let _ = weak.update(cx, |view, cx| {
                                view.operator_scrolled();
                                cx.notify();
                            });
                        });
                    }
                }
                // A band jump's room: enough under the last row for the
                // band to reach the top, used up as rows arrive under it.
                {
                    let state = scroll.list_state();
                    let mut now = runway.get();
                    // The rows' height from `from` down, when the list laid
                    // them all out (a row past the viewport means there is
                    // height enough already).
                    let below = |from: usize| -> Option<f32> {
                        let mut total = 0.0;
                        for index in from..gaps.len() {
                            total += f32::from(state.bounds_for_item(index)?.size.height);
                        }
                        Some(total)
                    };
                    if let Some(into) = now.jump {
                        let measured = below(now.index);
                        let view_h = f32::from(state.viewport_bounds().size.height);
                        let room =
                            measured.map_or(0.0, |below| (into + view_h - below - pad_b).max(0.0));
                        let settled = (room - now.room).abs() < 0.5;
                        now = Runway {
                            jump: (!settled).then_some(into),
                            index: now.index,
                            room,
                            below: measured.unwrap_or(now.below),
                        };
                        runway.set(now);
                        if !settled {
                            // An ease on its way owns the offset; the room
                            // is still sized under it.
                            if !scroll.easing() {
                                state.scroll_to(gpui::ListOffset {
                                    item_ix: now.index,
                                    offset_in_item: px(into),
                                });
                            }
                            let weak = weak.clone();
                            window.defer(cx, move |_, cx| {
                                let _ = weak.update(cx, |_, cx| cx.notify());
                            });
                        }
                    } else if now.room > 0.0 {
                        match below(now.index) {
                            Some(grown) if grown > now.below + 0.5 => {
                                now.room = (now.room - (grown - now.below)).max(0.0);
                                now.below = grown;
                                runway.set(now);
                            }
                            Some(_) => {}
                            // The rows under it outgrew the viewport.
                            None => runway.set(Runway::default()),
                        }
                    }
                }
                // A pinned band covers the list's top down to its foot; the
                // rows pass under it into its fade.
                let now = pinned_of(&gaps, scroll.list_state(), pinned_h.get());
                let settled = scroll.set_top_mask(now.map(|(_, shift)| pinned_h.get() + shift));
                // What the list measured feeds the minimap's estimates.
                {
                    let state = scroll.list_state();
                    let mut heights = heights.borrow_mut();
                    let start = state.logical_scroll_top().item_ix;
                    for index in start..gaps.len() {
                        let Some(bounds) = state.bounds_for_item(index) else {
                            break;
                        };
                        if let Some(row) = gaps.get(index) {
                            heights.insert(row.id().clone(), f32::from(bounds.size.height));
                        }
                    }
                    if heights.len() > 2 * gaps.len() + 64 {
                        let live: HashSet<_> = gaps.rows().iter().map(|row| row.id()).collect();
                        heights.retain(|id, _| live.contains(id));
                    }
                }
                // The visuals near the screen stay live (or open before
                // they scroll in); the far ones may go. After the event: a
                // view's open can wake the engine synchronously.
                if !gaps.visual_rows().is_empty() {
                    let shelf = shelf.clone();
                    let state = scroll.list_state().clone();
                    let measured = heights.clone();
                    let rows = gaps.clone();
                    window.defer(cx, move |window, cx| {
                        let measured = measured.borrow();
                        let height_of = |index: usize| {
                            rows.get(index)
                                .and_then(|row| measured.get(row.id()).copied())
                        };
                        let visuals: Vec<(usize, &crate::visual::Visual)> = rows
                            .visual_rows()
                            .iter()
                            .filter_map(|&index| {
                                let block = rows.get(index)?.blocks().first()?;
                                crate::visual::visual_of(block).map(|visual| (index, visual))
                            })
                            .collect();
                        shelf.reach(&state, &visuals, &height_of, reading, inset, window, cx);
                    });
                }
                // The band pinned at render came from the previous layout:
                // when this layout moves it, render again.
                let drawn = pinned_drawn.get();
                let moved = match (now, drawn) {
                    (Some((a, x)), Some((b, y))) => a != b || (x - y).abs() > px(0.5),
                    (None, None) => false,
                    _ => true,
                };
                // Ask once per position: a layout that keeps moving the band
                // (it cannot, but a loop must never be possible) waits for the
                // next ordinary render.
                if moved && pinned_asked.get() != Some(now) {
                    pinned_asked.set(Some(now));
                    let weak = weak.clone();
                    window.defer(cx, move |_, cx| {
                        let _ = weak.update(cx, |_, cx| cx.notify());
                    });
                }
                list_h.set(f32::from(scroll.list_state().viewport_bounds().size.height));
                let width = scroll.list_state().viewport_bounds().size.width;
                if (f32::from(width) - list_w.get()).abs() >= 0.5 {
                    // A visual row lays its page out from it, and the rows'
                    // inset follows it (`tx_inset_at`): draw again.
                    let had_visuals = !gaps.visual_rows().is_empty();
                    let inset_moved =
                        theme::tx_inset_at(f32::from(width), solo) != inset;
                    list_w.set(f32::from(width));
                    if inset_moved {
                        scroll.remeasure_held();
                    }
                    if had_visuals || inset_moved {
                        let weak = weak.clone();
                        window.defer(cx, move |_, cx| {
                            let _ = weak.update(cx, |_, cx| cx.notify());
                        });
                    }
                }
                if anchored || settled {
                    // Lay this transcript (and the Pane around it) out again
                    // where the scroll settled; nothing else moved.
                    let weak = weak.clone();
                    window.defer(cx, move |_, cx| {
                        let _ = weak.update(cx, |_, cx| cx.notify());
                    });
                }
            })
            .id(SharedString::from(format!(
                "transcript-{}",
                self.input.namespace
            )))
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .w_full()
            .min_h_0()
            // One grid for the whole transcript (theme rule 1): Geist at
            // the reading size on its 1.5x line, every row inheriting it;
            // a row laid out on cells sets Geist Mono (`pane::on_cells`).
            .font_family(theme::FONT_UI)
            .text_size(px(grid.size))
            .line_height(px(grid.line))
            .text_color(gpui::rgb(theme::INK_STRONG))
            .hover_text()
            .track_focus(&self.transcript_focus)
            .capture_any_mouse_down(cx.listener(Self::pressed))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::released))
            // A scroll fades the Cite button out, and stops a programmatic
            // scroll where it stands.
            .on_scroll_wheel(cx.listener(|view, _, _, cx| {
                view.operator_scrolled();
                view.scrolled(cx)
            }))
            .child(list)
            .child(
                div()
                    .id("transcript-controls-end")
                    .track_focus(&self.controls_end),
            )
            .text_selection_scope(if self.input.focused {
                gpui::base::TextSelectionScopeId::default()
            } else {
                self.input.selection_scope
            });
        let pinned = pinned.and_then(|(index, shift)| self.pinned_band(index, shift, cx));
        // Scrolled past the body's top padding: a fade over the padding
        // alone would dim the first row (the list's logical top can name a
        // later row while the rows fit, so read the pixel offset).
        let scrolled = -self.scroll.list_state().scroll_px_offset_for_scrollbar().y
            > px(self.input.shape().top_pad.max(1.));
        // A row runs on under the foot: it fades into the Composer, as
        // Codex's do, rather than ending on a hard cut.
        let cut_below = self.rows_below() > px(0.);
        let latest = self.scroll_to_end_pill(cx);
        let minimap = self.minimap(cx);
        if crate::file_links::take_fading() {
            cx.notify();
        }
        let reply = self.reply_float(window, cx);
        let output_menu = self.output_menu_float(cx);
        let message_menu = self.message_menu_float(cx);
        let find_bar = self.find_bar();
        div()
            .relative()
            .flex()
            .flex_col()
            .min_w_0()
            .size_full()
            .min_h_0()
            .font_family(theme::FONT_UI)
            .text_size(px(grid.size))
            .line_height(px(grid.line))
            .child(list)
            // The thin overlay bar (Claude's): it shows under the pointer
            // and while the list moves, and drags the list.
            .child(
                ferrite_ui::components::scrollbar_layer()
                    .debug_selector(|| "transcript-scrollbar".into())
                    .child(ferrite_ui::components::scrollbar_bar(
                        SharedString::from(format!("transcript-scrollbar-{}", self.input.namespace)),
                        self.scroll.list_state(),
                    )),
            )
            // What scrolls under the top fades out into the card, under the
            // pinned prompt when there is one.
            .children(
                pinned
                    .is_none()
                    .then_some(scrolled)
                    .filter(|s| *s)
                    .map(|_| {
                        div()
                            .debug_selector(|| "transcript-top-fade".into())
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .child(top_fade(self.input.solo))
                    }),
            )
            .children(pinned)
            .children(cut_below.then(|| {
                div()
                    .debug_selector(|| "transcript-bottom-fade".into())
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .child(bottom_fade(self.input.solo))
            }))
            .children(latest)
            .children(minimap)
            .children(reply)
            .children(output_menu)
            .children(message_menu)
            .children(find_bar)
            .on_action(cx.listener(|view, _: &crate::find::Next, _, cx| view.find_step(true, cx)))
            .on_action(
                cx.listener(|view, _: &crate::find::Previous, _, cx| view.find_step(false, cx)),
            )
            .on_action(cx.listener(|view, _: &crate::find::Close, _, cx| view.close_find(cx)))
    }
}

#[cfg(test)]
mod tests {
    use super::{answer_times, Arrival};
    use ferrite_ui::motion::{BUBBLE_IN, STREAM_FADE};
    use std::time::{Duration, Instant};

    /// A row's entrance starts on the frame that first draws it, however
    /// long the frame after its arrival took: the whole entrance plays.
    #[test]
    fn an_arrival_starts_its_clock_on_its_first_draw() {
        let noted = Instant::now();
        let ms = |ms: u64| noted + Duration::from_millis(ms);
        let mut arrival = Arrival::new(BUBBLE_IN, noted);
        assert_eq!(
            arrival.drawn(ms(200)),
            Some(0.0),
            "drawn late, from its start"
        );
        let half = arrival.drawn(ms(200) + BUBBLE_IN.duration() / 2).unwrap();
        assert!(half > 0.0 && half < 1.1, "mid-entrance: {half}");
        assert!(arrival.moving(ms(200) + BUBBLE_IN.duration() - Duration::from_millis(1)));
        assert_eq!(
            arrival.drawn(ms(200) + BUBBLE_IN.duration()),
            None,
            "landed"
        );
    }

    /// A row never drawn (scrolled away) asks for frames no longer than
    /// its entrance would have run, and then simply stands there.
    #[test]
    fn an_arrival_never_drawn_stops_asking() {
        let noted = Instant::now();
        let mut arrival = Arrival::new(STREAM_FADE, noted);
        assert!(arrival.moving(noted + STREAM_FADE.duration() / 2));
        let late = noted + STREAM_FADE.duration();
        assert!(!arrival.moving(late));
        assert_eq!(arrival.drawn(late), None, "drawn after: no entrance");
    }
    use ferrite_core::transcript::{Body, Input, Transcript};
    use ferrite_core::SessionEvent;

    fn turn(transcript: &mut Transcript, prompt: &str, answer: &str, at: Option<&str>) {
        transcript.apply(Input::Prompt(prompt.into()));
        transcript.apply(Input::Event(SessionEvent::TextDelta {
            text: answer.into(),
        }));
        if let Some(at) = at {
            transcript.apply(Input::CompletionObservation {
                elapsed_ms: 1_000,
                completed_at: at.into(),
                input_tokens: None,
                output_tokens: None,
            });
        }
    }

    /// An answer's time is when its turn finished, keyed by the turn's last
    /// prose; a turn never observed to finish has none.
    #[test]
    fn an_answer_is_timed_by_its_turn_end() {
        let mut transcript = Transcript::default();
        turn(&mut transcript, "one", "First.\n\n", Some("7:10 pm"));
        turn(&mut transcript, "two", "Second.\n\n", None);
        let times = answer_times(transcript.blocks());
        let prose: Vec<_> = transcript
            .blocks()
            .iter()
            .filter(|block| matches!(block.body, Body::Paragraph { .. }))
            .map(|block| block.id)
            .collect();
        assert_eq!(times.len(), 1);
        assert_eq!(times.get(&prose[0]).map(String::as_str), Some("7:10 pm"));
    }
}
