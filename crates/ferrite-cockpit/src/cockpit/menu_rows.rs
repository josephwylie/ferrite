//! The rows the Composer's menus are made of, shared by the `/`, `@` and
//! `$` menu, a draft's band menu and the model picker: the popover that
//! holds them (`Popover`), each row beside what picking it does
//! (`Row`, `Consequence`), the tokens the line's text opens a menu on, and
//! the builders that turn commands, files and Ferrite's own choices into
//! rows.

use super::*;

/// The one popover the Composer's slot can hold — the `/` or `@` menu
/// (#23), the provider picker (#25) or a draft's band chip (#29) — and
/// everything it shows. At most one for the whole cockpit, always on the
/// focused Pane's Composer: `pane` names that Pane by the roster's own
/// identity. Rows are discovered when it opens and re-derived only by its
/// kind's own rule, never per frame; each row carries what picking it
/// does, so the two can never drift apart.
pub(super) struct Popover<K = Kind> {
    /// The Pane whose Composer holds it.
    pub(super) pane: PaneIdentity,
    pub(super) kind: K,
    pub(super) rows: Vec<Row>,
    pub(super) selected: usize,
    /// The row list's scroll: arrows keep the keyboard cursor in view.
    /// A fresh popover reveals its first cursor once (`revealed`), which
    /// may be a picker's standing choice below the fold.
    pub(super) scroll: ScrollHandle,
    pub(super) revealed: std::cell::Cell<bool>,
}

/// Which popover holds the slot. The rows, keys, dismissal, heal and paint
/// are shared; the kind decides the row recipe and the rule that closes it.
pub(super) enum Kind {
    /// `/` — the Session's own commands (Claude's initialize `commands[]`,
    /// Codex's skills/list), straight from core. Nothing static — except
    /// Ferrite's own local rows riding on top, never sent to the provider:
    /// `provider` (#25) — the picker's door pre-lock, an inert explanation
    /// after — then `import` while the Thread still offers adoption (#11).
    /// Derived from the line's own text: every edit re-syncs it.
    Commands,
    /// `@` — files under the Thread's workspace binding. The walk runs once
    /// when the menu opens and is filtered per keystroke; `token_start` is
    /// where the `@` sits, so a pick knows what to splice out. Text-derived
    /// like `Commands`.
    Files {
        files: std::rc::Rc<Vec<String>>,
        token_start: usize,
        query: String,
        request_id: u64,
    },
}

/// One row beside its own consequence. The paint half is the Pane's
/// `MenuRow`, reached through `Deref` so a row reads like the line it
/// draws.
pub(super) struct Row {
    pub(super) row: pane::MenuRow,
    /// The ✓ — the standing choice on picker and band rows; never on a
    /// menu row.
    pub(super) active: bool,
    pub(super) consequence: Consequence,
}

impl Row {
    pub(super) fn consequence_is_inert(&self) -> bool {
        matches!(self.consequence, Consequence::Inert)
    }
}

impl std::ops::Deref for Row {
    type Target = pane::MenuRow;

    fn deref(&self) -> &pane::MenuRow {
        &self.row
    }
}

/// What picking a row does. A command or a file lands in the line; every
/// other pick is Ferrite's own act, never a prompt.
#[derive(Clone, PartialEq)]
pub(super) enum Consequence {
    /// Replace the active `/filter` with `/name ` — sent later as plain text
    /// on Claude and translated to the typed skill item inside the Codex
    /// Session.
    Command(SharedString),
    /// Replace the active `$filter` with the skill's `$name ` token, which
    /// goes on the wire as `/name` (`composer::wire_text`).
    Skill(SharedString),
    /// The local `provider` row pre-lock (#25): clear the line and open
    /// the picker in the menu's place.
    OpenProviderPicker,
    /// The local `effort` row: clear the line and open the effort picker.
    OpenEffortPicker,
    /// An effort-row pick: this level, or None for the operator's default.
    Effort(Option<String>),
    /// The local `import` row (#11): open the import picker — clearing the
    /// line on a Thread, and leaving it alone on a draft, whose typed
    /// command must survive a refused adoption.
    OpenImportPicker,
    /// The locked door's row is an explanation, not an offer: its pick
    /// dismisses and nothing else.
    Inert,
    /// Replace the `@token` with `@rel/path ` and stage the comp's pill
    /// over it, whichever the provider: the wire stays untouched — Claude's
    /// CLI reads the `@path` text itself, Codex's send derives its mention
    /// item — the pick just paints the standing token.
    Mention(SharedString),
    Provision(ProviderChoice),
    Band(BandChoice),
}

