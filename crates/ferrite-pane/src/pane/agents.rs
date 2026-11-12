//! Subagents in the transcript that started them: T3's subagent card over
//! two or more started together, one agent row per call, each row the way
//! into that agent's own transcript.
//!
//! A call's row says how its agent stands, not how its call does: a
//! background agent's call returns at once (`Async agent launched`) while
//! the agent works on. So the cockpit hands the transcript each agent's
//! standing ([`AgentFaces`]), and the calls take it on before they are
//! grouped and drawn ([`AgentFaces::apply`]): the group's tense, its failure
//! count and the shut card's live rows then read as any group's do.
use super::*;
use std::collections::BTreeMap;

/// How a subagent stands, for its call's row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentRun {
    /// Starting, working, or waiting on the operator.
    Running,
    Done,
    Failed,
}

/// One subagent as its call's row shows it: how it stands, while it runs
/// what it is doing (its latest progress summary, Claude's
/// `task_progress.summary`), and how long it has run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFace {
    pub run: AgentRun,
    pub summary: Option<String>,
    pub clock: Option<AgentClock>,
}

/// How long a subagent has run: since its turn began while it runs (read
/// as the row draws), else the time its last turn took.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentClock {
    Since(std::time::Instant),
    Took(std::time::Duration),
}

impl AgentClock {
    pub fn elapsed(self) -> std::time::Duration {
        match self {
            Self::Since(at) => ferrite_core::clock::instant().saturating_duration_since(at),
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
            Self::Took(took) => SharedString::from(agent_elapsed(took)).into_any_element(),
        }
    }

    /// The longest of `clocks`: a card's, its agents' together.
    pub(crate) fn longest(clocks: impl Iterator<Item = Self>) -> Option<Self> {
        clocks.max_by_key(|clock| clock.elapsed())
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

    /// The agent calls in `blocks` take on their agents' standing: running
    /// while the agent runs, its summary as the call's result line (what it
    /// is doing now); failed when it failed; settled once it is done.
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
                AgentRun::Running => {
                    tool.state = ToolState::Running;
                    tool.result_line = face.summary.clone();
                }
                AgentRun::Failed if !matches!(tool.state, ToolState::Failed(_)) => {
                    tool.state = ToolState::Failed(String::new());
                }
                AgentRun::Done if tool.state == ToolState::Running => {
                    tool.state = ToolState::Ok;
                }
                AgentRun::Failed | AgentRun::Done => {}
            }
        }
    }
}

/// Whether a call starts a subagent, and so draws as [`render_agent`].
pub(crate) fn is_agent(tool: &ToolBlock) -> bool {
    ferrite_core::transcript::tools::policy(&tool.name).kind
        == ferrite_core::transcript::tools::ToolKind::Agent
}

