//! Asking before Ferrite goes (⌘Q, or the window's close button) while
//! an agent is working or a Decision waits: either would be cut off
//! mid-turn. With nothing in flight it goes at once, as before. A window on
//! the Server never asks: its agents keep running without it (ADR 0025).

use super::*;
use gpui::{App, PromptLevel};

impl CockpitView {
    /// ⌘Q: quit, asking first when work would be interrupted.
    pub(crate) fn request_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let warning = self.leaving_warning();
        if self.confirm_leaving("Quit", warning, window, cx, |_, cx| cx.quit()) {
            cx.quit();
        }
    }

    /// The window's close button: the same question. `true` closes now.
    pub(crate) fn close_requested(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let warning = self.leaving_warning();
        self.confirm_leaving("Close", warning, window, cx, |window, _| {
            window.remove_window()
        })
    }

    /// Whether leaving may go ahead now — nothing would be cut off, as
    /// `warning` says; otherwise the operator is asked, and `leave` runs on
    /// their yes. A second ⌘Q while the question is up asks nothing more.
    pub(super) fn confirm_leaving(
        &mut self,
        verb: &'static str,
        warning: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
        leave: impl FnOnce(&mut Window, &mut App) + 'static,
    ) -> bool {
        let Some(warning) = warning else {
            return true;
        };
        if self.leave_asked {
            return false;
        }
        self.leave_asked = true;
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("{verb} Ferrite?"),
            Some(&warning),
            &[verb, "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |view, cx| {
            let confirmed = answer.await == Ok(0);
            let _ = view.update_in(cx, |view, window, cx| {
                view.leave_asked = false;
                if confirmed {
                    leave(window, cx);
                }
            });
        })
        .detach();
        false
    }

    /// What leaving now would interrupt, or `None` when nothing would.
    pub(crate) fn leaving_warning(&self) -> Option<String> {
        if self.cockpit.server().is_some() {
            return None;
        }
        self.in_flight()
            .map(|what| format!("{what}. Leaving stops them; their Threads reopen next launch."))
    }

    /// Who is working and who waits on a Decision, or `None` when every
    /// agent is idle. Work behind Main (its Subagents, its background
    /// tasks) counts: it outlives Main's turn, and leaving ends it too.
    pub(crate) fn in_flight(&self) -> Option<String> {
        let waiting = self.cockpit.blocked().len();
        let working = self
            .cockpit
            .threads()
            .into_iter()
            .filter_map(|thread| self.cockpit.thread(thread))
            .filter(|thread| {
                (thread.working() || thread.background_work()) && thread.pending().is_none()
            })
            .count();
        in_flight(working, waiting)
    }
}

/// Who is working and who waits.
fn in_flight(working: usize, waiting: usize) -> Option<String> {
    let agents = |n: usize| match n {
        1 => "1 agent is".to_string(),
        n => format!("{n} agents are"),
    };
    Some(match (working, waiting) {
        (0, 0) => return None,
        (working, 0) => format!("{} working", agents(working)),
        (0, waiting) => format!("{} waiting on a Decision", agents(waiting)),
        (working, waiting) => format!(
            "{} working and {waiting} waiting on a Decision",
            agents(working)
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::in_flight;

    #[test]
    fn the_question_names_who_is_working_and_who_waits() {
        assert_eq!(in_flight(0, 0), None);
        assert_eq!(in_flight(1, 0).unwrap(), "1 agent is working");
        assert_eq!(
            in_flight(0, 2).unwrap(),
            "2 agents are waiting on a Decision"
        );
        assert_eq!(
            in_flight(3, 1).unwrap(),
            "3 agents are working and 1 waiting on a Decision"
        );
    }
}
