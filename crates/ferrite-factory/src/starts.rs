//! A Workflow's "Tasks start" settings on the Workflows surface (#225,
//! ADR 0037): the one picker of five sources, each showing only its own
//! settings, and the switch for automatic starts, apart from Save.
//!
//! The start is part of the draft: every edit is one `edit_start` ask
//! carrying the whole start, so what was typed for one source survives
//! choosing another. Repositories and labels are listed from the
//! operator's `gh` login where the Server runs, never typed.
//!
//! What the window keeps of it lives in a view of its own (ADR 0041),
//! `StartsView`: the lists the login gave and the ones still out. The
//! Factory holds it. Its render is the inspector showing the start,
//! reading the shown Workflow's start through a `Source` the window makes
//! over itself; what a press asks it says as a `StartsEvent`. Whether the
//! start shows stays the Workflows editor's, and every edit, asking the
//! login for a list and the automatic starts' switch stay the window's,
//! since they are Server calls; the window's pump has the view take the
//! answers.

use std::collections::HashMap;
use std::sync::mpsc::Receiver;

use ferrite_core::factory_board::{
    self, Missed, Overlap, Recurrence, Start, StartKind, Switch, Trigger, Turn, WorkflowId,
};
use ferrite_core::protocol::GitHubAsk;
use ferrite_pane::composer::Composer;
use ferrite_ui::source::Source;
use gpui::component::Disableable as _;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, AnyElement, ClickEvent, Context, Div, Entity, EventEmitter, Focusable,
    MouseDownEvent, Pixels, Point, SharedString, Stateful, Window,
};

use crate::field::{field_input, Field};

/// A start picker's row.
#[derive(Clone, Debug, PartialEq)]
pub enum StartPick {
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
pub enum Listing {
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
pub fn script_command(executable: &str, workflow: &str) -> String {
    format!("\"{executable}\" task submit {workflow} \"<identity>\" \"<instructions>\"")
}

/// This window's own `ferrite`, which is also its CLI.
fn ferrite_executable() -> String {
    std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "ferrite".into())
}

/// What the start's inspector draws from the window: the shown Workflow's
/// start as the draft holds it, its automatic starts and the start's two
/// fields.
#[derive(Clone)]
pub struct StartsInputs {
    pub workflow: WorkflowId,
    pub start: Start,
    /// The latest saved version and the start it saved.
    pub saved: Option<(u32, StartKind)>,
    pub switch: Switch,
    pub due_ms: Option<i64>,
    pub waiting: bool,
    pub error: Option<String>,
    pub backlog: usize,
    pub schedule_instructions: Option<Entity<Composer>>,
    pub extra_instructions: Option<Entity<Composer>>,
}

/// What the start's inspector asks of the window.
pub enum StartsEvent {
    /// A picker's button: open its rows under it.
    Picks {
        picks: Vec<(ferrite_ui::menu::Item, StartPick)>,
        trigger: crate::picker::Trigger,
        at: Point<Pixels>,
    },
    /// A segment chosen.
    Pick(StartPick),
    /// A press on one of the start's fields: it holds the keyboard now.
    Field(Field),
    /// The automatic starts' switch.
    Turn(Turn),
    /// The backlog's answer: take what already matched, or not.
    Backlog(bool),
}

/// A Factory's starts: what the GitHub login listed for a GitHub start's
/// pickers, and the lists still out.
pub struct StartsView {
    /// The shown Workflow's start, as the window has it now.
    inputs: Source<StartsInputs>,
    repositories: Listing,
    labels: HashMap<String, Listing>,
}

impl EventEmitter<StartsEvent> for StartsView {}

impl StartsView {
    pub fn new(inputs: Source<StartsInputs>) -> Self {
        Self {
            inputs,
            repositories: Listing::Unasked,
            labels: HashMap::new(),
        }
    }

    /// Whether `ask`'s list was never asked for.
    pub fn unasked(&self, ask: &GitHubAsk) -> bool {
        let listing = match ask {
            GitHubAsk::Repositories => Some(&self.repositories),
            GitHubAsk::Labels { repository } => self.labels.get(repository),
            GitHubAsk::Unknown => return false,
        };
        matches!(listing, None | Some(Listing::Unasked))
    }