/// What picking a band row does to the focused draft.
#[derive(Clone, PartialEq)]
pub(super) enum BandChoice {
    Provider(ProviderChoice),
    /// The effort chip: a rung, or None for the operator's default.
    Effort(Option<String>),
    Project(ProjectKey),
    /// The type-a-path row: register the typed path as a project, then
    /// choose it.
    RegisterPath(std::path::PathBuf),
    Target(DraftTarget),
    /// Open the Project card on the draft's machine.
    AddProject,
    /// Run the draft on this machine instead (`pick_environment`).
    Environment(ferrite_core::MachineId),
    /// The card on this copy (`show_copy`).
    Copy(super::copies::CopyKey),
    /// Settings › Connections: `Add environment…`, or a machine the window
    /// cannot reach.
    Connections,
}

/// How many rows a Composer menu shows — a dense keyboard menu, not a
/// browser; the fuzzy filter is how the operator reaches the rest.
pub(super) const MENU_ROWS_MAX: usize = 8;

/// How many files the `@` walk will offer. Bounds one open, not a frame:
/// the walk runs when the menu opens and keystrokes only re-filter it.
pub(super) const MENTION_FILE_CAP: usize = 2000;

/// The slash and filter immediately before the caret, anywhere in a draft.
/// Whitespace ends the filter; a letter must directly follow the slash.
/// A lone leading slash keeps the existing full-command discovery shortcut.
pub(super) fn slash_token(text: &str, cursor: usize) -> Option<(usize, &str)> {
    let head = text.get(..cursor)?;
    let start = head.rfind('/')?;
    let filter = &head[start + 1..];
    if filter.contains(char::is_whitespace) {
        return None;
    }
    let valid = filter
        .chars()
        .next()
        .map_or(start == 0, char::is_alphabetic);
    valid.then_some((start, filter))
}

/// T3's empty Composer menus' words.
pub(super) const NO_COMMAND: &str = "No matching command.";

pub(super) const NO_FILES: &str = "No matching files or folders.";

/// A Composer menu's one inert line: what it is searching, or that
/// nothing matched.
pub(super) fn note_row(note: &'static str) -> Row {
    Row {
        row: pane::MenuRow {
            insert: "".into(),
            name: note.into(),
            detail: "".into(),
            matched: Vec::new(),
            prose_detail: true,
            inert: true,
            tag: None,
        },
        active: false,
        consequence: Consequence::Inert,
    }
}

/// A Composer menu holding only `note_row(note)`.
pub(super) fn note_popover(pane: PaneIdentity, note: &'static str) -> Popover {
    Popover {
        scroll: ScrollHandle::new(),
        revealed: Default::default(),
        pane,
        kind: Kind::Commands,
        rows: vec![note_row(note)],
        selected: 0,
    }
}

/// The `$` token the caret sits in (T3's skill trigger): the `$`'s byte
/// offset and the filter typed after it. As `@`, the `$` opens a token —
/// start of line or after whitespace — and a digit after it is a price,
/// not a skill (`$5`).
pub(super) fn skill_token(text: &str, cursor: usize) -> Option<(usize, &str)> {
    let head = text.get(..cursor)?;
    let at = head.rfind('$')?;
    let filter = &head[at + 1..];
    if filter.contains(char::is_whitespace) || filter.starts_with(|ch: char| ch.is_ascii_digit()) {
        return None;
    }
    let opens_token = at == 0 || text[..at].ends_with(char::is_whitespace);
    opens_token.then_some((at, filter))
}

