//! The context menu (theme WP-E, `menu`): what a right-click, a row's `⋯`
//! or a title's ▾ offers for a Thread, a Pane, a Project, the
//! Parked view, a Review line, a prompt's Rewind or a page — the rows each
//! target offers (`context_rows`), their one-letter keys, a destructive
//! row's arming, a parent row's submenu beside it (`Submenu`), and the
//! verbs the rows run (`run_menu_verb`).

use super::menu_button::Edge;
use super::*;
use ferrite_pane::link_target::{thread_link, Open};

/// What a right-click was on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum MenuTarget {
    /// A nav row.
    Thread(ThreadKey),
    /// A nav row's Snooze button: the reminder choices directly.
    Snooze(ThreadKey),
    /// A Pane — the same Thread, but the rename opens in the head.
    Pane(ThreadKey),
    /// The Parked shelf's header at the foot of the nav.
    Parked,
    /// The search row's Filter button: a Project to filter by.
    NavFilter,
    /// The search row's Factory button: a Project's board to open.
    NavFactory,
    /// A diff line in the Review beside the Thread's Pane (ADR 0021).
    Review {
        thread: ThreadId,
        line: usize,
    },
    /// A sent prompt's Rewind: how much of it to take back.
    Prompt {
        thread: ThreadId,
        block: ferrite_core::transcript::BlockId,
    },
    /// The page a Pane's preview shows: its title's menu.
    Page(PaneIdentity),
    /// A Files tree row (the path is the Files state's `pressed`).
    File(ThreadId),
    /// The Files pane's `⋯`.
    FilesMore(ThreadId),
    /// The Changes scope dropdown, and its `⋯`.
    ChangesScope(tools::Diff),
    ChangesMore(tools::Diff),
    /// The Changes toolbar's base: the branches to compare against.
    ChangesBase(tools::Diff),
    /// The terminal's right-click, and its `⋯`.
    Terminal(ThreadId),
    TerminalMore(ThreadId),
    /// The Panel head's `+`: every Surface the Thread's Panel can open.
    SurfaceMenu(ThreadId),
    /// A Panel tab's right-click, by its place in the strip.
    SurfaceTab { thread: ThreadId, at: usize },
    /// The pull request Surface's `⋯`.
    PrMore(super::panel::pr_read::PrTarget),
    /// The nav card's PR badge: the PR's own doors, not the Thread's.
    PrLink(ThreadKey),
    /// A Factory picker (#221): its rows are the Factory view's `picks`.
    Factory,
    /// A right-click on the Thread's web page.
    BrowserPage(ThreadId),
    Details(ThreadId),
}

/// One thing a context-menu row does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum MenuVerb {
    Rename,
    Snooze(ferrite_core::snooze::Preset),
    SnoozeCustom,
    Wake,
    /// List the Thread under the nav's `Pinned`, or no longer.
    Pin,
    Unpin,
    Focus,
    /// Unfold or fold the Parked section.
    ToggleParked,
    /// Delete every Thread the Parked section lists — exactly those rows,
    /// so what the menu erases is what the operator can see.
    DeleteAllParked,
    /// Park the Thread in one act, on screen or not (`Cockpit::park_thread`),
    /// the same as cmd-w.
    Park,
    /// The transcript menu's own verbs: what is highlighted (in the
    /// Review, the pick), or all of it.
    CopySelection,
    CopyTranscript,
    /// The Review's own verbs: comment on the pick, read the line's file.
    Comment,
    OpenAtLine,
    NewThread,
    Reveal,
    CopyPath,
    /// The Thread's PR: ask `gh` again now, or open it in the browser.
    RefreshPr,
    OpenPr,
    Delete,
    /// Back to before the prompt (`MenuTarget::Prompt`): the conversation
    /// alone, or its files too.
    Rewind,
    RewindCode,
    /// A page's own verbs: read its file again, open it in the browser.
    Reload,
    OpenInBrowser,
    /// A parent row: its submenu opens beside it, under the pointer or on
    /// a press.
    Submenu(Submenu),
    /// The Thread's checkout in a Terminal window (`Open in › Terminal`).
    OpenTerminal,
    /// Claude's `Mark as unread` (`Notifications::mark_unread`).
    MarkUnread,
    /// A new Thread holding the whole conversation, opened focused.
    Fork,
    /// A Files verb (`tools::FilesVerb`).
    Files(tools::FilesVerb),
    /// A Changes verb (`tools::ChangesVerb`).
    Changes(tools::ChangesVerb),
    /// A Terminal verb (`tools::TerminalVerb`).
    Terminal(tools::TerminalVerb),
    /// A web page verb (`browser::page_menu::PageVerb`).
    Browser(super::browser::PageVerb),
    /// The Thread's id in the form the CLI takes (`thread_link`).
    CopyLink,
    /// A `+` menu row: its Surface opens in the Panel.
    OpenSurface(panel::Opener),
    /// A Panel tab's closes.
    CloseSurface,
    CloseOtherSurfaces,
    CloseSurfacesRight,
    CloseAllSurfaces,
    FilterProject(Option<ProjectId>),
    OpenFactory(ProjectId),
    /// A pull request Surface's row.
    Pr(panel::PrAction),
    /// A PR link's rows (`MenuTarget::PrLink`): open it by a door, or copy
    /// its forge address.
    PrOpen(Open),
    PrCopy,
    /// The Factory picker's row at this index.
    Factory(usize),
    DetailsItem(ferrite_core::settings::ContextCardItem),
    DetailsReset,
}

/// A parent row's submenu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Submenu {
    /// Where the Thread's checkout opens: Finder, a Terminal.
    OpenIn,
    Snooze,
    /// The Files menus' parent rows: `Copy ›` and `Tab size ›`.
    Files(tools::FilesSub),
    /// `Compare against ›`.
    Changes(tools::ChangesSub),
    /// `Cursor style ›` and `Scrollback ›`.
    Terminal(tools::TerminalSub),
}

