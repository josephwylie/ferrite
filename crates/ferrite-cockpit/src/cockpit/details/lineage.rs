//! The details card's Lineage, as T3's Thread details draw it: for the
//! Subject a Pane shows, the parent it came from, the agents it has
//! running, and the ones that finished, folded under one counted
//! `Previous agents` row. Main's also has T3's rows for a Thread on another
//! machine (ADR 0047): the Thread there that delegated it, first, and
//! each Delegate it started, after its own agents in the order it started
//! them (T3 lists a projection's subagents as they were recorded).

use super::super::delegates::{Delegator, Serving};
use super::super::subagents::{agent_name, parent_of, state_word};
use super::*;
use ferrite_core::activity::{ActivityView, AgentStatus, Subject, SubjectView};
use ferrite_core::delegate::{DelegateId, Delegated, Delegating, Status};
use ferrite_core::server::ServerId;
use ferrite_core::TurnOutcome;
use ferrite_ui::icons::{self, icon};
use std::collections::HashMap;

/// One Lineage row: what the card leads to.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Relative {
    pub(super) leads: Leads,
    pub(super) title: String,
    /// At the right: how long it has run or ran, else its state word.
    pub(super) meta: String,
    /// Whether `meta` is a clock, which T3 sets in tabular figures at the
    /// regular weight.
    pub(super) timed: bool,
    pub(super) state: Dot,
    /// The provider it runs on when not the Thread's: a Delegate's, there.
    pub(super) provider: Option<Provider>,
    /// What the pointer on it reads (T3's `LineageTooltipContent`): a row
    /// for a Thread on another machine says where it opens, and what holds
    /// it up.
    pub(super) tooltip: Option<String>,
}

/// Where a row's press goes.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Leads {
    /// Main or a Subagent, in this Pane.
    Subject(Subject),
    /// One of Main's Delegates, by its Server and its Thread there, which
    /// `cancel` stops while it is open. `reached`: a machine of this window
    /// answers as that Server, so the row opens it (T3 opens it only while
    /// connected to that environment).
    Elsewhere {
        server: ServerId,
        thread: ThreadId,
        reached: bool,
        cancel: Option<DelegateId>,
    },
    /// The Thread on another machine that delegated this one, opened as
    /// `Elsewhere` is once its machine says it still has it
    /// (`CockpitView::open_delegator`).
    Delegator {
        server: ServerId,
        thread: ThreadId,
        reached: bool,
    },
}

/// The dot on a row's icon, T3's `ThreadRelationshipIcon`: blue while it
/// runs or waits, red failed, green done, muted for anything else (stopped).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Dot {
    Running,
    Failed,
    Done,
    Idle,
}

impl Dot {
    fn of(status: AgentStatus, fresh: bool) -> Self {
        match status {
            // How it ended outlives the Session that saw it end.
            AgentStatus::Failed | AgentStatus::NotFound => Self::Failed,
            AgentStatus::Idle => Self::Done,
            _ if !fresh => Self::Idle,
            AgentStatus::Working | AgentStatus::Pending | AgentStatus::Waiting => Self::Running,
            AgentStatus::Unknown => Self::Done,
            _ => Self::Idle,
        }
    }

    fn ink(self) -> u32 {
        match self {
            Self::Running => BLUE,
            Self::Failed => BLOCKED,
            Self::Done => GREEN,
            Self::Idle => ferrite_ui::theme::TEXT_FAINT,
        }
    }
}

/// How a parent stands (T3's `activityRunStatus`): at rest, as its last turn
/// ended, so a Thread stopped with Escape does not read Done.
fn parent_status(view: SubjectView<'_>) -> AgentStatus {
    match (view.status(), view.last_outcome()) {
        (AgentStatus::Idle, Some(TurnOutcome::Interrupted)) => AgentStatus::Interrupted,
        (AgentStatus::Idle, Some(TurnOutcome::Error(_))) => AgentStatus::Failed,
        (status, _) => status,
    }
}

/// A Subject's Lineage: the Thread that delegated it (Main's alone), its
/// parent (a Subagent's alone), then its own agents newest first and its
/// Delegates oldest first, running ones apart from finished ones.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Lineage {
    pub(super) delegated_from: Option<Relative>,
    pub(super) parent: Option<Relative>,
    pub(super) running: Vec<Relative>,
    pub(super) previous: Vec<Relative>,
    /// The heading's count (T3's `runningCount`): the running agents, and
    /// the Delegates whose status is Running.
    pub(super) running_count: usize,
}