/// An agent call's row, alone or in its card (T3's `SubagentTimelineLink`):
/// the agent's avatar with its status dot, its task over what it does
/// (its progress while it runs, else its standing), the standing again at
/// the right while it is not done, how long it has run, and a `›` that
/// brightens under the pointer. `open`, laid over the whole row, opens the
/// agent's own transcript. `targeted`: the keyboard's walk has landed on
/// the row (its focus, whose keys open the agent and stop it); it wears
/// `paint::RAISED`, as a targeted disclosure does.
pub(crate) fn render_agent(
    block: BlockId,
    tool: &ToolBlock,
    row_cx: &RowCx,
    face: AgentRowCx,
) -> AnyElement {
    let AgentRowCx {
        clock,
        open,
        targeted,
        live,
        ground,
    } = face;
    let selection = row_cx.selection;
    let standing = Standing::of(tool);
    let detail = text::agent_detail(tool);
    let title = div()
        .debug_selector(|| "agent-call".into())
        .min_w_0()
        .truncate()
        .text_size(px(theme::FS_BADGE))
        .line_height(px(theme::LH_META))
        .font_weight(theme::W_LABEL)
        .text_color(rgb(INK_STRONG))
        .child(selection.line(block, text::agent_title(tool), Vec::new()));
    let detail_line = div()
        .debug_selector(|| "agent-detail".into())
        .min_w_0()
        .truncate()
        .text_size(px(theme::AGENT_DETAIL_FS))
        .line_height(px(theme::AGENT_DETAIL_LH))
        .text_color(rgb(INK_SUBTLE))
        .child(selection.line(block, detail.clone(), Vec::new()));
    let label = (standing != Standing::Done && detail != standing.word()).then(|| {
        div()
            .flex_shrink_0()
            .text_size(px(theme::FS_BADGE))
            .text_color(rgb(standing.ink()))
            .child(standing.word())
    });
    let elapsed = clock.map(|clock| {
        div()
            .debug_selector(|| "agent-elapsed".into())
            .flex_shrink_0()
            .text_size(px(theme::FS_BADGE))
            .text_color(rgb(INK_SUBTLE))
            .child(clock.face())
    });
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
        .child(avatar(Some(standing), ground))
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
            div()
                .flex_shrink_0()
                .text_color(rgb(theme::TEXT_FAINT))
                .group_hover(DISCLOSURE_ROW, |style| style.text_color(rgb(INK_STRONG)))
                .child(icons::icon(icons::CHEVRON_RIGHT, theme::AGENT_CHEVRON, theme::TEXT_FAINT))
        }))
        .children(open)
        // Over the opener, so a press on it opens the tasks pane instead.
        .children(live);
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

/// What an agent row draws besides its call: how long its agent has run,
/// its opener, the keyboard's target, its background link, and the ground
/// its avatar's ring is cut from.
pub(crate) struct AgentRowCx<'a> {
    pub clock: Option<AgentClock>,
    pub open: Option<AnyElement>,
    pub targeted: Option<&'a gpui::FocusHandle>,
    pub live: Option<AnyElement>,
    pub ground: u32,
}

/// Two or more agents started together (T3's `V2SubagentGroup`): a
/// trigger row (up to `AGENT_AVATARS` stacked avatars and `+N`, `3
/// subagents` over how they stand, how long they have run, a chevron that
/// turns when it opens), and open, a panel of their rows that snaps open.
/// The trigger recedes to `AGENT_CARD_SETTLED_OPACITY` once every agent is
/// done and it is shut, coming up under the pointer.
pub(crate) fn render_agent_card(
    members: &[Block],
    row_cx: &RowCx,
    card: AgentCardCx,
) -> AnyElement {
    let AgentCardCx {
        disclosure,
        open,
        clock,
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
    let shown = tools.len().min(theme::AGENT_AVATARS);
    let more = tools.len() - shown;
    let stack = div()
        .flex()
        .flex_shrink_0()
        .children((0..shown).map(|at| {
            avatar(None, ground).when(at > 0, |avatar| avatar.ml(px(-theme::AGENT_AVATAR_OVERLAP)))
        }))
        .children((more > 0).then(|| {
            avatar_disc(ground)
                .ml(px(-theme::AGENT_AVATAR_OVERLAP))
                .text_size(px(theme::AGENT_MORE_FS))
                .font_weight(theme::W_LABEL)
                .text_color(rgb(INK_SUBTLE))
                .child(SharedString::from(format!("+{more}")))
        }));
    let status_ink = match () {
        _ if failed => theme::BLOCKED,
        _ if active => theme::BLUE,
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
                .text_color(rgb(status_ink))
                .child(row_cx.selection.line(leader.id, text::agent_card_status(&tools), Vec::new())),
        );
    let receded = !open && !active;
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
        .when(receded, |row| {
            row.opacity(theme::AGENT_CARD_SETTLED_OPACITY)
                .hover(|style| style.opacity(1.))
        })
        .child(stack)
        .child(words)
        .children(clock.map(|clock| {
            div()
                .debug_selector(|| "agent-card-elapsed".into())
                .flex_shrink_0()
                .text_size(px(theme::FS_BADGE))
                .text_color(rgb(INK_SUBTLE))
                .child(clock.face())
        }))
        .child(
            icons::icon(
                if open { icons::CHEVRON_UP } else { icons::CHEVRON_DOWN },
                theme::AGENT_CHEVRON,
                INK_SUBTLE,
            )
            .flex_shrink_0(),
        )
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
/// it is open, how long its agents have run, their rows (open), and the
/// ground its avatars' rings are cut from.
pub(crate) struct AgentCardCx {
    pub disclosure: Option<Disclosure>,
    pub open: bool,
    pub clock: Option<AgentClock>,
    pub rows: Vec<AnyElement>,
    pub ground: u32,
}

/// An agent's standing as its row and its card count it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Standing {
    Running,
    Done,
    Failed,
    Stopped,
}

impl Standing {
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
            Self::Done => "Completed",
            Self::Failed => "Failed",
            Self::Stopped => "Stopped",
        }
    }

    /// Its status dot's ink: info while it runs, success done,
    /// destructive failed, muted stopped.
    fn ink(self) -> u32 {
        match self {
            Self::Running => theme::BLUE,
            Self::Done => theme::GREEN,
            Self::Failed => theme::BLOCKED,
            Self::Stopped => theme::TEXT_FAINT,
        }
    }
}

