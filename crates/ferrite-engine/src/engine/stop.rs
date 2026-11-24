//! A whole Thread stopped, as another Server's `thread.stop` asks: T3
//! Code's `thread.stop`, which its `task_cancel` sends to a delegated
//! task's thread. Unlike Main's interrupt, it ends the Subagents, the
//! background commands and the Delegates too.

use super::*;
use ferrite_core::activity::AgentStatus;
use ferrite_core::progress::TaskStatus;
use ferrite_core::transcript::tools::{policy, ToolKind};

impl Engine {
    /// Stop everything `thread` runs: its Subagents, its background
    /// commands and Main's turn. What waits in its queue goes back to the
    /// Composer first, as T3's Stop holds its queue, so nothing the stop
    /// ends starts again by itself. Its Delegates are stopped too, depth
    /// first, as T3's Stop stops its delegated tasks. A Thread with nothing
    /// running is left as it is. A stop that fails is said on the Thread,
    /// as an interrupt's is.
    pub fn stop(&mut self, thread: ThreadId) {
        let stopped = self.stop_delegates(thread);
        let Some(state) = self.threads.get_mut(&thread) else {
            return;
        };
        if let Err(error) = stopped {
            state.apply(Input::Notice(format!("stop failed: {error}")));
        }
        let turn = state.busy();
        state.hold_queue();
        let (agents, commands) = state.running_work();
        if let Some(session) = state.ready_session() {
            let refused: Vec<io::Error> = commands
                .into_iter()
                .filter_map(|id| run_control(session, ferrite_core::SessionControl::StopTask { id }).err())
                .collect();
            for error in refused {
                state.apply(Input::Notice(format!("stop failed: {error}")));
            }
        }
        for agent in agents {
            self.stop_subagent(thread, &agent);
        }
        if turn {
            self.interrupt(thread);
            // Nothing is left to send when the turn ends.
            let state = self.threads.get_mut(&thread).expect("checked");
            state.resend_held_on_turn_end = false;
        }
    }
}

impl Thread {
    /// Every prompt still queued goes back to the Composer, oldest first:
    /// the provider's own queue is told to let each go, and what Ferrite
    /// held for the Session is never handed over.
    fn hold_queue(&mut self) {
        let queued = self.native_queue.flush();
        if let Some(session) = self
            .session
            .as_mut()
            .and_then(SessionLifecycle::session_mut)
        {
            // A steer has no handle to let go of; its turn's interrupt
            // ends it.
            for item in &queued {
                let _ = session.cancel_queued(&item.id);
            }
        }
        let held = self.awaiting.prompts.drain(..).map(|held| held.text);
        let texts = queued.into_iter().map(|item| item.text).chain(held);
        self.native_queue
            .recovered
            .extend(texts.map(|text| (text, false)));
    }

    /// What runs behind Main and can be stopped: each Subagent whose
    /// status is known, and each background command, by the id its
    /// provider stops it by.
    fn running_work(&self) -> (Vec<AgentKey>, Vec<String>) {
        let view = self.activity.view();
        let agents = view
            .running_agents()
            .filter(|(_, agent)| agent.status() != AgentStatus::Unknown)
            .map(|(_, agent)| agent.key().clone())
            .collect();
        let transcript = self.transcript();
        let mut commands: Vec<String> = transcript
            .progress()
            .background()
            .iter()
            .filter(|task| task.status == TaskStatus::Working)
            .map(|task| task.id.clone())
            .collect();
        let launched = transcript
            .background_calls()
            .filter(|call| call.in_background() && policy(&call.name).kind != ToolKind::Agent)
            .filter_map(|call| call.background.as_ref().map(|work| work.task.clone()));
        for task in launched {
            if !commands.contains(&task) {
                commands.push(task);
            }
        }
        (agents, commands)
    }
}