/// The context menu up on screen.
#[derive(Clone)]
pub(super) struct ContextMenu {
    pub(super) target: MenuTarget,
    pub(super) at: Point<Pixels>,
    /// The menu's corner at `at`: its top left at the pointer, the corner
    /// on its button's edge under a button (`open_context_menu_below`).
    pub(super) corner: gpui::Anchor,
    /// Opened under a button: `at` is `MENU_UNDER_GAP` over the button and
    /// this drops the menu to the same gap under it. Where the window has
    /// less room below than the menu needs, the menu scrolls there, or
    /// hangs over the button where even two rows would not fit (see
    /// `context_menu_element`). None at the pointer.
    pub(super) drop: Option<Pixels>,
    pub(super) rows: Vec<Option<(menu::Item, MenuVerb)>>,
    /// Opened from its own button (`menu_button::Door`): a press there is
    /// the button's, which shuts it.
    pub(super) from_door: bool,
    /// The destructive row pressed once, waiting for its confirmation.
    pub(super) armed: Option<usize>,
    /// The submenu open beside its parent row: that row's index and the
    /// submenu's rows.
    pub(super) sub: Option<(usize, Vec<(menu::Item, MenuVerb)>)>,
    /// Where the keyboard's cursor stands; none until a key moves it.
    pub(super) cursor: MenuCursor,
}

/// The row the keyboard would press: in the menu, or in the submenu open
/// beside it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum MenuCursor {
    Idle,
    Row(usize),
    SubRow(usize),
}

/// What heads a copy that is only the latest part of a conversation.
const PARTIAL_HISTORY: &str =
    "[Earlier messages are not included: this view keeps only the latest.]";

/// The key the open submenu records its bounds under: a press there is
/// the menu's, though it lies outside the menu.
pub(super) const SUBMENU: &str = "context-submenu";

