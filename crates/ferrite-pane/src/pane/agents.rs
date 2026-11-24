//! Subagents in the transcript that started them: T3's subagent card over
//! two or more started together, one agent row per call, each row the way
//! into that agent's own transcript. A Delegate's start (`delegate_task`,
//! ADR 0047) is one of these rows too, as T3 draws a task in a linked
//! environment, saying the machine it runs `on`.
//!
//! A call's row says how its agent stands, not how its call does: a
//! background agent's call returns at once (`Async agent launched`) while
//! the agent works on. So the cockpit hands the transcript each agent's
//! standing ([`AgentFaces`]), and the calls take it on before they are
//! grouped and drawn ([`AgentFaces::apply`]): the group's tense, its failure
//! count and the shut card's live rows then read as any group's do.
use super::*;
use ferrite_core::provider::Provider;
use std::collections::BTreeMap;

/// How a subagent stands, for its call's row (T3's subagent statuses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentRun {
    /// Starting or working.
    Running,
    /// Held on the operator: a question or an approval.
    Waiting,
    Done,
    Failed,
    /// Stopped before it finished.
    Stopped,
}

/// One subagent as its call's row shows it: how it stands, what it is
/// doing (its latest progress summary, Claude's `task_progress.summary`),
/// what it answered once its turn ended, how long it has run, and the
/// provider running it (its avatar's mark). A Delegate's also names the
/// machine it runs `on` (T3's `on {environment}`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFace {
    pub provider: Provider,
    pub run: AgentRun,
    pub summary: Option<String>,
    pub result: Option<String>,
    pub clock: Option<AgentClock>,
    pub on: Option<Elsewhere>,
}

impl AgentFace {
    /// Whether its row opens it: a subagent always, a Delegate while this
    /// window reaches its machine (T3's row has no opener without one).
    pub fn opens(&self) -> bool {
        self.on.as_ref().is_none_or(|on| on.reached)
    }
}

/// The machine a Delegate runs on, by the name it was sent to, and whether
/// this window reaches it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Elsewhere {
    pub machine: String,
    pub reached: bool,
}

/// How long a subagent has run: since its turn began while it runs (read
/// as the row draws), else between the wall times it started and ended,
/// else (an older log, a launch that failed) only how long it took.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentClock {
    Since(std::time::Instant),
    Ran {
        from: std::time::SystemTime,
        to: std::time::SystemTime,
    },
    Took(std::time::Duration),
}

impl AgentClock {
    pub fn elapsed(self) -> std::time::Duration {
        match self {
            Self::Since(at) => ferrite_core::clock::instant().saturating_duration_since(at),
            Self::Ran { from, to } => to.duration_since(from).unwrap_or_default(),
            Self::Took(took) => took,
        }
    }

    /// How the clock reads: while it runs, counting on its own (the pulse
    /// clock wakes it each second, `working::counting_clock`), not only
    /// when something else redraws the transcript.
    fn face(self) -> AnyElement {
        match self {
            Self::Since(at) => {
                super::working::counting_clock(at, "agent-elapsed-live", agent_elapsed)
            }
            settled => SharedString::from(agent_elapsed(settled.elapsed())).into_any_element(),
        }
    }

    /// A card's clock, T3's `subagentGroupTiming`: from its agents' earliest
    /// start, counting while any runs, else to their latest end. An agent
    /// known only by how long it took has no place on that line, so the card
    /// reads at least its time.
    pub(crate) fn across(clocks: impl Iterator<Item = Self>) -> Option<Self> {
        let clocks: Vec<Self> = clocks.collect();
        if clocks.iter().any(|clock| matches!(clock, Self::Since(_))) {
            return clocks
                .iter()
                .filter_map(|clock| match *clock {
                    Self::Since(at) => Some(at),
                    Self::Ran { from, .. } => Some(ferrite_core::clock::instant_at(from)),
                    Self::Took(_) => None,
                })
                .min()
                .map(Self::Since);
        }
        let ran = clocks
            .iter()
            .filter_map(|clock| match *clock {
                Self::Ran { from, to } => Some((from, to)),
                _ => None,
            })
            .reduce(|(from, to), (next_from, next_to)| (from.min(next_from), to.max(next_to)))
            .map(|(from, to)| Self::Ran { from, to });
        clocks
            .into_iter()
            .filter(|clock| matches!(clock, Self::Took(_)))
            .chain(ran)
            .max_by_key(|clock| clock.elapsed())
    }
}

/// The subagents a transcript's calls started, by call id. Only a call
/// with a face opens its agent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentFaces(BTreeMap<String, AgentFace>);

