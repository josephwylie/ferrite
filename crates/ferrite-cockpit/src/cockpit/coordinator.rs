//! The Factory's Coordinator surface (#224): the Coordinator's ordinary
//! transcript and Composer, its Provider, model and effort, and the
//! proposals it raised as cards the operator accepts or dismisses.
//!
//! The Coordinator is an ordinary owned Thread the Server starts on the
//! operator's first word; until then the surface shows an empty
//! conversation and a field whose Send starts it. Once it runs, its Pane
//! is the one this window already holds, drawn here as the Task panel's
//! Worker tab draws a Worker's. A proposal changes nothing until accepted.

use super::workflows::Pick;
use super::*;
use ferrite_core::factory::{Agent, Proposal, ProposalStatus};
use ferrite_core::protocol::{CoordinatorAction, Done};
use ferrite_pane::composer::{Composer, Role};

/// The proposals' column beside the conversation.
const PROPOSALS_W: f32 = 380.;

/// A choice on the Coordinator's settings.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CoordinatorPick {
    Provider(Provider),
    /// Empty: the Provider's default.
    Model(String),
    Effort(String),
}

impl CockpitView {
    /// The shown Project's Coordinator's Thread, once started.
    pub(crate) fn coordinator_thread(&self) -> Option<ThreadId> {
        let project = self.factory_view.as_ref()?.project;
        self.cockpit.factory().coordinator(project)?.thread
    }

    /// Its Pane in this window, once started.
    fn coordinator_pane(&self) -> Option<usize> {
        self.pane_for(self.coordinator_thread()?)
    }

    /// The Coordinator drawn on its surface, which streams whole.
    pub(super) fn coordinator_drawn(&self) -> Option<ThreadId> {
        let view = self.factory_view.as_ref()?;
        (view.surface == super::workflows::Surface::Coordinator)
            .then(|| self.coordinator_thread())
            .flatten()
            .filter(|thread| self.pane_for(*thread).is_some())
    }