    /// Wait for `ask`'s list, or keep why it could not be asked.
    pub fn listen(
        &mut self,
        ask: GitHubAsk,
        asked: std::io::Result<Receiver<std::io::Result<Vec<String>>>>,
        cx: &mut Context<Self>,
    ) {
        let listing = match asked {
            Ok(pending) => Listing::Pending(pending),
            Err(error) => Listing::Failed(error.to_string()),
        };
        match ask {
            GitHubAsk::Repositories => self.repositories = listing,
            GitHubAsk::Labels { repository } => {
                self.labels.insert(repository, listing);
            }
            GitHubAsk::Unknown => {}
        }
        cx.notify();
    }

    /// Take each list that answered.
    pub fn listed(&mut self, cx: &mut Context<Self>) {
        let mut heard = false;
        for listing in std::iter::once(&mut self.repositories).chain(self.labels.values_mut()) {
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

    fn kind_picks(start: &Start) -> Vec<(ferrite_ui::menu::Item, StartPick)> {
        StartKind::ALL
            .into_iter()
            .map(|kind| {
                (
                    ferrite_ui::menu::Item::new(kind.label())
                        .detail(kind.detail())
                        .checked(start.kind == kind),
                    StartPick::Kind(kind),
                )
            })
            .collect()
    }

    fn recurrence_picks(start: &Start) -> Vec<(ferrite_ui::menu::Item, StartPick)> {
        Recurrence::ALL
            .into_iter()
            .map(|recurrence| {
                let mut item = ferrite_ui::menu::Item::new(recurrence.label())
                    .checked(start.schedule.recurrence == recurrence);
                if recurrence == Recurrence::Weekdays {
                    item = item.detail("Mon to Fri");
                }
                (item, StartPick::Recurrence(recurrence))
            })
            .collect()
    }

    fn weekday_picks(start: &Start) -> Vec<(ferrite_ui::menu::Item, StartPick)> {
        factory_board::WEEKDAYS
            .iter()
            .enumerate()
            .map(|(day, name)| {
                let day = day as u8;
                (
                    ferrite_ui::menu::Item::new(*name).checked(start.schedule.weekday == day),
                    StartPick::Weekday(day),
                )
            })
            .collect()
    }

    fn interval_picks(start: &Start) -> Vec<(ferrite_ui::menu::Item, StartPick)> {
        factory_board::INTERVAL_HOURS
            .into_iter()
            .map(|hours| {
                let plural = if hours == 1 { "" } else { "s" };
                (
                    ferrite_ui::menu::Item::new(format!("{hours} hour{plural}"))
                        .checked(start.schedule.interval_hours == hours),
                    StartPick::Interval(hours),
                )
            })
            .collect()
    }

    fn time_picks(start: &Start) -> Vec<(ferrite_ui::menu::Item, StartPick)> {
        times()
            .into_iter()
            .map(|time| {
                (
                    ferrite_ui::menu::Item::new(time.clone())
                        .checked(start.schedule.time.as_deref() == Some(time.as_str())),
                    StartPick::Time(time),
                )
            })
            .collect()
    }

    fn repository_picks(&self, start: &Start) -> Vec<(ferrite_ui::menu::Item, StartPick)> {
        self.repositories
            .names()
            .iter()
            .map(|name| {
                (
                    ferrite_ui::menu::Item::new(name.clone())
                        .checked(start.github.repository.as_deref() == Some(name.as_str())),
                    StartPick::Repository(name.clone()),
                )
            })
            .collect()
    }

    fn label_picks(&self, start: &Start) -> Vec<(ferrite_ui::menu::Item, StartPick)> {
        let Some(listing) = start
            .github
            .repository
            .as_ref()
            .and_then(|repository| self.labels.get(repository))
        else {
            return Vec::new();
        };
        listing
            .names()
            .iter()
            .map(|name| {
                (
                    ferrite_ui::menu::Item::new(name.clone())
                        .checked(start.github.label.as_deref() == Some(name.as_str())),
                    StartPick::Label(name.clone()),
                )
            })
            .collect()
    }

    fn trigger_picks(start: &Start) -> Vec<(ferrite_ui::menu::Item, StartPick)> {
        Trigger::offered(start.kind)
            .iter()
            .map(|trigger| {
                (
                    ferrite_ui::menu::Item::new(trigger.label())
                        .checked(start.github.trigger == *trigger),
                    StartPick::Trigger(*trigger),
                )
            })
            .collect()
    }

    // ----------------------------------------------------------- drawing

    /// A picker's button: what it holds, opening its rows; disabled, it
    /// opens nothing.
    fn start_chooser(
        selector: &'static str,
        text: String,
        flagged: bool,
        disabled: bool,
        picks: Vec<(ferrite_ui::menu::Item, StartPick)>,
        cx: &mut Context<Self>,
    ) -> Div {
        use gpui::component::Disableable;
        let picks = std::rc::Rc::new(picks);
        let look = ferrite_ui::controls::Look {
            flagged,
            full_width: true,
            disabled,
        };
        let trigger = crate::picker::Trigger::default();
        div().min_w_0().child(
            ferrite_ui::controls::chooser(selector, "", text.into(), look, cx)
                .disabled(disabled)
                .relative()
                .child(trigger.probe())
                .debug_selector(move || {
                    if flagged {
                        format!("{selector}-flagged")
                    } else {
                        selector.to_string()
                    }
                })
                .when(!disabled, |chooser| {
                    chooser.on_click(cx.listener(move |_, event: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        cx.emit(StartsEvent::Picks {
                            picks: (*picks).clone(),
                            trigger: trigger.clone(),
                            at: event.position(),
                        });
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
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.emit(StartsEvent::Pick(pick.clone()))
                    })),
            );
        }
        row
    }

    /// One of the start's text fields; a press on it gives it the
    /// keyboard.
    fn field(
        field: Field,
        label: &'static str,
        composer: Option<&Entity<Composer>>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let composer = composer?.clone();
        let focused = composer.read(cx).focus_handle(cx).is_focused(window);
        let chosen = field.clone();
        Some(field_input(
            &field,
            label,
            composer,
            focused,
            cx.listener(move |_, _: &MouseDownEvent, _, cx| {
                cx.emit(StartsEvent::Field(chosen.clone()))
            }),
        ))
    }

    /// The inspector, showing the start: the picker, the chosen source's
    /// own settings and the automatic starts' switch.
    fn inspector(
        &self,
        inputs: &StartsInputs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        use ferrite_ui::theme::*;
        let start = inputs.start.clone();
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
                Self::kind_picks(&start),
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
                                factory_board::WEEKDAYS[usize::from(schedule.weekday.min(6))].into(),
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
                    .map(|at| format!(" Next: {}.", factory_board::when_label(at)))
                    .unwrap_or_default();
                body = body
                    .child(row)
                    .child(meta(
                        "start-schedule-when",
                        format!(
                            "Host-local time \u{b7} {} (this computer's time zone). {}.{next}",
                            factory_board::zone_label(now),
                            schedule.describe()
                        ),
                    ))
                    .children(Self::field(
                        Field::ScheduleInstructions(inputs.workflow.clone()),
                        "Task instructions for each run",
                        inputs.schedule_instructions.as_ref(),
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
                body = body.child(self.github_settings(inputs, &start, window, cx));
            }
            StartKind::External => {
                let command = script_command(&ferrite_executable(), &inputs.workflow);
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
        body.child(Self::automation_row(inputs, cx))
    }

    /// A GitHub start's settings: repository, trigger and label from the
    /// login, the sentence that says what is checked, what each Task
    /// receives, and its extra instructions.
    fn github_settings(
        &self,
        inputs: &StartsInputs,
        start: &Start,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        use ferrite_ui::theme::*;
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
        let repository_wait = self.repositories.wait("repositories");
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
                .and_then(|repository| self.labels.get(repository));
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
            .children(Self::field(
                Field::ExtraInstructions(inputs.workflow.clone()),
                "Extra instructions for every task (optional)",
                inputs.extra_instructions.as_ref(),
                window,
                cx,
            ))
    }

    /// Automatic starts: the switch, the version it uses, the last
    /// check's failure, and the question over a backlog.
    fn automation_row(inputs: &StartsInputs, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let saved = inputs.saved;
        let switch = inputs.switch;
        let can = saved.is_some_and(|(_, kind)| kind.automatic());
        if !can && !inputs.start.kind.automatic() {
            return div();
        }
        let hint = match saved {
            Some((version, kind)) if kind.automatic() => {
                let mut hint = format!(
                    "{}. Uses version {version}. Saving a version does not turn this on.",
                    switch.word()
                );
                if let Some(due) = inputs.due_ms.filter(|_| switch == Switch::On) {
                    hint.push_str(&format!(" Next run: {}.", factory_board::when_label(due)));
                }
                if inputs.waiting {
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
                            move |_, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                cx.emit(StartsEvent::Turn(turn));
                            },
                        )),
                    ),
            );
        if let Some(error) = &inputs.error {
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
        if inputs.backlog > 0 {
            let count = inputs.backlog;
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
                                ferrite_ui::form::primary_button(
                                    "backlog-new-only",
                                    "New ones only",
                                    false,
                                    cx,
                                )
                                .debug_selector(|| "backlog-new-only".into())
                                .on_click(cx.listener(
                                    |_, _: &ClickEvent, _, cx| {
                                        cx.stop_propagation();
                                        cx.emit(StartsEvent::Backlog(false));
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
                                    |_, _: &ClickEvent, _, cx| {
                                        cx.stop_propagation();
                                        cx.emit(StartsEvent::Backlog(true));
                                    },
                                )),
                            ),
                    ),
            );
        }
        row
    }
}

impl Render for StartsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(inputs) = self.inputs.read(&(), cx) else {
            return div().id("inspector");
        };
        self.inspector(&inputs, window, cx)
    }
}

