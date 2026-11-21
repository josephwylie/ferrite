//! The Composer's `/`, `@` and `$` menu (#23): what the focused line's
//! text opens at its caret, drawn as a drawer on that Pane's Composer.
//!
//! A view of its own (ADR 0041): it keeps the open menu and the line it
//! stands on, the mute an escape or a press elsewhere leaves until the
//! text moves, the one edit a recalled prompt makes without opening it,
//! and the file search a Thread's Session is running for `@`. It derives
//! its rows from what the root reads for it (`composer_menu_inputs`),
//! which asks the Provider or the Session only for what the caret opens.
//! A command, a skill or a file it splices into the line itself; every
//! other pick is the root's (`ComposerMenuEvent::Pick`). Its keys stay the
//! root's actions, since the keyboard is in the Composer, and the root
//! hands them over, as it does the Decision keys.

use super::*;

/// The open menu and the line it stands on.
struct Open {
    popover: Popover,
    line: Entity<Composer>,
}

/// A Thread's Session searching its files for the `@` menu.
struct PendingFileSearch {
    pane: PaneIdentity,
    thread: ThreadId,
    generation: u64,
    request_id: u64,
    query: String,
    token_start: usize,
    root: std::path::PathBuf,
    started: std::time::Instant,
    reply: std::sync::mpsc::Receiver<std::io::Result<Vec<ferrite_core::providers::FileSuggestion>>>,
    fallback: Option<std::sync::mpsc::Receiver<Vec<pane::MenuRow>>>,
}

static NEXT_FILE_SEARCH: AtomicU64 = AtomicU64::new(1);

/// What the line's text opens at the caret.
pub(super) enum Wanted {
    /// `$`: the skills.
    Skills { filter: String },
    /// `/`: the commands, with Ferrite's own rows when the token is the
    /// whole line (`standalone`).
    Commands { filter: String, standalone: bool },
    /// `@`: the files, from where the `@` sits.
    Files { token_start: usize, filter: String },
}

impl Wanted {
    /// What `text` opens with the caret at `cursor`, if anything. A slash
    /// inside an active `@path` belongs to file completion.
    fn at(text: &str, cursor: usize) -> Option<Self> {
        let mention = mention_token(text, cursor);
        if mention.is_none() {
            if let Some((_, filter)) = skill_token(text, cursor) {
                return Some(Self::Skills {
                    filter: filter.to_string(),
                });
            }
            if let Some((token_start, filter)) = slash_token(text, cursor) {
                // Local actions consume the entire command line. Inside
                // prose, offer provider skills only so a pick cannot
                // discard the draft.
                return Some(Self::Commands {
                    filter: filter.to_string(),
                    standalone: token_start == 0 && cursor == text.len(),
                });
            }
        }
        let (token_start, filter) = mention?;
        Some(Self::Files {
            token_start,
            filter: filter.to_string(),
        })
    }
}

/// The commands a `/` or `$` menu lists.
pub(super) enum Listed {
    /// The Thread's Session's.
    Session(Vec<ferrite_core::SessionCommand>),
    /// A draft's, before it has a Session (`DraftCommands`): what its
    /// Provider found, whether the ask is still out, why it could not.
    Draft {
        commands: Vec<ferrite_core::SessionCommand>,
        loading: bool,
        error: Option<String>,
    },
    /// A draft whose folder is gone: only `/import` is left.
    Unbound,
}

/// A Thread's Session asked to search its files.
pub(super) struct FileSearch {
    pub(super) thread: ThreadId,
    pub(super) generation: u64,
    pub(super) reply: std::sync::mpsc::Receiver<std::io::Result<Vec<ferrite_core::providers::FileSuggestion>>>,
}

