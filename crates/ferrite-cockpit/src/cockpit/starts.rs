//! A Workflow's "Tasks start" settings on the Workflows surface (#225,
//! ADR 0037): the one picker of five sources, each showing only its own
//! settings, and the switch for automatic starts, apart from Save.
//!
//! The start is part of the draft: every edit is one `edit_start` ask
//! carrying the whole start, so what was typed for one source survives
//! choosing another. Repositories and labels are listed from the
//! operator's `gh` login where the Server runs, never typed.

use std::collections::HashMap;
use std::sync::mpsc::Receiver;

use super::workflows::{Field, Pick};
use super::*;
use ferrite_core::factory::{
    self as core_factory, Missed, Overlap, Recurrence, Start, StartKind, Switch, Trigger, Turn,
    Workflow,
};
use ferrite_core::protocol::{GitHubAsk, WorkflowAction};
use gpui::component::Disableable as _;

/// A start picker's row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StartPick {
    Kind(StartKind),
    Recurrence(Recurrence),
    /// 0 is Sunday.
    Weekday(u8),
    Interval(u32),
    /// `HH:MM`.
    Time(String),
    Repository(String),
    Trigger(Trigger),
    Label(String),
    Missed(Missed),
    Overlap(Overlap),
}

/// One list the login gives a picker.
#[derive(Default)]
pub(crate) enum Listing {
    #[default]
    Unasked,
    Pending(Receiver<std::io::Result<Vec<String>>>),
    Listed(Vec<String>),
    Failed(String),
}

impl Listing {
    fn names(&self) -> &[String] {
        match self {
            Listing::Listed(names) => names,
            _ => &[],
        }
    }

    /// What the picker says while it has nothing to offer.
    fn wait(&self, what: &str) -> Option<String> {
        match self {
            Listing::Unasked | Listing::Pending(_) => Some(format!("Listing {what}\u{2026}")),
            Listing::Failed(why) => Some(format!("Could not list {what}: {why}")),
            Listing::Listed(names) if names.is_empty() => Some(format!("No {what} found")),
            Listing::Listed(_) => None,
        }
    }
}

/// What the operator's GitHub login lists, asked once per window.
#[derive(Default)]
pub(crate) struct GitHubLists {
    pub(crate) repositories: Listing,
    pub(crate) labels: HashMap<String, Listing>,
}

/// The half-hour times a schedule offers.
fn times() -> Vec<String> {
    (0..48)
        .map(|half| {
            format!(
                "{:02}:{}",
                half / 2,
                if half % 2 == 1 { "30" } else { "00" }
            )
        })
        .collect()
}

/// The command a script runs to submit a Task: the real `ferrite`.
pub(crate) fn script_command(executable: &str, workflow: &str) -> String {
    format!("\"{executable}\" task submit {workflow} \"<identity>\" \"<instructions>\"")
}

impl CockpitView {
    /// Settings' stepper on how many Factory Tasks work at once.
    pub(crate) fn step_factory_limit(&mut self, delta: i32, cx: &mut Context<Self>) {
        let now = self.prefs.settings.factory_tasks_at_once;
        let next = if delta == 0 {
            core_factory::DEFAULT_LIMIT
        } else {
            core_factory::clamp_limit(now.saturating_add_signed(delta as isize))
        };
        if next != now {
            self.change_settings(|settings| settings.factory_tasks_at_once = next, cx);
            cx.notify();
        }
    }
}

/// This window's own `ferrite`, which is also its CLI.
fn ferrite_executable() -> String {
    std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "ferrite".into())
}

impl CockpitView {
    /// The start's settings are what the inspector shows.
    pub(crate) fn start_shown(&self) -> bool {
        self.factory_view
            .as_ref()
            .is_some_and(|view| view.workflows.start_open)
    }