impl AgentFaces {
    pub fn insert(&mut self, call: String, face: AgentFace) {
        self.0.insert(call, face);
    }

    pub fn get(&self, call: &str) -> Option<&AgentFace> {
        self.0.get(call)
    }

    /// Whether `tool` draws as an agent's row ([`is_agent`]).
    pub(crate) fn draws(&self, tool: &ToolBlock) -> bool {
        is_agent(tool, |call| self.0.contains_key(call))
    }

    /// The `delegate_task` calls a Delegate answers for.
    pub(crate) fn delegate_calls(&self) -> std::collections::BTreeSet<String> {
        self.0
            .iter()
            .filter(|(_, face)| face.on.is_some())
            .map(|(call, _)| call.clone())
            .collect()
    }

    /// The agent calls in `blocks` take on their agents' standing, so a
    /// group's tense and counts read them: running while the agent runs or
    /// waits; failed or stopped as it ended; settled once it is done.
    pub fn apply(&self, blocks: &mut [Block]) {
        if self.0.is_empty() {
            return;
        }
        for block in blocks {
            let Body::Tool(tool) = &mut block.body else {
                continue;
            };
            let Some(face) = self.0.get(&tool.call) else {
                continue;
            };
            match face.run {
                AgentRun::Running | AgentRun::Waiting => {
                    tool.state = ToolState::Running;
                    tool.result_line = face.summary.clone();
                }
                AgentRun::Failed if !matches!(tool.state, ToolState::Failed(_)) => {
                    tool.state = ToolState::Failed(String::new());
                }
                // T3 reads the agent's own status: the error result its
                // stop left on the call is not a failure.
                AgentRun::Stopped => tool.state = ToolState::Stopped,
                AgentRun::Done if tool.state == ToolState::Running => {
                    tool.state = ToolState::Ok;
                }
                AgentRun::Failed | AgentRun::Done => {}
            }
        }
    }
}

/// Whether a call draws as [`render_agent`]: a subagent's, or a
/// `delegate_task` call that `started` a Delegate. One still out or refused
/// keeps its ordinary tool row with its error (T3's
/// `withoutSubagentDelegationRows`).
pub(crate) fn is_agent(tool: &ToolBlock, started: impl Fn(&str) -> bool) -> bool {
    use ferrite_core::transcript::tools::{policy, ToolKind};
    match policy(&tool.name).kind {
        ToolKind::Agent => true,
        ToolKind::Delegate => started(&tool.call),
        _ => false,
    }
}

/// What an agent's row says (T3's `SubagentTimelineLink`), before it is
/// drawn: its task, its detail line (progress while it runs, its result once
/// settled, else its status word), and its status word again at the right
/// when the detail line holds something else and it has not completed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AgentRow {
    pub standing: Standing,
    pub title: String,
    /// `on pc`: the machine a Delegate runs on.
    pub on: Option<String>,
    pub detail: String,
    pub label: Option<&'static str>,
}

/// The row of the agent `tool` started, as `face` says it stands; with no
/// face (no agent answers for the call), as the call reads.
pub(crate) fn agent_row(tool: &ToolBlock, face: Option<&AgentFace>) -> AgentRow {
    let standing = face.map_or_else(|| Standing::of(tool), |face| Standing::from(face.run));
    let (progress, result) = match face {
        Some(face) => (face.summary.as_deref(), face.result.as_deref()),
        None if standing.settled() => (None, tool.result_line.as_deref()),
        None => (tool.result_line.as_deref(), None),
    };
    let progress = progress.and_then(flat);
    let result = result.and_then(flat);
    let detail = if standing.settled() {
        result.or(progress)
    } else {
        progress.or(result)
    };
    AgentRow {
        standing,
        title: text::agent_title(tool),
        on: face.and_then(|face| face.on.as_ref()).map(|on| format!("on {}", on.machine)),
        label: detail
            .as_ref()
            .filter(|_| standing != Standing::Done)
            .map(|_| standing.word()),
        detail: detail.unwrap_or_else(|| standing.word().to_owned()),
    }
}

/// A progress or result as one line (T3's `subagentCardDetail`): links to
/// their words, no backticks or list marks, whitespace collapsed.
fn flat(text: &str) -> Option<String> {
    let lines: Vec<&str> = text
        .lines()
        .map(|line| {
            let line = line.trim_start();
            line.strip_prefix("- ")
                .or_else(|| line.strip_prefix("* "))
                .unwrap_or(line)
        })
        .collect();
    let joined = unlink(&lines.join(" ")).replace('`', "");
    let words: Vec<&str> = joined.split_whitespace().collect();
    (!words.is_empty()).then(|| words.join(" "))
}