#[cfg(test)]
mod tests {
    use ferrite_ui::testing::{click, mount, record, shows};
    use gpui::{TestAppContext, VisualTestContext};

    use super::*;

    /// The shown Workflow's start as the window keeps it: none while the
    /// inspector shows something else.
    struct Shown(Option<StartsInputs>);

    /// The start's inspector over `start`, unsaved and switched off, its
    /// two fields made as the window makes them, tall enough to show every
    /// setting.
    fn inspector(
        start: Start,
        cx: &mut TestAppContext,
    ) -> (Entity<StartsView>, Entity<Shown>, &mut VisualTestContext) {
        let shown = cx.new(|cx| {
            Shown(Some(StartsInputs {
                workflow: "w-1".into(),
                start,
                saved: None,
                switch: Switch::Off,
                due_ms: None,
                waiting: false,
                error: None,
                backlog: 0,
                schedule_instructions: Some(cx.new(Composer::new)),
                extra_instructions: Some(cx.new(Composer::new)),
            }))
        });
        let inputs = Source::of(&shown, |shown: &Shown, _: &(), _| shown.0.clone());
        let (view, cx) = mount(cx, |_, _| StartsView::new(inputs));
        cx.simulate_resize(gpui::size(px(480.), px(2400.)));
        cx.run_until_parked();
        (view, shown, cx)
    }