impl CockpitView {
    /// `thread`'s conversation to the clipboard. Once the view dropped its
    /// oldest Blocks, the whole of it is replayed from the Thread's log off
    /// the window's thread; with no log to read here, or none readable, the
    /// kept part is copied and says it is partial.
    pub(super) fn copy_transcript(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(open) = self.cockpit.read(cx).thread(thread) else {
            return;
        };
        let kept = plain_text(open.transcript().blocks());
        if kept.is_empty() {
            return;
        }
        if !open.transcript().earlier_dropped() {
            cx.write_to_clipboard(ClipboardItem::new_string(kept));
            return;
        }
        let partial = format!("{PARTIAL_HISTORY}\n\n{kept}");
        let Some(store) = self.cockpit.read(cx).history_log(thread).cloned() else {
            cx.write_to_clipboard(ClipboardItem::new_string(partial));
            return;
        };
        cx.spawn(async move |_, cx| {
            let whole = cx
                .background_executor()
                .spawn(async move { store.full_history_text(thread) })
                .await;
            let text = whole.ok().filter(|text| !text.is_empty()).unwrap_or(partial);
            cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_string(text)));
        })
        .detach();
    }

    /// Summon the context menu for `target` at the pointer, in place of
    /// whatever popup was up (`show_popup`).
    pub(super) fn open_context_menu(
        &mut self,
        target: MenuTarget,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.open_context_menu_at(target, at, gpui::Anchor::TopLeft, None, cx);
    }

    /// The menu ⇧F10 opens under a row, as Claude's `⋯` does: under it,
    /// its right edge on the row's.
    pub(super) fn open_context_menu_under(
        &mut self,
        target: MenuTarget,
        row: gpui::Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.open_context_menu_below(target, row, Edge::Right, false, cx);
    }

    /// The menu under `button`, its `edge` on the button's matching edge,
    /// `MENU_UNDER_GAP` below it, scrolling where the window has less room
    /// below than its rows need and over the button where it has room for
    /// less than two.
    pub(super) fn open_context_menu_below(
        &mut self,
        target: MenuTarget,
        button: gpui::Bounds<Pixels>,
        edge: Edge,
        from_door: bool,
        cx: &mut Context<Self>,
    ) {
        let gap = px(ferrite_ui::theme::MENU_UNDER_GAP);
        let (x, corner) = match edge {
            Edge::Left => (button.left(), gpui::Anchor::TopLeft),
            Edge::Right => (button.right(), gpui::Anchor::TopRight),
        };
        let at = gpui::point(x, button.top() - gap);
        let drop = button.size.height + gap * 2.;
        self.show_context_menu(target, at, corner, Some(drop), from_door, cx);
    }

    pub(super) fn open_context_menu_at(
        &mut self,
        target: MenuTarget,
        at: Point<Pixels>,
        corner: gpui::Anchor,
        drop: Option<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.show_context_menu(target, at, corner, drop, false, cx);
    }

    fn show_context_menu(
        &mut self,
        target: MenuTarget,
        at: Point<Pixels>,
        corner: gpui::Anchor,
        drop: Option<Pixels>,
        from_door: bool,
        cx: &mut Context<Self>,
    ) {
        let rows = self.context_rows(target, cx);
        self.show_popup(
            Popup::ContextMenu(ContextMenu {
                target,
                at,
                corner,
                drop,
                rows,
                from_door,
                armed: None,
                sub: None,
                cursor: MenuCursor::Idle,
            }),
            cx,
        );
    }

    /// The rows a target offers. `None` is a gap between groups of rows.
    pub(super) fn context_rows(
        &self,
        target: MenuTarget,
        cx: &gpui::App,
    ) -> Vec<Option<(menu::Item, MenuVerb)>> {
        use ferrite_ui::components::bound_chord;
        let mut rows: Vec<Option<(menu::Item, MenuVerb)>> = Vec::new();
        match target {
            // The nav row: where a Thread lives in the roster — open it,
            // park it, delete it.
            MenuTarget::Thread(key) => self.thread_rows(key, &mut rows, cx),
            MenuTarget::Snooze(_) => rows.extend(self.snooze_rows().into_iter().map(Some)),
            // The transcript: what is on screen — copy it, name it, size
            // it, and the Pane's own park/close. Deleting a Thread is the
            // nav's act; the transcript never offers it.
            MenuTarget::Pane(_) => {
                let selected = self.native_copy.is_some();
                rows.push(Some((
                    menu::Item::new("Copy")
                        .shortcut(bound_chord("cockpit::CopySelection").unwrap_or_default())
                        .disabled(!selected),
                    MenuVerb::CopySelection,
                )));
                rows.push(Some((
                    menu::Item::new("Copy transcript"),
                    MenuVerb::CopyTranscript,
                )));
                rows.push(None);
                rows.push(Some((menu::Item::new("Rename"), MenuVerb::Rename)));
                rows.push(None);
                if !self.on_machine(cx) {
                    rows.push(Some((
                        menu::Item::new("Reveal in Finder"),
                        MenuVerb::Reveal,
                    )));
                }
                rows.push(Some((menu::Item::new("Copy path"), MenuVerb::CopyPath)));
                rows.push(None);
                // cmd-w parks in one act (`Cockpit::park_thread`).
                rows.push(Some((
                    menu::Item::new("Park thread")
                        .shortcut(bound_chord("cockpit::CloseThread").unwrap_or_default()),
                    MenuVerb::Park,
                )));
            }
            // The Parked shelf's header: the shelf, and the one bulk act it
            // exists for — clearing the parked history it holds.
            MenuTarget::NavFilter => rows.extend(self.filter_rows(cx)),
            MenuTarget::NavFactory => rows.extend(self.factory_rows(cx)),
            MenuTarget::Parked => {
                rows.push(Some((
                    menu::Item::new(if self.shelf_open(nav::Shelf::Parked) {
                        "Hide parked threads"
                    } else {
                        "Show parked threads"
                    }),
                    MenuVerb::ToggleParked,
                )));
                rows.push(None);
                rows.push(Some((
                    menu::Item::new("Delete parked threads")
                        .destructive()
                        .disabled(self.parked_threads(cx).is_empty()),
                    MenuVerb::DeleteAllParked,
                )));
            }
            // The Review's diff: the pick — comment on it, read its file,
            // copy it. The reader reads this computer's files only.
            MenuTarget::Review { .. } => {
                rows.push(Some((
                    menu::Item::new("Comment").shortcut("c"),
                    MenuVerb::Comment,
                )));
                rows.push(Some((
                    menu::Item::new("Open file at this line").disabled(self.on_machine(cx)),
                    MenuVerb::OpenAtLine,
                )));
                rows.push(Some((menu::Item::new("Copy"), MenuVerb::CopySelection)));
            }
            // A prompt's Rewind: each row says what does not come back, and
            // arms before it runs. The code comes back only where the agent
            // keeps file checkpoints.
            MenuTarget::Prompt { thread, .. } => {
                rows.push(Some((
                    menu::Item {
                        note: Some("Your files stay as they are now".into()),
                        ..menu::Item::new("Rewind conversation").destructive()
                    },
                    MenuVerb::Rewind,
                )));
                let files = self.cockpit.read(cx).thread(thread).is_some_and(|open| {
                    open.supports_control(ferrite_core::ControlKind::RewindFiles)
                });
                rows.extend(files.then(|| {
                    Some((
                        menu::Item {
                            note: Some(
                                "Edits made by Bash or subagents don't come back; see git".into(),
                            ),
                            ..menu::Item::new("Rewind conversation and code").destructive()
                        },
                        MenuVerb::RewindCode,
                    ))
                }));
            }
            MenuTarget::File(_) => rows = tools::FilesView::row_rows(self.on_machine(cx)),
            MenuTarget::FilesMore(thread) => rows = self.files.read(cx).more_rows(self.cockpit.read(cx).thread_key(thread)),
            MenuTarget::ChangesScope(diff) => rows = self.changes.read(cx).scope_rows(diff),
            MenuTarget::ChangesMore(diff) => rows = self.changes.read(cx).more_rows(diff),
            MenuTarget::ChangesBase(diff) => {
                rows = self.changes.read(cx).branch_rows(diff).into_iter().map(Some).collect()
            }
            MenuTarget::BrowserPage(thread) => rows = self.browser_page_rows(self.cockpit.read(cx).thread_key(thread), cx),
            MenuTarget::Details(_) => rows = self.details_menu_rows(),
            MenuTarget::Terminal(_) => rows = self.terminals.read(cx).context_rows(),
            MenuTarget::TerminalMore(_) => rows = self.terminals.read(cx).more_rows(),
            MenuTarget::SurfaceMenu(thread) => rows = self.surface_menu_rows(thread, cx),
            MenuTarget::SurfaceTab { thread, at } => rows = self.surface_tab_rows(thread, at, cx),
            MenuTarget::PrMore(target) => {
                rows = self.pull_request.read(cx).menu_rows(target, self.branch_pr(target, cx))
            }
            MenuTarget::PrLink(key) => rows = self.pr_link_rows(key.thread(), cx),
            MenuTarget::Factory => {
                rows = self
                    .factory_pick_rows(cx)
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| Some((item, MenuVerb::Factory(index))))
                    .collect()
            }
            MenuTarget::Page(owner) => {
                let address = self
                    .page_source(owner)
                    .is_some_and(|page| matches!(page, PageSource::Address(_)));
                if !address {
                    rows.push(Some((menu::Item::new("Reload"), MenuVerb::Reload)));
                }
                rows.push(Some((
                    menu::Item::new("Open in browser"),
                    MenuVerb::OpenInBrowser,
                )));
                rows.push(Some((
                    menu::Item::new(if address { "Copy address" } else { "Copy path" }),
                    MenuVerb::CopyPath,
                )));
            }
        }
        rows
    }

    /// The PR badge's menu: the PR's native detail and the two browsers,
    /// then its forge address. A PR the forge gave no address says so on
    /// the rows that need one.
    fn pr_link_rows(&self, thread: ThreadId, cx: &gpui::App) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let Some(target) = self.pr_target(thread, cx) else {
            return Vec::new();
        };
        let address = |label: &'static str, verb| {
            let item = menu::Item::new(label);
            let item = match target.address() {
                Ok(_) => item,
                Err(why) => item.disabled_because(why),
            };
            Some((item, verb))
        };
        vec![
            Some((menu::Item::new("Open pull request"), MenuVerb::PrOpen(Open::Primary))),
            address("Open in integrated browser", MenuVerb::PrOpen(Open::Integrated)),
            address("Open in system browser", MenuVerb::PrOpen(Open::System)),
            None,
            address("Copy link address", MenuVerb::PrCopy),
        ]
    }

    /// The Thread's own menu, the nav row's and the title's ▾ alike, in
    /// Claude's order (claude/row-menu.png): where it opens, then the row's
    /// own verbs, its paths, its PR, its place, and its end. Every row is
    /// a one-letter key, pressed while the menu is up (`menu_mnemonic`); a
    /// parent row (`›`) has none.
    fn thread_rows(
        &self,
        target: ThreadKey,
        rows: &mut Vec<Option<(menu::Item, MenuVerb)>>,
        cx: &gpui::App,
    ) {
        let thread = target.thread();
        let key =
            |label: &'static str, letter: &'static str| menu::Item::new(label).shortcut(letter);
        let rule = |rows: &mut Vec<Option<(menu::Item, MenuVerb)>>| {
            if rows.last().is_some_and(Option::is_some) {
                rows.push(None);
            }
        };
        let open = self.cockpit.read(cx).thread(thread);
        // A machine's path is not in this computer's Finder or Terminal.
        if !self.on_machine(cx) {
            rows.push(Some((
                menu::Item::new("Open in"),
                MenuVerb::Submenu(Submenu::OpenIn),
            )));
            rule(rows);
        }
        if self.pins(cx).contains(&target) {
            rows.push(Some((key("Unpin", "P"), MenuVerb::Unpin)));
        } else {
            rows.push(Some((key("Pin", "P"), MenuVerb::Pin)));
        }
        // On screen, the Thread is seen again the next frame.
        let visible = self
            .cockpit
            .read(cx)
            .visible()
            .contains(&PaneIdentity::Thread(thread));
        if !visible && self.cockpit.read(cx).notifications().can_mark_unread(thread) {
            rows.push(Some((key("Mark as unread", "U"), MenuVerb::MarkUnread)));
        }
        rows.push(Some((key("Rename", "R"), MenuVerb::Rename)));
        if let Some(allowed) = open.as_ref().and_then(super::transcript_glue::rewind_gate) {
            let fork = key("Fork", "F");
            let fork = match allowed {
                Ok(()) => fork,
                Err(why) => fork.disabled_because(why),
            };
            rows.push(Some((fork, MenuVerb::Fork)));
        }
        if self.cockpit.read(cx).roster().focused_thread() != Some(thread) {
            rows.push(Some((
                key(if open.is_some() { "Open" } else { "Resume" }, "O"),
                MenuVerb::Focus,
            )));
        }
        rule(rows);
        rows.push(Some((
            key("New thread in this project", "N"),
            MenuVerb::NewThread,
        )));
        rows.push(Some((key("Copy path", "C"), MenuVerb::CopyPath)));
        rows.push(Some((key("Copy thread link", "K"), MenuVerb::CopyLink)));
        rule(rows);
        rows.push(Some((key("Refresh PR status", "S"), MenuVerb::RefreshPr)));
        if self.facts.pr(self.cockpit.read(cx).thread_key(thread)).is_some_and(|pr| pr.url.is_some()) {
            rows.push(Some((key("Open PR in browser", "B"), MenuVerb::OpenPr)));
        }
        rule(rows);
        // Park is Ferrite's archive: Claude's `A`.
        if let Some(open) = open {
            rows.push(Some((
                menu::Item::new("Snooze"),
                MenuVerb::Submenu(Submenu::Snooze),
            )));
            if open.snooze().is_some() {
                rows.push(Some((menu::Item::new("Wake thread"), MenuVerb::Wake)));
            }

            let park = match self.nav_view.read(cx).menu_threads(target).len() {
                1 => key("Park thread", "A"),
                count => menu::Item::new(format!("Park ({count})")).shortcut("A"),
            };
            rows.push(Some((park, MenuVerb::Park)));
        }
        rows.push(Some((
            key("Delete thread", "D").destructive(),
            MenuVerb::Delete,
        )));
    }

    /// A submenu's rows (`Submenu`).
    fn submenu_rows(
        &self,
        target: MenuTarget,
        kind: Submenu,
        cx: &gpui::App,
    ) -> Vec<(menu::Item, MenuVerb)> {
        match (kind, target) {
            (Submenu::OpenIn, _) => vec![
                (menu::Item::new("Finder"), MenuVerb::Reveal),
                (menu::Item::new("Terminal"), MenuVerb::OpenTerminal),
            ],
            (Submenu::Snooze, MenuTarget::Thread(_) | MenuTarget::Pane(_)) => self.snooze_rows(),
            (Submenu::Snooze, _) => Vec::new(),
            (Submenu::Files(sub), MenuTarget::File(thread) | MenuTarget::FilesMore(thread)) => {
                self.files.read(cx).sub_rows(self.cockpit.read(cx).thread_key(thread), sub)
            }
            (Submenu::Files(_), _) => Vec::new(),
            (Submenu::Changes(_), MenuTarget::ChangesScope(diff)) => {
                self.changes.read(cx).branch_rows(diff)
            }
            (Submenu::Changes(_), _) => Vec::new(),
            (Submenu::Terminal(sub), _) => self.terminals.read(cx).sub_rows(sub),
        }
    }

    #[cfg(test)]
    pub(super) fn context_submenu_rows_for_test(
        &self,
        target: MenuTarget,
        kind: Submenu,
        cx: &gpui::App,
    ) -> Vec<(menu::Item, MenuVerb)> {
        self.submenu_rows(target, kind, cx)
    }

    /// Open row `parent`'s submenu beside it (a second press on the parent
    /// keeps it open).
    pub(super) fn open_submenu(&mut self, parent: usize, kind: Submenu, cx: &mut Context<Self>) {
        let Some(target) = self.context_menu().map(|open| open.target) else {
            return;
        };
        let rows = self.submenu_rows(target, kind, cx);
        if let Some(open) = self.context_menu_mut() {
            if open.sub.as_ref().is_some_and(|(at, _)| *at == parent) {
                return;
            }
            open.armed = None;
            open.sub = Some((parent, rows));
            cx.notify();
        }
    }

    /// A press on the open submenu's row `index`: its verb runs on the
    /// menu's target, and the menu closes.
    pub(super) fn press_submenu_row(
        &mut self,
        parent: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.context_menu() else {
            return;
        };
        let Some(verb) = open
            .sub
            .as_ref()
            .filter(|(at, _)| *at == parent)
            .and_then(|(_, rows)| rows.get(index))
            .map(|(_, verb)| *verb)
        else {
            return;
        };
        let target = open.target;
        self.close_popup(cx);
        self.run_menu_verb(target, verb, window, cx);
        cx.notify();
    }

    /// The row of the open menu whose one-letter key `keystroke` is
    /// (Claude's row menu: `P` pins), unmodified and case-blind.
    pub(super) fn menu_mnemonic(&self, keystroke: &gpui::Keystroke) -> Option<usize> {
        let open = self.context_menu()?;
        if keystroke.modifiers.modified() || keystroke.key.chars().count() != 1 {
            return None;
        }
        open.rows.iter().position(|row| {
            row.as_ref().is_some_and(|(item, _)| {
                !item.disabled
                    && item
                        .shortcut
                        .as_deref()
                        .is_some_and(|key| key.eq_ignore_ascii_case(&keystroke.key))
            })
        })
    }

    /// ↑ ↓ ← → ⏎ on the open menu. Up and down walk the rows of the menu
    /// (or of its submenu while the cursor is in it) and wrap past the
    /// ends; → enters a submenu, ← leaves it; ⏎ presses the cursor row.
    /// True when the key was the menu's. (Escape is every popup's:
    /// `close_popup`.)
    pub(super) fn menu_navigate(
        &mut self,
        keystroke: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(open) = self.context_menu_mut() else {
            return false;
        };
        if keystroke.modifiers.modified() {
            return false;
        }
        let live = |rows: &[Option<(menu::Item, MenuVerb)>], at: usize| {
            rows.get(at)
                .and_then(Option::as_ref)
                .is_some_and(|(item, _)| !item.disabled)
        };
        let step = |len: usize, from: Option<usize>, forward: bool, live: &dyn Fn(usize) -> bool| {
            let first = if forward { 0 } else { len.saturating_sub(1) };
            let mut at = from.map_or(first, |at| {
                if forward { (at + 1) % len.max(1) } else { (at + len.max(1) - 1) % len.max(1) }
            });
            for _ in 0..len {
                if live(at) {
                    return Some(at);
                }
                at = if forward { (at + 1) % len } else { (at + len - 1) % len };
            }
            None
        };
        match (keystroke.key.as_str(), open.cursor) {
            ("up" | "down", MenuCursor::SubRow(at)) => {
                let Some((_, rows)) = open.sub.as_ref() else {
                    return false;
                };
                let rows: Vec<_> = rows.iter().map(|(item, _)| item.disabled).collect();
                let forward = keystroke.key == "down";
                if let Some(next) = step(rows.len(), Some(at), forward, &|i| !rows[i]) {
                    open.cursor = MenuCursor::SubRow(next);
                }
            }
            ("up" | "down", cursor) => {
                let from = match cursor {
                    MenuCursor::Row(at) => Some(at),
                    // A picker's first arrow walks from its one checked
                    // row, the choice already made, so ↓ ⏎ makes the next.
                    _ => chosen_row(&open.rows),
                };
                let forward = keystroke.key == "down";
                let len = open.rows.len();
                if let Some(next) = step(len, from, forward, &|i| live(&open.rows, i)) {
                    open.cursor = MenuCursor::Row(next);
                    open.armed = None;
                    open.sub = None;
                }
            }
            ("right", MenuCursor::Row(at)) => {
                let Some(Some((_, MenuVerb::Submenu(kind)))) = open.rows.get(at) else {
                    return true;
                };
                let kind = *kind;
                self.open_submenu(at, kind, cx);
                if let Some(open) = self.context_menu_mut() {
                    if open.sub.as_ref().is_some_and(|(_, rows)| !rows.is_empty()) {
                        open.cursor = MenuCursor::SubRow(0);
                    }
                }
            }
            ("left", MenuCursor::SubRow(_)) => {
                if let Some((parent, _)) = open.sub.take() {
                    open.cursor = MenuCursor::Row(parent);
                }
            }
            ("enter", MenuCursor::Row(at)) => {
                self.press_menu_row(at, window, cx);
                return true;
            }
            ("enter", MenuCursor::SubRow(at)) => {
                if let Some(parent) = open.sub.as_ref().map(|(parent, _)| *parent) {
                    self.press_submenu_row(parent, at, window, cx);
                }
                return true;
            }
            ("left" | "right" | "enter", _) => return true,
            _ => return false,
        }
        cx.notify();
        true
    }

    /// A row's press: a destructive row arms on the first press and runs
    /// on the second; anything else runs at once. The menu closes after
    /// a verb runs, and stays up while a row is armed.
    pub(super) fn press_menu_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let confirm_delete = self.prefs.settings.confirm_delete;
        let Some(open) = self.context_menu_mut() else {
            return;
        };
        let Some(Some((item, verb))) = open.rows.get(index) else {
            return;
        };
        if item.disabled {
            return;
        }
        let verb = *verb;
        if let MenuVerb::Submenu(kind) = verb {
            self.open_submenu(index, kind, cx);
            return;
        }
        // The single-Thread setting is honoured for a single Thread. The
        // bulk delete always asks: one press must never erase a dozen
        // transcripts, whatever the preference says about one.
        let confirm = confirm_delete || verb != MenuVerb::Delete;
        if item.destructive && confirm && open.armed != Some(index) {
            open.armed = Some(index);
            cx.notify();
            return;
        }
        let target = open.target;
        self.close_popup(cx);
        self.run_menu_verb(target, verb, window, cx);
        cx.notify();
    }

    pub(super) fn run_menu_verb(
        &mut self,
        target: MenuTarget,
        verb: MenuVerb,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match (target, verb) {
            (MenuTarget::Details(_), MenuVerb::DetailsItem(item)) => self.change_settings(|settings| {
                let visible = settings.context_card_preferences.visible(item);
                settings.context_card_preferences.set(item, !visible);
            }, cx),
            (MenuTarget::Details(_), MenuVerb::DetailsReset) => self.change_settings(|settings| {
                settings.context_card_preferences = Default::default();
            }, cx),
            (_, MenuVerb::Factory(index)) => self.factory_pick(index, cx),
            (
                MenuTarget::Thread(key) | MenuTarget::Pane(key) | MenuTarget::Snooze(key),
                MenuVerb::Snooze(preset),
            ) => {
                if let Some(until) = preset.until(ferrite_core::clock::system_time()) {
                    self.snooze_thread(key, Some(until), cx);
                } else {
                    self.nav_notice
                        .hint("That snooze time has passed. Choose another time.");
                }
            }
            (
                MenuTarget::Thread(key) | MenuTarget::Pane(key) | MenuTarget::Snooze(key),
                MenuVerb::SnoozeCustom,
            ) => {
                self.nav_view
                    .update(cx, |nav, cx| nav.edit_snooze(key, window, cx));
                cx.notify();
            }
            (MenuTarget::Thread(key) | MenuTarget::Pane(key), MenuVerb::Wake) => {
                self.snooze_thread(key, None, cx)
            }
            (MenuTarget::Thread(key), MenuVerb::Rename) => {
                self.start_rename(RenameTarget::Thread(key.thread()), cx)
            }
            (MenuTarget::Pane(key), MenuVerb::Rename) => {
                self.start_rename(RenameTarget::ViewTitle(key.thread()), cx)
            }
            (MenuTarget::Thread(key) | MenuTarget::Pane(key), MenuVerb::Focus) => {
                if self.pane_for(key.thread()).is_some() {
                    self.focus_thread(key.thread(), cx);
                } else {
                    self.revive_thread(key.thread(), cx);
                }
            }
            (MenuTarget::Thread(key), MenuVerb::Pin) => self.set_pinned(key, true, cx),
            (MenuTarget::Thread(key), MenuVerb::Unpin) => self.set_pinned(key, false, cx),
            // One act (`Cockpit::park_thread`), straight into the parked
            // rows.
            (MenuTarget::Thread(key), MenuVerb::Park)
                if self.nav_view.read(cx).menu_threads(key).len() > 1 =>
            {
                let keys = self.nav_view.read(cx).menu_threads(key);
                self.park_listed(&keys, cx);
                self.parked_changed(cx);
            }
            (MenuTarget::Thread(key) | MenuTarget::Pane(key), MenuVerb::Park) => {
                self.close_pane(PaneIdentity::Thread(key.thread()), cx);
                // A Thread parked from the nav may have had no Pane in
                // this view, so the grid did not change: refresh the rows.
                self.parked_changed(cx);
            }
            (MenuTarget::Pane(_), MenuVerb::CopySelection) => {
                if let Some(text) = self.native_copy.clone() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            (MenuTarget::Page(owner), verb) => self.run_page_verb(owner, verb, cx),
            (MenuTarget::SurfaceMenu(thread), MenuVerb::OpenSurface(opener)) => {
                self.run_opener(thread, opener, cx)
            }
            (MenuTarget::SurfaceTab { thread, at }, verb) => {
                self.run_surface_tab_verb(thread, at, verb, cx)
            }
            (MenuTarget::PrMore(target), MenuVerb::Pr(action)) => self.run_pr_action(target, action, cx),
            (MenuTarget::PrLink(key), MenuVerb::PrOpen(how)) => {
                self.open_pr_as(key.thread(), how, window, cx)
            }
            (MenuTarget::PrLink(key), MenuVerb::PrCopy) => {
                if let Some(Ok(address)) = self.pr_target(key.thread(), cx).map(|target| target.address()) {
                    cx.write_to_clipboard(ClipboardItem::new_string(address));
                }
            }
            (MenuTarget::File(thread) | MenuTarget::FilesMore(thread), MenuVerb::Files(verb)) => {
                self.files.update(cx, |files, cx| files.run_verb(self.cockpit.read(cx).thread_key(thread), verb, cx))
            }
            (
                MenuTarget::ChangesScope(diff)
                | MenuTarget::ChangesMore(diff)
                | MenuTarget::ChangesBase(diff),
                MenuVerb::Changes(verb),
            ) => self
                .changes
                .update(cx, |changes, cx| changes.run_verb(diff, verb, cx)),
            (MenuTarget::BrowserPage(thread), MenuVerb::Browser(verb)) => {
                self.run_browser_page_verb(self.cockpit.read(cx).thread_key(thread), verb, cx)
            }
            (
                MenuTarget::Terminal(thread) | MenuTarget::TerminalMore(thread),
                MenuVerb::Terminal(verb),
            ) => self.run_terminal_verb(thread, verb, cx),
            (MenuTarget::Prompt { thread, block }, MenuVerb::Rewind | MenuVerb::RewindCode) => {
                self.cut_at_prompt(
                    thread,
                    block,
                    ferrite_pane::transcript::PromptAction::Rewind,
                    verb == MenuVerb::RewindCode,
                    cx,
                );
            }
            (MenuTarget::Review { thread, .. }, MenuVerb::Comment) => {
                if let Some(review) = self.review_of(thread) {
                    review.update(cx, |review, cx| review.open_editor(window, cx));
                }
            }
            (MenuTarget::Review { thread, line }, MenuVerb::OpenAtLine) => {
                let place = self.review_of(thread).and_then(|review| {
                    let (path, number) = review.read(cx).place(line)?;
                    Some((self.thread_path(thread, cx)?.join(path), number))
                });
                if let (Some((path, number)), Some(index)) = (place, self.pane_for(thread)) {
                    self.focus_pane(index, cx);
                    self.open_preview(path, Some(number), window, cx);
                }
            }
            (MenuTarget::Review { thread, .. }, MenuVerb::CopySelection) => {
                if let Some(text) = self
                    .review_of(thread)
                    .and_then(|review| review.read(cx).picked_text())
                {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            (MenuTarget::Pane(key), MenuVerb::CopyTranscript) => {
                self.copy_transcript(key.thread(), cx);
            }
            (MenuTarget::Parked, MenuVerb::ToggleParked) => {
                let open = self.shelf_open(nav::Shelf::Parked);
                self.set_shelf_open(nav::Shelf::Parked, !open, cx);
            }
            (MenuTarget::NavFilter, MenuVerb::FilterProject(project)) => {
                self.choose_nav_filter(project, cx)
            }
            (MenuTarget::NavFactory, MenuVerb::OpenFactory(project)) => {
                self.open_factory(project, cx)
            }
            (MenuTarget::Parked, MenuVerb::DeleteAllParked) => {
                self.delete_parked_threads(cx);
            }
            (_, MenuVerb::NewThread) => {
                let project = match target {
                    MenuTarget::Thread(key)
                    | MenuTarget::Pane(key)
                    | MenuTarget::Snooze(key) => self.cockpit.read(cx).project_id(key.thread()),
                    MenuTarget::Parked
                    | MenuTarget::NavFilter
                    | MenuTarget::NavFactory
                    | MenuTarget::Review { .. }
                    | MenuTarget::Prompt { .. }
                    | MenuTarget::File(_)
                    | MenuTarget::FilesMore(_)
                    | MenuTarget::ChangesScope(_)
                    | MenuTarget::ChangesMore(_)
                    | MenuTarget::ChangesBase(_)
                    | MenuTarget::Terminal(_)
                    | MenuTarget::TerminalMore(_)
                    | MenuTarget::SurfaceMenu(_)
                    | MenuTarget::SurfaceTab { .. }
                    | MenuTarget::PrMore(_)
                    | MenuTarget::PrLink(_)
                    | MenuTarget::Factory
                    | MenuTarget::BrowserPage(_)
                    | MenuTarget::Details(_)
                    | MenuTarget::Page(_) => None,
                };
                self.open_draft(DraftTarget::Main, cx);
                if let (Some(project), Some(draft)) = (project, self.focused_draft_mut(cx)) {
                    draft.binding.choose_checkout(project);
                }
            }
            (_, MenuVerb::Reveal) => {
                if let Some(path) = self.target_path(target, cx) {
                    cx.reveal_path(&path);
                }
            }
            (MenuTarget::Thread(key), MenuVerb::MarkUnread) => self.cockpit.update(cx, |cockpit, _| cockpit.mark_unread(key.thread())),
            (MenuTarget::Thread(key), MenuVerb::Fork) => {
                self.fork_thread(key.thread(), None, None, cx)
            }
            (MenuTarget::Thread(key), MenuVerb::CopyLink) => {
                cx.write_to_clipboard(ClipboardItem::new_string(thread_link(key.thread())));
            }
            (_, MenuVerb::OpenTerminal) => {
                if let Some(path) = self.target_path(target, cx) {
                    if let Err(error) = terminal_at(&path).spawn() {
                        ferrite_core::log!(
                            "a Terminal did not open at {}: {error}",
                            path.display()
                        );
                    }
                }
            }
            (MenuTarget::Thread(key), MenuVerb::RefreshPr) => self.check_pr(key.thread(), true, cx),
            (MenuTarget::Thread(key), MenuVerb::OpenPr) => self.open_pr(key.thread(), cx),
            (_, MenuVerb::CopyPath) => {
                if let Some(path) = self.target_path(target, cx) {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        path.to_string_lossy().to_string(),
                    ));
                }
            }
            (MenuTarget::Thread(key) | MenuTarget::Pane(key), MenuVerb::Delete) => {
                let thread = key.thread();
                match self.delete_thread(thread, cx) {
                    Err(error) => self.nav_notice.problem(format!("delete refused: {error}")),
                    Ok(()) => self.nav_notice.clear(),
                }
                if self.composer_menu.read(cx).pane() == Some(PaneIdentity::Thread(thread)) {
                    self.close_composer_menu(cx);
                }
                self.sync_panes(cx);
                self.parked_changed(cx);
            }
            _ => {}
        }
    }

    /// The context menu, floated at the pointer and clamped inside the
    /// window.
    pub(super) fn context_menu_element(
        &self,
        open: &ContextMenu,
        face: popups::Face,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let mut shell = menu::shell();
        // Where the open submenu's parent row stands: the submenu's top.
        let mut parent_top = px(ferrite_ui::theme::FLOAT_PAD);
        let mut y = parent_top;
        if matches!(open.target, MenuTarget::Details(_)) {
            shell = shell.child(div().h(px(28.)).px(px(8.)).flex().items_center()
                .text_size(px(11.)).text_color(rgb(ferrite_ui::theme::INK_SUBTLE))
                .child("Show in thread details"));
            y += px(28.);
        }
        for (index, row) in open.rows.iter().enumerate() {
            shell = match row {
                None => {
                    y += px(1. + 2. * ferrite_ui::theme::MENU_RULE_GAP);
                    shell.child(menu::rule())
                }
                Some((item, verb)) => {
                    if open
                        .sub
                        .as_ref()
                        .is_some_and(|(parent, _)| *parent == index)
                    {
                        parent_top = y;
                    }
                    y += px(row_height(item));
                    let parent = matches!(verb, MenuVerb::Submenu(_));
                    let lit = open.sub.as_ref().is_some_and(|(at, _)| *at == index);
                    let cursor = open.cursor == MenuCursor::Row(index);
                    let face = menu::row(index, item, cursor, open.armed == Some(index))
                        .debug_selector(move || format!("context-menu-row-{index}"))
                        .when(lit, |row| row.bg(ferrite_ui::theme::paint::FLOAT_HOVER))
                        .when(parent, |row| {
                            row.child(ferrite_ui::icons::icon(
                                ferrite_ui::icons::CHEVRON_RIGHT,
                                ferrite_ui::theme::ROW_ICON,
                                ferrite_ui::theme::INK_SUBTLE,
                            ))
                        })
                        // Resting on a parent opens its submenu; on any
                        // other row, closes it. (The row's own hover is
                        // its fill's.)
                        .on_mouse_move(cx.listener(move |view, _: &gpui::MouseMoveEvent, _, cx| {
                            let Some(open) = view.context_menu_mut() else {
                                return;
                            };
                            match open.rows.get(index) {
                                Some(Some((_, MenuVerb::Submenu(kind)))) => {
                                    let kind = *kind;
                                    view.open_submenu(index, kind, cx);
                                }
                                _ if open.sub.take().is_some() => cx.notify(),
                                _ => {}
                            }
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                view.press_menu_row(index, window, cx);
                            }),
                        );
                    shell.child(face)
                }
            };
        }
        // Hung under its trigger, a menu keeps below it and scrolls in the
        // room it has. Where that room cannot show two rows, it hangs over
        // the trigger instead (the same gap above), scrolling in the room
        // there.
        let pad = px(ferrite_ui::theme::GRID_PAD);
        let usable = px(2. * (ferrite_ui::theme::FLOAT_ROW_H + ferrite_ui::theme::FLOAT_PAD));
        let hang = open.drop.map(|drop| {
            let below = window.viewport_size().height - open.at.y - drop - pad;
            let above = open.at.y - pad;
            if below < usable && above > below {
                (ferrite_ui::motion::Opens::Up, above)
            } else {
                (ferrite_ui::motion::Opens::Down, below)
            }
        });
        let shell = match hang {
            Some((_, room)) if y + px(ferrite_ui::theme::FLOAT_PAD) > room => shell
                .max_h(room)
                .id("context-menu-scroll")
                .overflow_y_scroll()
                .into_any_element(),
            _ => shell.into_any_element(),
        };
        let side = open.sub.as_ref().map(|(parent, rows)| {
            let mut side = menu::shell().debug_selector(|| SUBMENU.into());
            for (index, (item, _)) in rows.iter().enumerate() {
                let parent = *parent;
                side = side.child(
                    menu::item_row(
                        ("context-submenu-row", index),
                        item,
                        open.cursor == MenuCursor::SubRow(index),
                        false,
                    )
                        .debug_selector(move || format!("context-submenu-row-{index}"))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                view.press_submenu_row(parent, index, window, cx);
                            }),
                        ),
                );
            }
            // Its first row level with its parent (the shell's pad above both).
            crate::float::beside(
                ferrite_ui::theme::SUBMENU_GAP,
                f32::from(parent_top) - ferrite_ui::theme::FLOAT_PAD,
                ferrite_ui::theme::GRID_PAD,
                ferrite_ui::components::on_bounds(side, self.record_trigger(SUBMENU.into()))
                    .into_any_element(),
            )
        });
        let shell = div()
            .relative()
            .debug_selector(|| "context-menu".into())
            .child(shell)
            .children(side);
        let opens = hang.map_or(ferrite_ui::motion::Opens::Down, |(opens, _)| opens);
        // Every menu moves in from the window's edge rather than switch
        // sides: switched, a hung menu would lose its drop and lie over its
        // trigger. Its height already fits the room it hangs in.
        let placed = match (open.drop, opens) {
            (Some(_), ferrite_ui::motion::Opens::Up) => anchored().anchor(bottom_of(open.corner)).position(open.at),
            (Some(drop), _) => anchored()
                .anchor(open.corner)
                .position(open.at)
                .offset(gpui::point(px(0.), drop)),
            (None, _) => anchored().anchor(open.corner).position(open.at),
        }
        .snap_to_window_with_margin(pad);
        Some(
            placed
                .child(self.popup_face(face, "context-menu-in", shell, opens, cx))
                .into_any_element(),
        )
    }
}