/// `[words](target)` and `![words](target)` as their words.
fn unlink(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let link = rest[open + 1..]
            .split_once("](")
            .filter(|(words, _)| !words.contains('['))
            .and_then(|(words, after)| after.find(')').map(|close| (words, &after[close + 1..])));
        let Some((words, after)) = link else {
            out.push_str(&rest[..=open]);
            rest = &rest[open + 1..];
            continue;
        };
        out.push_str(rest[..open].strip_suffix('!').unwrap_or(&rest[..open]));
        out.push_str(words);
        rest = after;
    }
    out.push_str(rest);
    out
}

/// An agent call's row, alone or in its card (T3's `SubagentTimelineLink`):
/// the agent's avatar with its status dot, its task (and a Delegate's `on
/// pc` beside it) over what it does
/// (its progress while it runs, else its standing), the standing again at
/// the right while it is not done, how long it has run, and a `›` that
/// brightens under the pointer ([`agent_row`] says which words). `open`,
/// laid over the whole row, opens the agent's own transcript. `targeted`: the keyboard's walk has landed on
/// the row (its focus, whose keys open the agent and stop it); it wears
/// `paint::RAISED`, as a targeted disclosure does.
pub(crate) fn render_agent(
    block: BlockId,
    tool: &ToolBlock,
    row_cx: &RowCx,
    face: AgentRowCx,
) -> AnyElement {
    let AgentRowCx {
        face,
        open,
        targeted,
        ground,
    } = face;
    let selection = row_cx.selection;
    let AgentRow {
        standing,
        title,
        on,
        detail,
        label,
    } = agent_row(tool, face);
    let clock = face.and_then(|face| face.clock);
    // T3: the status reads muted, destructive once it failed.
    let ink = if standing == Standing::Failed {
        theme::BLOCKED
    } else {
        INK_SUBTLE
    };
    let title = div()
        .debug_selector(|| "agent-call".into())
        .min_w_0()
        .truncate()
        .text_size(px(theme::FS_BADGE))
        .line_height(px(theme::LH_META))
        .font_weight(theme::W_LABEL)
        .text_color(rgb(INK_STRONG))
        .child(selection.line(block, title, Vec::new()));
    // T3: the title, then `on pc` muted at the same size, kept whole.
    let title = div()
        .flex()
        .items_baseline()
        .gap(px(theme::SPACE_2))
        .min_w_0()
        .child(title)
        .children(on.map(|on| {
            div()
                .debug_selector(|| "agent-on".into())
                .flex_shrink_0()
                .text_size(px(theme::FS_BADGE))
                .line_height(px(theme::LH_META))
                .text_color(rgb(INK_SUBTLE))
                .child(on)
        }));
    let detail_line = div()
        .debug_selector(|| "agent-detail".into())
        .min_w_0()
        .truncate()
        .text_size(px(theme::AGENT_DETAIL_FS))
        .line_height(px(theme::AGENT_DETAIL_LH))
        .text_color(rgb(ink))
        .child(selection.line(block, detail, Vec::new()));
    let label = label.map(|word| {
        div()
            .debug_selector(move || format!("agent-status-{word}"))
            .flex_shrink_0()
            .text_size(px(theme::FS_BADGE))
            .line_height(px(theme::LH_META))
            .text_color(rgb(ink))
            .child(word)
    });
    let elapsed = clock.map(|clock| elapsed("agent-elapsed", clock));
    let opens = open.is_some();
    let row_selector = format!("agent-row-{}", tool.call);
    let row = div()
        .id(SharedString::from(row_selector.clone()))
        .debug_selector(move || row_selector.clone())
        .group(DISCLOSURE_ROW)
        .relative()
        .flex()
        .items_center()
        .gap(px(theme::AGENT_ROW_GAP))
        .w_full()
        .min_w_0()
        .px(px(theme::AGENT_ROW_PAD_X))
        .py(px(theme::AGENT_ROW_PAD_Y))
        .rounded(px(theme::AGENT_ROW_RADIUS))
        .when(opens, |row| row.hover(|style| style.bg(theme::paint::HOVER)))
        .child(avatar(face.map(|face| face.provider), Some(standing), ground, false))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(title)
                .child(detail_line),
        )
        .children(label)
        .children(elapsed)
        .children(opens.then(|| {
            icons::icon(icons::CHEVRON_RIGHT, theme::AGENT_CHEVRON, theme::TEXT_FAINT)
                .group_hover(DISCLOSURE_ROW, |style| style.text_color(rgb(INK_STRONG)))
        }))
        .children(open);
    match targeted {
        Some(focus) => row
            .bg(theme::paint::RAISED)
            .child(
                div()
                    .debug_selector(|| "tool-disclosure-keyboard-target".into())
                    .absolute()
                    .inset_0(),
            )
            .track_focus(focus)
            .key_context("ToolDisclosure")
            .into_any_element(),
        None => row.into_any_element(),
    }
}

