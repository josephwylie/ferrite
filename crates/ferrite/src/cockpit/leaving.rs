//! Asking before Ferrite goes (⌘Q, or the window's close button) while
//! an agent is working or a Decision waits: either would be cut off
//! mid-turn. With nothing in flight it goes at once, as before.

use super::*;
use gpui::{App, PromptLevel};

impl CockpitView {
    /// ⌘Q: quit, asking first when work would be interrupted.
    pub(crate) fn request_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.confirm_leaving("Quit", window, cx, |_, cx| cx.quit()) {
            cx.quit();
        }
    }

    /// The window's close button: the same question. `true` closes now.
    pub(crate) fn close_requested(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.confirm_leaving("Close", window, cx, |window, _| window.remove_window())
    }

    /// Whether leaving may go ahead now; otherwise the operator is asked,
    /// and `leave` runs on their yes. A second ⌘Q while the question is
    /// up asks nothing more.
    fn confirm_leaving(
        &mut self,
        verb: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
        leave: impl FnOnce(&mut Window, &mut App) + 'static,
    ) -> bool {
        let Some(warning) = self.leaving_warning() else {
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
        let waiting = self.cockpit.blocked().len();
        let working = self
            .cockpit
            .threads()
            .into_iter()
            .filter_map(|thread| self.cockpit.thread(thread))
            .filter(|thread| thread.busy() && thread.pending().is_none())
            .count();
        interrupted(working, waiting)
    }
}

/// The question's detail: who is working, who waits, and that they stop.
fn interrupted(working: usize, waiting: usize) -> Option<String> {
    let agents = |n: usize| match n {
        1 => "1 agent is".to_string(),
        n => format!("{n} agents are"),
    };
    let what = match (working, waiting) {
        (0, 0) => return None,
        (working, 0) => format!("{} working", agents(working)),
        (0, waiting) => format!("{} waiting on a Decision", agents(waiting)),
        (working, waiting) => format!(
            "{} working and {waiting} waiting on a Decision",
            agents(working)
        ),
    };
    Some(format!(
        "{what}. Leaving stops them; their Threads reopen next launch."
    ))
}

#[cfg(test)]
mod tests {
    use super::interrupted;

    #[test]
    fn the_question_names_who_is_working_and_who_waits() {
        assert_eq!(interrupted(0, 0), None);
        assert_eq!(
            interrupted(1, 0).unwrap(),
            "1 agent is working. Leaving stops them; their Threads reopen next launch."
        );
        assert!(interrupted(0, 2)
            .unwrap()
            .starts_with("2 agents are waiting on a Decision."));
        assert!(interrupted(3, 1)
            .unwrap()
            .starts_with("3 agents are working and 1 waiting on a Decision."));
    }
}