/// The `@` token the caret sits in: the `@`'s byte offset and the filter
/// typed after it. The `@` must open a token — start of line or after
/// whitespace — so `a@b.example` stays prose, exactly as the wire reads it.
pub(super) fn mention_token(text: &str, cursor: usize) -> Option<(usize, &str)> {
    let head = text.get(..cursor)?;
    let at = head.rfind('@')?;
    let filter = &head[at + 1..];
    if filter.contains(char::is_whitespace) {
        return None;
    }
    let opens_token = at == 0 || text[..at].ends_with(char::is_whitespace);
    opens_token.then_some((at, filter))
}

/// The `/` menu's rows: the Session's own commands through the fuzzy
/// filter, best first (ties keep the provider's order), capped, each
/// tagged with where it comes from.
pub(super) fn command_rows(commands: &[ferrite_core::SessionCommand], filter: &str) -> Vec<pane::MenuRow> {
    let mut scored: Vec<(i64, pane::MenuRow)> = commands
        .iter()
        .filter_map(|command| {
            let (score, matched) = ferrite_ui::fuzzy::matches(filter, &command.name)?;
            Some((
                score,
                pane::MenuRow {
                    insert: SharedString::from(command.name.clone()),
                    // The bare name, as Claude's menu lists it: the `/` is
                    // already on the line.
                    name: SharedString::from(command.name.clone()),
                    matched,
                    detail: SharedString::from(command.description.clone()),
                    prose_detail: true,
                    inert: false,
                    // Where it comes from, as the provider says.
                    tag: command
                        .source
                        .as_ref()
                        .and_then(ferrite_core::CommandSource::tag)
                        .map(SharedString::from),
                },
            ))
        })
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored
        .into_iter()
        .take(MENU_ROWS_MAX)
        .map(|(_, row)| row)
        .collect()
}

/// The `@` menu's rows: the walked files through the fuzzy filter. The
/// match runs over the whole relative path; the row shows name and
/// directory apart (PromptBox state 03), so highlights are clamped into
/// the name they decorate.
pub(super) fn mention_rows(files: &[String], filter: &str) -> Vec<pane::MenuRow> {
    let mut scored: Vec<(i64, pane::MenuRow)> = files
        .iter()
        .filter_map(|file| {
            let (score, matched) = ferrite_ui::fuzzy::matches(filter, file)?;
            // A directory ends in `/`: its name is the last segment with
            // that slash, its directory everything before.
            let stem = file.strip_suffix('/').unwrap_or(file);
            let split = stem.rfind('/').map(|at| at + 1).unwrap_or(0);
            let matched = matched
                .into_iter()
                .filter_map(|range| {
                    let start = range.start.max(split);
                    (range.end > split).then(|| start - split..range.end - split)
                })
                .collect();
            Some((
                score,
                pane::MenuRow {
                    insert: SharedString::from(file.clone()),
                    name: SharedString::from(file[split..].to_string()),
                    matched,
                    detail: SharedString::from(if split == 0 {
                        String::new()
                    } else {
                        file[..split - 1].to_string()
                    }),
                    prose_detail: false,
                    inert: false,
                    tag: None,
                },
            ))
        })
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored
        .into_iter()
        .take(MENU_ROWS_MAX)
        .map(|(_, row)| row)
        .collect()
}

