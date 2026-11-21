//! A Factory's GitHub starts and a window's picker lists, read off the
//! pump (ADR 0025) through core's `GitHub` seam.

use std::sync::mpsc;
use std::sync::Arc;

use ferrite_core::factory::{Command, GitHubQuery, GitHubRead};
use ferrite_core::server::github::GitHub;

use super::Server;

/// A read still out: whose, of which version, and its answer to come.
pub(super) type Reading = (String, u32, mpsc::Receiver<Result<GitHubRead, String>>);

/// A Task's PR head read still out.
pub(super) type HeadReading = (String, mpsc::Receiver<Result<String, String>>);

impl Server {
    /// Read a GitHub start on a thread of its own; the next pass hears it.
    pub(super) fn read_github(&mut self, workflow: String, version: u32, query: GitHubQuery) {
        let (answer, answered) = mpsc::channel();
        let github = Arc::clone(&self.github);
        std::thread::spawn(move || {
            ferrite_core::spawn::serve_the_operator();
            let _ = answer.send(github.matches(&query));
        });
        self.github_reads.push((workflow, version, answered));
    }

    /// Read a Task's PR head on a thread of its own; the next pass hears
    /// it.
    pub(super) fn read_head(&mut self, task: String, repository: String, number: u64) {
        let (answer, answered) = mpsc::channel();
        let github = Arc::clone(&self.github);
        std::thread::spawn(move || {
            ferrite_core::spawn::serve_the_operator();
            let _ = answer.send(github.head(&repository, number));
        });
        self.head_reads.push((task, answered));
    }

    /// What each finished read found, as the commands they are.
    pub(super) fn github_answers(&mut self) -> Vec<Command> {
        let mut heard = Vec::new();
        self.head_reads
            .retain(|(task, answer)| match answer.try_recv() {
                Ok(head) => {
                    heard.push(Command::GitHubHead {
                        task: task.clone(),
                        head,
                    });
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                Err(mpsc::TryRecvError::Disconnected) => false,
            });
        self.github_reads
            .retain(|(workflow, version, answer)| match answer.try_recv() {
                Ok(result) => {
                    heard.push(Command::GitHubRead {
                        workflow: workflow.clone(),
                        version: *version,
                        result,
                    });
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                Err(mpsc::TryRecvError::Disconnected) => {
                    heard.push(Command::GitHubRead {
                        workflow: workflow.clone(),
                        version: *version,
                        result: Err("the read ended".into()),
                    });
                    false
                }
            });
        heard
    }
}

/// A window's picker list, as its reply.
pub(super) fn list(
    github: &dyn GitHub,
    ask: &ferrite_core::protocol::GitHubAsk,
) -> Result<Option<ferrite_core::protocol::Done>, ferrite_core::protocol::WireError> {
    use ferrite_core::protocol::{ErrorCode, GitHubAsk, WireError};
    let names = match ask {
        GitHubAsk::Repositories => github.repositories(),
        GitHubAsk::Labels { repository } => github.labels(repository),
        GitHubAsk::Unknown => {
            return Err(WireError {
                code: ErrorCode::Unsupported,
                message: "this server does not know that GitHub list".into(),
            })
        }
    };
    names
        .map(|names| Some(ferrite_core::protocol::Done::Names { names }))
        .map_err(|message| WireError {
            code: ErrorCode::Failed,
            message,
        })
}

