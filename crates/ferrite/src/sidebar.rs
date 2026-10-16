//! herdr's sidebar, drawn by the nav (ADR 0031, Seam 3): what the board's
//! one writer (`sw_flow::flow::sidebar`) would tell herdr of each live
//! agent's pane and of each task's own space, and the board header's tally
//! for the bottom bar. Every word is the vendored crates' own
//! (`flow/sidebar_entry.rs`, `core/status_words.rs`); nothing is written
//! anywhere. A pane is a Thread here, and its id is the Thread's.
//!
//! The board's rows are made as `flow::live::board_for` makes them, with
//! this window's Threads standing in for herdr's `agent list`: an open
//! Thread is a live pane, and its Attention is the agent's status.

use std::collections::{BTreeMap, HashMap};

use ferrite_core::{Attention, ThreadId};
use sw_board::board::view::list::tally;
use sw_board::coordinator::look::look;
use sw_board::flow::live::{shown, Session};
use sw_board::flow::sidebar::shown_name;
use sw_board::flow::sidebar_entry::{coordinator_entry, entry_of, reviewer_entry, Entry};
use sw_board::flow::task::is_agent_task;
use sw_board::json::*;
use sw_board::shown::ink::Ink;
use sw_board::shown::states::{as_row, group_of, status_ink};
use sw_board::status_words::coordinator_stands;
use sw_board::store::tasks::{list, ListOpts};

use crate::nav::TaskRows;
use crate::theme;

/// How often the nav reads the board, as the upstream writer ticks.
pub(crate) const REFRESH: std::time::Duration = std::time::Duration::from_secs(1);

/// What the nav and the bottom bar show of the board.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Sidebar {
    /// Each live agent's entry, by its Thread: a task's agent, its reviewer
    /// while a review runs, the Coordinator.
    pub agents: HashMap<ThreadId, Told>,
    /// A task's own space, by its Project's API id: named with the task's
    /// number and title, the row's numbers under it.
    pub spaces: HashMap<String, Entry>,
    /// The board header's tally: `3 need you · 1 ready · 2 working`.
    pub tally: String,
}

/// One entry, and the ink of where its task stands.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Told {
    pub entry: Entry,
    pub ink: Ink,
}

impl Told {
    /// Rows 2 and 3 of the entry, as the nav draws them under row 1.
    pub(crate) fn rows(&self) -> TaskRows {
        let entry = &self.entry;
        TaskRows {
            stands_timed: shown_name(entry).into(),
            stands: entry.stands.clone().into(),
            ink: ink(self.ink),
            third: if entry.progress.is_empty() {
                entry.line.clone().into()
            } else {
                entry.progress.clone().into()
            },
        }
    }
}

/// The board's data as the nav shows it, for this window's open Threads.
/// Reads files and asks the Server for the Coordinator: off the UI thread.
pub(crate) fn read(threads: &HashMap<ThreadId, Attention>) -> Sidebar {
    let here = sw_board::herdr::cli::current_socket();
    let tasks = list(ListOpts::default());
    let session = session(threads);
    let mut seen = BTreeMap::new();
    let mut rows: Vec<Obj> = tasks
        .iter()
        .map(|task| shown(task, &session, &here, &mut seen))
        .collect();
    sw_board::shown::queue::number(&mut rows);
    let mut sidebar = of(&rows, sw_board::clock::now());
    if let Some((thread, told)) = coordinator(&here) {
        sidebar.agents.insert(thread, told);
    }
    sidebar
}

/// herdr's session as this window's Threads make it: every open Thread a
/// pane with an agent in it.
fn session(threads: &HashMap<ThreadId, Attention>) -> Session {
    let pane = |thread: &ThreadId| thread.get().to_string();
    Session::Up {
        agents: threads
            .iter()
            .map(|(thread, attention)| {
                let status = serde_json::to_value(attention).unwrap_or_default();
                let agent = json!({ "pane_id": pane(thread), "agent_status": status });
                (pane(thread), agent)
            })
            .collect(),
        panes: threads
            .keys()
            .map(|thread| (pane(thread), json!({ "pane_id": pane(thread) })))
            .collect(),
    }
}

/// The entries of the board's rows (`flow::live::board_for`'s), as
/// `sync_sidebar` picks them: each live pane of a task or one of its
/// attempts on this machine, never an archived one's, and each task's own
/// space. A task waiting for its pick has no pane and no numbers: its
/// attempts' entries say `pick one`.
pub(crate) fn of(rows: &[Obj], now: i64) -> Sidebar {
    let mut sidebar = Sidebar {
        tally: tallied(rows),
        ..Sidebar::default()
    };
    for task in rows.iter().flat_map(with_attempts) {
        let live = task.at("live");
        let away = ["gone", "down", "away"].iter().any(|word| live.b(word));
        if !is_agent_task(&task) || task.b("archivedAt") || !live.is_object() || away {
            continue;
        }
        let ink = status_ink(&as_row(&task));
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
fn coordinator(socket: &str) -> Option<(ThreadId, Told)> {
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
    Some((thread_of(&look.pane)?, coordinator_told(entry)))
}

/// The Coordinator's entry in the line above the board's box's ink.
pub(crate) fn coordinator_told(entry: Entry) -> Told {
    Told {
        entry,
        ink: Ink::Muted,
    }
}

/// The Thread a pane id names.
fn thread_of(pane: &str) -> Option<ThreadId> {
    pane.parse().ok().map(ThreadId::new)
}

/// A board ink in Ferrite's theme: the one place the nav turns the board's
/// named inks into colours.
pub(crate) fn ink(ink: Ink) -> u32 {
    match ink {
        Ink::Red => theme::RED,
        Ink::Green => theme::GREEN,
        Ink::Yellow => theme::YELLOW,
        Ink::Teal => theme::CYAN,
        Ink::Peach => theme::ORANGE,
        Ink::Mauve | Ink::Auto => theme::MAGENTA,
        Ink::Accent => theme::ACCENT,
        Ink::Review => theme::BLUE,
        Ink::Plain | Ink::Text | Ink::Soft | Ink::Check => theme::TEXT,
        Ink::Muted | Ink::Queue | Ink::Sidebar | Ink::Selection => theme::CHROME_MUTED,
        Ink::Todo | Ink::Rule | Ink::Dim => theme::CHROME_FAINT,
    }
}

#[cfg(test)]
mod tests;