/// Preserve provider order and matches, while keeping repository metadata
/// and repeated paths from crowding working files out of the bounded menu.
pub(super) fn native_mention_rows(files: &[ferrite_core::prompt_files::FileSuggestion]) -> Vec<pane::MenuRow> {
    let mut seen = std::collections::HashSet::new();
    files
        .iter()
        .filter(|file| {
            !std::path::Path::new(&file.path)
                .components()
                .any(|part| part.as_os_str() == ".git")
                && seen.insert(file.path.trim_end_matches('/'))
        })
        .take(MENU_ROWS_MAX)
        .map(|file| {
            let mut path = file.path.clone();
            if file.is_directory && !path.ends_with('/') {
                path.push('/');
            }
            let stem = path.strip_suffix('/').unwrap_or(&path);
            let split = stem.rfind('/').map(|at| at + 1).unwrap_or(0);
            let chars: Vec<_> = path.char_indices().collect();
            let matched = file
                .matched
                .iter()
                .filter_map(|index| {
                    let (start, character) = *chars.get(*index)?;
                    (start >= split).then_some(
                        start.saturating_sub(split)
                            ..start.saturating_sub(split) + character.len_utf8(),
                    )
                })
                .collect();
            pane::MenuRow {
                insert: path.clone().into(),
                name: path[split..].to_string().into(),
                matched,
                detail: if split == 0 {
                    "".into()
                } else {
                    path[..split - 1].to_string().into()
                },
                prose_detail: false,
                inert: false,
                tag: None,
            }
        })
        .collect()
}

/// One Ferrite-local `/` row (#11's `import`, #25's `provider`) — never
/// the provider's, which its description says out loud. The same fuzzy
/// filter as every row.
pub(super) fn local_row(
    filter: &str,
    name: &'static str,
    detail: &'static str,
    inert: bool,
) -> Option<pane::MenuRow> {
    let (_, matched) = ferrite_ui::fuzzy::matches(filter, name)?;
    Some(pane::MenuRow {
        insert: SharedString::from(name),
        name: SharedString::from(name),
        matched,
        detail: SharedString::from(detail),
        prose_detail: true,
        inert,
        tag: None,
    })
}

/// One band row: a label and its detail beside the choice it makes.
pub(super) fn band_row(label: SharedString, detail: SharedString, active: bool, choice: BandChoice) -> Row {
    Row {
        row: pane::MenuRow {
            insert: SharedString::default(),
            name: label,
            matched: Vec::new(),
            detail,
            prose_detail: false,
            inert: false,
            tag: None,
        },
        active,
        consequence: Consequence::Band(choice),
    }
}

/// A draft's Project chip's rows: each registered Project, `draft`'s own
/// ticked, then, when the line `typed` reads `path <dir>`, a row that
/// registers it, and last `Add project…`.
pub(super) fn project_rows(
    projects: &[(ProjectKey, &ferrite_core::project::Project, Option<String>)],
    draft: &pane::DraftBinding,
    typed: &str,
) -> Vec<Row> {
    let mut rows: Vec<Row> = projects
        .iter()
        .map(|(key, project, machine)| {
            let mut detail = if project.additional_roots.is_empty() {
                project.root.display().to_string()
            } else {
                let count = project.additional_roots.len();
                format!(
                    "{} · +{count} director{}",
                    project.root.display(),
                    if count == 1 { "y" } else { "ies" }
                )
            };
            // Another machine's Project says whose it is.
            if let Some(machine) = machine {
                detail = format!("{machine} · {detail}");
            }
            band_row(
                SharedString::from(project.title.clone()),
                SharedString::from(detail),
                draft.binding.project() == *key,
                BandChoice::Project(*key),
            )
        })
        .collect();
    // Explicit type-a-path grammar. Ordinary drafted prose is
    // never reinterpreted or erased as registry input.
    if let Some(path) = typed
        .strip_prefix("path ")
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        rows.push(band_row(
            SharedString::from(format!("add {path}")),
            SharedString::from("register path"),
            false,
            // As typed: `~` is the home of the computer the
            // Server runs on, which spells it out.
            BandChoice::RegisterPath(path.into()),
        ));
    }
    // The same verb the nav's dropdown ends on, opening the
    // same card: one way to add a Project, wherever it is
    // asked for.
    rows.push(band_row(
        SharedString::from("Add project…"),
        SharedString::from("name it and choose its directories"),
        false,
        BandChoice::AddProject,
    ));
    rows
}