    fn kind(kind: StartKind) -> Start {
        Start {
            kind,
            ..Start::default()
        }
    }

    /// The window changes what the inspector shows.
    fn change(shown: &Entity<Shown>, cx: &mut VisualTestContext, change: impl FnOnce(&mut StartsInputs)) {
        shown.update(cx, |shown, cx| {
            change(shown.0.as_mut().unwrap());
            cx.notify();
        });
        cx.run_until_parked();
    }

    fn said(event: &StartsEvent) -> String {
        match event {
            StartsEvent::Picks { picks, .. } => {
                let rows: Vec<String> = picks
                    .iter()
                    .map(|(item, pick)| {
                        let mark = if item.checked { "*" } else { "" };
                        format!("{mark}{} {pick:?}", item.label)
                    })
                    .collect();
                format!("picks {}", rows.join(", "))
            }
            StartsEvent::Pick(pick) => format!("pick {pick:?}"),
            StartsEvent::Field(field) => format!("field {}", field.key()),
            StartsEvent::Turn(turn) => format!("turn {turn:?}"),
            StartsEvent::Backlog(take) => format!("backlog {take}"),
        }
    }

    fn text(cx: &mut VisualTestContext, selector: &str) -> String {
        cx.update(|window, _| window.debug_text(selector)).unwrap_or_default().join(" ")
    }

