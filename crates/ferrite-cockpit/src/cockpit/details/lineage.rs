//! The details card's Lineage, as T3's Thread details draw it: for the
//! Subject a Pane shows, the parent it came from, the agents it has
//! running, and the ones that finished, folded under one counted
//! `Previous agents` row.

use super::super::subagents::{agent_name, parent_of, state_word};
use super::*;
use ferrite_core::activity::{ActivityView, AgentStatus, Subject};
use ferrite_ui::icons::{self, icon};
use std::collections::HashMap;

/// One Lineage row: a Subject the card leads to.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Relative {
    pub(super) subject: Subject,
    pub(super) title: String,
    /// At the right: how long it has run or ran, else its state word.
    pub(super) meta: String,
    pub(super) state: Dot,
}

/// The dot on a row's icon, in the shared state inks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Dot {
    Running,
    NeedsYou,
    Failed,
    Idle,
}

impl Dot {
    fn of(status: AgentStatus, fresh: bool) -> Self {
        match status {
            _ if !fresh => Self::Idle,
            AgentStatus::Working | AgentStatus::Pending => Self::Running,
            AgentStatus::Waiting => Self::NeedsYou,
            AgentStatus::Failed | AgentStatus::NotFound => Self::Failed,
            _ => Self::Idle,
        }
    }

    fn ink(self) -> u32 {
        match self {
            Self::Running => RUNNING,
            Self::NeedsYou => ATTENTION,
            Self::Failed => BLOCKED,
            Self::Idle => IDLE,
        }
    }
}

/// A Subject's Lineage: its parent (a Subagent's alone), then its own
/// agents, running ones apart from finished ones, newest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Lineage {
    pub(super) parent: Option<Relative>,
    pub(super) running: Vec<Relative>,
    pub(super) previous: Vec<Relative>,
}

/// `subject`'s Lineage in `activity`; Main, its parent, is `main_title`.
pub(super) fn lineage(activity: ActivityView<'_>, subject: &Subject, main_title: &str) -> Lineage {
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
        Some(Relative {
            meta: state_word(view.status(), view.fresh()).to_owned(),
            state: Dot::of(view.status(), view.fresh()),
            subject: parent,
            title,
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
        let meta = elapsed
            .get(&agent.subject())
            .filter(|_| state != Dot::Failed)
            .map_or_else(
                || state_word(agent.status(), agent.fresh()).to_owned(),
                |elapsed| crate::nav::working_for(*elapsed),
            );
        let row = Relative {
            subject: agent.subject(),
            title: agent_name(agent.info()),
            meta,
            state,
        };
        if agent.running() {
            lineage.running.push(row);
        } else {
            lineage.previous.push(row);
        }
    }
    lineage
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
        self.parent.is_none() && self.running.is_empty() && self.previous.is_empty()
    }

    /// How many rows it draws under its heading.
    pub(super) fn rows(&self, previous_open: bool, pages: usize) -> usize {
        let (shown, more) = self.previous_page(pages);
        usize::from(self.parent.is_some())
            + self.running.len()
            + match self.previous.len() {
                0 => 0,
                _ if previous_open => 1 + shown.len() + usize::from(more > 0),
                _ => 1,
            }
    }

    /// The section inside `block`: `Lineage · N running` (the heading,
    /// left out when only `Previous agents` is there to name itself), the
    /// parent, the running agents, then `Previous agents`, which folds
    /// the finished ones a page at a time.
    pub(super) fn render(
        &self,
        mut block: Div,
        thread: ThreadId,
        provider: Provider,
        previous_open: bool,
        pages: usize,
        cx: &mut Context<CockpitView>,
    ) -> Div {
        use super::super::surfaces::SurfaceAction;
        if self.parent.is_some() || !self.running.is_empty() {
            let running = self.running.len();
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
        let (mark, mark_ink) = ferrite_ui::provider::mark(provider);
        if let Some(parent) = &self.parent {
            let subject = parent.subject.clone();
            block = block.child(
                press(
                    "details-lineage-parent".into(),
                    row(parent, icons::CORNER_LEFT_UP, INK_SUBTLE),
                    cx,
                )
                .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    view.select_subject(thread, subject.clone(), window, cx)
                })),
            );
        }
        let agent = |agent: &Relative, cx: &mut Context<CockpitView>| {
            let subject = agent.subject.clone();
            press(
                format!("details-lineage-agent-{}", agent.title),
                row(agent, mark, mark_ink),
                cx,
            )
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.open_subagent(thread, subject.clone(), cx)
            }))
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
    .child(
        body.flex()
            .items_center()
            .w_full()
            .min_w_0()
            .gap(px(10.))
            .text_size(px(13.))
            .font_weight(W_LABEL),
    )
}

/// A row's body: `glyph` with its state dot, the title cut to fit, the meta.
fn row(relative: &Relative, glyph: &'static str, ink: u32) -> Div {
    div()
        .text_color(rgba(0xffffffcc))
        .child(
            div()
                .relative()
                .flex_shrink_0()
                .size(px(16.))
                .child(icon(glyph, 16., ink))
                .child(
                    div()
                        .absolute()
                        .right(px(-3.))
                        .bottom(px(-3.))
                        .size(px(8.))
                        .rounded_full()
                        .border_2()
                        // The card's own ground, so the dot cuts into its icon.
                        .border_color(rgb(0x111111))
                        .bg(rgb(relative.state.ink())),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(relative.title.clone()),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(12.))
                .text_color(rgb(INK_SUBTLE))
                .child(relative.meta.clone()),
        )
}