    /// Show the shown Workflow's start in the inspector.
    pub(crate) fn open_start(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = self.factory_view.as_mut() {
            view.workflows.start_open = true;
            view.workflows.focused = None;
        }
        self.ensure_fields(cx);
        self.ask_github_lists();
        cx.notify();
    }

    /// Change the shown draft's start, whole.
    pub(crate) fn edit_start(&mut self, change: impl FnOnce(&mut Start), cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_workflow() else {
            return;
        };
        let id = workflow.id.clone();
        let mut start = workflow.draft.start.clone();
        change(&mut start);
        self.workflow_ask(
            WorkflowAction::EditStart {
                workflow: id,
                start,
            },
            cx,
        );
        self.ask_github_lists();
    }

    pub(crate) fn apply_start_pick(&mut self, pick: StartPick, cx: &mut Context<Self>) {
        self.edit_start(
            |start| match pick {
                StartPick::Kind(kind) => {
                    start.kind = kind;
                    if !Trigger::offered(kind).contains(&start.github.trigger) {
                        start.github.trigger = Trigger::Labelled;
                    }
                }
                StartPick::Recurrence(recurrence) => start.schedule.recurrence = recurrence,
                StartPick::Weekday(day) => start.schedule.weekday = day,
                StartPick::Interval(hours) => start.schedule.interval_hours = hours,
                StartPick::Time(time) => start.schedule.time = Some(time),
                StartPick::Repository(repository) => {
                    if start.github.repository.as_deref() != Some(repository.as_str()) {
                        start.github.label = None;
                    }
                    start.github.repository = Some(repository);
                }
                StartPick::Trigger(trigger) => start.github.trigger = trigger,
                StartPick::Label(label) => start.github.label = Some(label),
                StartPick::Missed(missed) => start.schedule.missed = missed,
                StartPick::Overlap(overlap) => start.schedule.overlap = overlap,
            },
            cx,
        );
    }

    /// Turn the shown Workflow's automatic starts on, pause or resume.
    pub(crate) fn turn_automation(&mut self, turn: Turn, cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_workflow().map(|shown| shown.id.clone()) else {
            return;
        };
        self.workflow_ask(WorkflowAction::Automation { workflow, turn }, cx);
    }

    /// The operator's word on what already matched.
    pub(crate) fn answer_backlog(&mut self, take: bool, cx: &mut Context<Self>) {
        let Some(workflow) = self.shown_workflow().map(|shown| shown.id.clone()) else {
            return;
        };
        self.workflow_ask(WorkflowAction::Backlog { workflow, take }, cx);
    }

    /// The start's text fields, as the draft holds them.
    pub(crate) fn start_fields(&self) -> Vec<(Field, String)> {
        let Some(workflow) = self.shown_workflow().filter(|_| self.start_shown()) else {
            return Vec::new();
        };
        let start = &workflow.draft.start;
        vec![
            (
                Field::ScheduleInstructions(workflow.id.clone()),
                start.schedule.instructions.clone(),
            ),
            (
                Field::ExtraInstructions(workflow.id.clone()),
                start.github.extra_instructions.clone(),
            ),
        ]
    }

    /// What was typed in one of the start's fields goes to the draft.
    pub(crate) fn start_field_edited(
        &mut self,
        field: &Field,
        text: String,
        cx: &mut Context<Self>,
    ) {
        match field {
            Field::ScheduleInstructions(_) => {
                self.edit_start(|start| start.schedule.instructions = text, cx)
            }
            Field::ExtraInstructions(_) => {
                self.edit_start(|start| start.github.extra_instructions = text, cx)
            }
            _ => {}
        }
    }