/// What an agent row draws besides its call: how its agent stands (none
/// when no agent answers for the call), its opener, the keyboard's target,
/// and the ground its avatar's ring is cut from.
pub(crate) struct AgentRowCx<'a> {
    pub face: Option<&'a AgentFace>,
    pub open: Option<AnyElement>,
    pub targeted: Option<&'a gpui::FocusHandle>,
    pub ground: u32,
}

/// Two or more agents started together (T3's `V2SubagentGroup`): a
/// trigger row (up to `AGENT_AVATARS` stacked avatars and `+N`, `3
/// subagents` over how they stand, how long they have run, a chevron that
/// turns when it opens), and open, a panel of their rows that snaps open.
/// The trigger recedes to `AGENT_CARD_SETTLED_OPACITY` once every agent is
/// done and it is shut, coming up under the pointer ([`receding`]).
pub(crate) fn render_agent_card(
    members: &[Block],
    row_cx: &RowCx,
    card: AgentCardCx,
) -> AnyElement {
    let AgentCardCx {
        disclosure,
        open,
        clock,
        providers,
        rows,
        ground,
    } = card;
    let Some(leader) = members.first() else {
        return div().into_any_element();
    };
    let tools: Vec<&ToolBlock> = text::group_tools(members).collect();
    let standings: Vec<Standing> = tools.iter().map(|tool| Standing::of(tool)).collect();
    let active = standings.contains(&Standing::Running);
    let failed = standings.contains(&Standing::Failed);
    let (overlay, targeted, focus) = match disclosure {
        Some(Disclosure {
            overlay,
            targeted,
            focus,
            ..
        }) => (overlay, targeted, focus),
        None => (None, false, None),
    };
    let key = tools
        .first()
        .map_or_else(|| format!("{:?}", leader.id), |tool| tool.call.clone());
    let more = tools.len().saturating_sub(theme::AGENT_AVATARS);
    let receded = !open && !active;
    let stack = div()
        .debug_selector(|| "agent-card-avatars".into())
        .flex()
        .flex_shrink_0()
        .children(providers.iter().take(theme::AGENT_AVATARS).enumerate().map(|(at, provider)| {
            avatar(*provider, None, ground, receded)
                .when(at > 0, |avatar| avatar.ml(px(-theme::AGENT_AVATAR_OVERLAP)))
        }))
        .children((more > 0).then(|| {
            ringed(
                disc()
                    .text_size(px(theme::AGENT_MORE_FS))
                    .font_weight(theme::W_LABEL)
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(format!("+{more}"))),
                ground,
                receded,
            )
            .ml(px(-theme::AGENT_AVATAR_OVERLAP))
        }));
    let status_ink = match () {
        _ if active => theme::BLUE,
        _ if failed => theme::BLOCKED,
        _ => INK_SUBTLE,
    };
    let words = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .debug_selector(|| "agent-card-title".into())
                .text_size(px(theme::FS_BADGE))
                .line_height(px(theme::LH_META))
                .font_weight(theme::W_STRONG)
                .child(row_cx.selection.line(leader.id, text::agent_card_title(&tools), Vec::new())),
        )
        .child(
            div()
                .debug_selector(|| "agent-card-status".into())
                .min_w_0()
                .truncate()
                .text_size(px(theme::AGENT_MORE_FS))
                .line_height(px(theme::AGENT_MORE_LH))
                .text_color(rgb(status_ink))
                .child(row_cx.selection.line(leader.id, text::agent_card_status(&tools), Vec::new())),
        );
    let trigger_key = SharedString::from(format!("agent-card-{key}"));
    let mut trigger = div()
        .id(trigger_key.clone())
        .debug_selector(move || trigger_key.to_string())
        .group(DISCLOSURE_ROW)
        .relative()
        .flex()
        .items_center()
        .gap(px(theme::AGENT_ROW_GAP))
        .w_full()
        .min_w_0()
        .py(px(theme::AGENT_CARD_PAD_Y))
        .pr(px(theme::AGENT_CARD_PAD_R))
        .text_color(rgb(if receded { INK_SUBTLE } else { INK_STRONG }))
        .child(stack)
        .child(receding(words, receded))
        .children(clock.map(|clock| receding(elapsed("agent-card-elapsed", clock), receded)))
        .child(receding(
            div().flex_shrink_0().child(ferrite_ui::motion::settled(
                SharedString::from(format!("agent-card-chevron-{key}")),
                open,
                ferrite_ui::motion::AGENT_CHEVRON,
                |turn| {
                    icons::icon(icons::CHEVRON_DOWN, theme::AGENT_CHEVRON, INK_SUBTLE)
                        .with_transformation(gpui::Transformation::rotate(gpui::radians(
                            std::f32::consts::PI * turn,
                        )))
                },
            )),
            receded,
        ))
        .children(overlay);
    if targeted {
        trigger = trigger.bg(theme::paint::RAISED).child(
            div()
                .debug_selector(|| "tool-disclosure-keyboard-target".into())
                .absolute()
                .inset_0(),
        );
        if let Some(focus) = &focus {
            trigger = trigger.track_focus(focus).key_context("ToolDisclosure");
        }
    }
    let panel = open.then(|| {
        div()
            .debug_selector(|| "agent-card-panel".into())
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .my(px(theme::AGENT_PANEL_MARGIN_Y))
            .p(px(theme::AGENT_PANEL_PAD))
            .rounded(px(theme::AGENT_PANEL_RADIUS))
            .border_1()
            .border_color(gpui::Hsla::from(theme::paint::LINE).opacity(0.6))
            .bg(gpui::Hsla::from(theme::paint::CARD).opacity(0.3))
            .children(rows)
    });
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .child(trigger)
        .children(panel)
        .into_any_element()
}

