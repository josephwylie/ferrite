//! The Factory's Coordinator surface (#224): the Coordinator's ordinary
//! transcript and Composer, its Provider, model and effort, and the
//! proposals it raised as cards the operator accepts or dismisses.
//!
//! The Coordinator is an ordinary owned Thread the Server starts on the
//! operator's first word; until then the surface shows an empty
//! conversation and a field whose Send starts it. Once it runs, its Pane
//! is the one the window already holds, handed in as its cell
//! (`ferrite_pane::cell::CellSlot`) and drawn here as a Task's Worker tab
//! draws a Worker's. A proposal changes nothing until accepted.
//!
//! `CoordinatorView` owns the opening field (ADR 0041), held by the
//! Factory. It reads the Coordinator and its proposals through a `Source`
//! the window makes over itself and a test over a fixture
//! (`CoordinatorInputs`); what is pressed it says as a `CoordinatorEvent`,
//! since every change is a Server ask (`cockpit/coordinator.rs`).

use ferrite_core::factory_board::{Agent, ProposalId, ProposalStatus, TaskId};
use ferrite_core::provider::Provider;
use ferrite_pane::cell::CellSlot;
use ferrite_pane::composer::{Composer, Role};
use ferrite_ui::source::Source;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, AnyElement, App, ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle,
    Focusable, MouseButton, Pixels, Point, SharedString, Window,
};

use crate::picker::{CoordinatorPick, Pick, Trigger};
use crate::workflows::pickers::{model_choices, model_chosen, model_name};

/// The proposals' column beside the conversation.
const PROPOSALS_W: f32 = 380.;

/// What the Coordinator surface asks of the window.
pub enum CoordinatorEvent {
    /// Send: the Coordinator starts on the opening field's words.
    Send,
    /// A setting's chooser: its rows open under `trigger`.
    Picks {
        picks: Vec<(ferrite_ui::menu::Item, Pick)>,
        trigger: Trigger,
        at: Point<Pixels>,
    },
    /// A proposal's action: accepting it runs it.
    Accept(ProposalId),
    /// `Not now`: the proposal is dismissed and changes nothing.
    Dismiss(ProposalId),
    /// `Open task` on an accepted proposal: the Task's panel on the Board.
    OpenTask(TaskId),
}

/// What the surface draws from the window: the Coordinator and its
/// proposals as the board says them now.
pub struct CoordinatorInputs {
    /// What it runs on: the Factory's choice, else the operator's defaults.
    pub agent: Agent,
    /// That Provider's models, as a chooser names and offers them.
    pub models: Vec<ferrite_core::ModelInfo>,
    /// It is being started, or has started.
    pub starting: bool,
    /// Why it could not start, last time.
    pub error: Option<String>,
    /// Its proposals, waiting ones first, newest first within each.
    pub proposals: Vec<ProposalCard>,
    /// Its Pane's cell, once it runs and the window holds the Pane.
    pub pane: Option<CellSlot>,
}

/// One proposal as its card says it.
pub struct ProposalCard {
    pub id: ProposalId,
    pub title: String,
    /// Exactly what accepting it does, line by line.
    pub lines: Vec<(String, String)>,
    /// Its accept button's word.
    pub action: &'static str,
    pub status: ProposalStatus,
    /// The Task it started or redirects, while the board has it.
    pub task: Option<TaskId>,
}

/// A Factory's Coordinator surface: the field whose Send starts the
/// Coordinator, made once for as long as the Factory is open.
pub struct CoordinatorView {
    /// The Coordinator and its proposals, as the window has them.
    inputs: Source<CoordinatorInputs>,
    field: Entity<Composer>,
}

impl EventEmitter<CoordinatorEvent> for CoordinatorView {}

impl CoordinatorView {
    pub fn new(inputs: Source<CoordinatorInputs>, cx: &mut Context<Self>) -> Self {
        let field = cx.new(Composer::new);
        field.update(cx, |field, cx| field.set_role(Role::Field(None), cx));
        cx.observe(&field, |_, _, cx| cx.notify()).detach();
        Self { inputs, field }
    }

    /// The opening field, whose words Send starts the Coordinator on.
    pub fn field(&self) -> &Entity<Composer> {
        &self.field
    }

    /// The opening field's focus, which holds the keyboard while the
    /// surface shows and the Coordinator has no Pane.
    pub fn focus(&self, cx: &App) -> FocusHandle {
        self.field.read(cx).focus_handle(cx)
    }