    /// Ask the login for what a GitHub start's pickers need: the
    /// repositories once, and the chosen repository's labels.
    fn ask_github_lists(&mut self) {
        let Some(start) = self
            .shown_workflow()
            .filter(|_| self.start_shown())
            .map(|workflow| workflow.draft.start.clone())
        else {
            return;
        };
        if !start.kind.is_github() {
            return;
        }
        let unasked = |listing: Option<&Listing>| matches!(listing, None | Some(Listing::Unasked));
        let want_repositories = self
            .factory_view
            .as_ref()
            .is_some_and(|view| unasked(Some(&view.workflows.github.repositories)));
        if want_repositories {
            let listing = match self.cockpit.github_names(GitHubAsk::Repositories) {
                Ok(pending) => Listing::Pending(pending),
                Err(error) => Listing::Failed(error.to_string()),
            };
            if let Some(view) = self.factory_view.as_mut() {
                view.workflows.github.repositories = listing;
            }
        }
        let Some(repository) = start.github.repository else {
            return;
        };
        let want_labels = self
            .factory_view
            .as_ref()
            .is_some_and(|view| unasked(view.workflows.github.labels.get(&repository)));
        if want_labels {
            let listing = match self.cockpit.github_names(GitHubAsk::Labels {
                repository: repository.clone(),
            }) {
                Ok(pending) => Listing::Pending(pending),
                Err(error) => Listing::Failed(error.to_string()),
            };
            if let Some(view) = self.factory_view.as_mut() {
                view.workflows.github.labels.insert(repository, listing);
            }
        }
    }

    /// A list that answered.
    pub(super) fn poll_start_lists(&mut self, cx: &mut Context<Self>) {
        // The draft a window is told of may name a repository whose labels
        // were never asked for.
        self.ask_github_lists();
        let Some(view) = self.factory_view.as_mut() else {
            return;
        };
        let lists = &mut view.workflows.github;
        let mut heard = false;
        for listing in std::iter::once(&mut lists.repositories).chain(lists.labels.values_mut()) {
            let Listing::Pending(pending) = listing else {
                continue;
            };
            let answer = match pending.try_recv() {
                Ok(answer) => answer,
                Err(std::sync::mpsc::TryRecvError::Empty) => continue,
                Err(error) => Err(std::io::Error::other(error)),
            };
            heard = true;
            *listing = match answer {
                Ok(names) => Listing::Listed(names),
                Err(error) => Listing::Failed(error.to_string()),
            };
        }
        if heard {
            cx.notify();
        }
    }

    // ----------------------------------------------------------- pickers