/// A subagent's avatar (T3's `SubagentAvatar`): an `AGENT_AVATAR` disc
/// holding the bot, ringed in the ground; with a `standing`, its status
/// dot at the foot of its right side.
fn avatar(standing: Option<Standing>, ground: u32) -> Div {
    avatar_disc(ground)
        .relative()
        .child(icons::icon(icons::BOT, theme::AGENT_AVATAR_ICON, INK_SUBTLE))
        .children(standing.map(|standing| {
            div()
                .when(standing == Standing::Running, |dot| {
                    dot.debug_selector(|| "agent-running".into())
                })
                .absolute()
                .right(px(-1.))
                .bottom(px(-1.))
                .size(px(theme::AGENT_DOT))
                .rounded_full()
                .bg(rgb(standing.ink()))
                .shadow(vec![ring(ground)])
        }))
}

fn avatar_disc(ground: u32) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(theme::AGENT_AVATAR))
        .rounded_full()
        .border_1()
        .border_color(gpui::Hsla::from(theme::paint::LINE).opacity(0.7))
        .bg(rgba(theme::AGENT_AVATAR_FILL))
        .shadow(vec![ring(ground)])
}

/// T3's `ring-2 ring-background`: the ground cut 2px round a disc.
fn ring(ground: u32) -> gpui::BoxShadow {
    gpui::BoxShadow {
        inset: false,
        color: rgb(ground).into(),
        offset: gpui::point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(theme::AGENT_RING),
    }
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
            AgentFace {
                run: AgentRun::Running,
                summary: Some("Reading nav.rs".into()),
                clock: None,
            },
        );
        faces.insert(
            "b".into(),
            AgentFace {
                run: AgentRun::Failed,
                summary: None,
                clock: None,
            },
        );
        faces.insert(
            "c".into(),
            AgentFace {
                run: AgentRun::Done,
                summary: None,
                clock: None,
            },
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

    /// An agent row reads as T3's: its task over what it does while it
    /// runs, else its standing; a card counts its agents by how they stand.
    #[test]
    fn an_agent_row_and_its_card_read_as_t3s() {
        let blocks = agents(&[("a", "Agent"), ("b", "Agent"), ("c", "Agent")]);
        let mut running = tool(&blocks[0]).clone();
        running.result_line = Some("Reading nav.rs".into());
        assert_eq!(text::agent_title(&running), "Count files");
        assert_eq!(text::agent_detail(&running), "Reading nav.rs");
        let mut done = running.clone();
        done.state = ToolState::Ok;
        assert_eq!(text::agent_detail(&done), "Completed");
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
}