/// What a subagent card draws besides its calls: its disclosure, whether
/// it is open, how long its agents have run, the provider of each (none for
/// a call no agent answers), their rows (open), and the ground its avatars'
/// rings are cut from.
pub(crate) struct AgentCardCx {
    pub disclosure: Option<Disclosure>,
    pub open: bool,
    pub clock: Option<AgentClock>,
    pub providers: Vec<Option<Provider>>,
    pub rows: Vec<AnyElement>,
    pub ground: u32,
}

/// An agent's standing as its row and its card count it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Standing {
    Running,
    Waiting,
    Done,
    Failed,
    Stopped,
}

impl From<AgentRun> for Standing {
    fn from(run: AgentRun) -> Self {
        match run {
            AgentRun::Running => Self::Running,
            AgentRun::Waiting => Self::Waiting,
            AgentRun::Done => Self::Done,
            AgentRun::Failed => Self::Failed,
            AgentRun::Stopped => Self::Stopped,
        }
    }
}

impl Standing {
    /// Ended: its result, not its progress, is what it says.
    fn settled(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Stopped)
    }

    pub(crate) fn of(tool: &ToolBlock) -> Self {
        match tool.state {
            ToolState::Running => Self::Running,
            ToolState::Failed(_) => Self::Failed,
            ToolState::Stopped | ToolState::Unavailable => Self::Stopped,
            ToolState::Ok => Self::Done,
        }
    }

    /// Its word on the row (T3's status labels).
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Waiting => "Waiting",
            Self::Done => "Completed",
            Self::Failed => "Failed",
            Self::Stopped => "Stopped",
        }
    }

    /// Its status dot's ink: info while it runs or waits, success done,
    /// destructive failed, muted stopped.
    fn ink(self) -> u32 {
        match self {
            Self::Running | Self::Waiting => theme::BLUE,
            Self::Done => theme::GREEN,
            Self::Failed => theme::BLOCKED,
            Self::Stopped => theme::TEXT_FAINT,
        }
    }
}

/// A subagent's avatar (T3's `SubagentAvatar`): an `AGENT_AVATAR` disc
/// holding its provider's mark (the bot when no agent answers for the call),
/// ringed in the ground; with a `standing`, its ringed status dot at the
/// disc's lower right.
fn avatar(
    provider: Option<Provider>,
    standing: Option<Standing>,
    ground: u32,
    receded: bool,
) -> Div {
    let (mark, ink) = provider.map_or((icons::BOT, INK_SUBTLE), ferrite_ui::provider::mark);
    ringed(
        disc()
            .border_1()
            .border_color(gpui::Hsla::from(theme::paint::LINE).opacity(0.7))
            .child(icons::icon(mark, theme::AGENT_AVATAR_ICON, ink)),
        ground,
        receded,
    )
    .children(standing.map(|_| ring(theme::AGENT_DOT, ground)))
    .children(standing.map(|standing| {
        div()
            .when(standing == Standing::Running, |dot| {
                dot.debug_selector(|| "agent-running".into())
            })
            .absolute()
            .right_0()
            .bottom_0()
            .size(px(theme::AGENT_DOT))
            .rounded_full()
            .bg(rgb(standing.ink()))
    }))
}

/// The disc an avatar and T3's borderless `+N` share.
fn disc() -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(theme::AGENT_AVATAR))
        .rounded_full()
        .bg(rgba(theme::AGENT_AVATAR_FILL))
}