    fn kind_picks(&self, start: &Start) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        StartKind::ALL
            .into_iter()
            .map(|kind| {
                (
                    ferrite_ui::menu::Item::new(kind.label())
                        .detail(kind.detail())
                        .checked(start.kind == kind),
                    Pick::Start(StartPick::Kind(kind)),
                )
            })
            .collect()
    }

    fn recurrence_picks(start: &Start) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        Recurrence::ALL
            .into_iter()
            .map(|recurrence| {
                let mut item = ferrite_ui::menu::Item::new(recurrence.label())
                    .checked(start.schedule.recurrence == recurrence);
                if recurrence == Recurrence::Weekdays {
                    item = item.detail("Mon to Fri");
                }
                (item, Pick::Start(StartPick::Recurrence(recurrence)))
            })
            .collect()
    }

    fn weekday_picks(start: &Start) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        core_factory::WEEKDAYS
            .iter()
            .enumerate()
            .map(|(day, name)| {
                let day = day as u8;
                (
                    ferrite_ui::menu::Item::new(*name).checked(start.schedule.weekday == day),
                    Pick::Start(StartPick::Weekday(day)),
                )
            })
            .collect()
    }

    fn interval_picks(start: &Start) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        core_factory::INTERVAL_HOURS
            .into_iter()
            .map(|hours| {
                let plural = if hours == 1 { "" } else { "s" };
                (
                    ferrite_ui::menu::Item::new(format!("{hours} hour{plural}"))
                        .checked(start.schedule.interval_hours == hours),
                    Pick::Start(StartPick::Interval(hours)),
                )
            })
            .collect()
    }

    fn time_picks(start: &Start) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        times()
            .into_iter()
            .map(|time| {
                (
                    ferrite_ui::menu::Item::new(time.clone())
                        .checked(start.schedule.time.as_deref() == Some(time.as_str())),
                    Pick::Start(StartPick::Time(time)),
                )
            })
            .collect()
    }

    fn repository_picks(&self, start: &Start) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        let Some(view) = self.factory_view.as_ref() else {
            return Vec::new();
        };
        view.workflows
            .github
            .repositories
            .names()
            .iter()
            .map(|name| {
                (
                    ferrite_ui::menu::Item::new(name.clone())
                        .checked(start.github.repository.as_deref() == Some(name.as_str())),
                    Pick::Start(StartPick::Repository(name.clone())),
                )
            })
            .collect()
    }

    fn label_picks(&self, start: &Start) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        let Some(listing) = start.github.repository.as_ref().and_then(|repository| {
            self.factory_view
                .as_ref()?
                .workflows
                .github
                .labels
                .get(repository)
        }) else {
            return Vec::new();
        };
        listing
            .names()
            .iter()
            .map(|name| {
                (
                    ferrite_ui::menu::Item::new(name.clone())
                        .checked(start.github.label.as_deref() == Some(name.as_str())),
                    Pick::Start(StartPick::Label(name.clone())),
                )
            })
            .collect()
    }

    fn trigger_picks(start: &Start) -> Vec<(ferrite_ui::menu::Item, Pick)> {
        Trigger::offered(start.kind)
            .iter()
            .map(|trigger| {
                (
                    ferrite_ui::menu::Item::new(trigger.label())
                        .checked(start.github.trigger == *trigger),
                    Pick::Start(StartPick::Trigger(*trigger)),
                )
            })
            .collect()
    }

    // ----------------------------------------------------------- drawing

    /// The outline's row for the start, above the agents.
    pub(super) fn start_outline_row(
        &self,
        workflow: &Workflow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        let selected = self.start_shown();
        let key = if selected {
            "outline-start-selected"
        } else {
            "outline-start"
        };
        let mut said = workflow.draft.start.kind.label().to_string();
        if workflow.draft.start.kind.automatic() {
            said = format!(
                "{said} \u{b7} automatic starts {}",
                workflow.automation.switch.word().to_lowercase()
            );
        }
        div()
            .id("outline-start")
            .debug_selector(move || key.into())
            .flex()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_2))
            .py(px(SPACE_1_5))
            .rounded(px(R_CONTROL))
            .cursor_pointer()
            .map(|row| {
                if selected {
                    row.bg(paint::SELECTED)
                } else {
                    row.hover(|row| row.bg(paint::HOVER))
                }
            })
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(W_LABEL)
                    .text_color(rgb(INK_STRONG))
                    .child("Tasks start"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(FS_META))
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(said)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.open_start(cx);
                }),
            )
            .into_any_element()
    }

    /// A picker's button: what it holds, opening its rows; disabled, it
    /// opens nothing.
    fn start_chooser(
        selector: &'static str,
        text: String,
        flagged: bool,
        disabled: bool,
        picks: Vec<(ferrite_ui::menu::Item, Pick)>,
        cx: &mut Context<Self>,
    ) -> Div {
        use gpui::component::Disableable;
        let picks = std::rc::Rc::new(picks);
        let look = ferrite_ui::controls::Look {
            flagged,
            full_width: true,
            disabled,
        };
        div().flex_1().min_w_0().child(
            ferrite_ui::controls::chooser(selector, "", text.into(), look, cx)
                .disabled(disabled)
                .debug_selector(move || {
                    if flagged {
                        format!("{selector}-flagged")
                    } else {
                        selector.to_string()
                    }
                })
                .when(!disabled, |chooser| {
                    chooser.on_click(cx.listener(move |view, event: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.open_picks((*picks).clone(), event.position(), cx);
                    }))
                }),
        )
    }

    /// A segmented choice: one of a few, full width, the chosen one
    /// selected as `{selector}-{at}-on`.
    fn start_segments(
        selector: &'static str,
        options: Vec<(&'static str, bool, StartPick)>,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut row = ferrite_ui::controls::segments(true).debug_selector(move || selector.into());
        for (at, (label, chosen, pick)) in options.into_iter().enumerate() {
            let key = format!("{selector}-{at}{}", if chosen { "-on" } else { "" });
            row = row.child(
                ferrite_ui::controls::segment((selector, at), label, chosen, true, cx)
                    .debug_selector(move || key.clone())
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.apply_start_pick(pick.clone(), cx)
                    })),
            );
        }
        row
    }

    /// The inspector, showing the start: the picker, the chosen source's
    /// own settings and the automatic starts' switch.
    pub(super) fn start_inspector(
        &self,
        workflow: &Workflow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        let start = workflow.draft.start.clone();
        let label = |text: SharedString| {
            div()
                .text_size(px(FS_META))
                .font_weight(W_LABEL)
                .text_color(rgb(INK_SUBTLE))
                .child(text)
        };
        let meta = |selector: &'static str, text: String| {
            div()
                .debug_selector(move || selector.into())
                .text_size(px(FS_META))
                .text_color(rgb(INK_SUBTLE))
                .child(SharedString::from(text))
        };
        let problems = start.problems();
        let flagged = |words: &str| problems.iter().any(|problem| problem.contains(words));
        let mut body = div()
            .id("inspector")
            .debug_selector(|| "inspector-start".into())
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(SPACE_3))
            .p(px(SPACE_4))
            .overflow_y_scroll()
            .child(label("Tasks start".into()))
            .child(Self::start_chooser(
                "start-kind",
                start.kind.label().into(),
                false,
                false,
                self.kind_picks(&start),
                cx,
            ))
            .child(meta(
                "start-kind-note",
                if start.kind == StartKind::Manual {
                    "You, or the coordinator on your confirmation, start each task.".into()
                } else {
                    "You can always start tasks manually as well.".into()
                },
            ));
        match start.kind {
            StartKind::Manual => {}
            StartKind::Schedule => {
                let schedule = &start.schedule;
                let mut row = div().flex().gap(px(SPACE_2)).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(SPACE_1))
                        .child(label("Repeats".into()))
                        .child(Self::start_chooser(
                            "start-recurrence",
                            schedule.recurrence.label().into(),
                            false,
                            false,
                            Self::recurrence_picks(&start),
                            cx,
                        )),
                );
                if schedule.recurrence == Recurrence::Weekly {
                    row = row.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(SPACE_1))
                            .child(label("On".into()))
                            .child(Self::start_chooser(
                                "start-weekday",
                                core_factory::WEEKDAYS[usize::from(schedule.weekday.min(6))].into(),
                                false,
                                false,
                                Self::weekday_picks(&start),
                                cx,
                            )),
                    );
                }
                row = row.child(if schedule.recurrence == Recurrence::Interval {
                    let hours = schedule.interval_hours;
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(SPACE_1))
                        .child(label("Every".into()))
                        .child(Self::start_chooser(
                            "start-interval",
                            format!("{hours} hour{}", if hours == 1 { "" } else { "s" }),
                            false,
                            false,
                            Self::interval_picks(&start),
                            cx,
                        ))
                } else {
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(SPACE_1))
                        .child(label("At".into()))
                        .child(Self::start_chooser(
                            "start-time",
                            schedule
                                .time
                                .clone()
                                .unwrap_or_else(|| "Choose a time".into()),
                            flagged("time"),
                            false,
                            Self::time_picks(&start),
                            cx,
                        ))
                });
                let now = ferrite_core::clock::now().timestamp_millis();
                let next = schedule
                    .next_after_ms(now)
                    .map(|at| format!(" Next: {}.", core_factory::when_label(at)))
                    .unwrap_or_default();
                body = body
                    .child(row)
                    .child(meta(
                        "start-schedule-when",
                        format!(
                            "Host-local time \u{b7} {} (this computer's time zone). {}.{next}",
                            core_factory::zone_label(now),
                            schedule.describe()
                        ),
                    ))
                    .children(self.text_input(
                        &Field::ScheduleInstructions(workflow.id.clone()),
                        "Task instructions for each run",
                        window,
                        cx,
                    ))
                    .child(label("If the computer was asleep at the time".into()))
                    .child(Self::start_segments(
                        "start-missed",
                        vec![
                            (
                                "Skip the missed run",
                                schedule.missed == Missed::Skip,
                                StartPick::Missed(Missed::Skip),
                            ),
                            (
                                "Run once on wake",
                                schedule.missed == Missed::RunOnce,
                                StartPick::Missed(Missed::RunOnce),
                            ),
                        ],
                        cx,
                    ))
                    .child(label("If the previous run is still going".into()))
                    .child(Self::start_segments(
                        "start-overlap",
                        vec![
                            (
                                "Skip this run",
                                schedule.overlap == Overlap::Skip,
                                StartPick::Overlap(Overlap::Skip),
                            ),
                            (
                                "Queue another task",
                                schedule.overlap == Overlap::Queue,
                                StartPick::Overlap(Overlap::Queue),
                            ),
                        ],
                        cx,
                    ));
            }
            StartKind::GithubIssue | StartKind::GithubPr => {
                body = body.child(self.github_settings(workflow, &start, window, cx));
            }
            StartKind::External => {
                let command = script_command(&ferrite_executable(), &workflow.id);
                body = body
                    .child(label("Your script runs".into()))
                    .child(
                        div()
                            .debug_selector(|| "start-script-command".into())
                            .px(px(SPACE_2))
                            .py(px(SPACE_1_5))
                            .rounded(px(R_CONTROL))
                            .bg(paint::RAISED)
                            .font_family(FONT_CODE)
                            .text_size(px(FS_META))
                            .text_color(rgb(INK_STRONG))
                            .child(SharedString::from(command)),
                    )
                    .child(meta(
                        "start-script-note",
                        "Each submission needs instructions and an identity of your own (an \
                         issue key, a ticket number). The same identity again returns the task \
                         it already started. It runs the latest saved version, only while \
                         automatic starts are on."
                            .into(),
                    ));
            }
        }
        body.child(self.automation_row(workflow, cx))
            .into_any_element()
    }

    /// A GitHub start's settings: repository, trigger and label from the
    /// login, the sentence that says what is checked, what each Task
    /// receives, and its extra instructions.
    fn github_settings(
        &self,
        workflow: &Workflow,
        start: &Start,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        use ferrite_ui::theme::*;
        let Some(view) = self.factory_view.as_ref() else {
            return div();
        };
        let lists = &view.workflows.github;
        let pr = start.kind == StartKind::GithubPr;
        let thing = if pr { "pull request" } else { "issue" };
        let github = &start.github;
        let problems = start.problems();
        let flagged = |words: &str| problems.iter().any(|problem| problem.contains(words));
        let label = |text: SharedString| {
            div()
                .text_size(px(FS_META))
                .font_weight(W_LABEL)
                .text_color(rgb(INK_SUBTLE))
                .child(text)
        };
        let meta = |selector: &'static str, text: String| {
            div()
                .debug_selector(move || selector.into())
                .text_size(px(FS_META))
                .text_color(rgb(INK_SUBTLE))
                .child(SharedString::from(text))
        };
        let repository_wait = lists.repositories.wait("repositories");
        let mut settings = div()
            .flex()
            .flex_col()
            .gap(px(SPACE_3))
            .child(label("Repository".into()))
            .child(Self::start_chooser(
                "start-repository",
                github
                    .repository
                    .clone()
                    .unwrap_or_else(|| "Choose a repository".into()),
                flagged("repository"),
                repository_wait.is_some(),
                self.repository_picks(start),
                cx,
            ))
            .child(meta(
                "start-repository-note",
                repository_wait.unwrap_or_else(|| {
                    "The repositories your GitHub CLI login can reach, on the computer the \
                     Server runs on."
                        .into()
                }),
            ));
        let mut row = div().flex().gap(px(SPACE_2)).child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(SPACE_1))
                .child(label(
                    format!(
                        "Start a task when {}",
                        if pr { "a pull request" } else { "an issue" }
                    )
                    .into(),
                ))
                .child(Self::start_chooser(
                    "start-trigger",
                    github.trigger.label().into(),
                    flagged("review"),
                    false,
                    Self::trigger_picks(start),
                    cx,
                )),
        );
        let mut label_wait = None;
        if github.trigger == Trigger::Labelled {
            let listing = github
                .repository
                .as_ref()
                .and_then(|repository| lists.labels.get(repository));
            label_wait = match (&github.repository, listing) {
                (None, _) => Some("Choose a repository first".to_string()),
                (Some(_), None) => Some("Listing labels\u{2026}".to_string()),
                (Some(_), Some(listing)) => listing.wait("labels"),
            };
            row = row.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_1))
                    .child(label("Label".into()))
                    .child(Self::start_chooser(
                        "start-label",
                        github.label.clone().unwrap_or_else(|| {
                            label_wait
                                .clone()
                                .unwrap_or_else(|| "Choose a label".into())
                        }),
                        flagged("label"),
                        label_wait.is_some(),
                        self.label_picks(start),
                        cx,
                    )),
            );
        }
        settings = settings.child(row);
        if let Some(wait) = label_wait.filter(|_| github.repository.is_some()) {
            settings = settings.child(meta("start-label-note", wait));
        }
        let receives = [
            format!("The {thing}'s title and description as its instructions"),
            if pr {
                "The pull request as the task's PR, so every agent works on it".to_string()
            } else {
                "A link back to the issue".to_string()
            },
            format!("One task per {thing}. Seeing the same {thing} again starts nothing."),
        ];
        let mut list = div()
            .debug_selector(|| "start-receives".into())
            .flex()
            .flex_col()
            .gap(px(SPACE_0_5))
            .text_size(px(FS_META));
        for line in receives {
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_1))
                    .child(ferrite_ui::icons::icon(
                        ferrite_ui::icons::CHECK,
                        ROW_ICON,
                        RUNNING,
                    ))
                    .child(SharedString::from(line)),
            );
        }
        settings
            .child(meta(
                "start-github-sentence",
                format!(
                    "{} Nothing is installed on GitHub and Ferrite writes nothing to it. It checks \
                     only while this computer and Ferrite are running; anything that matched in \
                     the meantime is picked up at the next check.",
                    github.describe(start.kind)
                ),
            ))
            .child(label("Each task receives".into()))
            .child(list)
            .children(self.text_input(
                &Field::ExtraInstructions(workflow.id.clone()),
                "Extra instructions for every task (optional)",
                window,
                cx,
            ))
    }

    /// Automatic starts: the switch, the version it uses, the last
    /// check's failure, and the question over a backlog.
    fn automation_row(&self, workflow: &Workflow, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let saved = workflow
            .latest()
            .map(|latest| (latest.version, latest.definition.start.kind));
        let automation = &workflow.automation;
        let switch = automation.switch;
        let can = saved.is_some_and(|(_, kind)| kind.automatic());
        if !can && !workflow.draft.start.kind.automatic() {
            return div();
        }
        let hint = match saved {
            Some((version, kind)) if kind.automatic() => {
                let mut hint = format!(
                    "{}. Uses version {version}. Saving a version does not turn this on.",
                    switch.word()
                );
                if let Some(due) = automation.due_ms.filter(|_| switch == Switch::On) {
                    hint.push_str(&format!(" Next run: {}.", core_factory::when_label(due)));
                }
                if automation.waiting {
                    hint.push_str(" A run waits for the last one to finish.");
                }
                hint
            }
            _ => "Save a version with this start setting before you can turn automatic starts on."
                .into(),
        };
        let (button, turn) = match switch {
            Switch::On => ("Pause", Turn::Pause),
            Switch::Paused => ("Resume", Turn::Resume),
            Switch::Off => ("Turn on", Turn::On),
        };
        let key = format!("automation-{}", switch.word().to_lowercase());
        let mut row = div()
            .debug_selector(move || key.clone())
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .pt(px(SPACE_3))
            .border_t_1()
            .border_color(paint::LINE)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .font_weight(W_LABEL)
                                    .text_color(rgb(INK_STRONG))
                                    .child("Automatic starts"),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "automation-hint".into())
                                    .text_size(px(FS_META))
                                    .text_color(rgb(INK_SUBTLE))
                                    .child(SharedString::from(hint)),
                            ),
                    )
                    .child(
                        ferrite_ui::components::secondary_button(
                            "automation-turn",
                            None,
                            button,
                            cx,
                        )
                        .debug_selector(|| "automation-turn".into())
                        .disabled(!can)
                        .on_click(cx.listener(
                            move |view, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                view.turn_automation(turn, cx);
                            },
                        )),
                    ),
            );
        if let Some(error) = &automation.error {
            row = row.child(
                div()
                    .debug_selector(|| "automation-error".into())
                    .flex()
                    .items_center()
                    .gap(px(SPACE_1))
                    .text_size(px(FS_META))
                    .text_color(rgb(BLOCKED))
                    .child(ferrite_ui::icons::icon(
                        ferrite_ui::icons::WARNING,
                        ROW_ICON,
                        BLOCKED,
                    ))
                    .child(SharedString::from(format!(
                        "The last check failed: {error}. Ferrite tries again at the next check."
                    ))),
            );
        }
        if !automation.backlog.is_empty() {
            let count = automation.backlog.len();
            let thing = match (saved.map(|(_, kind)| kind), count) {
                (Some(StartKind::GithubPr), 1) => "pull request already matches",
                (Some(StartKind::GithubPr), _) => "pull requests already match",
                (_, 1) => "issue already matches",
                _ => "issues already match",
            };
            row = row.child(
                div()
                    .debug_selector(|| "automation-backlog".into())
                    .flex()
                    .flex_col()
                    .gap(px(SPACE_2))
                    .p(px(SPACE_2))
                    .rounded(px(R_CONTROL))
                    .bg(paint::RAISED)
                    .child(SharedString::from(format!(
                        "{count} {thing}. Start a task for each, or only for new ones from now on?"
                    )))
                    .child(
                        div()
                            .flex()
                            .gap(px(SPACE_2))
                            .child(
                                crate::project_editor::primary_button(
                                    "backlog-new-only",
                                    "New ones only",
                                    false,
                                    cx,
                                )
                                .debug_selector(|| "backlog-new-only".into())
                                .on_click(cx.listener(
                                    |view, _: &ClickEvent, _, cx| {
                                        cx.stop_propagation();
                                        view.answer_backlog(false, cx);
                                    },
                                )),
                            )
                            .child(
                                ferrite_ui::components::secondary_button(
                                    "backlog-take",
                                    None,
                                    "Take them",
                                    cx,
                                )
                                .debug_selector(|| "backlog-take".into())
                                .on_click(cx.listener(
                                    |view, _: &ClickEvent, _, cx| {
                                        cx.stop_propagation();
                                        view.answer_backlog(true, cx);
                                    },
                                )),
                            ),
                    ),
            );
        }
        row
    }
}