    /// A manual start shows the one picker of five sources and nothing of
    /// its own: no settings, no switch.
    #[gpui::test]
    fn a_manual_start_offers_the_five_sources(cx: &mut TestAppContext) {
        let (view, _shown, cx) = inspector(kind(StartKind::Manual), cx);
        let heard = record(cx, &view, said);
        assert!(shows(cx, "inspector-start"));
        assert!(text(cx, "start-kind-note").starts_with("You, or the coordinator"));
        assert!(!shows(cx, "start-recurrence"), "manual has no settings");
        assert!(!shows(cx, "automation-off"), "nor a switch");
        click(cx, "start-kind");
        assert_eq!(
            heard.take(),
            [
                "picks *Manually Kind(Manual), On a schedule Kind(Schedule), \
                 From a GitHub issue Kind(GithubIssue), \
                 From a GitHub pull request Kind(GithubPr), \
                 From your own script Kind(External)"
            ]
        );
    }

    /// A schedule shows only its own settings, flags the time it lacks,
    /// and says each choice, a press on its field and each picker's rows.
    #[gpui::test]
    fn a_schedule_shows_its_own_settings(cx: &mut TestAppContext) {
        let (view, shown, cx) = inspector(kind(StartKind::Schedule), cx);
        let heard = record(cx, &view, said);
        for selector in [
            "start-recurrence",
            "start-time-flagged",
            "start-schedule-when",
            "start-schedule-instructions",
            "start-missed-0-on",
            "start-overlap-0-on",
            "automation-off",
        ] {
            assert!(shows(cx, selector), "{selector}");
        }
        assert!(text(cx, "start-kind-note").starts_with("You can always start"));
        assert!(!shows(cx, "start-repository-flagged"), "not GitHub's");
        assert!(!shows(cx, "start-weekday") && !shows(cx, "start-interval"));
        click(cx, "start-overlap-1");
        click(cx, "start-missed-1");
        click(cx, "start-schedule-instructions");
        assert_eq!(
            heard.take(),
            [
                "pick Overlap(Queue)",
                "pick Missed(RunOnce)",
                "field start-schedule-instructions"
            ]
        );
        click(cx, "start-time-flagged");
        let times = heard.take().join("");
        assert!(times.starts_with("picks 00:00 Time(\"00:00\"), 00:30 Time(\"00:30\")"));
        assert!(times.contains(", 09:00 Time(\"09:00\"), "));
        assert_eq!(times.matches("Time(").count(), 48, "every half hour");
        click(cx, "start-recurrence");
        assert!(heard.take()[0].starts_with("picks *Every day Recurrence(Daily), Weekdays"));

        change(&shown, cx, |inputs| {
            inputs.start.schedule.time = Some("09:00".into());
            inputs.start.schedule.recurrence = Recurrence::Weekly;
            inputs.start.schedule.weekday = 3;
        });
        assert!(shows(cx, "start-time"), "no longer flagged");
        assert_eq!(text(cx, "start-weekday"), "Wednesday");
        click(cx, "start-weekday");
        assert!(heard.take()[0].contains("*Wednesday Weekday(3)"));
        change(&shown, cx, |inputs| {
            inputs.start.schedule.recurrence = Recurrence::Interval;
        });
        assert!(shows(cx, "start-interval"), "every few hours");
        assert!(!shows(cx, "start-time") && !shows(cx, "start-weekday"), "has no time");
        click(cx, "start-interval");
        assert!(heard.take()[0].contains("*4 hours Interval(4)"));
    }

