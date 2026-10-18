//! herdr's sidebar, drawn by the nav (ADR 0031, Seam 3): what the board's
//! one writer (`sw_flow::flow::sidebar`) would tell herdr of each live
//! agent's pane and of each task's own space, and the board header's tally
//! for the bottom bar. Every word is the vendored crates' own
//! (`flow/sidebar_entry.rs`, `core/status_words.rs`); nothing is written
//! anywhere.
//!
//! The rows are `flow::live::board_for`'s, read through Ferrite's herdr
//! adapter as the board reads them, so the nav and the board never
//! disagree. A pane's id is the adapter's (`p3` for a pane the board
//! placed); its agent is an open Thread, whose id the adapter gives as the
//! agent's terminal.

use std::collections::{BTreeMap, HashMap, HashSet};

use ferrite_core::ThreadId;
use sw_board::board::theme::Theme;
use sw_board::board::view::list::tally;
use sw_board::coordinator::look::look;
use sw_board::flow::live::{board_seen, Session};
use sw_board::flow::sidebar::shown_name;
use sw_board::flow::sidebar_entry::{coordinator_entry, entry_of, reviewer_entry, Entry};
use sw_board::flow::task::is_agent_task;
use sw_board::json::*;
use sw_board::shown::ink::Ink;
use sw_board::shown::states::{as_row, group_of, status_ink};
use sw_board::status_words::coordinator_stands;

use crate::board::render;
use crate::nav::TaskRows;

/// How often the nav reads the board, as the upstream writer ticks.
pub(crate) const REFRESH: std::time::Duration = std::time::Duration::from_secs(1);

/// What the nav and the bottom bar show of the board.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Sidebar {
    /// The live Coordinator's Thread, independent of its provider or title.
    pub coordinator: Option<ThreadId>,
    /// Board-owned Threads, including parked agents and the Coordinator.
    pub owned: HashSet<ThreadId>,
    /// Each live agent's entry, by its Thread: a task's agent, its reviewer
    /// while a review runs, the Coordinator.
    pub agents: HashMap<ThreadId, Told>,
    /// A task's own space, by its Space's id: named with the task's number
    /// and title, the row's numbers under it.
    pub spaces: HashMap<String, Entry>,
    /// Each live agent's Space, by its Thread, as the adapter places it: a
    /// Project's id, a Space the board made, a linked worktree's path.
    pub places: HashMap<ThreadId, String>,
    /// The board header's tally: `3 need you · 1 ready · 2 working`.
    pub tally: String,
}

/// One entry, and the colour of where its task stands: its status ink in
/// the board's theme, so the nav and the board colour a status alike.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Told {
    pub entry: Entry,
    pub ink: u32,
}

impl Sidebar {
    /// What a Project heading says when it stands for a task's own space:
    /// that space's entry, when every one of `threads` the adapter places
    /// is in the same Space.
    pub(crate) fn heading(&self, threads: impl IntoIterator<Item = ThreadId>) -> Option<&Entry> {
        let mut places = threads
            .into_iter()
            .filter_map(|thread| self.places.get(&thread));
        let first = places.next()?;
        places
            .all(|other| other == first)
            .then(|| self.spaces.get(first))
            .flatten()
    }
}

impl Told {
    /// Rows 2 and 3 of the entry, as the nav draws them under row 1.
    pub(crate) fn rows(&self) -> TaskRows {
        let entry = &self.entry;
        TaskRows {
            stands_timed: shown_name(entry).into(),
            stands: entry.stands.clone().into(),
            ink: self.ink,
            third: if entry.progress.is_empty() {
                entry.line.clone().into()
            } else {
                entry.progress.clone().into()
            },
        }
    }
}

/// The board's data as the nav shows it, in the board's theme. Reads files
/// and asks the Server: off the UI thread.
pub(crate) fn read() -> Sidebar {
    let here = sw_board::herdr::cli::current_socket();
    let (rows, session) = board_seen(&here);
    let theme = render::theme_set();
    let now = sw_board::clock::now();
    let mut sidebar = of(&rows, &threads_of(&session), &here, now, &theme);
    sidebar.places = places_of(&session);
    // Ownership comes from placement, never an internal title prefix. It
    // survives an agent finishing, parking, or being renamed.
    sidebar.owned = sw_board::herdr::ferrite::owned_threads()
        .into_iter()
        .map(ThreadId::new)
        .collect();
    sidebar.owned.extend(sidebar.agents.keys().copied());
    if let Some((thread, told)) = coordinator(&here, &theme) {
        sidebar.coordinator = Some(thread);
        sidebar.owned.insert(thread);
        sidebar.agents.insert(thread, told);
    }
    sidebar
}

/// Each live agent's Thread, by its pane's id, from herdr's agent list as
/// Ferrite's adapter answers it.
fn threads_of(session: &Session) -> HashMap<String, ThreadId> {
    let Session::Up { agents, .. } = session else {
        return HashMap::new();
    };
    agents
        .iter()
        .filter_map(|(pane, agent)| Some((pane.clone(), thread_in(agent)?)))
        .collect()
}