/// What the menu is to list, with what the root read for it.
pub(super) enum Ask {
    Skills {
        filter: String,
        listed: Listed,
    },
    Commands {
        filter: String,
        standalone: bool,
        listed: Listed,
    },
    Files {
        token_start: usize,
        filter: String,
        /// The Pane's root, which the files are under.
        root: std::path::PathBuf,
        /// The Session's search, when a Thread's Session took it.
        search: Option<FileSearch>,
        /// A machine's files are its Sessions' to search (ADR 0026).
        on_machine: bool,
    },
}

/// What the root reads for the menu (`composer_menu_inputs`).
pub(super) struct ComposerMenuInputs {
    /// The focused Pane, whose Composer the menu stands on.
    pub(super) pane: PaneIdentity,
    pub(super) line: Entity<Composer>,
    pub(super) ask: Ask,
}

/// A pick only the root can make: open a picker, choose a Provider, an
/// effort or a band choice.
pub(super) enum ComposerMenuEvent {
    Pick {
        pane: PaneIdentity,
        consequence: Consequence,
    },
}

impl EventEmitter<ComposerMenuEvent> for ComposerMenuView {}

#[derive(Default)]
pub(super) struct ComposerMenuView {
    open: Option<Open>,
    /// Escape (or a press elsewhere) dismissed the menu: it stays shut
    /// until the text moves again, or the next derive would reopen it on
    /// the very text the operator dismissed it over.
    muted: bool,
    /// A recalled slash or mention prompt is a programmatic edit: its
    /// `Edited` opens nothing. The next operator edit lifts the mute and
    /// derives again.
    suppress_recall_once: bool,
    pending_files: Option<PendingFileSearch>,
}

impl ComposerMenuView {
    /// The open menu.
    pub(super) fn popover(&self) -> Option<&Popover> {
        self.open.as_ref().map(|open| &open.popover)
    }

    /// The Pane whose Composer the menu stands on.
    pub(super) fn pane(&self) -> Option<PaneIdentity> {
        self.popover().map(|open| open.pane)
    }

    pub(super) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The `/` menu's row under the cursor, which ⇥ picks.
    pub(super) fn command_selected(&self) -> Option<usize> {
        self.popover()
            .filter(|open| matches!(open.kind, Kind::Commands))
            .map(|open| open.selected)
    }

    #[cfg(test)]
    pub(super) fn muted(&self) -> bool {
        self.muted
    }

    /// What `line` opens at its caret, unless the menu is muted.
    pub(super) fn wanted(&self, line: &Composer) -> Option<Wanted> {
        if self.muted {
            return None;
        }
        Wanted::at(line.text(), line.cursor())
    }

    /// The line was edited: whether to derive the menu again. The edit a
    /// recalled prompt makes closes it instead.
    pub(super) fn edited(&mut self, cx: &mut Context<Self>) -> bool {
        if self.suppress_recall_once {
            self.suppress_recall_once = false;
            self.close(cx);
            return false;
        }
        self.muted = false;
        true
    }

    /// A prompt was recalled into the line: shut, and its edit opens
    /// nothing.
    pub(super) fn recalled(&mut self, cx: &mut Context<Self>) {
        self.close(cx);
        self.muted = true;
        self.suppress_recall_once = true;
    }

    /// Shut, as another surface opens or the Pane goes.
    pub(super) fn close(&mut self, cx: &mut Context<Self>) {
        if self.open.take().is_some() {
            cx.notify();
        }
    }