/// The row a picker's check marks, when exactly one is: the choice made.
/// Several checked rows are toggles, not one choice.
fn chosen_row(rows: &[Option<(menu::Item, MenuVerb)>]) -> Option<usize> {
    let mut checked = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.as_ref().is_some_and(|(item, _)| item.checked));
    match (checked.next(), checked.next()) {
        (Some((at, _)), None) => Some(at),
        _ => None,
    }
}

/// `corner`'s twin on the bottom edge: the menu hangs by it over its button.
fn bottom_of(corner: gpui::Anchor) -> gpui::Anchor {
    match corner {
        gpui::Anchor::TopLeft | gpui::Anchor::BottomLeft => gpui::Anchor::BottomLeft,
        _ => gpui::Anchor::BottomRight,
    }
}

/// A row's height in the menu: a note takes a second line.
fn row_height(item: &menu::Item) -> f32 {
    if item.note.is_some() {
        ferrite_ui::theme::FLOAT_ROW_NOTE_H
    } else {
        ferrite_ui::theme::FLOAT_ROW_H
    }
}

/// A Terminal window at `path`: macOS's Terminal, else the desktop's
/// default emulator started there.
fn terminal_at(path: &std::path::Path) -> std::process::Command {
    let mut command = if cfg!(target_os = "macos") {
        let mut open = std::process::Command::new("open");
        open.args(["-a", "Terminal"]).arg(path);
        open
    } else {
        let mut emulator = std::process::Command::new("x-terminal-emulator");
        emulator.current_dir(path);
        emulator
    };
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_threads_link_is_the_cli_command_that_focuses_it() {
        assert_eq!(thread_link(ThreadId::new(12)), "ferrite thread focus 12");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_terminal_opens_at_the_checkout() {
        let command = terminal_at(std::path::Path::new("/work/repo"));
        assert_eq!(command.get_program(), "open");
        let args: Vec<_> = command.get_args().filter_map(|arg| arg.to_str()).collect();
        assert_eq!(args, ["-a", "Terminal", "/work/repo"]);
    }
}
