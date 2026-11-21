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
    Project(ProjectId),
    /// The type-a-path row: register the typed path as a project, then
    /// choose it.
    RegisterPath(std::path::PathBuf),
    Target(DraftTarget),
    /// Open the Project card, as the nav's dropdown does.
    AddProject,
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
            let (score, matched) = crate::fuzzy::matches(filter, &command.name)?;
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
            let (score, matched) = crate::fuzzy::matches(filter, file)?;
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
pub(super) fn native_mention_rows(files: &[ferrite_core::providers::FileSuggestion]) -> Vec<pane::MenuRow> {
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
    let (_, matched) = crate::fuzzy::matches(filter, name)?;
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
