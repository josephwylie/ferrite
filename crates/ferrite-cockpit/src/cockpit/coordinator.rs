//! The Factory's Coordinator surface (#224), as the window keeps it: its
//! view is `ferrite_factory::coordinator::CoordinatorView`, which the
//! Factory holds. The window answers what it draws (`coordinator_inputs`),
//! its Pane among them, and every ask it says, each a Server ask; the
//! keyboard goes to the Coordinator's Pane once it runs, else to the
//! opening field.

use super::*;
use ferrite_factory::board::Surface;
use ferrite_core::factory_board::{Act, Agent, Proposal, ProposalStatus};
use ferrite_core::protocol::{CoordinatorAction, Done};
use ferrite_factory::coordinator::{CoordinatorEvent, CoordinatorInputs, CoordinatorView, ProposalCard};
use ferrite_factory::picker::CoordinatorPick;
use ferrite_ui::source::Source;

impl CockpitView {
    /// A Factory's Coordinator surface, heard from its making.
    pub(super) fn new_coordinator(&mut self, cx: &mut Context<Self>) -> Entity<CoordinatorView> {
        let inputs = Source::of(&cx.entity(), |root: &Self, _: &(), cx| root.coordinator_inputs(cx));
        let coordinator = cx.new(|cx| CoordinatorView::new(inputs, cx));
        cx.subscribe(&coordinator, Self::on_coordinator_event).detach();
        coordinator
    }

    /// What the surface draws: the Coordinator and its proposals as the
    /// board says them now, and its Pane's cell once it runs, made as the
    /// window drew (`make_factory_cells`).
    fn coordinator_inputs(&self, cx: &gpui::App) -> Option<CoordinatorInputs> {
        let key = self.factory_project()?;
        let board = self.cockpit.read(cx).factory(key.machine());
        let project = key.project();
        let coordinator = board.coordinator(project);
        let agent = self.coordinator_agent(cx);
        let mut proposals: Vec<&Proposal> = board.proposals_of(project).collect();
        // Waiting ones first, newest first within each.
        proposals.reverse();
        proposals.sort_by_key(|proposal| proposal.status != ProposalStatus::Pending);
        Some(CoordinatorInputs {
            models: self.cockpit.read(cx).model_catalog(key.machine(), agent.provider),
            agent,
            starting: coordinator.is_some_and(|c| c.starting || c.thread.is_some()),
            error: coordinator.and_then(|c| c.error.clone()),
            proposals: proposals
                .into_iter()
                .map(|proposal| ProposalCard {
                    id: proposal.id.clone(),
                    title: proposal.title(board.reading()),
                    lines: proposal.lines(board.reading()),
                    action: proposal.action(),
                    status: proposal.status,
                    task: proposal
                        .task
                        .clone()
                        .or_else(|| match &proposal.act {
                            Act::Redirect { task, .. } => Some(task.clone()),
                            _ => None,
                        })
                        .filter(|task| board.task(task).is_some()),
                })
                .collect(),
            pane: self
                .coordinator_pane(cx)
                .and_then(|index| self.made_cell_slot(index)),
        })
    }

