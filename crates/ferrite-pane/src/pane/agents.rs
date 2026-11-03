//! Subagents in the transcript that started them: Claude's `Ran 2 agents`
//! card, one `Ran agent <task>` row per call, each row the way into that
//! agent's own transcript.
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

/// One subagent as its call's row shows it: how it stands, and while it
/// runs what it is doing (its latest progress summary, Claude's
/// `task_progress.summary`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentFace {
    pub run: AgentRun,
    pub summary: Option<String>,
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

/// An agent call's row, alone or in its card: `Ran agent` in `INK_SUBTLE`,
/// its task (the provider's description) a step up in `INK_STRONG`; while the
/// agent runs, `Running agent`, what it is doing after the seam
/// (`text::agent_line`) and the running spinner at the row's end; a failed
/// one's `Failed` tag, as any call's. No disclosure: `open`, laid over the
/// whole row, opens the agent's own transcript, and the chevron that shows
/// under the pointer says the row goes somewhere.
pub(crate) fn render_agent(
    block: BlockId,
    tool: &ToolBlock,
    row_cx: &RowCx,
    open: Option<AnyElement>,
) -> AnyElement {
    let grid = row_cx.grid();
    let small = grid.activity();
    let selection = row_cx.selection;
    let running = tool.state == ToolState::Running;
    let (line, task) = text::agent_line(tool);
    let mut highlights = separators(&line);
    highlights.push((
        task,
        HighlightStyle {
            color: Some(rgb(INK_STRONG).into()),
            ..Default::default()
        },
    ));
    highlights.sort_by_key(|(range, _)| range.start);
    let verdict = text::call_verdict(tool).map(|verdict| {
        let mut highlights = separators(&verdict);
        highlights.insert(
            0,
            (
                0..text::FAILED_TAG.len(),
                HighlightStyle {
                    color: Some(rgb(BLOCKED).into()),
                    ..Default::default()
                },
            ),
        );
        div()
            .debug_selector(|| "tool-verdict".into())
            .flex_shrink_0()
            .whitespace_nowrap()
            .ml(px(theme::SPACE_2))
            .child(selection.line(block, verdict, highlights))
    });
    let opens = open.is_some();
    let words = div()
        .flex_1()
        .child(
            div()
                .debug_selector(|| "agent-call".into())
                .flex_shrink(1.)
                .min_w_0()
                .when(opens, |words| {
                    words.group_hover(DISCLOSURE_ROW, |style| style.text_color(rgb(INK_STRONG)))
                })
                .child(edge_cut(
                    grid.line,
                    INK_SUBTLE,
                    div()
                        .pr(px(small.cell()))
                        .child(selection.line(block, line, highlights)),
                )),
        )
        .children(verdict)
        .children(opens.then(|| disclosure_chevron(false, false)))
        .child(div().flex_1())
        .children(running.then(|| {
            div()
                .debug_selector(|| "agent-running".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .ml(px(theme::SPACE_2))
                .child(components::running_spinner(theme::TASK_ICON, INK_SUBTLE))
        }));
    let row_selector = format!("agent-row-{}", tool.call);
    activity_line(grid, None, words)
        .id(SharedString::from(row_selector.clone()))
        .debug_selector(move || row_selector.clone())
        .group(DISCLOSURE_ROW)
        .children(open)
        .into_any_element()
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
            },
        );
        faces.insert(
            "b".into(),
            AgentFace {
                run: AgentRun::Failed,
                summary: None,
            },
        );
        faces.insert(
            "c".into(),
            AgentFace {
                run: AgentRun::Done,
                summary: None,
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

    #[test]
    fn an_agent_row_reads_as_claudes() {
        let blocks = agents(&[("a", "Agent")]);
        let mut running = tool(&blocks[0]).clone();
        running.result_line = Some("Reading nav.rs".into());
        let (line, task) = text::agent_line(&running);
        assert_eq!(line, "Running agent Count files \u{b7} Reading nav.rs");
        assert_eq!(&line[task], "Count files");
        let mut done = running.clone();
        done.state = ToolState::Ok;
        let (line, task) = text::agent_line(&done);
        assert_eq!(line, "Ran agent Count files");
        assert_eq!(&line[task], "Count files");
    }
}