    /// Escape or a press elsewhere: shut, muted until the text moves.
    /// Whether it was open.
    pub(super) fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        if self.open.take().is_none() {
            return false;
        }
        self.muted = true;
        cx.notify();
        true
    }

    /// The menu for what the root read, or none.
    pub(super) fn derive(&mut self, inputs: Option<ComposerMenuInputs>, cx: &mut Context<Self>) {
        let open = inputs.and_then(|inputs| {
            let line = inputs.line.clone();
            self.rows(inputs).map(|popover| Open { popover, line })
        });
        self.open = open;
        cx.notify();
    }

    fn rows(&mut self, inputs: ComposerMenuInputs) -> Option<Popover> {
        let identity = inputs.pane;
        match inputs.ask {
            Ask::Skills { filter, listed } => skill_menu(identity, &filter, listed),
            Ask::Commands {
                filter,
                standalone,
                listed,
            } => command_menu(identity, &filter, standalone, listed),
            Ask::Files {
                token_start,
                filter,
                root,
                search,
                on_machine,
            } => self.file_menu(identity, token_start, filter, root, search, on_machine),
        }
    }

    fn file_menu(
        &mut self,
        pane: PaneIdentity,
        token_start: usize,
        filter: String,
        root: std::path::PathBuf,
        search: Option<FileSearch>,
        on_machine: bool,
    ) -> Option<Popover> {
        if let Some(search) = search {
            let request_id = NEXT_FILE_SEARCH.fetch_add(1, Ordering::Relaxed);
            self.pending_files = Some(PendingFileSearch {
                pane,
                thread: search.thread,
                generation: search.generation,
                request_id,
                query: filter.clone(),
                token_start,
                root,
                started: std::time::Instant::now(),
                reply: search.reply,
                fallback: None,
            });
            return Some(Popover {
                scroll: ScrollHandle::new(),
                revealed: Default::default(),
                pane,
                kind: Kind::Files {
                    files: std::rc::Rc::new(Vec::new()),
                    token_start,
                    query: filter,
                    request_id,
                },
                rows: vec![Row {
                    row: pane::MenuRow {
                        insert: SharedString::default(),
                        name: "Searching workspace files...".into(),
                        matched: Vec::new(),
                        detail: SharedString::default(),
                        prose_detail: true,
                        inert: true,
                        tag: None,
                    },
                    active: false,
                    consequence: Consequence::Inert,
                }],
                selected: 0,
            });
        }
        // A machine's files are its Sessions' to search (ADR 0026).
        if on_machine {
            return None;
        }
        // The walk runs once per open menu; keystrokes only re-filter it.
        let walked = match self.popover() {
            Some(open) if open.pane == pane => match &open.kind {
                Kind::Files { files, .. } => Some(files.clone()),
                _ => None,
            },
            _ => None,
        };
        let files = walked.unwrap_or_else(|| {
            std::rc::Rc::new(ferrite_core::workspace::mention_files(
                &root,
                MENTION_FILE_CAP,
            ))
        });
        let rows: Vec<Row> = mention_rows(&files, &filter)
            .into_iter()
            .map(|row| Row {
                consequence: Consequence::Mention(row.insert.clone()),
                row,
                active: false,
            })
            .collect();
        if rows.is_empty() {
            return Some(note_popover(pane, NO_FILES));
        }
        Some(Popover {
            scroll: ScrollHandle::new(),
            revealed: Default::default(),
            pane,
            kind: Kind::Files {
                files,
                token_start,
                query: filter,
                request_id: 0,
            },
            rows,
            selected: 0,
        })
    }

    /// The Thread whose Session is searching files for the `@` menu.
    pub(super) fn searching(&self) -> Option<ThreadId> {
        self.pending_files.as_ref().map(|pending| pending.thread)
    }

    /// Take the Session's file search answer, if it came: `focused` is the
    /// focused Pane and `generation` the searching Thread's Session's, as
    /// the root reads them now. An answer for a menu since closed or
    /// moved, or a Session since restarted, is dropped. A Session silent
    /// for five seconds is walked for locally instead.
    pub(super) fn poll_files(
        &mut self,
        focused: Option<PaneIdentity>,
        generation: Option<u64>,
        on_machine: bool,
        cx: &mut Context<Self>,
    ) {
        use std::sync::mpsc::TryRecvError;
        let Some(mut pending) = self.pending_files.take() else {
            return;
        };
        let current = focused == Some(pending.pane)
            && generation == Some(pending.generation)
            && self.popover().is_some_and(|open| open.pane == pending.pane && matches!(&open.kind, Kind::Files { request_id, query, token_start, .. } if *request_id == pending.request_id && query == &pending.query && *token_start == pending.token_start));
        if !current {
            return;
        }
        let rows = if let Some(fallback) = &pending.fallback {
            match fallback.try_recv() {
                Ok(rows) => Some(rows),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Vec::new()),
            }
        } else {
            match pending.reply.try_recv() {
                Ok(Ok(files)) => Some(native_mention_rows(&files)),
                Err(TryRecvError::Empty) if pending.started.elapsed() < Duration::from_secs(5) => None,
                // A machine's files are walked by none of this
                // computer's threads (ADR 0026).
                _ if on_machine => Some(Vec::new()),
                _ => {
                    let root = pending.root.clone();
                    let query = pending.query.clone();
                    let (tx, rx) = std::sync::mpsc::channel();
                    if std::thread::Builder::new()
                        .name("ferrite-file-fallback".into())
                        .spawn(move || {
                            let paths =
                                ferrite_core::workspace::mention_files(&root, MENTION_FILE_CAP);
                            let _ = tx.send(mention_rows(&paths, &query));
                        })
                        .is_ok()
                    {
                        pending.fallback = Some(rx);
                        None
                    } else {
                        Some(Vec::new())
                    }
                }
            }
        };
        let Some(rows) = rows else {
            self.pending_files = Some(pending);
            return;
        };
        if let Some(open) = &mut self.open {
            let open = &mut open.popover;
            open.rows = rows
                .into_iter()
                .map(|row| Row {
                    consequence: Consequence::Mention(row.insert.clone()),
                    row,
                    active: false,
                })
                .collect();
            if open.rows.is_empty() {
                open.rows = vec![note_row(NO_FILES)];
            }
            open.selected = 0;
        }
        cx.notify();
    }

    /// ↑ or ↓: step the cursor by `delta`.
    pub(super) fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.open.as_mut().is_some_and(|open| open.popover.step(delta)) {
            cx.notify();
        }
    }

    /// The pointer over row `at` of the `/` menu puts the cursor there.
    fn hover(&mut self, at: usize, cx: &mut Context<Self>) {
        if let Some(open) = self.open.as_mut().map(|open| &mut open.popover) {
            if matches!(open.kind, Kind::Commands)
                && open.selected != at
                && open.rows.get(at).is_some_and(|row| !row.inert)
            {
                open.selected = at;
                cx.notify();
            }
        }
    }

    /// ↵ (the cursor's row) or a press on row `at`: a command, a skill or
    /// a file lands in the line; any other pick is the root's. The menu
    /// closes either way: a picker it opens takes its place.
    pub(super) fn pick(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(Open { popover, line }) = self.open.take() else {
            return;
        };
        cx.notify();
        let Some(row) = popover.rows.get(at) else {
            return;
        };
        match &row.consequence {
            Consequence::Command(name) => {
                line.update(cx, |composer, cx| {
                    let cursor = composer.cursor();
                    let text = composer.text();
                    let Some((start, _)) = slash_token(text, cursor) else {
                        return;
                    };
                    // Complete the whole token even when editing its middle.
                    let mut end = cursor
                        + text[cursor..]
                            .find(char::is_whitespace)
                            .unwrap_or(text.len() - cursor);
                    // Reuse an existing separator instead of inserting two spaces.
                    if text[end..].starts_with(' ') {
                        end += 1;
                    }
                    // The pick is a chip, as a mention is; it sends as
                    // the plain `/name` it reads.
                    let token = format!("/{name}");
                    composer.splice(start..end, &format!("{token} "), cx);
                    composer.stage_chip(SharedString::from(token), cx);
                });
            }
            Consequence::Skill(name) => {
                line.update(cx, |composer, cx| {
                    let cursor = composer.cursor();
                    let text = composer.text();
                    let Some((start, _)) = skill_token(text, cursor) else {
                        return;
                    };
                    let mut end = cursor
                        + text[cursor..]
                            .find(char::is_whitespace)
                            .unwrap_or(text.len() - cursor);
                    if text[end..].starts_with(' ') {
                        end += 1;
                    }
                    let token = format!("${name}");
                    composer.splice(start..end, &format!("{token} "), cx);
                    composer.stage_chip(SharedString::from(token), cx);
                });
            }
            Consequence::Mention(path) => {
                let Kind::Files { token_start, .. } = popover.kind else {
                    return;
                };
                let token = format!("@{path}");
                line.update(cx, |composer, cx| {
                    let cursor = composer.cursor();
                    composer.splice(token_start..cursor, &format!("{token} "), cx);
                    composer.stage_chip(SharedString::from(token), cx);
                });
            }
            Consequence::Inert => {}
            consequence => cx.emit(ComposerMenuEvent::Pick {
                pane: popover.pane,
                consequence: consequence.clone(),
            }),
        }
    }
}

