//! The context menu (theme WP-E, `menu`): what a right-click, a row's `⋯`
//! or a title's ▾ offers for a Thread, a Pane, a Group, a Project, the
//! Parked view, a Review line, a prompt's Rewind or a page — the rows each
//! target offers (`context_rows`), their one-letter keys, a destructive
//! row's arming, a parent row's submenu beside it (`Submenu`), and the
//! verbs the rows run (`run_menu_verb`).

use super::*;

/// What a right-click was on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum MenuTarget {
    /// A nav row.
    Thread(ThreadId),
    /// A nav row's Snooze button: the reminder choices directly.
    Snooze(ThreadId),
    /// A Pane — the same Thread, but the rename opens in the head.
    Pane(ThreadId),
    /// The Parked shelf's header at the foot of the nav.
    Parked,
    /// The search row's Filter button: a Project to filter by, a Group to
    /// open.
    NavFilter,
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
    PrMore(ThreadId),
    /// A Factory picker (#221): its rows are the Factory view's `picks`.
    Factory,
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
    Fullscreen,
    /// Unfold or fold the Parked section.
    ToggleParked,
    /// Delete every Thread the Parked section lists — exactly those rows,
    /// so what the menu erases is what the operator can see.
    DeleteAllParked,
    /// Park the Thread in one act, on screen or not: a Group member leaves
    /// its Group and parks (`Cockpit::park_thread`), the same as cmd-w.
    Park,
    /// The transcript menu's own verbs: what is highlighted (in the
    /// Review, the pick), or all of it.
    CopySelection,
    CopyTranscript,
    /// The Review's own verbs: comment on the pick, read the line's file.
    Comment,
    OpenAtLine,
    LeaveGroup,
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
    /// The Thread's id in the form the CLI takes (`thread_link`).
    CopyLink,
    /// Move the Thread into this Group (Claude's `Move to section ›`).
    JoinGroup(GroupId),
    /// A `+` menu row: its Surface opens in the Panel.
    OpenSurface(panel::Opener),
    /// A Panel tab's closes.
    CloseSurface,
    CloseOtherSurfaces,
    CloseSurfacesRight,
    CloseAllSurfaces,
    FilterProject(Option<ProjectId>),
    OpenGroup(GroupId),
    /// A pull request Surface's row.
    Pr(panel::PrAction),
    /// The Factory picker's row at this index.
    Factory(usize),
}

/// A parent row's submenu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Submenu {
    /// Where the Thread's checkout opens: Finder, a Terminal.
    OpenIn,
    /// The other Groups the Thread can move to.
    MoveToGroup,
    Snooze,
    /// The Files menus' parent rows: `Copy ›` and `Tab size ›`.
    Files(tools::FilesSub),
    /// `Compare against ›`.
    Changes(tools::ChangesSub),
    /// `Cursor style ›` and `Scrollback ›`.
    Terminal(tools::TerminalSub),
}

/// The context menu up on screen.
pub(super) struct ContextMenu {
    pub(super) target: MenuTarget,
    pub(super) at: Point<Pixels>,
    /// The menu's corner at `at`: its top left at the pointer, its top
    /// right under the `⋯` it opened from (`open_context_menu_under`).
    pub(super) corner: gpui::Anchor,
    /// Opened from a `⋯`: `at` is `MENU_UNDER_GAP` over the button and
    /// this drops the menu to the same gap under it. Where the window has
    /// no room below, the menu flips to hang over the button instead
    /// (gpui's `SwitchAnchor` flips about `at`). None at the pointer.
    pub(super) drop: Option<Pixels>,
    pub(super) rows: Vec<Option<(menu::Item, MenuVerb)>>,
    /// The destructive row pressed once, waiting for its confirmation.
    pub(super) armed: Option<usize>,
    /// The submenu open beside its parent row: that row's index and the
    /// submenu's rows.
    pub(super) sub: Option<(usize, Vec<(menu::Item, MenuVerb)>)>,
}

impl CockpitView {
    /// Summon the context menu for `target` at the pointer. Whatever else
    /// was up (a popover, a card, a rename) closes: one floating thing at a
    /// time.
    pub(super) fn open_context_menu(
        &mut self,
        target: MenuTarget,
        at: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.open_context_menu_at(target, at, gpui::Anchor::TopLeft, None, cx);
    }