/// Each live agent's Space, by its Thread, from the same agent list.
fn places_of(session: &Session) -> HashMap<ThreadId, String> {
    let Session::Up { agents, .. } = session else {
        return HashMap::new();
    };
    agents
        .values()
        .filter_map(|agent| Some((thread_in(agent)?, agent.t("workspace_id"))))
        .filter(|(_, space)| !space.is_empty())
        .collect()
}

/// The Thread an agent of the adapter's runs in: its terminal's id. A pane
/// with no open Thread has no agent.
pub(crate) fn thread_in(agent: &Value) -> Option<ThreadId> {
    agent
        .s("terminal_id")
        .split('@')
        .next()?
        .parse()
        .ok()
        .map(ThreadId::new)
}

/// The entries of the board's rows (`flow::live::board_for`'s), each by
/// its pane's Thread (`threads`), as `sync_sidebar` picks them: each live
/// pane of a task or one of its attempts on this machine, never an
/// archived one's, and each task's own space. A task waiting for its pick
/// on this machine (`here`) has no pane: its space keeps its name and has
/// no numbers, and its attempts' entries say `pick one`. Status inks are
/// `theme`'s.
pub(crate) fn of(
    rows: &[Obj],
    threads: &HashMap<String, ThreadId>,
    here: &str,
    now: i64,
    theme: &Theme,
) -> Sidebar {
    let mut sidebar = Sidebar {
        tally: tallied(rows),
        ..Sidebar::default()
    };
    let thread_of = |pane: &str| threads.get(pane).copied();
    for task in rows.iter().filter(|task| waits_for_pick(task, here)) {
        let entry = Entry {
            progress: String::new(),
            ..entry_of(task, "", now)
        };
        sidebar.spaces.insert(task.at("link").t("workspace"), entry);
    }
    for task in rows.iter().flat_map(with_attempts) {
        let live = task.at("live");
        let away = ["gone", "down", "away"].iter().any(|word| live.b(word));
        if !is_agent_task(&task) || task.b("archivedAt") || !live.is_object() || away {
            continue;
        }
        let ink = render::ink(theme, status_ink(&as_row(&task)));
        let entry = entry_of(&task, "", now);
        if let Some(reviewer) = thread_of(task.at("link").at("reviewer").s("pane")) {
            let entry = reviewer_entry(&task, now);
            sidebar.agents.insert(reviewer, Told { entry, ink });
        }
        if task.at("link").b("ownsWorkspace") {
            let space = task.at("link").t("workspace");
            sidebar.spaces.insert(space, entry.clone());
        }
        if let Some(agent) = thread_of(task.at("link").s("pane")) {
            sidebar.agents.insert(agent, Told { entry, ink });
        }
    }
    sidebar
}

/// A task of `here` that waits for its pick in a space of its own: upstream
/// renamed that space while its agent ran, and only clears its numbers now.
fn waits_for_pick(task: &Obj, here: &str) -> bool {
    let link = task.at("link");
    task.s("stage") == "pick"
        && !is_agent_task(task)
        && !task.b("archivedAt")
        && link.b("ownsWorkspace")
        && link.s("socket") == here
}

/// A task, then each attempt of it not yet chosen, knowing its task's stage.
fn with_attempts(task: &Obj) -> Vec<Obj> {
    let attempts = task
        .arr("tries")
        .iter()
        .filter_map(|attempt| attempt.as_object())
        .filter(|attempt| !attempt.b("chosen"))
        .map(|attempt| {
            let mut attempt = attempt.clone();
            attempt.put("parentStage", task.at("stage").clone());
            attempt
        });
    std::iter::once(task.clone()).chain(attempts).collect()
}

/// The header's tally of the rows (`update::list::counts`, archived ones
/// aside), said as the header says it.
fn tallied(rows: &[Obj]) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for row in rows.iter().filter(|row| !row.b("archivedAt")) {
        *counts.entry(group_of(&as_row(row))).or_default() += 1;
    }
    tally(|group| counts.get(group).copied().unwrap_or(0)).join(" \u{b7} ")
}

/// The Coordinator's entry, as `coordinator::sidebar::tell_sidebar` says
/// it: while it has a pane and an agent in it.
fn coordinator(socket: &str, theme: &Theme) -> Option<(ThreadId, Told)> {
    let state = sw_board::coordinator::state::get(socket)?;
    if !state.b("pane") {
        return None;
    }
    let look = look(socket);
    look.status.as_ref()?;
    let said = if look.error.is_empty() {
        &look.said
    } else {
        &look.error
    };
    let stands = coordinator_stands(&look.state, &look.error);
    let entry = coordinator_entry(&stands, look.queue, said);
    Some((thread_in(&look.agent)?, coordinator_told(entry, theme)))
}

/// The Coordinator's entry: in `theme`'s text, as upstream dims only its
/// message (row 3).
pub(crate) fn coordinator_told(entry: Entry, theme: &Theme) -> Told {
    Told {
        entry,
        ink: render::ink(theme, Ink::Text),
    }
}

#[cfg(test)]
mod tests;