impl Render for ComposerMenuView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.open {
            Some(open) => menu_surface(
                &open.popover,
                Some(&open.popover.kind),
                Self::hover,
                |menu, at, _, cx| menu.pick(at, cx),
                cx,
            ),
            None => div().into_any_element(),
        }
    }
}

/// The `$` menu (T3's): the skills the Session serving the focused Pane
/// lists, through the fuzzy filter; while a draft's catalogue loads, or
/// when none matches, one inert line says so.
fn skill_menu(pane: PaneIdentity, filter: &str, listed: Listed) -> Option<Popover> {
    let (skills, loading) = match listed {
        Listed::Session(commands) => (commands, false),
        Listed::Draft { commands, loading, .. } => (commands, loading),
        Listed::Unbound => return None,
    };
    let skills: Vec<_> = skills.into_iter().filter(|command| command.is_skill()).collect();
    let mut rows: Vec<Row> = command_rows(&skills, filter)
        .into_iter()
        .map(|row| Row {
            consequence: Consequence::Skill(row.insert.clone()),
            row,
            active: false,
        })
        .collect();
    if rows.is_empty() {
        rows.push(note_row(if loading {
            "Searching workspace skills..."
        } else {
            "No skills found. Try / to browse provider commands."
        }));
    }
    Some(Popover {
        scroll: ScrollHandle::new(),
        revealed: Default::default(),
        pane,
        kind: Kind::Commands,
        rows,
        selected: 0,
    })
}