/// One environment a draft can run on, as its chip's menu lists it.
pub(super) struct Environment {
    pub(super) machine: ferrite_core::MachineId,
    pub(super) name: SharedString,
    /// Why it cannot be chosen now, when it cannot.
    pub(super) unreached: Option<SharedString>,
}

/// The copy a draft waits on, as its chip's menu shows it.
pub(super) struct Waited {
    /// The copy's Machine as the window holds it: none while it is off or
    /// once it was removed.
    pub(super) machine: Option<ferrite_core::MachineId>,
    /// What the Machine is called.
    pub(super) name: SharedString,
    /// How far the copy is, or that it stopped, in words.
    pub(super) detail: SharedString,
    pub(super) copy: super::copies::CopyKey,
}

/// A copy the window keeps, as a chip's menu lists it: what it copies
/// where, and how it stands (how far, done or stopped).
pub(super) struct KeptCopy {
    pub(super) name: SharedString,
    pub(super) detail: SharedString,
    pub(super) copy: super::copies::CopyKey,
}

/// A draft's environment chip's rows: each environment on, `draft`'s
/// ticked, one the window cannot reach dimmed with why (its pick opens
/// Connections). While the draft waits on a copy (`waited`), that
/// Machine is ticked instead and says how far, a row of its own while it
/// is off or gone, its pick the copy's card; the draft's own says it
/// sends here instead. Then `Add environment…`, and under a `Copies`
/// heading each other copy the window keeps (`kept`), its pick its card.
pub(super) fn environment_rows(
    environments: &[Environment],
    draft: &pane::DraftBinding,
    waited: Option<Waited>,
    kept: Vec<KeptCopy>,
) -> Vec<Row> {
    let here = draft.binding.project().machine();
    let ticked = waited.as_ref().map_or(Some(here), |waited| waited.machine);
    let mut rows: Vec<Row> = environments
        .iter()
        .map(|environment| {
            let choice = match environment.unreached {
                None => BandChoice::Environment(environment.machine),
                Some(_) => BandChoice::Connections,
            };
            let detail = match (&waited, &environment.unreached) {
                (_, Some(why)) => why.clone(),
                (Some(waited), None) if waited.machine == Some(environment.machine) => waited.detail.clone(),
                (Some(_), None) if environment.machine == here => "send here instead".into(),
                _ => SharedString::default(),
            };
            let mut row = band_row(
                environment.name.clone(),
                detail,
                ticked == Some(environment.machine),
                choice,
            );
            row.row.inert = environment.unreached.is_some();
            row
        })
        .collect();
    if let Some(waited) = waited.filter(|waited| waited.machine.is_none()) {
        rows.push(band_row(waited.name, waited.detail, true, BandChoice::Copy(waited.copy)));
    }
    rows.push(band_row(
        SharedString::from("Add environment…"),
        SharedString::default(),
        false,
        BandChoice::Connections,
    ));
    if !kept.is_empty() {
        rows.push(note_row("Copies"));
    }
    rows.extend(
        kept
            .into_iter()
            .map(|copy| band_row(copy.name, copy.detail, false, BandChoice::Copy(copy.copy))),
    );
    rows
}