/// What Main's Lineage draws of other machines, from the Thread's digest
/// (`ThreadFacts`), and which machine of this window answers as a Server.
pub(super) struct Elsewhere<'a, F: Fn(&ServerId) -> Option<Serving>> {
    pub(super) delegating: &'a Delegating,
    pub(super) delegated_from: Option<Delegator>,
    pub(super) serving: F,
}

/// `subject`'s Lineage in `activity`; Main, its parent, is `main_title`.
pub(super) fn lineage<F: Fn(&ServerId) -> Option<Serving>>(
    activity: ActivityView<'_>,
    subject: &Subject,
    main_title: &str,
    elsewhere: Elsewhere<'_, F>,
) -> Lineage {
    let agents = activity.children();
    let parent = parent_of(activity, subject).and_then(|parent| {
        let (title, view) = match &parent {
            Subject::Main => (main_title.to_owned(), activity.main()),
            Subject::Subagent(_) => (
                agents
                    .iter()
                    .find(|agent| agent.subject() == parent)
                    .map(|agent| agent_name(agent.info()))?,
                activity.subject(&parent)?,
            ),
        };
        let status = parent_status(view);
        Some(Relative {
            meta: state_word(status, view.fresh()).to_owned(),
            timed: false,
            state: Dot::of(status, view.fresh()),
            leads: Leads::Subject(parent),
            title,
            provider: None,
            tooltip: None,
        })
    });
    // An agent's clock is its spawning call's, in its parent's timings.
    let elapsed: HashMap<Subject, std::time::Duration> = activity
        .invocations()
        .filter_map(|(call, agent)| {
            let parent = parent_of(activity, &agent.subject())?;
            let timing = *activity.subject(&parent)?.timings().get(&call)?;
            Some((agent.subject(), timing.elapsed()))
        })
        .collect();
    let mut own: Vec<_> = agents
        .into_iter()
        .filter(|agent| parent_of(activity, &agent.subject()).as_ref() == Some(subject))
        .collect();
    own.sort_by_key(|agent| std::cmp::Reverse(agent.discovered()));
    let mut lineage = Lineage {
        parent,
        ..Lineage::default()
    };
    for agent in own {
        let state = Dot::of(agent.status(), agent.fresh());
        let clock = elapsed
            .get(&agent.subject())
            .filter(|_| state != Dot::Failed)
            .map(|elapsed| crate::nav::working_for(*elapsed));
        let row = Relative {
            leads: Leads::Subject(agent.subject()),
            title: agent_name(agent.info()),
            timed: clock.is_some(),
            meta: clock.unwrap_or_else(|| state_word(agent.status(), agent.fresh()).to_owned()),
            state,
            provider: None,
            tooltip: None,
        };
        if agent.running() {
            lineage.running.push(row);
            lineage.running_count += 1;
        } else {
            lineage.previous.push(row);
        }
    }
    if *subject != Subject::Main {
        return lineage;
    }
    lineage.delegated_from = elsewhere.delegated_from.map(delegated_from);
    for delegate in &elsewhere.delegating.delegates {
        let row = delegate_row(delegate, (elsewhere.serving)(&delegate.server).is_some());
        lineage.running_count += usize::from(delegate.status == Status::Running);
        match delegate.status.ended() {
            false => lineage.running.push(row),
            true => lineage.previous.push(row),
        }
    }
    lineage
}