    /// Send starts the Coordinator; a chooser opens its rows; a
    /// proposal's buttons ask the Server, or open the Task it started.
    fn on_coordinator_event(
        &mut self,
        _: Entity<CoordinatorView>,
        event: &CoordinatorEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            CoordinatorEvent::Send => self.speak_to_coordinator(cx),
            CoordinatorEvent::Picks { picks, trigger, at } => {
                self.open_picks(picks.clone(), trigger, *at, cx)
            }
            CoordinatorEvent::Accept(proposal) => self.accept_proposal(proposal, cx),
            CoordinatorEvent::Dismiss(proposal) => self.dismiss_proposal(proposal, cx),
            CoordinatorEvent::OpenTask(task) => {
                self.factory_surface(Surface::Board, cx);
                if let Some(machine) = self.factory_machine() {
                    self.open_task(machine, task, cx);
                }
            }
        }
    }

    /// The shown Project's Coordinator's Thread, once started.
    pub(crate) fn coordinator_thread(&self, cx: &gpui::App) -> Option<ThreadKey> {
        let project = self.factory_project()?;
        let board = self.cockpit.read(cx).factory(project.machine());
        Some(board.key(board.coordinator(project.project())?.thread?))
    }

    /// Its Pane in this window, once started.
    pub(super) fn coordinator_pane(&self, cx: &gpui::App) -> Option<usize> {
        self.pane_for(self.coordinator_thread(cx)?)
    }

    /// The Coordinator drawn on its surface, which streams whole.
    pub(super) fn coordinator_drawn(&self, cx: &gpui::App) -> Option<ThreadKey> {
        let view = self.factory()?.read(cx);
        (view.surface() == Surface::Coordinator)
            .then(|| self.coordinator_thread(cx))
            .flatten()
            .filter(|thread| self.pane_for(*thread).is_some())
    }

    /// What the Coordinator runs on: the Factory's choice, else the
    /// operator's defaults.
    pub(crate) fn coordinator_agent(&self, cx: &gpui::App) -> Agent {
        let chosen = self.factory_project().and_then(|project| {
            self.cockpit
                .read(cx)
                .factory(project.machine())
                .coordinator(project.project())?
                .agent
                .clone()
        });
        chosen.unwrap_or_else(|| {
            let settings = &self.prefs.settings;
            let provider = settings.default_provider;
            let (model, effort) = match provider {
                Provider::Claude => (&settings.claude_model, &settings.claude_effort),
                Provider::Codex => (&settings.codex_model, &settings.codex_effort),
            };
            Agent {
                provider,
                model: model.clone(),
                effort: effort.clone(),
            }
        })
    }

    /// The keyboard while the Coordinator surface shows: its Pane's
    /// Composer once it runs, else the opening field.
    pub(super) fn coordinator_focus(&self, window: &Window, cx: &gpui::App) -> Option<FocusHandle> {
        let view = self.factory()?.read(cx);
        if view.surface() != Surface::Coordinator {
            return None;
        }
        if let Some(index) = self.coordinator_pane(cx) {
            return Some(self.panes[index].read(cx).composer.read(cx).focus_target(window, cx));
        }
        Some(view.coordinator().read(cx).focus(cx))
    }

    /// A Coordinator command, on the machine of the Factory shown; its
    /// refusal in the view's note.
    fn coordinator_ask(
        &mut self,
        action: CoordinatorAction,
        cx: &mut Context<Self>,
    ) -> Option<Done> {
        let machine = self.factory_machine()?;
        let (done, note) = match self
            .cockpit
            .update(cx, |cockpit, _| cockpit.coordinator(machine, action))
        {
            Ok(done) => (done, None),
            Err(error) => (None, Some(SharedString::from(error.to_string()))),
        };
        self.update_factory(cx, |view, _| view.set_note(note));
        cx.notify();
        done
    }

    /// Send the opening field's words: the Coordinator starts on them.
    pub(crate) fn speak_to_coordinator(&mut self, cx: &mut Context<Self>) {
        let (Some(project), Some(view)) = (self.factory_project(), self.factory()) else {
            return;
        };
        let field = view.read(cx).coordinator().read(cx).field().clone();
        let text = field.read(cx).text().trim().to_string();
        if text.is_empty() {
            return;
        }
        let action = CoordinatorAction::Say {
            project: project.project(),
            text,
        };
        if self.coordinator_ask(action, cx).is_some() {
            field.update(cx, |field, cx| {
                field.take(cx);
            });
        }
    }

    pub(crate) fn accept_proposal(&mut self, proposal: &str, cx: &mut Context<Self>) {
        let skills = self.factory_catalog(cx).skills;
        let action = CoordinatorAction::Accept {
            proposal: proposal.to_string(),
            skills,
        };
        self.coordinator_ask(action, cx);
    }

    pub(crate) fn dismiss_proposal(&mut self, proposal: &str, cx: &mut Context<Self>) {
        let action = CoordinatorAction::Dismiss {
            proposal: proposal.to_string(),
        };
        self.coordinator_ask(action, cx);
    }

    pub(crate) fn apply_coordinator_pick(&mut self, pick: CoordinatorPick, cx: &mut Context<Self>) {
        let Some(project) = self.factory_project() else {
            return;
        };
        let mut agent = self.coordinator_agent(cx);
        match pick {
            CoordinatorPick::Provider(provider) => {
                if agent.provider != provider {
                    agent = Agent {
                        provider,
                        model: None,
                        effort: None,
                    };
                }
            }
            CoordinatorPick::Model(model) => agent.model = (!model.is_empty()).then_some(model),
            CoordinatorPick::Effort(effort) => {
                agent.effort = (!effort.is_empty()).then_some(effort)
            }
        }
        self.coordinator_ask(
            CoordinatorAction::Agent {
                project: project.project(),
                agent,
            },
            cx,
        );
    }
}