    /// What the Coordinator runs on: the Factory's choice, else the
    /// operator's defaults.
    pub(crate) fn coordinator_agent(&self) -> Agent {
        let chosen = self.factory_view.as_ref().and_then(|view| {
            self.cockpit
                .factory()
                .coordinator(view.project)?
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

    /// The opening field, made once.
    pub(crate) fn ensure_coordinator_field(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.factory_view.as_ref() else {
            return;
        };
        if view.opening.is_some() {
            return;
        }
        let composer = cx.new(Composer::new);
        composer.update(cx, |composer, cx| composer.set_role(Role::Field(None), cx));
        cx.observe(&composer, |_, _, cx| cx.notify()).detach();
        if let Some(view) = self.factory_view.as_mut() {
            view.opening = Some(composer);
        }
    }

    /// The keyboard while the Coordinator surface shows: its Pane's
    /// Composer once it runs, else the opening field.
    pub(super) fn coordinator_focus(&self, window: &Window, cx: &gpui::App) -> Option<FocusHandle> {
        let view = self.factory_view.as_ref()?;
        if view.surface != super::workflows::Surface::Coordinator {
            return None;
        }
        if let Some(index) = self.coordinator_pane() {
            return Some(self.panes[index].composer.read(cx).focus_target(window, cx));
        }
        Some(view.opening.as_ref()?.read(cx).focus_handle(cx))
    }

    /// A Coordinator command; its refusal in the view's note.
    fn coordinator_ask(
        &mut self,
        action: CoordinatorAction,
        cx: &mut Context<Self>,
    ) -> Option<Done> {
        let (done, note) = match self.cockpit.coordinator(action) {
            Ok(done) => (done, None),
            Err(error) => (None, Some(SharedString::from(error.to_string()))),
        };
        if let Some(view) = self.factory_view.as_mut() {
            view.note = note;
        }
        cx.notify();
        done
    }

    /// Send the opening field's words: the Coordinator starts on them.
    pub(crate) fn speak_to_coordinator(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.factory_view.as_ref() else {
            return;
        };
        let project = view.project;
        let Some(field) = view.opening.clone() else {
            return;
        };
        let text = field.read(cx).text().trim().to_string();
        if text.is_empty() {
            return;
        }
        let action = CoordinatorAction::Say { project, text };
        if self.coordinator_ask(action, cx).is_some() {
            field.update(cx, |field, cx| {
                field.take(cx);
            });
        }
    }

    pub(crate) fn accept_proposal(&mut self, proposal: &str, cx: &mut Context<Self>) {
        let skills = self.factory_catalog().skills;
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
        let Some(project) = self.factory_view.as_ref().map(|view| view.project) else {
            return;
        };
        let mut agent = self.coordinator_agent();
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
        self.coordinator_ask(CoordinatorAction::Agent { project, agent }, cx);
    }

    fn coordinator_picks(&self, which: &str) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        let agent = self.coordinator_agent();
        let pick = |item: ferrite_ui::menu::Item, choice: CoordinatorPick| {
            (item, Pick::Coordinator(choice))
        };
        let models = self.cockpit.model_catalog(agent.provider);
        match which {
            "provider" => Provider::ALL
                .into_iter()
                .map(|provider| {
                    pick(
                        ferrite_ui::menu::Item::new(provider.to_string())
                            .checked(agent.provider == provider),
                        CoordinatorPick::Provider(provider),
                    )
                })
                .collect(),
            "model" => super::workflows::model_choices(&models)
                .into_iter()
                .map(|(label, value)| {
                    let chosen = super::workflows::model_chosen(agent.model.as_deref(), &value);
                    pick(
                        ferrite_ui::menu::Item::new(label).checked(chosen),
                        CoordinatorPick::Model(value),
                    )
                })
                .collect(),
            _ => {
                let mut efforts: Vec<String> = agent
                    .model
                    .as_ref()
                    .and_then(|value| models.iter().find(|model| &model.value == value))
                    .map(|model| model.efforts.clone())
                    .unwrap_or_default();
                if efforts.is_empty() {
                    for model in &models {
                        for effort in &model.efforts {
                            if !efforts.contains(effort) {
                                efforts.push(effort.clone());
                            }
                        }
                    }
                }
                std::iter::once(pick(
                    ferrite_ui::menu::Item::new("Default").checked(agent.effort.is_none()),
                    CoordinatorPick::Effort(String::new()),
                ))
                .chain(efforts.into_iter().map(|effort| {
                    pick(
                        ferrite_ui::menu::Item::new(effort.clone())
                            .checked(agent.effort.as_deref() == Some(effort.as_str())),
                        CoordinatorPick::Effort(effort),
                    )
                }))
                .collect()
            }
        }
    }

    // ----------------------------------------------------------- drawing

    /// The Coordinator surface: its settings, the conversation (or the
    /// empty one with its opening field), and the proposals beside it.
    pub(super) fn coordinator_surface(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let Some(view) = self.factory_view.as_ref() else {
            return div();
        };
        let project = view.project;
        let board = self.cockpit.factory();
        let coordinator = board.coordinator(project);
        let agent = self.coordinator_agent();

        let settings = div()
            .debug_selector(|| "coordinator-settings".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_4))
            .pb(px(SPACE_2))
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child("Coordinator runs on"),
            )
            .children(
                [
                    (
                        "coordinator-provider",
                        agent.provider.to_string(),
                        "provider",
                    ),
                    (
                        "coordinator-model",
                        agent.model.as_deref().map_or_else(
                            || "Default model".into(),
                            |model| {
                                super::workflows::model_name(
                                    &self.cockpit.model_catalog(agent.provider),
                                    model,
                                )
                            },
                        ),
                        "model",
                    ),
                    (
                        "coordinator-effort",
                        agent
                            .effort
                            .clone()
                            .unwrap_or_else(|| "Default effort".into()),
                        "effort",
                    ),
                ]
                .into_iter()
                .map(|(selector, text, which)| {
                    // Full-width choosers, each given a share of the row.
                    div()
                        .flex_1()
                        .min_w_0()
                        .max_w(px(ferrite_ui::theme::SETTINGS_MENU_MAX_W))
                        .child(super::direction::chooser(
                            selector,
                            text,
                            false,
                            self.coordinator_picks(which),
                            cx,
                        ))
                }),
            )
            .child(div().flex_1());
        // On its own line, so the choosers never cut it short.
        let settings = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .child(settings)
            .child(
                div()
                    .px(px(SPACE_4))
                    .pb(px(SPACE_2))
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child("Talking starts nothing; work starts only when you accept."),
            );

        let conversation = match self.coordinator_pane() {
            Some(index) => div()
                .debug_selector(|| "coordinator-pane".into())
                .flex_1()
                .min_w_0()
                .min_h_0()
                .flex()
                .flex_col()
                .p(px(SPACE_2))
                .child(self.pane_slot(index, Level::Transcript, window, cx)),
            None => self.coordinator_opening(coordinator, window, cx),
        };

        let mut proposals: Vec<&Proposal> = board.proposals_of(project).collect();
        // Waiting ones first, newest first within each.
        proposals.reverse();
        proposals.sort_by_key(|proposal| proposal.status != ProposalStatus::Pending);
        let column = (!proposals.is_empty()).then(|| {
            div()
                .id("coordinator-proposals")
                .debug_selector(|| "coordinator-proposals".into())
                .flex()
                .flex_col()
                .flex_shrink_0()
                .w(px(PROPOSALS_W))
                .min_h_0()
                .gap(px(SPACE_3))
                .p(px(SPACE_4))
                .border_l_1()
                .border_color(paint::LINE)
                .overflow_y_scroll()
                .child(
                    div()
                        .font_weight(W_LABEL)
                        .text_size(px(FS_LABEL))
                        .child("Proposals"),
                )
                .children(
                    proposals
                        .into_iter()
                        .map(|proposal| self.proposal_card(proposal, cx)),
                )
        });

        div()
            .debug_selector(|| "coordinator-surface".into())
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(settings)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(conversation)
                    .children(column),
            )
    }

    /// Before the Coordinator runs: an empty conversation and the field
    /// whose Send starts it.
    fn coordinator_opening(
        &self,
        coordinator: Option<&ferrite_core::factory::Coordinator>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        use ferrite_ui::theme::*;
        let starting = coordinator.is_some_and(|c| c.starting || c.thread.is_some());
        let error = coordinator.and_then(|c| c.error.clone());
        let field = self
            .factory_view
            .as_ref()
            .and_then(|view| view.opening.clone());
        let focused = field
            .as_ref()
            .is_some_and(|field| field.read(cx).focus_handle(cx).is_focused(window));
        let send = crate::project_editor::primary_button("coordinator-send", "Send", false, cx)
            .debug_selector(|| "coordinator-send".into())
            .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.speak_to_coordinator(cx)
            }));
        div()
            .debug_selector(|| "coordinator-empty".into())
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(SPACE_3))
            .p(px(SPACE_6))
            .child(ferrite_ui::icons::icon(
                ferrite_ui::icons::FERRITE_MONO,
                SPACE_6,
                INK_SUBTLE,
            ))
            .child(
                div()
                    .font_weight(W_LABEL)
                    .text_color(rgb(INK_STRONG))
                    .child("What should we work on?"),
            )
            .child(
                div()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(if starting {
                        "Starting the Coordinator\u{2026}"
                    } else {
                        "Talk it through, hand off a task, start a workflow, or describe a new one."
                    }),
            )
            .children(error.map(|error| {
                div()
                    .debug_selector(|| "coordinator-error".into())
                    .text_size(px(FS_META))
                    .text_color(rgb(BLOCKED))
                    .child(SharedString::from(format!(
                        "The Coordinator could not start: {error}"
                    )))
            }))
            .children(field.map(|field| {
                div()
                    .id("coordinator-field")
                    .debug_selector(|| "coordinator-field".into())
                    .w_full()
                    .max_w(px(560.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(crate::project_editor::text_field(
                        "Ask, delegate, or change how work happens".into(),
                        field,
                        focused,
                        "factory-field-focused",
                    ))
            }))
            .child(div().flex().child(send))
    }

    /// One proposal: what it is, exactly what accepting it does, and its
    /// buttons while it waits (its outcome after).
    fn proposal_card(&self, proposal: &Proposal, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let board = self.cockpit.factory();
        let key = format!("proposal-{}", proposal.id);
        let status = match proposal.status {
            ProposalStatus::Pending => None,
            ProposalStatus::Accepted => Some("accepted"),
            ProposalStatus::Dismissed => Some("dismissed"),
        };
        let mut head = div().flex().items_center().gap(px(SPACE_2)).child(
            div()
                .flex_1()
                .min_w_0()
                .font_weight(W_LABEL)
                .child(SharedString::from(proposal.title(board))),
        );
        if let Some(word) = status {
            let selector = format!("{key}-{word}");
            head = head.child(
                div()
                    .debug_selector(move || selector.clone())
                    .px(px(SPACE_1))
                    .rounded(px(R_CONTROL))
                    .bg(paint::RAISED)
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(word),
            );
        }
        let lines: Vec<Div> = proposal
            .lines(board)
            .into_iter()
            .enumerate()
            .map(|(at, (label, value))| {
                let selector = format!("{key}-line-{at}");
                div()
                    .debug_selector(move || selector.clone())
                    .flex()
                    .gap(px(SPACE_2))
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(80.))
                            .text_size(px(FS_META))
                            .text_color(rgb(INK_SUBTLE))
                            .child(SharedString::from(label)),
                    )
                    .child(div().flex_1().min_w_0().child(SharedString::from(value)))
            })
            .collect();
        let mut actions = div().flex().items_center().gap(px(SPACE_2));
        match proposal.status {
            ProposalStatus::Pending => {
                let accept_id = proposal.id.clone();
                let dismiss_id = proposal.id.clone();
                let accept_key = format!("{key}-accept");
                let dismiss_key = format!("{key}-dismiss");
                actions = actions
                    .child(
                        crate::project_editor::primary_button(
                            SharedString::from(accept_key.clone()),
                            proposal.action(),
                            false,
                            cx,
                        )
                        .debug_selector(move || accept_key.clone())
                        .on_click(cx.listener(
                            move |view, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                view.accept_proposal(&accept_id, cx)
                            },
                        )),
                    )
                    .child(
                        ferrite_ui::components::ghost_button(
                            SharedString::from(dismiss_key.clone()),
                            "Not now",
                            cx,
                        )
                        .debug_selector(move || dismiss_key.clone())
                        .on_click(cx.listener(
                            move |view, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                view.dismiss_proposal(&dismiss_id, cx)
                            },
                        )),
                    );
            }
            ProposalStatus::Accepted => {
                let task = proposal.task.clone().or_else(|| match &proposal.act {
                    ferrite_core::factory::Act::Redirect { task, .. } => Some(task.clone()),
                    _ => None,
                });
                if let Some(task) = task.filter(|task| board.task(task).is_some()) {
                    let open_key = format!("{key}-open");
                    actions = actions.child(
                        ferrite_ui::components::secondary_button(
                            SharedString::from(open_key.clone()),
                            None,
                            format!("Open task {task}"),
                            cx,
                        )
                        .debug_selector(move || open_key.clone())
                        .on_click(cx.listener(
                            move |view, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                view.factory_surface(super::workflows::Surface::Board, cx);
                                view.open_task(&task, cx);
                            },
                        )),
                    );
                }
            }
            ProposalStatus::Dismissed => {}
        }
        div()
            .id(SharedString::from(key.clone()))
            .debug_selector(move || key.clone())
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .p(px(SPACE_3))
            .rounded(px(R_CARD))
            .border_1()
            .border_color(paint::LINE)
            .when(proposal.status == ProposalStatus::Dismissed, |card| {
                card.opacity(0.6)
            })
            .child(head)
            .children(lines)
            .child(actions)
            .into_any_element()
    }
}