/// T3's `RemoteSubagentRow`: its title (else `Delegate`, where T3 says
/// `Subagent`), and `on pc · Running`, its dot in T3's status inks. One
/// held up says why under the pointer, in its Server's words: following
/// it, or a stop that has not reached it.
fn delegate_row(delegate: &Delegated, reached: bool) -> Relative {
    let title = delegate.title.clone().unwrap_or_else(|| "Delegate".to_owned());
    // T3's hint, pressable or not: this window has no read of a Thread
    // through another machine to offer instead (T3's `Read the thread on`).
    let hint = format!("Open on {}", delegate.machine);
    let problem = delegate.unstopped().or_else(|| delegate.problem.clone());
    Relative {
        tooltip: Some(
            [Some(title.clone()), Some(hint), problem]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        meta: format!("on {} \u{b7} {}", delegate.machine, status_label(delegate.status)),
        timed: false,
        state: match delegate.status {
            Status::Starting | Status::Running | Status::Waiting => Dot::Running,
            Status::Failed => Dot::Failed,
            Status::Completed => Dot::Done,
            Status::Cancelled | Status::Interrupted => Dot::Idle,
        },
        leads: Leads::Elsewhere {
            server: delegate.server.clone(),
            thread: delegate.thread,
            reached,
            cancel: (!delegate.status.ended()).then_some(delegate.id),
        },
        provider: delegate.provider,
        title,
    }
}

/// T3's `DelegatedFromRow`: the delegating Thread's title, and `on mac`
/// (`Delegator`).
fn delegated_from(from: Delegator) -> Relative {
    let hint = match from.reached {
        true => format!("Open parent agent on {}", from.machine),
        false => format!(
            "Delegated from {}. Connect to it to open the parent thread.",
            from.machine
        ),
    };
    Relative {
        tooltip: Some(format!("{}\n{hint}", from.title)),
        meta: format!("on {}", from.machine),
        timed: false,
        state: Dot::Idle,
        leads: Leads::Delegator {
            server: from.server,
            thread: from.thread,
            reached: from.reached,
        },
        provider: None,
        title: from.title,
    }
}

/// T3's `threadRelationshipStatusLabel` for a delegated task's status.
fn status_label(status: Status) -> &'static str {
    match status {
        Status::Starting => "Starting",
        Status::Running => "Running",
        Status::Waiting => "Waiting",
        Status::Completed => "Done",
        Status::Failed => "Failed",
        Status::Cancelled | Status::Interrupted => "Stopped",
    }
}

/// `Previous agents` opens on its first six; each `Show more` adds twelve.
const FIRST_PAGE: usize = 6;
const PAGE: usize = 12;

impl Lineage {
    /// The finished agents shown after `pages` presses of `Show more`, and
    /// how many the next press adds (none when all are shown).
    pub(super) fn previous_page(&self, pages: usize) -> (&[Relative], usize) {
        let shown = self.previous.len().min(FIRST_PAGE + PAGE * pages);
        (
            &self.previous[..shown],
            (self.previous.len() - shown).min(PAGE),
        )
    }

    pub(super) fn is_empty(&self) -> bool {
        self.delegated_from.is_none()
            && self.parent.is_none()
            && self.running.is_empty()
            && self.previous.is_empty()
    }

    /// How many rows it draws under its heading.
    pub(super) fn rows(&self, previous_open: bool, pages: usize) -> usize {
        let (shown, more) = self.previous_page(pages);
        usize::from(self.delegated_from.is_some())
            + usize::from(self.parent.is_some())
            + self.running.len()
            + match self.previous.len() {
                0 => 0,
                _ if previous_open => 1 + shown.len() + usize::from(more > 0),
                _ => 1,
            }
    }