/// `disc` on its ring, in a slot of its size that a status dot can sit in.
/// The ring and the dot are layers beside the disc, not its shadow and
/// child: GPUI paints a border over children, which would run the disc's
/// edge through the dot, and a shadow's spread keeps the disc's radius, so
/// the ring would not come out round. Only the disc recedes: the ring stays
/// solid, to hide the avatar it overlaps.
fn ringed(disc: Div, ground: u32, receded: bool) -> Div {
    div()
        .relative()
        .flex_shrink_0()
        .size(px(theme::AGENT_AVATAR))
        .child(ring(theme::AGENT_AVATAR, ground))
        .child(receding(disc, receded))
}

/// A settled card's part at `AGENT_CARD_SETTLED_OPACITY`, whole under the
/// pointer. Each part fades on its own because GPUI's opacity multiplies
/// every primitive under it rather than compositing them as a group, so
/// fading the whole card would show one avatar through the next one's ring.
fn receding(part: Div, receded: bool) -> Div {
    part.when(receded, |part| {
        part.opacity(theme::AGENT_CARD_SETTLED_OPACITY)
            .group_hover(DISCLOSURE_ROW, |style| style.opacity(1.))
    })
}

/// T3's `ring-2 ring-background` round a `size` mark at its slot's lower
/// right: the ground, `AGENT_RING` wider on every side.
fn ring(size: f32, ground: u32) -> Div {
    div()
        .absolute()
        .right(px(-theme::AGENT_RING))
        .bottom(px(-theme::AGENT_RING))
        .size(px(size + 2. * theme::AGENT_RING))
        .rounded_full()
        .bg(rgb(ground))
}

/// How long it has run, at a row's or card's right (T3's `text-xs
/// tabular-nums`).
fn elapsed(selector: &'static str, clock: AgentClock) -> Div {
    ferrite_ui::components::tabular(
        div()
            .debug_selector(move || selector.into())
            .flex_shrink_0()
            .text_size(px(theme::FS_BADGE))
            .line_height(px(theme::LH_META))
            .text_color(rgb(INK_SUBTLE)),
    )
    .child(clock.face())
}