/// A draft's Workspace chip's rows, scoped to its Project alone: the
/// branch its checkout is on (`checked_out`), each of the repo's other
/// worktrees (as git lists them, hand-made ones included), each other
/// local branch, and the two ways to start a fresh branch — no global list
/// anywhere. The git half is `listing`, read once at open; until it lands
/// the rows are the checkout and the fresh starts.
pub(super) fn workspace_rows(
    draft: &pane::DraftBinding,
    checked_out: SharedString,
    listing: Option<&RepoListing>,
) -> Vec<Row> {
    // Where the repo already stands, then the fresh starts:
    // the checkout as it is, each other worktree, each other
    // branch, then a new branch — beside the operator, or off
    // in an isolated worktree.
    let target = draft.binding.target();
    let mut rows = vec![band_row(
        SharedString::from("Current checkout"),
        checked_out.clone(),
        *target == DraftTarget::Main,
        BandChoice::Target(DraftTarget::Main),
    )];
    // A branch already on offer as the checkout or as a
    // worktree is not offered again as a plain branch: git
    // would refuse to switch the checkout onto it anyway.
    let mut offered: Vec<&str> = vec![checked_out.as_ref()];
    for worktree in listing
        .map(|listing| listing.worktrees.as_slice())
        .unwrap_or_default()
    {
        let directory = worktree
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| worktree.path.display().to_string());
        let (name, detail) = match &worktree.branch {
            Some(branch) => (branch.clone(), directory),
            None => (directory, "detached".to_string()),
        };
        if let Some(branch) = &worktree.branch {
            offered.push(branch);
        }
        rows.push(band_row(
            SharedString::from(format!("Previous worktree ({name})")),
            SharedString::from(detail),
            matches!(
                target,
                DraftTarget::Existing { path, .. } if *path == worktree.path
            ),
            BandChoice::Target(DraftTarget::Existing {
                branch: name,
                path: worktree.path.clone(),
            }),
        ));
    }
    for branch in listing
        .map(|listing| listing.branches.as_slice())
        .unwrap_or_default()
    {
        if offered.contains(&branch.as_str()) {
            continue;
        }
        rows.push(band_row(
            SharedString::from(branch.clone()),
            SharedString::from("existing branch"),
            matches!(target, DraftTarget::Branch { name } if name == branch),
            BandChoice::Target(DraftTarget::Branch {
                name: branch.clone(),
            }),
        ));
    }
    rows.push(band_row(
        SharedString::from("New branch"),
        SharedString::from("in the project checkout"),
        *target == DraftTarget::NewBranch,
        BandChoice::Target(DraftTarget::NewBranch),
    ));
    rows.push(band_row(
        SharedString::from("New worktree"),
        SharedString::from("in an isolated checkout"),
        *target == DraftTarget::New,
        BandChoice::Target(DraftTarget::New),
    ));
    rows
}

impl<K> Popover<K> {
    /// Clamp-step the cursor by `delta`; whether it moved. Inert rows (a
    /// picker's section headers) are skipped over: the arrows land only
    /// where ↵ would do something.
    pub(super) fn step(&mut self, delta: isize) -> bool {
        let mut stepped = self.selected;
        loop {
            let next = stepped.saturating_add_signed(delta);
            if next >= self.rows.len() || (delta < 0 && stepped == 0) {
                break;
            }
            stepped = next;
            if !self.rows[stepped].inert {
                break;
            }
        }
        if self.rows.get(stepped).is_some_and(|row| row.inert) || stepped == self.selected {
            return false;
        }
        self.selected = stepped;
        self.scroll.scroll_to_item(stepped);
        true
    }
}