    /// The section inside `block`: `Lineage · N running` (the heading,
    /// left out when only `Previous agents` is there to name itself), the
    /// Thread that delegated it, the parent, the running agents and
    /// Delegates, then `Previous agents`, which folds the finished ones a
    /// page at a time.
    pub(super) fn render(
        &self,
        mut block: Div,
        thread: ThreadKey,
        provider: Provider,
        previous_open: bool,
        pages: usize,
        cx: &mut Context<CockpitView>,
    ) -> Div {
        use super::super::surfaces::SurfaceAction;
        if self.delegated_from.is_some() || self.parent.is_some() || !self.running.is_empty() {
            let running = self.running_count;
            block = block.child(
                div()
                    .when(running > 0, |heading| {
                        heading.debug_selector(move || format!("details-lineage-running-{running}"))
                    })
                    .flex()
                    .items_center()
                    .h(px(32.))
                    .px(px(10.))
                    .text_size(px(11.))
                    .text_color(rgb(INK_SUBTLE))
                    .child(if running > 0 {
                        format!("Lineage \u{b7} {running} running")
                    } else {
                        "Lineage".to_owned()
                    }),
            );
        }
        if let Some(from) = &self.delegated_from {
            block = block.child(relative(
                "details-lineage-delegated-from".into(),
                from,
                (icons::CORNER_LEFT_UP, INK_SUBTLE),
                thread,
                cx,
            ));
        }
        if let Some(parent) = &self.parent {
            block = block.child(relative(
                "details-lineage-parent".into(),
                parent,
                (icons::CORNER_LEFT_UP, INK_SUBTLE),
                thread,
                cx,
            ));
        }
        let agent = |agent: &Relative, cx: &mut Context<CockpitView>| {
            let selector = match &agent.leads {
                Leads::Elsewhere { .. } | Leads::Delegator { .. } => {
                    format!("details-lineage-delegate-{}", agent.title)
                }
                Leads::Subject(_) => format!("details-lineage-agent-{}", agent.title),
            };
            let mark = ferrite_ui::provider::mark(agent.provider.unwrap_or(provider));
            relative(selector, agent, mark, thread, cx)
        };
        for running in &self.running {
            block = block.child(agent(running, cx));
        }
        if self.previous.is_empty() {
            return block;
        }
        let count = self.previous.len();
        let failed = self
            .previous
            .iter()
            .filter(|agent| agent.state == Dot::Failed)
            .count();
        let previous = div()
            .child(icon(icons::BOT, 16., INK_SUBTLE))
                .child(div().flex_1().min_w_0().truncate().child("Previous agents"))
                .when(failed > 0, |row| {
                    row.child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(12.))
                            .text_color(rgb(BLOCKED))
                            .child(format!("{failed} failed")),
                    )
                })
                .when(!previous_open, |row| {
                    row.child(
                        div()
                            .debug_selector(move || {
                                format!("details-lineage-previous-count-{count}")
                            })
                            .flex_shrink_0()
                            .text_size(px(12.))
                            .text_color(rgb(INK_SUBTLE))
                            .child(count.to_string()),
                    )
                })
                .child(ferrite_ui::components::fold_chevron(
                    "details-lineage-previous-chevron",
                    previous_open,
                    NAV_HEADING_CARET,
                    INK_SUBTLE,
                ));
        block = block.child(
            press("details-lineage-previous".into(), previous, cx).on_click(cx.listener(
                move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.apply_surface(thread, SurfaceAction::TogglePreviousAgents);
                    cx.notify();
                },
            )),
        );
        if !previous_open {
            return block;
        }
        let (shown, more) = self.previous_page(pages);
        for previous in shown {
            block = block.child(agent(previous, cx));
        }
        if more > 0 {
            let show_more = div()
                .text_color(rgb(INK_SUBTLE))
                .child(icon(icons::PLUS, 16., INK_SUBTLE))
                .child(format!("Show {more} more"));
            block = block.child(
                press(format!("details-lineage-show-more-{more}"), show_more, cx)
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        if let Some(index) = view.pane_for(thread) {
                            view.panes[index].details.show_more();
                            cx.notify();
                        }
                    })),
            );
        }
        block
    }
}

/// What an open Delegate's stop says: T3's `Stop subagent`, naming T3's
/// delegated task.
const STOP_TIP: &str = "Stop delegated task";

