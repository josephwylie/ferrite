//! A fixture world's Factory (ADR 0037): the window tests and visual
//! scenes, which have no Server, have their Factory asks answered here as
//! a Server's Factory answers them. A real window never holds one: every
//! ask goes to its Server.

use std::io;

use crate::factory::{Board, Factory};
use crate::protocol::{Done, GitHubAsk, Op, TaskAction};
use crate::store::Provider;

#[derive(Default)]
pub(crate) struct FactoryFixture {
    /// Its own Factory, answering Workflow and Coordinator asks
    /// (`fixture_live_factory`).
    pub(crate) factory: Option<Factory>,
    /// Its Task asks, kept to be read back (`fixture_record_tasks`).
    pub(crate) asks: Option<Vec<TaskAction>>,
    /// What its GitHub login lists (`fixture_github`): each repository and
    /// its labels, in place of `gh`.
    pub(crate) github: Option<Vec<(String, Vec<String>)>>,
}

impl FactoryFixture {
    /// `op` answered here, with the board the window is to be told, or
    /// `None` when this fixture does not answer it.
    pub(crate) fn execute(&mut self, op: Op) -> Option<(io::Result<Option<Done>>, Option<Board>)> {
        let agent = crate::factory::Agent {
            provider: Provider::Claude,
            model: None,
            effort: None,
        };
        let command = match op {
            Op::Task { action } => {
                let asks = self.asks.as_mut()?;
                asks.push(action);
                return Some((Ok(Some(Done::Changed { changed: true })), None));
            }
            Op::Workflow { action } => {
                self.factory.as_ref()?;
                crate::factory::workflow_command(action, &agent, Vec::new())
            }
            Op::Coordinator { action } => {
                self.factory.as_ref()?;
                crate::factory::coordinator_command(action, &agent, Default::default())
            }
            _ => return None,
        };
        let factory = self.factory.as_mut()?;
        let Some(command) = command else {
            return Some((
                Err(io::Error::new(io::ErrorKind::Unsupported, "unknown ask")),
                None,
            ));
        };
        let answer = factory
            .command(command, 0)
            .map(|answer| Some(crate::factory::done(answer)))
            .map_err(|refusal| io::Error::other(refusal.message));
        Some((answer, Some(factory.board())))
    }

    /// A command its Factory hears as a Server's would (a GitHub read's
    /// answer, say), and the board the window is to be told.
    pub(crate) fn hear(&mut self, command: crate::factory::Command) -> Option<Board> {
        let factory = self.factory.as_mut()?;
        if let Err(refusal) = factory.command(command, 0) {
            crate::log!("fixture: the Factory refused: {refusal}");
        }
        Some(factory.board())
    }

    /// What its GitHub login lists for `ask`, if it stands in for one.
    pub(crate) fn github_names(&self, ask: &GitHubAsk) -> Option<Vec<String>> {
        let world = self.github.as_ref()?;
        Some(match ask {
            GitHubAsk::Repositories => world.iter().map(|(name, _)| name.clone()).collect(),
            GitHubAsk::Labels { repository } => world
                .iter()
                .find(|(name, _)| name == repository)
                .map(|(_, labels)| labels.clone())
                .unwrap_or_default(),
            GitHubAsk::Unknown => Vec::new(),
        })
    }
}
