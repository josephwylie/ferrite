//! The details card's Lineage, as T3's Thread details draw it: for the
//! Subject a Pane shows, the parent it came from, the agents it has
//! running, and the ones that finished, folded under one counted
//! `Previous agents` row. Main's also has T3's rows for a Thread on another
//! machine (ADR 0047): the Thread there that delegated it, first, and
//! each Delegate it started, after its own agents in the order it started
//! them (T3 lists a projection's subagents as they were recorded).

use ferrite_core::activity::{AgentStatus, Subject};
use ferrite_core::delegate::DelegateId;
use ferrite_core::provider::Provider;
use ferrite_core::server::ServerId;
use ferrite_core::ThreadId;
use ferrite_ui::icons::{self, icon};
use ferrite_ui::theme::*;
use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, AnyElement, Div, SharedString};

use crate::cell::{PaneAsk, PaneAsks};

/// One Lineage row: what the card leads to.
#[derive(Clone, Debug, PartialEq)]
pub struct Relative {
    pub leads: Leads,
    pub title: String,
    /// At the right: how long it has run or ran, else its state word.
    pub meta: String,
    /// Whether `meta` is a clock, which T3 sets in tabular figures at the
    /// regular weight.
    pub timed: bool,
    pub state: Dot,
    /// The provider it runs on when not the Thread's: a Delegate's, there.
    pub provider: Option<Provider>,
    /// What the pointer on it reads (T3's `LineageTooltipContent`): a row
    /// for a Thread on another machine says where it opens, and what holds
    /// it up.
    pub tooltip: Option<String>,
}

/// Where a row's press goes.
#[derive(Clone, Debug, PartialEq)]
pub enum Leads {
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
pub enum Dot {
    Running,
    Failed,
    Done,
    Idle,
}

impl Dot {
    pub fn of(status: AgentStatus, fresh: bool) -> Self {
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

/// A Subject's Lineage: the Thread that delegated it (Main's alone), its
/// parent (a Subagent's alone), then its own agents newest first and its
/// Delegates oldest first, running ones apart from finished ones.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lineage {
    pub delegated_from: Option<Relative>,
    pub parent: Option<Relative>,
    pub running: Vec<Relative>,
    pub previous: Vec<Relative>,
    /// The heading's count (T3's `runningCount`): the running agents, and
    /// the Delegates whose status is Running.
    pub running_count: usize,
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
        provider: Provider,
        previous_open: bool,
        pages: usize,
        asks: &PaneAsks,
        cx: &gpui::App,
    ) -> Div {
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
                asks,
                cx,
            ));
        }
        if let Some(parent) = &self.parent {
            block = block.child(relative(
                "details-lineage-parent".into(),
                parent,
                (icons::CORNER_LEFT_UP, INK_SUBTLE),
                asks,
                cx,
            ));
        }
        let agent = |agent: &Relative, cx: &gpui::App| {
            let selector = match &agent.leads {
                Leads::Elsewhere { .. } | Leads::Delegator { .. } => {
                    format!("details-lineage-delegate-{}", agent.title)
                }
                Leads::Subject(_) => format!("details-lineage-agent-{}", agent.title),
            };
            let mark = ferrite_ui::provider::mark(agent.provider.unwrap_or(provider));
            relative(selector, agent, mark, asks, cx)
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
                .child(
                    // By width alone, as a row's title: `truncate`'s clip
                    // would take the descenders.
                    div()
                        .flex_1()
                        .min_w_0()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child("Previous agents"),
                )
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
            press("details-lineage-previous".into(), previous, cx)
                .on_click(asks.press(PaneAsk::PreviousAgents)),
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
                    .on_click(asks.press(PaneAsk::MorePreviousAgents)),
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
    asks: &PaneAsks,
    cx: &gpui::App,
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
                .on_click(asks.press(PaneAsk::Subject { subject, parent }))
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
                press(selector.clone(), body, cx).on_click(asks.press(PaneAsk::Elsewhere {
                    server,
                    thread: there,
                    delegator,
                })),
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
                        .on_click(asks.press(PaneAsk::CancelDelegate(delegate))),
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