/// One row, pressed where it leads: a Subject in this Pane, or a Thread on
/// a machine this window reaches. A Thread on a machine it does not reach
/// is drawn alone, as T3 draws its row disabled. An open Delegate's row
/// has T3's stop at its right while the pointer is on it, over its meta.
fn relative(
    selector: String,
    relative: &Relative,
    (glyph, ink): (&'static str, u32),
    thread: ThreadKey,
    cx: &mut Context<CockpitView>,
) -> AnyElement {
    let group = SharedString::from(format!("{selector}-group"));
    let cancel = match &relative.leads {
        Leads::Elsewhere { cancel, .. } => *cancel,
        Leads::Subject(_) | Leads::Delegator { .. } => None,
    };
    let delegator = matches!(relative.leads, Leads::Delegator { .. });
    let body = row(relative, glyph, ink, cancel.map(|_| group.clone()));
    let tooltip = relative.tooltip.clone();
    let row: AnyElement = match relative.leads.clone() {
        Leads::Subject(subject) => {
            let parent = selector == "details-lineage-parent";
            press(selector.clone(), body, cx)
                .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    match parent {
                        true => view.select_subject(thread, subject.clone(), window, cx),
                        false => view.open_subagent(thread, subject.clone(), cx),
                    }
                }))
                .into_any_element()
        }
        Leads::Elsewhere {
            server,
            thread: there,
            reached: true,
            ..
        }
        | Leads::Delegator {
            server,
            thread: there,
            reached: true,
        } => div()
            .id(SharedString::from(format!("{selector}-tip")))
            .w_full()
            .when_some(tooltip, |row, tip| row.tooltip(ferrite_ui::menu::tooltip(tip)))
            .child(
                press(selector.clone(), body, cx).on_click(cx.listener(
                    move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        match delegator {
                            true => view.open_delegator(&server, there, cx),
                            false => view.open_elsewhere(&server, there, cx),
                        }
                    },
                )),
            )
            .into_any_element(),
        Leads::Elsewhere { reached: false, .. } | Leads::Delegator { reached: false, .. } => {
            let selector = selector.clone();
            div()
                .id(SharedString::from(selector.clone()))
                .debug_selector(move || selector.clone())
                .when_some(tooltip, |row, tip| row.tooltip(ferrite_ui::menu::tooltip(tip)))
                .w_full()
                .h(px(32.))
                .px(px(10.))
                .child(lineage_body(body))
                .into_any_element()
        }
    };
    let Some(delegate) = cancel else {
        return row;
    };
    let stop = format!("{selector}-stop");
    div()
        .relative()
        .group(group.clone())
        .child(row)
        .child(
            div()
                .absolute()
                .right(px(4.))
                .top(px(6.))
                .id(SharedString::from(format!("{stop}-tip")))
                .invisible()
                .group_hover(group, |style| style.visible())
                .tooltip(ferrite_ui::menu::tooltip(STOP_TIP))
                .child(
                    ferrite_ui::components::button(SharedString::from(stop.clone()))
                        .debug_selector(move || stop.clone())
                        .accessibility_label(STOP_TIP)
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(20.))
                        .child(icon(icons::STOP, 12., BLOCKED))
                        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            view.cancel_delegate(thread, delegate, cx)
                        })),
                ),
        )
        .into_any_element()
}

/// A Lineage row's press: the shared faded button, clear at rest and the
/// card's hover ground under the pointer, with `body` laid out in it.
fn press(selector: String, body: Div, cx: &gpui::App) -> gpui::component::button::Button {
    ferrite_ui::components::faded_button(
        SharedString::from(selector.clone()),
        rgba(0).into(),
        rgba(0xffffff14).into(),
        rgba(0xffffff1f).into(),
        rgba(0xffffffcc).into(),
        cx,
    )
    .debug_selector(move || selector.clone())
    .w_full()
    .h(px(32.))
    .px(px(10.))
    .rounded(px(8.))
    .child(lineage_body(body))
}

/// T3's Lineage row: a `text-sm` title in `foreground/85` and `text-2xs`
/// meta on `leading-4`.
const TITLE_INK: u32 = (INK_STRONG << 8) | 0xd9;
const TITLE_FS: f32 = 14.;
const META_FS: f32 = 11.;
const ROW_LH: f32 = 16.;

/// A row's body as a row lays it out. It sets T3's line height, since a kit
/// button's is the font size alone.
fn lineage_body(body: Div) -> Div {
    body.flex()
        .items_center()
        .w_full()
        .h_full()
        .min_w_0()
        .gap(px(10.))
        .text_size(px(13.))
        .line_height(px(ROW_LH))
        .font_weight(W_LABEL)
}