/// The `/` menu: the commands listed, through the fuzzy filter, with
/// Ferrite's own rows on top of a command line.
fn command_menu(pane: PaneIdentity, filter: &str, standalone: bool, listed: Listed) -> Option<Popover> {
    let popover = |rows: Vec<Row>| Popover {
        scroll: ScrollHandle::new(),
        revealed: Default::default(),
        pane,
        kind: Kind::Commands,
        rows,
        selected: 0,
    };
    let import = || local_row(filter, "import", "Resume a Claude or Codex session", false);
    match listed {
        Listed::Unbound => {
            // Import remains available when a workspace was removed.
            if !standalone {
                return None;
            }
            let row = import()?;
            Some(popover(vec![Row {
                row,
                active: false,
                consequence: Consequence::OpenImportPicker,
            }]))
        }
        Listed::Draft {
            commands,
            loading,
            error,
        } => {
            let mut rows: Vec<Row> = command_rows(&commands, filter)
                .into_iter()
                .map(|row| Row {
                    consequence: Consequence::Command(row.insert.clone()),
                    row,
                    active: false,
                })
                .collect();
            if let Some(row) = standalone.then(import).flatten() {
                rows.retain(|existing| existing.row.name != row.name);
                rows.insert(
                    0,
                    Row {
                        row,
                        active: false,
                        consequence: Consequence::OpenImportPicker,
                    },
                );
            }
            let status = if loading {
                Some(("Searching workspace skills...", ""))
            } else {
                error.as_deref().map(|error| ("Could not load commands", error))
            };
            if let Some((name, detail)) = status {
                rows.push(Row {
                    row: pane::MenuRow {
                        insert: "".into(),
                        name: name.into(),
                        detail: detail.to_string().into(),
                        matched: Vec::new(),
                        prose_detail: true,
                        inert: true,
                        tag: None,
                    },
                    active: false,
                    consequence: Consequence::Inert,
                });
            }
            if rows.is_empty() {
                return standalone.then(|| note_popover(pane, NO_COMMAND));
            }
            Some(popover(rows))
        }
        Listed::Session(commands) => {
            let mut rows: Vec<Row> = command_rows(&commands, filter)
                .into_iter()
                .map(|row| Row {
                    consequence: Consequence::Command(row.insert.clone()),
                    row,
                    active: false,
                })
                .collect();
            // Ferrite's local rows ride on top, through the same fuzzy
            // filter and under the same cap as every row. #11: `import`
            // always — in the Thread's place while it is blank, else
            // alongside. #25: `provider` always — live before the first
            // prompt, and kept visible but inert after it, so the door's
            // absence never reads as a bug.
            // A local row replaces a provider command of the same name
            // (Claude lists its own `effort`): Ferrite's opens the picker,
            // and the typed form (`/effort max`) still works — `submit`
            // reads it.
            let push_local = |rows: &mut Vec<Row>, row: Row| {
                rows.retain(|existing| existing.row.name != row.row.name);
                rows.insert(0, row);
                rows.truncate(MENU_ROWS_MAX);
            };
            if standalone {
                if let Some(row) = import() {
                    push_local(
                        &mut rows,
                        Row {
                            row,
                            active: false,
                            consequence: Consequence::OpenImportPicker,
                        },
                    );
                }
            }
            // The model can change at any time, and so can the provider:
            // after the first prompt the other one starts a fresh
            // conversation with the earlier one handed over as context,
            // which the picker itself explains.
            let detail = "Switch model or hand over to the other provider";
            if let Some(row) = standalone
                .then(|| local_row(filter, "effort", "Switch reasoning effort", false))
                .flatten()
            {
                push_local(
                    &mut rows,
                    Row {
                        row,
                        active: false,
                        consequence: Consequence::OpenEffortPicker,
                    },
                );
            }
            if let Some(row) = standalone
                .then(|| local_row(filter, "model", detail, false))
                .flatten()
            {
                push_local(
                    &mut rows,
                    Row {
                        row,
                        active: false,
                        consequence: Consequence::OpenProviderPicker,
                    },
                );
            }
            // A command line with no match says so; a `/word` in prose
            // that names nothing is prose, and opens nothing.
            if rows.is_empty() {
                return standalone.then(|| note_popover(pane, NO_COMMAND));
            }
            Some(popover(rows))
        }
    }
}