    /// A GitHub start's repositories and labels come from the login: a
    /// picker waits, disabled, until its list answers, and says why it
    /// could not list one.
    #[gpui::test]
    fn a_github_start_picks_from_what_the_login_lists(cx: &mut TestAppContext) {
        let (view, shown, cx) = inspector(kind(StartKind::GithubIssue), cx);
        let heard = record(cx, &view, said);
        for selector in [
            "start-repository-flagged",
            "start-trigger",
            "start-label-flagged",
            "start-github-sentence",
            "start-receives",
            "start-extra-instructions",
        ] {
            assert!(shows(cx, selector), "{selector}");
        }
        assert!(!shows(cx, "start-recurrence"), "not the schedule's");
        assert!(text(cx, "start-receives").contains("A link back to the issue"));
        assert_eq!(text(cx, "start-repository-note"), "Listing repositories\u{2026}");
        assert!(!shows(cx, "start-label-note"), "no repository yet");
        click(cx, "start-repository-flagged");
        assert!(heard.take().is_empty(), "nothing to pick while it lists");

        let repositories = GitHubAsk::Repositories;
        assert!(view.read_with(cx, |view, _| view.unasked(&repositories)));
        let (answer, pending) = std::sync::mpsc::channel();
        view.update(cx, |view, cx| view.listen(repositories.clone(), Ok(pending), cx));
        assert!(!view.read_with(cx, |view, _| view.unasked(&repositories)), "asked once");
        view.update(cx, |view, cx| view.listed(cx));
        cx.run_until_parked();
        assert_eq!(text(cx, "start-repository-note"), "Listing repositories\u{2026}", "still out");
        answer.send(Ok(vec!["acme/app".into(), "acme/site".into()])).unwrap();
        view.update(cx, |view, cx| view.listed(cx));
        cx.run_until_parked();
        assert!(text(cx, "start-repository-note").starts_with("The repositories your GitHub CLI"));
        click(cx, "start-repository-flagged");
        assert_eq!(
            heard.take(),
            ["picks acme/app Repository(\"acme/app\"), acme/site Repository(\"acme/site\")"]
        );

        change(&shown, cx, |inputs| {
            inputs.start.github.repository = Some("acme/gone".into());
        });
        assert!(shows(cx, "start-repository"), "no longer flagged");
        assert_eq!(text(cx, "start-label-note"), "Listing labels\u{2026}");
        let gone = GitHubAsk::Labels {
            repository: "acme/gone".into(),
        };
        assert!(view.read_with(cx, |view, _| view.unasked(&gone)));
        view.update(cx, |view, cx| {
            view.listen(gone.clone(), Err(std::io::Error::other("gh: not logged in")), cx)
        });
        cx.run_until_parked();
        assert_eq!(text(cx, "start-label-note"), "Could not list labels: gh: not logged in");
        assert!(!view.read_with(cx, |view, _| view.unasked(&gone)), "a failure is an answer");
        let listed = [("acme/app", vec!["bug", "ready-for-agent"]), ("acme/site", vec!["docs"])];
        for (repository, labels) in listed {
            let (answer, pending) = std::sync::mpsc::channel();
            let ask = GitHubAsk::Labels {
                repository: repository.into(),
            };
            view.update(cx, |view, cx| view.listen(ask, Ok(pending), cx));
            answer.send(Ok(labels.into_iter().map(String::from).collect())).unwrap();
        }
        view.update(cx, |view, cx| view.listed(cx));
        change(&shown, cx, |inputs| {
            inputs.start.github.repository = Some("acme/app".into());
            inputs.start.github.label = Some("bug".into());
        });
        assert!(shows(cx, "start-label") && !shows(cx, "start-label-note"));
        click(cx, "start-label");
        change(&shown, cx, |inputs| {
            inputs.start.github.repository = Some("acme/site".into());
            inputs.start.github.label = None;
        });
        click(cx, "start-label-flagged");
        assert_eq!(
            heard.take(),
            [
                "picks *bug Label(\"bug\"), ready-for-agent Label(\"ready-for-agent\")",
                "picks docs Label(\"docs\")"
            ],
            "each repository's own"
        );
        click(cx, "start-trigger");
        assert_eq!(
            heard.take(),
            ["picks *Has a label Trigger(Labelled), Is opened Trigger(Opened)"],
            "an issue cannot request a review"
        );
        click(cx, "start-extra-instructions");
        assert_eq!(heard.take(), ["field start-extra-instructions"]);
    }