/// How long an agent has run, as T3's `AgentElapsed` reads: `45s`, `2m 05s`,
/// `1h 03m`.
pub(crate) fn agent_elapsed(elapsed: std::time::Duration) -> String {
    let seconds = elapsed.as_secs();
    match seconds {
        s if s < 60 => format!("{s}s"),
        s if s < 3_600 => format!("{}m {:02}s", s / 60, s % 60),
        s => format!("{}h {:02}m", s / 3_600, s % 3_600 / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::transcript::{Input, Transcript};
    use ferrite_core::SessionEvent;

    fn agents(calls: &[(&str, &str)]) -> Vec<Block> {
        let mut transcript = Transcript::default();
        for (id, name) in calls {
            transcript.apply(Input::Event(SessionEvent::ToolStarted {
                id: (*id).into(),
                name: (*name).into(),
                input: serde_json::json!({ "description": "Count files" }),
            }));
        }
        transcript.blocks().to_vec()
    }

    fn tool(block: &Block) -> &ToolBlock {
        match &block.body {
            Body::Tool(tool) => tool,
            _ => panic!("a call"),
        }
    }

    /// A background agent's call settles at once; its row runs on with the
    /// agent, saying what it does, and fails when the agent does.
    #[test]
    fn a_call_takes_on_its_agents_standing() {
        let mut blocks = agents(&[
            ("a", "Agent"),
            ("b", "Agent"),
            ("c", "Agent"),
            ("r", "Read"),
        ]);
        for block in &mut blocks {
            if let Body::Tool(tool) = &mut block.body {
                tool.state = ToolState::Ok;
            }
        }
        let mut faces = AgentFaces::default();
        faces.insert(
            "a".into(),
            face(AgentRun::Running, Some("Reading nav.rs"), None),
        );
        faces.insert(
            "b".into(),
            face(AgentRun::Failed, None, None),
        );
        faces.insert(
            "c".into(),
            face(AgentRun::Done, None, None),
        );
        faces.apply(&mut blocks);
        assert_eq!(tool(&blocks[0]).state, ToolState::Running);
        assert_eq!(
            tool(&blocks[0]).result_line.as_deref(),
            Some("Reading nav.rs")
        );
        assert!(matches!(tool(&blocks[1]).state, ToolState::Failed(_)));
        assert_eq!(tool(&blocks[2]).state, ToolState::Ok);
        assert_eq!(tool(&blocks[3]).state, ToolState::Ok);
    }

    /// T3 reads the agent's status: a stop's error result is no failure.
    #[test]
    fn a_stopped_agent_reads_stopped_though_its_call_ended_in_an_error() {
        let mut blocks = agents(&[("a", "Agent")]);
        if let Body::Tool(tool) = &mut blocks[0].body {
            tool.state = ToolState::Failed("Stop me".into());
        }
        let mut faces = AgentFaces::default();
        faces.insert("a".into(), face(AgentRun::Stopped, None, None));
        faces.apply(&mut blocks);
        assert_eq!(tool(&blocks[0]).state, ToolState::Stopped);
    }

    /// A card's clock is T3's `subagentGroupTiming`: earliest start to latest
    /// end, not its longest agent, and it counts while any agent runs.
    #[test]
    fn a_cards_clock_runs_from_the_earliest_start_to_the_latest_end() {
        use std::time::{Duration, SystemTime};
        let at = |s: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(1_000 + s);
        let ran = |from, to| AgentClock::Ran { from: at(from), to: at(to) };
        let card = |clocks: &[AgentClock]| AgentClock::across(clocks.iter().copied());

        assert_eq!(card(&[]), None);
        assert_eq!(
            card(&[ran(0, 10), ran(5, 20)]).map(AgentClock::elapsed),
            Some(Duration::from_secs(20)),
            "overlapping agents: neither one's own 10s or 15s"
        );
        assert_eq!(
            card(&[ran(0, 84), AgentClock::Took(Duration::ZERO)]).map(AgentClock::elapsed),
            Some(Duration::from_secs(84)),
            "a launch that failed at once does not stand for the card"
        );
        assert_eq!(
            card(&[AgentClock::Took(Duration::from_secs(9)), ran(0, 4)]).map(AgentClock::elapsed),
            Some(Duration::from_secs(9)),
            "an agent known only by its length still counts"
        );
        let started = ferrite_core::clock::instant();
        assert!(
            matches!(
                card(&[AgentClock::Since(started), AgentClock::Took(Duration::ZERO)]),
                Some(AgentClock::Since(at)) if at == started
            ),
            "it counts while any agent runs"
        );
    }

    /// A card counts its agents by how they stand, as T3's does.
    #[test]
    fn an_agent_row_and_its_card_read_as_t3s() {
        let blocks = agents(&[("a", "Agent"), ("b", "Agent"), ("c", "Agent")]);
        let mut running = tool(&blocks[0]).clone();
        running.result_line = Some("Reading nav.rs".into());
        assert_eq!(text::agent_title(&running), "Count files");
        let mut done = running.clone();
        done.state = ToolState::Ok;
        let mut failed = running.clone();
        failed.state = ToolState::Failed(String::new());
        let tools = [&running, &done, &done];
        assert_eq!(text::agent_card_title(&tools), "3 subagents");
        assert_eq!(text::agent_card_status(&tools), "1 working \u{b7} 2 done");
        let tools = [&done, &failed];
        assert_eq!(text::agent_card_status(&tools), "1 done \u{b7} 1 failed");
        let elapsed = |s| agent_elapsed(std::time::Duration::from_secs(s));
        assert_eq!(
            [elapsed(45), elapsed(125), elapsed(3_780)],
            ["45s", "2m 05s", "1h 03m"]
        );
    }

    fn face(run: AgentRun, summary: Option<&str>, result: Option<&str>) -> AgentFace {
        AgentFace {
            provider: Provider::Claude,
            run,
            summary: summary.map(Into::into),
            result: result.map(Into::into),
            clock: None,
            on: None,
        }
    }

    /// A subagent's row in each state reads as T3's `SubagentTimelineLink`:
    /// its task over its progress while it runs (its result once settled),
    /// else its status word; the status again at the right when the detail
    /// line holds something else and it has not completed.
    #[test]
    fn an_agent_row_reads_as_t3s_in_every_state() {
        let blocks = agents(&[("a", "Agent")]);
        let call = tool(&blocks[0]);
        let row = |run, summary, result| agent_row(call, Some(&face(run, summary, result)));
        let read = |row: AgentRow| (row.standing, row.detail, row.label);
        use Standing::*;
        assert_eq!(row(AgentRun::Running, None, None).title, "Count files");
        assert_eq!(
            read(row(AgentRun::Running, Some("Reading nav.rs"), None)),
            (Running, "Reading nav.rs".into(), Some("Running"))
        );
        assert_eq!(read(row(AgentRun::Running, None, None)), (Running, "Running".into(), None));
        assert_eq!(
            read(row(AgentRun::Waiting, Some("Asks to run cargo"), None)),
            (Waiting, "Asks to run cargo".into(), Some("Waiting"))
        );
        assert_eq!(read(row(AgentRun::Waiting, None, None)), (Waiting, "Waiting".into(), None));
        assert_eq!(
            read(row(AgentRun::Done, Some("Reading"), Some("Found `3` in [the docs](https://x.y/z)\n- a.rs\n- b.rs"))),
            (Done, "Found 3 in the docs a.rs b.rs".into(), None),
            "settled, its result, flat"
        );
        assert_eq!(read(row(AgentRun::Done, None, None)), (Done, "Completed".into(), None));
        assert_eq!(
            read(row(AgentRun::Failed, None, Some("No such file"))),
            (Failed, "No such file".into(), Some("Failed"))
        );
        assert_eq!(read(row(AgentRun::Failed, None, None)), (Failed, "Failed".into(), None));
        assert_eq!(
            read(row(AgentRun::Stopped, Some("Reading nav.rs"), None)),
            (Stopped, "Reading nav.rs".into(), Some("Stopped"))
        );
        assert_eq!(read(row(AgentRun::Stopped, None, None)), (Stopped, "Stopped".into(), None));
    }

    /// A Delegate's start is T3's row for a task in a linked environment:
    /// its task title, `on pc` beside it, and how the Delegate stands.
    #[test]
    fn a_delegate_call_reads_as_t3s_remote_task_row() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "d".into(),
            name: "mcp__ferrite__delegate_task".into(),
            input: serde_json::json!({ "task": "Run the login test", "target": { "machineId": "s-pc" } }),
        }));
        let call = tool(&transcript.blocks()[0]).clone();
        let mut faces = AgentFaces::default();
        assert!(!faces.draws(&call), "an ordinary call until its Delegate answers");
        let face = AgentFace {
            on: Some(Elsewhere {
                machine: "pc".into(),
                reached: false,
            }),
            ..face(AgentRun::Done, None, Some("All green."))
        };
        faces.insert("d".into(), face.clone());
        assert!(faces.draws(&call), "its Delegate's row");
        assert!(!face.opens(), "a machine this window does not reach opens nothing");
        let row = agent_row(&call, Some(&face));
        assert_eq!(
            (row.title.as_str(), row.on.as_deref(), row.detail.as_str(), row.label),
            ("Run the login test", Some("on pc"), "All green.", None)
        );
    }

    /// A card counts its agents by kind, a Delegate under Ferrite's noun.
    #[test]
    fn a_card_of_delegates_says_delegates() {
        let mut transcript = Transcript::default();
        for (id, name) in [("a", "mcp__ferrite__delegate_task"), ("b", "delegate_task"), ("c", "Agent")] {
            transcript.apply(Input::Event(SessionEvent::ToolStarted {
                id: id.into(),
                name: name.into(),
                input: serde_json::json!({ "task": "x" }),
            }));
        }
        let calls: Vec<&ToolBlock> = transcript.blocks().iter().map(tool).collect();
        assert_eq!(text::agent_card_title(&calls[..2]), "2 delegates");
        assert_eq!(text::agent_card_title(&calls[1..]), "1 subagent and 1 delegate");
    }

    /// A refused call has no Delegate: it stays a tool row, its error kept.
    #[test]
    fn a_refused_delegate_call_is_no_agent_row() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Event(SessionEvent::ToolStarted {
            id: "d".into(),
            name: "mcp__ferrite__delegate_task".into(),
            input: serde_json::json!({ "task": "Run the login test" }),
        }));
        transcript.apply(Input::Event(SessionEvent::ToolCompleted {
            id: "d".into(),
            output: "target_required: Pass target.projectId".into(),
            is_error: true,
            result: ferrite_core::ToolResult::Opaque,
        }));
        let refused = tool(&transcript.blocks()[0]).clone();
        assert!(!AgentFaces::default().draws(&refused));
        assert!(matches!(&refused.state, ToolState::Failed(why) if why.contains("target_required")));
        assert_eq!(text::call_line(&refused).verb, "Failed to delegate ", "T3's sentence");
    }

    /// A call no agent answers for (a foreground call the activity has not
    /// seen) reads from the call alone: its result line, else its state.
    #[test]
    fn an_agent_row_without_its_agent_reads_its_call() {
        let blocks = agents(&[("a", "Agent")]);
        let mut call = tool(&blocks[0]).clone();
        assert_eq!(agent_row(&call, None).detail, "Running");
        call.state = ToolState::Ok;
        call.result_line = Some("All 12 files read".into());
        let row = agent_row(&call, None);
        assert_eq!(
            (row.standing, row.detail, row.label),
            (Standing::Done, "All 12 files read".into(), None)
        );
    }
}