    /// The Coordinator surface: its settings, the conversation (its Pane
    /// once it runs, else the empty one with its opening field), and the
    /// proposals beside it.
    fn surface(&self, inputs: CoordinatorInputs, window: &Window, cx: &mut Context<Self>) -> Div {
        let pane = inputs.pane.clone().map(CellSlot::draw);
        let inputs = &inputs;
        use ferrite_ui::theme::*;
        let agent = &inputs.agent;

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
                                model_name(&inputs.models, model)
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
                        .child({
                            let picks = std::rc::Rc::new(coordinator_picks(inputs, which));
                            crate::picker::picker_button(
                                selector,
                                text,
                                false,
                                move |_, trigger, at, cx: &mut Context<Self>| {
                                    cx.emit(CoordinatorEvent::Picks {
                                        picks: (*picks).clone(),
                                        trigger,
                                        at,
                                    })
                                },
                                cx,
                            )
                        })
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

        let conversation = match pane {
            Some(pane) => div()
                .debug_selector(|| "coordinator-pane".into())
                .flex_1()
                .min_w_0()
                .min_h_0()
                .flex()
                .flex_col()
                .p(px(SPACE_2))
                .child(pane),
            None => self.opening(inputs, window, cx),
        };

        let column = (!inputs.proposals.is_empty()).then(|| {
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
                    inputs
                        .proposals
                        .iter()
                        .map(|card| Self::proposal_card(card, cx)),
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
    fn opening(&self, inputs: &CoordinatorInputs, window: &Window, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let field = self.field.clone();
        let focused = field.read(cx).focus_handle(cx).is_focused(window);
        let send = ferrite_ui::form::primary_button("coordinator-send", "Send", false, cx)
            .debug_selector(|| "coordinator-send".into())
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(CoordinatorEvent::Send)
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
                    .child(if inputs.starting {
                        "Starting the Coordinator\u{2026}"
                    } else {
                        "Talk it through, hand off a task, start a workflow, or describe a new one."
                    }),
            )
            .children(inputs.error.clone().map(|error| {
                div()
                    .debug_selector(|| "coordinator-error".into())
                    .text_size(px(FS_META))
                    .text_color(rgb(BLOCKED))
                    .child(SharedString::from(format!(
                        "The Coordinator could not start: {error}"
                    )))
            }))
            .child(
                div()
                    .id("coordinator-field")
                    .debug_selector(|| "coordinator-field".into())
                    .w_full()
                    .max_w(px(560.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(ferrite_ui::field::text_field(
                        "Ask, delegate, or change how work happens".into(),
                        field,
                        focused,
                        "factory-field-focused",
                    )),
            )
            .child(div().flex().child(send))
    }

    /// One proposal: what it is, exactly what accepting it does, and its
    /// buttons while it waits (its outcome after).
    fn proposal_card(card: &ProposalCard, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let key = format!("proposal-{}", card.id);
        let status = match card.status {
            ProposalStatus::Pending => None,
            ProposalStatus::Accepted => Some("accepted"),
            ProposalStatus::Dismissed => Some("dismissed"),
        };
        let mut head = div().flex().items_center().gap(px(SPACE_2)).child(
            div()
                .flex_1()
                .min_w_0()
                .font_weight(W_LABEL)
                .child(SharedString::from(card.title.clone())),
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
        let lines: Vec<Div> = card
            .lines
            .iter()
            .cloned()
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
        match card.status {
            ProposalStatus::Pending => {
                let accept_id = card.id.clone();
                let dismiss_id = card.id.clone();
                let accept_key = format!("{key}-accept");
                let dismiss_key = format!("{key}-dismiss");
                actions = actions
                    .child(
                        ferrite_ui::form::primary_button(
                            SharedString::from(accept_key.clone()),
                            card.action,
                            false,
                            cx,
                        )
                        .debug_selector(move || accept_key.clone())
                        .on_click(cx.listener(
                            move |_, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                cx.emit(CoordinatorEvent::Accept(accept_id.clone()))
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
                            move |_, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                cx.emit(CoordinatorEvent::Dismiss(dismiss_id.clone()))
                            },
                        )),
                    );
            }
            ProposalStatus::Accepted => {
                if let Some(task) = card.task.clone() {
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
                            move |_, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                cx.emit(CoordinatorEvent::OpenTask(task.clone()))
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
            .when(card.status == ProposalStatus::Dismissed, |each| {
                each.opacity(0.6)
            })
            .child(head)
            .children(lines)
            .child(actions)
            .into_any_element()
    }
}

impl Render for CoordinatorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.inputs.read(&(), cx) {
            Some(inputs) => self.surface(inputs, window, cx),
            None => div(),
        }
    }
}

/// A chooser's rows: `which` of the Coordinator's settings, as `inputs`
/// say them.
fn coordinator_picks(inputs: &CoordinatorInputs, which: &str) -> Vec<(ferrite_ui::menu::Item, Pick)> {
    let agent = &inputs.agent;
    let pick = |item: ferrite_ui::menu::Item, choice: CoordinatorPick| {
        (item, Pick::Coordinator(choice))
    };
    let models = &inputs.models;
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
        "model" => model_choices(models)
            .into_iter()
            .map(|(label, value)| {
                let chosen = model_chosen(agent.model.as_deref(), &value);
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
                for model in models {
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

#[cfg(test)]
mod tests {
    use ferrite_ui::testing::{click, mount, record, shows};
    use gpui::{TestAppContext, VisualTestContext};

    use super::*;

    /// What a test's window keeps of the Coordinator: what it runs on,
    /// whether it is starting, why it could not, and its proposals.
    struct Kept {
        starting: bool,
        error: Option<String>,
        proposals: Vec<ProposalCard>,
    }

    fn proposal(id: &str, status: ProposalStatus, task: Option<&str>) -> ProposalCard {
        ProposalCard {
            id: id.into(),
            title: format!("Proposal {id}"),
            lines: vec![("Task".into(), "Fix the flaky login test".into())],
            action: "Start task",
            status,
            task: task.map(str::to_string),
        }
    }

    fn open(kept: Kept, cx: &mut TestAppContext) -> (Entity<CoordinatorView>, Entity<Kept>, &mut VisualTestContext) {
        let kept = cx.new(|_| kept);
        let inputs = Source::of(&kept, |kept: &Kept, _: &(), _| {
            Some(CoordinatorInputs {
                agent: Agent {
                    provider: Provider::Claude,
                    model: None,
                    effort: None,
                },
                models: Vec::new(),
                starting: kept.starting,
                error: kept.error.clone(),
                proposals: kept
                    .proposals
                    .iter()
                    .map(|card| proposal(&card.id, card.status, card.task.as_deref()))
                    .collect(),
                pane: None,
            })
        });
        let (view, cx) = mount(cx, |_, cx| CoordinatorView::new(inputs, cx));
        (view, kept, cx)
    }

    fn quiet() -> Kept {
        Kept {
            starting: false,
            error: None,
            proposals: Vec::new(),
        }
    }

    /// What a press asked, as a word.
    fn said(event: &CoordinatorEvent) -> String {
        match event {
            CoordinatorEvent::Send => "send".into(),
            CoordinatorEvent::Picks { picks, .. } => format!("picks {}", picks.len()),
            CoordinatorEvent::Accept(proposal) => format!("accept {proposal}"),
            CoordinatorEvent::Dismiss(proposal) => format!("dismiss {proposal}"),
            CoordinatorEvent::OpenTask(task) => format!("open {task}"),
        }
    }

    /// Before it runs, the surface is an empty conversation whose field
    /// takes what is typed, and Send asks for it to start; why it could not
    /// start is said.
    #[gpui::test]
    fn before_it_runs_its_field_and_send_start_it(cx: &mut TestAppContext) {
        let (view, kept, cx) = open(quiet(), cx);
        assert!(shows(cx, "coordinator-surface") && shows(cx, "coordinator-empty"));
        assert!(!shows(cx, "coordinator-pane"), "no Pane before it runs");
        assert!(!shows(cx, "coordinator-proposals"), "no proposals, no column");
        let heard = record(cx, &view, said);
        click(cx, "coordinator-field");
        cx.simulate_input("plan the week");
        cx.run_until_parked();
        let typed = view.read_with(cx, |view, cx| view.field().read(cx).text().to_string());
        assert_eq!(typed, "plan the week");
        click(cx, "coordinator-send");
        assert_eq!(heard.take(), ["send"]);

        assert!(!shows(cx, "coordinator-error"));
        kept.update(cx, |kept, cx| {
            kept.error = Some("no claude CLI".into());
            cx.notify();
        });
        cx.run_until_parked();
        assert!(shows(cx, "coordinator-error"));
    }

    /// A waiting proposal is accepted or dismissed; an accepted one opens
    /// the Task it started; a dismissed one offers nothing.
    #[gpui::test]
    fn a_proposal_is_accepted_dismissed_or_opens_its_task(cx: &mut TestAppContext) {
        let kept = Kept {
            proposals: vec![
                proposal("p-1", ProposalStatus::Pending, None),
                proposal("p-2", ProposalStatus::Accepted, Some("t-3")),
                proposal("p-3", ProposalStatus::Dismissed, None),
            ],
            ..quiet()
        };
        let (view, _kept, cx) = open(kept, cx);
        let heard = record(cx, &view, said);
        assert!(shows(cx, "coordinator-proposals"));
        assert!(shows(cx, "proposal-p-1-line-0"));
        assert!(shows(cx, "proposal-p-2-accepted") && shows(cx, "proposal-p-3-dismissed"));
        assert!(!shows(cx, "proposal-p-2-accept") && !shows(cx, "proposal-p-3-accept"));
        click(cx, "proposal-p-1-accept");
        click(cx, "proposal-p-1-dismiss");
        click(cx, "proposal-p-2-open");
        assert_eq!(heard.take(), ["accept p-1", "dismiss p-1", "open t-3"]);
    }

    /// Each setting's chooser asks for its rows: the Providers, the models
    /// with the default, the efforts with the default.
    #[gpui::test]
    fn each_settings_chooser_asks_for_its_rows(cx: &mut TestAppContext) {
        let (view, _kept, cx) = open(quiet(), cx);
        let heard = record(cx, &view, said);
        click(cx, "coordinator-provider");
        click(cx, "coordinator-effort");
        assert_eq!(heard.take(), [format!("picks {}", Provider::ALL.len()), "picks 1".into()]);
    }
}