impl CockpitView {
    /// Derive the Composer menu afresh from the focused line. A band menu
    /// reads the line itself (`sync_band_rows`).
    pub(super) fn sync_menu(&mut self, cx: &mut Context<Self>) {
        if self.band_menu().is_some() {
            return;
        }
        let inputs = self.composer_menu_inputs(cx);
        self.composer_menu
            .clone()
            .update(cx, |menu, cx| menu.derive(inputs, cx));
    }

    /// What the Composer menu reads of the window to list what the focused
    /// line opens at its caret: the Pane and its Composer, and the
    /// Thread's commands, the draft's listing (`DraftCommands`) or the
    /// Pane's root and the Session's file search. It asks the Provider or
    /// the Session only for what that menu lists. None when it opens
    /// nothing.
    fn composer_menu_inputs(&mut self, cx: &mut Context<Self>) -> Option<ComposerMenuInputs> {
        let index = self.focused(cx);
        let pane = self.panes.get(index)?;
        let wanted = self.composer_menu.read(cx).wanted(pane.composer.read(cx))?;
        let identity = pane.identity;
        let line = pane.composer.clone();
        let thread = pane.thread();
        let ask = match wanted {
            Wanted::Skills { filter } => {
                let listed = match thread {
                    Some(thread) => Listed::Session(self.cockpit.read(cx).thread(thread)?.commands().to_vec()),
                    None => {
                        let draft = pane.draft()?;
                        let provider = draft.binding.provider().provider;
                        let cwd = draft
                            .binding
                            .resolve(self.cockpit.read(cx).registry(), !self.on_machine(cx))
                            .ok()?
                            .source_root()
                            .to_path_buf();
                        self.draft_listed(provider, cwd, &line, cx)
                    }
                };
                Ask::Skills { filter, listed }
            }
            Wanted::Commands { filter, standalone } => {
                let listed = match thread {
                    Some(thread) => Listed::Session(self.cockpit.read(cx).thread(thread)?.commands().to_vec()),
                    None => {
                        let draft = pane.draft()?;
                        let provider = draft.binding.provider().provider;
                        let cwd = draft
                            .binding
                            .resolve(self.cockpit.read(cx).registry(), !self.on_machine(cx))
                            .ok()
                            .map(|workspace| workspace.source_root().to_path_buf());
                        match cwd {
                            Some(cwd) => self.draft_listed(provider, cwd, &line, cx),
                            None => {
                                // Never reuse commands from the previous
                                // valid binding.
                                self.draft_commands.forget();
                                Listed::Unbound
                            }
                        }
                    }
                };
                Ask::Commands {
                    filter,
                    standalone,
                    listed,
                }
            }
            Wanted::Files { token_start, filter } => {
                // No binding → nothing to walk → no menu.
                let root = self.pane_root(index, cx)?;
                let search = match thread {
                    Some(thread) => match self.cockpit.update(cx, |cockpit, _| cockpit.search_files(thread, &filter)) {
                        Ok(reply) => Some(FileSearch {
                            thread,
                            generation: self.cockpit.read(cx).thread(thread)?.generation(),
                            reply,
                        }),
                        Err(_) => None,
                    },
                    None => None,
                };
                Ask::Files {
                    token_start,
                    filter,
                    root,
                    search,
                    on_machine: self.on_machine(cx),
                }
            }
        };
        Some(ComposerMenuInputs {
            pane: identity,
            line,
            ask,
        })
    }