    /// The menu a row's `⋯` opens, as Claude's: under the button, its right
    /// edge on the button's, `MENU_UNDER_GAP` below it — over it, the same
    /// gap above, only where the window has no room below.
    pub(super) fn open_context_menu_under(
        &mut self,
        target: MenuTarget,
        button: gpui::Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let gap = px(ferrite_ui::theme::MENU_UNDER_GAP);
        let at = gpui::point(button.right(), button.top() - gap);
        let drop = button.size.height + gap * 2.;
        self.open_context_menu_at(target, at, gpui::Anchor::TopRight, Some(drop), cx);
    }

    pub(super) fn open_context_menu_at(
        &mut self,
        target: MenuTarget,
        at: Point<Pixels>,
        corner: gpui::Anchor,
        drop: Option<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.popover = None;
        self.context_checks = None;
        if self.rename.is_some() {
            self.finish_rename(false, cx);
        }
        let rows = self.context_rows(target);
        self.context_menu = Some(ContextMenu {
            target,
            at,
            corner,
            drop,
            rows,
            armed: None,
            sub: None,
        });
        cx.notify();
    }

    /// The rows a target offers. `None` is a gap between groups of rows.
    pub(super) fn context_rows(&self, target: MenuTarget) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let mut rows: Vec<Option<(menu::Item, MenuVerb)>> = Vec::new();
        let fullscreen =
            ferrite_ui::components::bound_chord("cockpit::ToggleFullscreen").unwrap_or_default();
        match target {
            // The nav row: where a Thread lives in the roster — open it,
            // park it, move it between Groups, delete it.
            MenuTarget::Thread(thread) => self.thread_rows(thread, &mut rows),
            MenuTarget::Snooze(_) => rows.extend(self.snooze_rows().into_iter().map(Some)),
            // The transcript: what is on screen — copy it, name it, size
            // it, and the Pane's own park/close. Deleting a Thread is the
            // nav's act; the transcript never offers it.
            MenuTarget::Pane(thread) => {
                let grouped = self.cockpit.groups().of(thread).is_some();
                let selected = self.native_copy.is_some();
                rows.push(Some((
                    menu::Item::new("Copy")
                        .shortcut("cmd-C")
                        .disabled(!selected),
                    MenuVerb::CopySelection,
                )));
                rows.push(Some((
                    menu::Item::new("Copy transcript"),
                    MenuVerb::CopyTranscript,
                )));
                rows.push(None);
                rows.push(Some((menu::Item::new("Rename"), MenuVerb::Rename)));
                rows.push(Some((
                    menu::Item::new("Toggle fullscreen").shortcut(fullscreen),
                    MenuVerb::Fullscreen,
                )));
                rows.push(None);
                if !self.on_machine() {
                    rows.push(Some((
                        menu::Item::new("Reveal in Finder"),
                        MenuVerb::Reveal,
                    )));
                }
                rows.push(Some((menu::Item::new("Copy path"), MenuVerb::CopyPath)));
                rows.push(None);
                // cmd-w parks in one act, Group member or not
                // (`Cockpit::park_thread`); leaving without parking is its
                // own row.
                rows.push(Some((
                    menu::Item::new("Park thread").shortcut("cmd-W"),
                    MenuVerb::Park,
                )));
                if grouped {
                    rows.push(Some((menu::Item::new("Leave group"), MenuVerb::LeaveGroup)));
                }
            }
            // The Parked shelf's header: the shelf, and the one bulk act it
            // exists for — clearing the parked history it holds.
            MenuTarget::NavFilter => rows.extend(self.filter_rows()),
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
                        .disabled(self.deletable_parked().is_empty()),
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
                    menu::Item::new("Open file at this line").disabled(self.on_machine()),
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
                let files = self.cockpit.thread(thread).is_some_and(|open| {
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
            MenuTarget::File(_) => rows = self.files_row_rows(),
            MenuTarget::FilesMore(thread) => rows = self.files_more_rows(thread),
            MenuTarget::ChangesScope(diff) => rows = self.changes_scope_rows(diff),
            MenuTarget::ChangesMore(diff) => rows = self.changes_more_rows(diff),
            MenuTarget::ChangesBase(diff) => {
                rows = self.changes_branch_rows(diff).into_iter().map(Some).collect()
            }
            MenuTarget::Terminal(_) => rows = self.terminal_context_rows(),
            MenuTarget::TerminalMore(_) => rows = self.terminal_more_rows(),
            MenuTarget::SurfaceMenu(thread) => rows = self.surface_menu_rows(thread),
            MenuTarget::SurfaceTab { thread, at } => rows = self.surface_tab_rows(thread, at),
            MenuTarget::PrMore(thread) => rows = self.pr_menu_rows(thread),
            MenuTarget::Factory => {
                rows = self
                    .factory_pick_rows()
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

    /// The Thread's own menu, the nav row's and the title's ▾ alike, in
    /// Claude's order (claude/row-menu.png): where it opens, then the row's
    /// own verbs, its paths, its PR, its place, and its end. Every row is
    /// a one-letter key, pressed while the menu is up (`menu_mnemonic`); a
    /// parent row (`›`) has none.
    fn thread_rows(&self, thread: ThreadId, rows: &mut Vec<Option<(menu::Item, MenuVerb)>>) {
        let key =
            |label: &'static str, letter: &'static str| menu::Item::new(label).shortcut(letter);
        let rule = |rows: &mut Vec<Option<(menu::Item, MenuVerb)>>| {
            if rows.last().is_some_and(Option::is_some) {
                rows.push(None);
            }
        };
        let shown = self.pane_for(thread).is_some();
        let open = self.cockpit.thread(thread);
        let grouped = self.cockpit.groups().of(thread).is_some();
        // A machine's path is not in this computer's Finder or Terminal.
        if !self.on_machine() {
            rows.push(Some((
                menu::Item::new("Open in"),
                MenuVerb::Submenu(Submenu::OpenIn),
            )));
            rule(rows);
        }
        // A Group member's place is its Group.
        if self.prefs.settings.pinned_threads.contains(&thread) {
            rows.push(Some((key("Unpin", "P"), MenuVerb::Unpin)));
        } else if !grouped {
            rows.push(Some((key("Pin", "P"), MenuVerb::Pin)));
        }
        // On screen, the Thread is seen again the next frame.
        let visible = self
            .cockpit
            .visible()
            .contains(&PaneIdentity::Thread(thread));
        if !visible && self.cockpit.notifications().can_mark_unread(thread) {
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
        if self.cockpit.roster().focused_thread() != Some(thread) {
            rows.push(Some((
                key(if open.is_some() { "Open" } else { "Resume" }, "O"),
                MenuVerb::Focus,
            )));
        }
        if shown {
            rows.push(Some((key("Toggle fullscreen", "T"), MenuVerb::Fullscreen)));
        }
        rule(rows);
        rows.push(Some((
            key("New thread in this project", "N"),
            MenuVerb::NewThread,
        )));
        rows.push(Some((key("Copy path", "C"), MenuVerb::CopyPath)));
        rows.push(Some((key("Copy link", "K"), MenuVerb::CopyLink)));
        rule(rows);
        rows.push(Some((key("Refresh PR status", "S"), MenuVerb::RefreshPr)));
        if self.facts.pr(thread).is_some_and(|pr| pr.url.is_some()) {
            rows.push(Some((key("Open PR in browser", "B"), MenuVerb::OpenPr)));
        }
        rule(rows);
        if !self.groups_to_join(thread).is_empty() {
            rows.push(Some((
                menu::Item::new("Move to group"),
                MenuVerb::Submenu(Submenu::MoveToGroup),
            )));
        }
        if grouped {
            rows.push(Some((key("Leave group", "L"), MenuVerb::LeaveGroup)));
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

            let park = match self.menu_threads(thread).len() {
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

    /// The Groups `thread` can move into: every Group but its home.
    fn groups_to_join(&self, thread: ThreadId) -> Vec<(GroupId, String)> {
        let home = self.cockpit.groups().of(thread).map(|group| group.id);
        self.cockpit
            .groups()
            .iter()
            .filter(|group| Some(group.id) != home && !group.members.contains(&thread))
            .map(|group| (group.id, group.title.clone()))
            .collect()
    }

    /// A submenu's rows (`Submenu`).
    fn submenu_rows(&self, target: MenuTarget, kind: Submenu) -> Vec<(menu::Item, MenuVerb)> {
        match (kind, target) {
            (Submenu::OpenIn, _) => vec![
                (menu::Item::new("Finder"), MenuVerb::Reveal),
                (menu::Item::new("Terminal"), MenuVerb::OpenTerminal),
            ],
            (Submenu::MoveToGroup, MenuTarget::Thread(thread)) => self
                .groups_to_join(thread)
                .into_iter()
                .map(|(group, title)| (menu::Item::new(title), MenuVerb::JoinGroup(group)))
                .collect(),
            (Submenu::Snooze, MenuTarget::Thread(_) | MenuTarget::Pane(_)) => self.snooze_rows(),
            (Submenu::Snooze, _) => Vec::new(),
            (Submenu::MoveToGroup, _) => Vec::new(),
            (Submenu::Files(sub), MenuTarget::File(thread) | MenuTarget::FilesMore(thread)) => {
                self.files_sub_rows(thread, sub)
            }
            (Submenu::Files(_), _) => Vec::new(),
            (Submenu::Changes(_), MenuTarget::ChangesScope(diff)) => {
                self.changes_branch_rows(diff)
            }
            (Submenu::Changes(_), _) => Vec::new(),
            (Submenu::Terminal(sub), _) => self.terminal_sub_rows(sub),
        }
    }

    #[cfg(test)]
    pub(super) fn context_submenu_rows_for_test(
        &self,
        target: MenuTarget,
        kind: Submenu,
    ) -> Vec<(menu::Item, MenuVerb)> {
        self.submenu_rows(target, kind)
    }

    /// Open row `parent`'s submenu beside it (a second press on the parent
    /// keeps it open).
    pub(super) fn open_submenu(&mut self, parent: usize, kind: Submenu, cx: &mut Context<Self>) {
        let Some(target) = self.context_menu.as_ref().map(|open| open.target) else {
            return;
        };
        let rows = self.submenu_rows(target, kind);
        if let Some(open) = self.context_menu.as_mut() {
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
        let Some(open) = self.context_menu.as_ref() else {
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
        self.context_menu = None;
        self.run_menu_verb(target, verb, window, cx);
        cx.notify();
    }

    /// The row of the open menu whose one-letter key `keystroke` is
    /// (Claude's row menu: `P` pins), unmodified and case-blind.
    pub(super) fn menu_mnemonic(&self, keystroke: &gpui::Keystroke) -> Option<usize> {
        let open = self.context_menu.as_ref()?;
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

    /// A row's press: a destructive row arms on the first press and runs
    /// on the second; anything else runs at once. The menu closes after
    /// a verb runs, and stays up while a row is armed.
    pub(super) fn press_menu_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.context_menu.as_mut() else {
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
        let confirm = self.prefs.settings.confirm_delete || verb != MenuVerb::Delete;
        if item.destructive && confirm && open.armed != Some(index) {
            open.armed = Some(index);
            cx.notify();
            return;
        }
        let target = open.target;
        self.context_menu = None;
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
            (_, MenuVerb::Factory(index)) => self.factory_pick(index, cx),
            (
                MenuTarget::Thread(thread) | MenuTarget::Pane(thread) | MenuTarget::Snooze(thread),
                MenuVerb::Snooze(preset),
            ) => {
                if let Some(until) = preset.until(ferrite_core::clock::system_time()) {
                    self.snooze_thread(thread, Some(until), cx);
                } else {
                    self.group_error =
                        Some("That snooze time has passed. Choose another time.".into());
                }
            }
            (
                MenuTarget::Thread(thread) | MenuTarget::Pane(thread) | MenuTarget::Snooze(thread),
                MenuVerb::SnoozeCustom,
            ) => self.edit_snooze(thread, window, cx),
            (MenuTarget::Thread(thread) | MenuTarget::Pane(thread), MenuVerb::Wake) => {
                self.snooze_thread(thread, None, cx)
            }
            (MenuTarget::Thread(thread), MenuVerb::Rename) => {
                self.start_rename(RenameTarget::Thread(thread), cx)
            }
            (MenuTarget::Pane(thread), MenuVerb::Rename) => {
                self.start_rename(RenameTarget::PaneTitle(thread), cx)
            }
            (MenuTarget::Thread(thread) | MenuTarget::Pane(thread), MenuVerb::Focus) => {
                if self.pane_for(thread).is_some() {
                    self.focus_thread(thread, cx);
                } else {
                    self.revive_thread(thread, cx);
                }
            }
            (MenuTarget::Thread(thread) | MenuTarget::Pane(thread), MenuVerb::Fullscreen) => {
                self.focus_thread(thread, cx);
                self.cockpit.toggle_fullscreen();
            }
            (MenuTarget::Thread(thread), MenuVerb::Pin) => self.set_pinned(thread, true, cx),
            (MenuTarget::Thread(thread), MenuVerb::Unpin) => self.set_pinned(thread, false, cx),
            // One act: a Group member leaves its Group and parks
            // (`Cockpit::park_thread`), straight into the parked rows.
            (MenuTarget::Thread(thread), MenuVerb::Park) if self.menu_threads(thread).len() > 1 => {
                let threads = self.menu_threads(thread);
                self.nav_picked.clear();
                self.park_listed(&threads, cx);
                self.parked_changed(cx);
            }
            (MenuTarget::Thread(thread) | MenuTarget::Pane(thread), MenuVerb::Park) => {
                self.close_pane(PaneIdentity::Thread(thread), cx);
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
            (MenuTarget::PrMore(thread), MenuVerb::Pr(action)) => self.run_pr_action(thread, action, cx),
            (MenuTarget::File(thread) | MenuTarget::FilesMore(thread), MenuVerb::Files(verb)) => {
                self.run_files_verb(thread, verb, cx)
            }
            (
                MenuTarget::ChangesScope(diff)
                | MenuTarget::ChangesMore(diff)
                | MenuTarget::ChangesBase(diff),
                MenuVerb::Changes(verb),
            ) => self.run_changes_verb(diff, verb, cx),
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
                    Some((self.thread_path(thread)?.join(path), number))
                });
                if let (Some((path, number)), Some(index)) = (place, self.pane_for(thread)) {
                    self.focus_pane(index);
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
            (MenuTarget::Pane(thread), MenuVerb::CopyTranscript) => {
                if let Some(open) = self.cockpit.thread(thread) {
                    let text = plain_text(open.transcript().blocks());
                    if !text.is_empty() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                }
            }
            (MenuTarget::Thread(thread) | MenuTarget::Pane(thread), MenuVerb::LeaveGroup) => {
                match self.cockpit.apply_group(GroupChange::Leave { thread }) {
                    Ok(_) => self.group_error = None,
                    Err(error) => self.group_error = Some(error.to_string().into()),
                }
                self.sync_panes(cx);
            }
            (MenuTarget::Parked, MenuVerb::ToggleParked) => {
                let open = self.shelf_open(nav::Shelf::Parked);
                self.set_shelf_open(nav::Shelf::Parked, !open, cx);
            }
            (MenuTarget::NavFilter, MenuVerb::FilterProject(project)) => {
                self.choose_nav_filter(project, cx)
            }
            (MenuTarget::NavFilter, MenuVerb::OpenGroup(group)) => self.enter_group(group, cx),
            (MenuTarget::Parked, MenuVerb::DeleteAllParked) => {
                self.delete_parked_threads(cx);
            }
            (_, MenuVerb::NewThread) => {
                let project = match target {
                    MenuTarget::Thread(thread)
                    | MenuTarget::Pane(thread)
                    | MenuTarget::Snooze(thread) => self.cockpit.project_id(thread),
                    MenuTarget::Parked
                    | MenuTarget::NavFilter
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
                    | MenuTarget::Factory
                    | MenuTarget::Page(_) => None,
                };
                self.open_draft(DraftTarget::Main, cx);
                if let (Some(project), Some(draft)) = (project, self.focused_draft_mut()) {
                    draft.binding.choose_checkout(project);
                }
            }
            (_, MenuVerb::Reveal) => {
                if let Some(path) = self.target_path(target) {
                    cx.reveal_path(&path);
                }
            }
            (MenuTarget::Thread(thread), MenuVerb::MarkUnread) => self.cockpit.mark_unread(thread),
            (MenuTarget::Thread(thread), MenuVerb::Fork) => {
                self.fork_thread(thread, None, None, cx)
            }
            (MenuTarget::Thread(thread), MenuVerb::CopyLink) => {
                cx.write_to_clipboard(ClipboardItem::new_string(thread_link(thread)));
            }
            (MenuTarget::Thread(thread), MenuVerb::JoinGroup(group)) => {
                let change = GroupChange::Join {
                    thread,
                    group,
                    index: None,
                };
                match self.cockpit.apply_group(change) {
                    Ok(_) => self.group_error = None,
                    Err(error) => self.group_error = Some(error.to_string().into()),
                }
                self.sync_panes(cx);
            }
            (_, MenuVerb::OpenTerminal) => {
                if let Some(path) = self.target_path(target) {
                    if let Err(error) = terminal_at(&path).spawn() {
                        ferrite_core::log!(
                            "a Terminal did not open at {}: {error}",
                            path.display()
                        );
                    }
                }
            }
            (MenuTarget::Thread(thread), MenuVerb::RefreshPr) => self.check_pr(thread, true, cx),
            (MenuTarget::Thread(thread), MenuVerb::OpenPr) => self.open_pr(thread, cx),
            (_, MenuVerb::CopyPath) => {
                if let Some(path) = self.target_path(target) {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        path.to_string_lossy().to_string(),
                    ));
                }
            }
            (MenuTarget::Thread(thread) | MenuTarget::Pane(thread), MenuVerb::Delete) => {
                if let Err(error) = self.cockpit.delete(thread) {
                    self.group_error = Some(format!("delete refused: {error}").into());
                } else {
                    self.group_error = None;
                }
                if self
                    .popover
                    .as_ref()
                    .is_some_and(|open| open.pane == PaneIdentity::Thread(thread))
                {
                    self.popover = None;
                }
                self.sync_panes(cx);
                self.parked_changed(cx);
            }
            _ => {}
        }
    }

    /// The context menu, floated at the pointer and clamped inside the
    /// window; `deferred` so nothing later in the tree paints over it. A
    /// press on the menu's own dead space is swallowed.
    pub(super) fn context_menu_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let open = self.context_menu.as_ref()?;
        let mut shell = menu::shell().on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
        );
        // Where the open submenu's parent row stands: the submenu's top.
        let mut parent_top = px(ferrite_ui::theme::FLOAT_PAD);
        let mut y = parent_top;
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
                    let face = menu::row(index, item, open.armed == Some(index))
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
                            let Some(open) = view.context_menu.as_mut() else {
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
        let side = open.sub.as_ref().map(|(parent, rows)| {
            let mut side = menu::shell().debug_selector(|| "context-submenu".into());
            for (index, (item, _)) in rows.iter().enumerate() {
                let parent = *parent;
                side = side.child(
                    menu::item_row(("context-submenu-row", index), item, false, false)
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
            // Beside its menu, `SUBMENU_GAP` off its right edge, its first
            // row level with its parent (the shell's pad above both).
            div()
                .absolute()
                .left(gpui::relative(1.))
                .top(parent_top - px(ferrite_ui::theme::FLOAT_PAD))
                .pl(px(ferrite_ui::theme::SUBMENU_GAP))
                .child(side)
        });
        let shell = div()
            .relative()
            .debug_selector(|| "context-menu".into())
            .child(shell)
            .children(side);
        let placed = anchored().anchor(open.corner).position(open.at);
        let placed = match open.drop {
            Some(drop) => placed.offset(gpui::point(px(0.), drop)),
            None => placed.snap_to_window_with_margin(px(ferrite_ui::theme::GRID_PAD)),
        };
        Some(
            deferred(placed.child(ferrite_ui::motion::menu_in(
                "context-menu-in",
                shell,
                ferrite_ui::motion::Opens::Down,
            )))
            .with_priority(2)
            .into_any_element(),
        )
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

/// A Thread's link, in the form the CLI takes: there is no `ferrite://`
/// scheme, so the command that lands every window on it (`thread.focus`,
/// ADR 0027).
pub(super) fn thread_link(thread: ThreadId) -> String {
    format!("ferrite thread focus {}", thread.get())
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