/// A row's body: `glyph` with its state dot, the title cut to fit, the
/// meta, in T3's destructive ink for a failure. Its meta gives way to a
/// stop under the pointer on `stop_group`.
fn row(relative: &Relative, glyph: &'static str, ink: u32, stop_group: Option<SharedString>) -> Div {
    div()
        .text_color(rgba(TITLE_INK))
        .child(
            div()
                .relative()
                .flex_shrink_0()
                .size(px(16.))
                .child(icon(glyph, 16., ink))
                // T3's `size-2 border-2 border-card` at `-right-1 -bottom-1`,
                // as two discs: a GPUI border leaves the fill's edge showing
                // round its outside.
                .child(
                    div()
                        .absolute()
                        .right(px(-4.))
                        .bottom(px(-4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(8.))
                        .rounded_full()
                        // The card's own ground, so the dot cuts into its icon.
                        .bg(rgb(0x111111))
                        .child(div().size(px(4.)).rounded_full().bg(rgb(relative.state.ink()))),
                ),
        )
        .child(
            div()
                .debug_selector({
                    let title = relative.title.clone();
                    move || format!("details-lineage-title-{title}")
                })
                .flex_1()
                .min_w_0()
                // Cut to fit by its width alone: `truncate`'s clip would take
                // the descenders, which reach below a 14px face's 16px line.
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(TITLE_FS))
                .child(relative.title.clone()),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(META_FS))
                .text_color(rgb(match relative.state {
                    Dot::Failed => BLOCKED,
                    _ => INK_SUBTLE,
                }))
                .when(relative.timed, |meta| {
                    ferrite_ui::components::tabular(meta.font_weight(W_BODY))
                })
                .when_some(stop_group, |meta, group| {
                    meta.group_hover(group, |style| style.invisible())
                })
                .child(relative.meta.clone()),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::activity::{Activity, ActivityInput};

    fn delegated(id: u32, title: Option<&str>, status: Status) -> Delegated {
        Delegated {
            id: DelegateId::new(id),
            server: ServerId::new("s-pc"),
            machine: "pc".into(),
            thread: ThreadId::new(7 + u64::from(id)),
            title: title.map(str::to_owned),
            provider: Some(Provider::Codex),
            status,
            problem: None,
            stop_owed: false,
            result: None,
            started_at: None,
            ended_at: None,
        }
    }

    /// `pc`, off screen, as this window saved it.
    fn pc() -> Serving {
        Serving {
            name: Some("pc (box)".into()),
            machine: ferrite_core::ThreadKey::fixture(1, 1).machine(),
        }
    }

    /// Main's Lineage with these Delegates, `pc` reached off screen as
    /// `pc (box)` when `reached`, delegated `from` another Thread.
    fn main_lineage(delegates: Vec<Delegated>, from: Option<Delegator>, reached: bool) -> Lineage {
        let activity = Activity::default();
        let delegating = Delegating {
            delegates,
            holds: true,
        };
        let serving = |_: &ServerId| reached.then(pc);
        lineage(
            activity.view(),
            &Subject::Main,
            "Main",
            Elsewhere {
                delegating: &delegating,
                delegated_from: from,
                serving,
            },
        )
    }

    /// A Server restart leaves every agent not fresh: how one ended still
    /// reads as it did, and what one was doing no longer claims to go on.
    #[test]
    fn how_an_agent_ended_outlives_a_restart() {
        let read = |status| (state_word(status, false), Dot::of(status, false));
        assert_eq!(read(AgentStatus::Interrupted), ("Stopped", Dot::Idle));
        assert_eq!(read(AgentStatus::Failed), ("Failed", Dot::Failed));
        assert_eq!(read(AgentStatus::Idle), ("Done", Dot::Done));
        assert_eq!(read(AgentStatus::Working), ("Unavailable", Dot::Idle));
    }

    /// A parent at rest reads as its last turn ended: stopped, not Done.
    #[test]
    fn a_parent_stopped_with_escape_reads_stopped() {
        let ended = |outcome| {
            let mut activity = Activity::default();
            activity.apply(ActivityInput::Connect { generation: 1 });
            activity.apply(ActivityInput::Main {
                input: ferrite_core::transcript::Input::Event(
                    ferrite_core::SessionEvent::TurnEnded { outcome, cost_usd: None },
                ),
                at: std::time::Instant::now(),
            });
            parent_status(activity.view().main())
        };
        assert_eq!(ended(TurnOutcome::Interrupted), AgentStatus::Interrupted);
        assert_eq!(ended(TurnOutcome::Error("x".into())), AgentStatus::Failed);
        assert_eq!(ended(TurnOutcome::Completed), AgentStatus::Idle);
    }

    /// T3's `RemoteSubagentRow`: the title (`Delegate` when it has none),
    /// `on pc · Running` in T3's status words, open ones after the agents
    /// and ended ones under `Previous agents`, each in the order they
    /// started; the heading counts the running ones alone.
    #[test]
    fn a_delegate_is_t3s_remote_row_in_t3s_words() {
        let lineage = main_lineage(
            vec![
                delegated(1, Some("Fix the tests"), Status::Running),
                delegated(2, None, Status::Waiting),
                delegated(3, Some("Lint"), Status::Completed),
                delegated(4, Some("Docs"), Status::Interrupted),
                delegated(5, Some("Bench"), Status::Failed),
            ],
            None,
            true,
        );
        let rows = |rows: &[Relative]| {
            rows.iter()
                .map(|row| (row.title.clone(), row.meta.clone(), row.state))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            rows(&lineage.running),
            [
                ("Fix the tests".to_owned(), "on pc \u{b7} Running".to_owned(), Dot::Running),
                ("Delegate".to_owned(), "on pc \u{b7} Waiting".to_owned(), Dot::Running),
            ]
        );
        assert_eq!(
            rows(&lineage.previous),
            [
                ("Lint".to_owned(), "on pc \u{b7} Done".to_owned(), Dot::Done),
                ("Docs".to_owned(), "on pc \u{b7} Stopped".to_owned(), Dot::Idle),
                ("Bench".to_owned(), "on pc \u{b7} Failed".to_owned(), Dot::Failed),
            ]
        );
        assert_eq!(lineage.running_count, 1, "T3 counts the running alone");
        let open = &lineage.running[0];
        assert_eq!(open.provider, Some(Provider::Codex));
        assert_eq!(
            open.leads,
            Leads::Elsewhere {
                server: ServerId::new("s-pc"),
                thread: ThreadId::new(8),
                reached: true,
                cancel: Some(DelegateId::new(1)),
            }
        );
        assert_eq!(open.tooltip.as_deref(), Some("Fix the tests\nOpen on pc"));
        assert!(
            matches!(lineage.previous[0].leads, Leads::Elsewhere { cancel: None, .. }),
            "an ended one has nothing to stop"
        );
        assert_eq!(
            lineage.previous[0].tooltip.as_deref(),
            Some("Lint\nOpen on pc"),
            "a problem is said only where there is one"
        );
    }

    /// A machine this window does not reach draws its row unpressable, as
    /// T3 disables it; what holds one up is said under the pointer in its
    /// Server's words, and a stop owed there in T3's cancel words.
    #[test]
    fn an_unreached_or_held_up_delegate_says_why() {
        let mut followed = delegated(1, Some("Fix the tests"), Status::Running);
        followed.problem = Some("pc did not answer: no route to host".into());
        let mut unstopped = delegated(2, Some("Lint"), Status::Cancelled);
        unstopped.problem = Some("pc did not answer".into());
        unstopped.stop_owed = true;
        let lineage = main_lineage(vec![followed, unstopped], None, false);
        let open = &lineage.running[0];
        assert!(matches!(open.leads, Leads::Elsewhere { reached: false, .. }));
        assert_eq!(
            open.tooltip.as_deref(),
            Some("Fix the tests\nOpen on pc\npc did not answer: no route to host")
        );
        assert_eq!(open.state, Dot::Running, "T3's dot is its status's");
        assert_eq!(
            lineage.previous[0].tooltip.as_deref(),
            Some("Lint\nOpen on pc\nIt could not be stopped on pc and may still run there: pc did not answer")
        );
    }

    /// T3's `DelegatedFromRow`: the delegating Thread's title as its
    /// machine says it now, else as it was sent, `on` the name this window
    /// saved that machine by, else the label its Server sent: this
    /// computer is not `local` to the Thread it delegated.
    #[test]
    fn a_delegated_thread_names_whose_it_is() {
        use ferrite_core::DelegatedFrom;
        let from = DelegatedFrom {
            server: ServerId::new("s-mac"),
            label: "mac".into(),
            thread: ThreadId::new(3),
            title: "Ship it".into(),
        };
        let lineage = |delegator| main_lineage(vec![], Some(delegator), false).delegated_from.unwrap();
        let reached = lineage(Delegator::of(&from, Some(pc()), Some("Ship it today")));
        assert_eq!(
            (reached.title.as_str(), reached.meta.as_str()),
            ("Ship it today", "on pc (box)")
        );
        assert_eq!(
            reached.tooltip.as_deref(),
            Some("Ship it today\nOpen parent agent on pc (box)")
        );
        assert_eq!(
            reached.leads,
            Leads::Delegator {
                server: ServerId::new("s-mac"),
                thread: ThreadId::new(3),
                reached: true,
            }
        );
        let home = Serving {
            name: None,
            machine: ferrite_core::ThreadKey::fixture(0, 1).machine(),
        };
        assert_eq!(lineage(Delegator::of(&from, Some(home), None)).meta, "on mac");
        let alone = lineage(Delegator::of(&from, None, None));
        assert_eq!((alone.title.as_str(), alone.meta.as_str()), ("Ship it", "on mac"));
        assert_eq!(
            alone.tooltip.as_deref(),
            Some("Ship it\nDelegated from mac. Connect to it to open the parent thread.")
        );
        assert!(matches!(alone.leads, Leads::Delegator { reached: false, .. }));
    }
}