    /// The draft's listing for `provider` in `cwd`, as the menu lists it.
    fn draft_listed(
        &mut self,
        provider: Provider,
        cwd: std::path::PathBuf,
        line: &Entity<Composer>,
        cx: &mut Context<Self>,
    ) -> Listed {
        let listing = self.draft_commands(provider, cwd, line, cx);
        Listed::Draft {
            commands: listing.commands.clone(),
            loading: listing.loading(),
            error: listing.error().map(str::to_string),
        }
    }

    /// The Session's file search answer for the `@` menu, if it came.
    pub(super) fn poll_composer_menu(&mut self, cx: &mut Context<Self>) {
        let menu = self.composer_menu.clone();
        let Some(thread) = menu.read(cx).searching() else {
            return;
        };
        let focused = self.panes.get(self.focused(cx)).map(|pane| pane.identity);
        let generation = self.cockpit.read(cx).thread(thread).map(|open| open.generation());
        let on_machine = self.on_machine(cx);
        menu.update(cx, |menu, cx| menu.poll_files(focused, generation, on_machine, cx));
    }

    /// Shut the Composer menu, as another surface opens or its Pane goes.
    pub(super) fn close_composer_menu(&mut self, cx: &mut Context<Self>) {
        self.composer_menu.update(cx, |menu, cx| menu.close(cx));
    }

    /// The open Composer menu, for a test to read.
    #[cfg(test)]
    pub(super) fn popover<'a>(&self, cx: &'a gpui::App) -> Option<&'a Popover> {
        self.composer_menu.read(cx).popover()
    }

    /// A pick the menu leaves to the root.
    pub(super) fn on_composer_menu_event(
        &mut self,
        _: &Entity<ComposerMenuView>,
        event: &ComposerMenuEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ComposerMenuEvent::Pick { pane, consequence } = event;
        self.run_pick(*pane, consequence, window, cx);
    }
}