/// A rows menu's surface — a `/`/`@` menu's (`text`) or a band menu's —
/// its rows wired to the `hover` and `pick` of the view that draws it, the
/// Composer menu's or the root's. Every row is the one menu row: `/`, `@` and
/// import rows through `pane::menu_row` (slash descriptions in one
/// aligned column), band rows through `pane::picker_row` with the
/// strong check on the standing choice; an inert explanation
/// ("Searching workspace files...", a refusal) is a `menu_note`. The rows scroll
/// inside the popover's cap.
pub(super) fn menu_surface<K, V: 'static>(
    open: &Popover<K>,
    text: Option<&Kind>,
    hover: fn(&mut V, usize, &mut Context<V>),
    pick: fn(&mut V, usize, &mut Window, &mut Context<V>),
    cx: &mut Context<V>,
) -> AnyElement {
    fn wire<E: InteractiveElement + IntoElement + 'static, V: 'static>(
        drawn: E,
        at: usize,
        hover: fn(&mut V, usize, &mut Context<V>),
        pick: fn(&mut V, usize, &mut Window, &mut Context<V>),
        cx: &mut Context<V>,
    ) -> AnyElement {
        drawn
            .debug_selector(move || format!("composer-menu-row-{at}"))
            .on_mouse_move(cx.listener(move |view, _: &gpui::MouseMoveEvent, _, cx| {
                hover(view, at, cx)
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    pick(view, at, window, cx);
                }),
            )
            .into_any_element()
    }
    // Slash commands align their descriptions in one column sized to
    // the longest name shown, measured in the row's face (a match
    // draws at `W_LABEL`), so no name is ever cut.
    let label_w = matches!(text, Some(Kind::Commands)).then(|| {
        // A label's advance reads no column width.
        let measure = nav::Measure::Text(cx, 0.);
        open.rows
            .iter()
            .map(|row| {
                measure.ui(
                    &row.name,
                    ferrite_ui::theme::FS_UI,
                    ferrite_ui::theme::W_LABEL,
                )
            })
            .fold(0., f32::max)
            .ceil()
    });
    if !open.revealed.replace(true) {
        open.scroll.scroll_to_item(open.selected);
    }
    let mut rows = div()
        .id("composer-menu-rows")
        .debug_selector(|| "composer-menu-rows".into())
        .flex()
        .flex_col()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(&open.scroll);
    for (at, row) in open.rows.iter().enumerate() {
        let cursor = at == open.selected;
        let explains = row.inert
            && row.consequence_is_inert()
            && row.detail.is_empty()
            && row.matched.is_empty();
        let drawn = match text {
            _ if explains => wire(ferrite_ui::menu::note(row.name.clone()), at, hover, pick, cx),
            Some(_) => wire(
                pane::menu_row(("composer-menu-row", at), &row.row, cursor, label_w),
                at,
                hover,
                pick,
                cx,
            ),
            None => wire(
                pane::picker_row(
                    ("composer-menu-row", at),
                    row.name.clone(),
                    row.detail.clone(),
                    cursor,
                    row.active,
                    row.inert,
                ),
                at,
                hover,
                pick,
                cx,
            ),
        };
        rows = rows.child(drawn);
    }
    // A press on the popover's own dead space is not a press outside
    // it: swallowed, so the root's dismissal never sees it. No key-hint
    // footer: the keys act in place (Claude's and Codex's menus).
    // Claude's slash picker explains the row under the cursor in a card
    // at its side (claude/slash-picker.png), so a description cut by the
    // popover's width is still read, by key as well as by pointer. The
    // card stands level with the row, rows being one grid row each.
    let explained = matches!(text, Some(Kind::Commands))
        .then(|| open.rows.get(open.selected))
        .flatten()
        .filter(|row| !row.row.inert && !row.row.detail.is_empty())
        .map(|row| row.row.detail.clone());
    let row_top = open.selected as f32 * ferrite_ui::theme::FLOAT_ROW_H
        + f32::from(open.scroll.offset().y);
    let cap = if text.is_some() {
        ferrite_ui::theme::SLASH_MENU_MAX_H
    } else {
        ferrite_ui::theme::MENU_MAX_H
    };
    let popover = pane::menu_popover(cap)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
        )
        .child(rows);
    div()
        .relative()
        .w_full()
        .child(popover)
        .children(explained.map(|detail| {
            let card = ferrite_ui::menu::float()
                .debug_selector(|| "composer-menu-tip".into())
                .w(px(ferrite_ui::theme::TOOLTIP_MAX_W))
                .px(px(ferrite_ui::theme::TOOLTIP_PAD_X))
                .py(px(ferrite_ui::theme::TOOLTIP_PAD_Y))
                .text_size(px(ferrite_ui::theme::FS_META))
                .line_height(px(ferrite_ui::theme::LH_META))
                .text_color(rgb(ferrite_ui::theme::INK_STRONG))
                .child(detail);
            // A drawer as wide as the box has no room on its right.
            ferrite_ui::float::beside(
                ferrite_ui::theme::SUBMENU_GAP,
                row_top.max(0.),
                ferrite_ui::theme::MENU_WINDOW_MARGIN,
                card.into_any_element(),
            )
        }))
        .into_any_element()
}