    /// A pull request's start offers a review request, which has no label.
    #[gpui::test]
    fn a_pull_requests_start_can_wait_for_a_review_request(cx: &mut TestAppContext) {
        let (view, shown, cx) = inspector(kind(StartKind::GithubPr), cx);
        let heard = record(cx, &view, said);
        assert!(text(cx, "start-receives").contains("The pull request as the task's PR"));
        click(cx, "start-trigger");
        assert!(heard.take()[0].ends_with(", Requests my review Trigger(ReviewRequested)"));
        change(&shown, cx, |inputs| {
            inputs.start.github.trigger = Trigger::ReviewRequested;
        });
        assert!(!shows(cx, "start-label") && !shows(cx, "start-label-flagged"));
    }

    /// A script's start says the command it runs, through this `ferrite`.
    #[gpui::test]
    fn a_scripts_start_says_its_command(cx: &mut TestAppContext) {
        let (_view, _shown, cx) = inspector(kind(StartKind::External), cx);
        let command = text(cx, "start-script-command");
        assert!(command.ends_with("\" task submit w-1 \"<identity>\" \"<instructions>\""), "{command}");
        assert!(shows(cx, "start-script-note") && shows(cx, "automation-off"));
    }

    #[test]
    fn a_script_submits_through_the_real_ferrite() {
        assert_eq!(
            script_command("/Apps/Ferrite/ferrite", "w-1"),
            "\"/Apps/Ferrite/ferrite\" task submit w-1 \"<identity>\" \"<instructions>\""
        );
    }

    /// The automatic starts' switch: nothing to turn on until a version
    /// with an automatic start is saved; then on, paused and resumed, each
    /// said; a failed check is said, and a backlog asks once.
    #[gpui::test]
    fn the_switch_says_each_turn_and_the_backlog_asks(cx: &mut TestAppContext) {
        let (view, shown, cx) = inspector(kind(StartKind::GithubIssue), cx);
        let heard = record(cx, &view, said);
        assert!(text(cx, "automation-hint").starts_with("Save a version with this start"));
        click(cx, "automation-turn");
        assert!(heard.take().is_empty(), "nothing saved to turn on");

        change(&shown, cx, |inputs| inputs.saved = Some((2, StartKind::GithubIssue)));
        assert_eq!(
            text(cx, "automation-hint"),
            "Off. Uses version 2. Saving a version does not turn this on."
        );
        click(cx, "automation-turn");
        change(&shown, cx, |inputs| {
            inputs.switch = Switch::On;
            inputs.waiting = true;
        });
        assert!(shows(cx, "automation-on"));
        assert!(text(cx, "automation-hint").ends_with("A run waits for the last one to finish."));
        click(cx, "automation-turn");
        change(&shown, cx, |inputs| inputs.switch = Switch::Paused);
        assert!(shows(cx, "automation-paused"));
        click(cx, "automation-turn");
        assert_eq!(heard.take(), ["turn On", "turn Pause", "turn Resume"]);

        assert!(!shows(cx, "automation-error") && !shows(cx, "automation-backlog"));
        change(&shown, cx, |inputs| {
            inputs.error = Some("gh: not logged in".into());
            inputs.backlog = 2;
        });
        assert_eq!(
            text(cx, "automation-error"),
            "The last check failed: gh: not logged in. Ferrite tries again at the next check."
        );
        assert!(text(cx, "automation-backlog").starts_with("2 issues already match."));
        click(cx, "backlog-new-only");
        click(cx, "backlog-take");
        assert_eq!(heard.take(), ["backlog false", "backlog true"]);
        change(&shown, cx, |inputs| {
            inputs.saved = Some((2, StartKind::GithubPr));
            inputs.backlog = 1;
        });
        assert!(text(cx, "automation-backlog").starts_with("1 pull request already matches."));
    }

    /// With something else inspected, it draws nothing.
    #[gpui::test]
    fn with_the_start_not_shown_it_draws_nothing(cx: &mut TestAppContext) {
        let (_view, shown, cx) = inspector(kind(StartKind::Schedule), cx);
        assert!(shows(cx, "inspector-start"));
        shown.update(cx, |shown, cx| {
            shown.0 = None;
            cx.notify();
        });
        cx.run_until_parked();
        assert!(!shows(cx, "inspector-start") && !shows(cx, "start-kind"));
    }
}
