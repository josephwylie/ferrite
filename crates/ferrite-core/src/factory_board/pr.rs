//! A Task's pull request and evidence (#223) as a window shows them: the
//! PR it carries with every revision reported, the evidence kept apart
//! by kind, and what a merge Step can report. Changing them is the
//! Factory's (`factory::pr`).

use serde::{Deserialize, Serialize};

use super::{Destination, Task};

/// What a Task holds of its delivery: its PR, the evidence, and why its
/// worktree stayed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Delivery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<PullRequest>,
    /// Agent conclusions, tool results and operator decisions, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<Evidence>,
    /// Why a completed Task's worktree stays, when it does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept: Option<String>,
    /// When removing a merged Task's kept worktree was last tried: it is
    /// tried again every `factory::TIDY_AGAIN_MS` until it goes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tidied_ms: Option<i64>,
}

/// A GitHub pull request, as `owner/repository#number`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequest {
    pub repository: String,
    pub number: u64,
    /// Every head reported, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revisions: Vec<Revision>,
    /// The revision approval was given for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved: Option<u32>,
    /// A merge Step confirmed it merged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub merged: bool,
}

/// One head of the PR, as a Worker reported it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    /// 1, 2, …
    pub revision: u32,
    /// The head commit.
    pub head: String,
    /// Who reported it: a Step's name, or "The agent" for a one-off.
    pub by: String,
    pub at_ms: i64,
}

impl PullRequest {
    /// `owner/repository#number`, or a `github.com` pull request URL.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let bad = || {
            format!(
                "`{text}` is no pull request: give owner/repository#number or its \
                 https://github.com/owner/repository/pull/number URL"
            )
        };
        let (repository, number) = match text.split_once('#') {
            Some((repository, number)) => (repository.to_string(), number),
            None => {
                let path = text
                    .strip_prefix("https://")
                    .or_else(|| text.strip_prefix("http://"))
                    .unwrap_or(text);
                let path = path
                    .strip_prefix("www.github.com/")
                    .or_else(|| path.strip_prefix("github.com/"))
                    .ok_or_else(bad)?;
                let path = path.split(['?', '#']).next().unwrap_or_default();
                let parts: Vec<&str> = path.split('/').collect();
                match parts.as_slice() {
                    [owner, name, "pull", number, ..] => (format!("{owner}/{name}"), *number),
                    _ => return Err(bad()),
                }
            }
        };
        let named = |part: &str| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        let valid = repository
            .split_once('/')
            .is_some_and(|(owner, name)| named(owner) && named(name));
        let number: u64 = number.parse().map_err(|_| bad())?;
        if !valid || number == 0 {
            return Err(bad());
        }
        Ok(Self {
            repository,
            number,
            revisions: Vec::new(),
            approved: None,
            merged: false,
        })
    }

    /// `owner/repository#number`.
    pub fn label(&self) -> String {
        format!("{}#{}", self.repository, self.number)
    }

    pub fn url(&self) -> String {
        format!(
            "https://github.com/{}/pull/{}",
            self.repository, self.number
        )
    }

    /// The latest revision reported.
    pub fn current(&self) -> Option<&Revision> {
        self.revisions.last()
    }

    pub fn revision(&self, revision: u32) -> Option<&Revision> {
        self.revisions.iter().find(|each| each.revision == revision)
    }

    /// The approval covers the current revision.
    pub fn approval_current(&self) -> bool {
        self.approved.is_some() && self.approved == self.current().map(|each| each.revision)
    }
}

/// A head's first seven characters, as the operator reads it.
pub fn short(head: &str) -> &str {
    &head[..head.len().min(7)]
}

/// What kind of evidence: kept apart, so an agent's word is never read
/// as a tool's result or the operator's decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// What an agent concluded: its reported outcome.
    Conclusion,
    /// What a command or tool showed, as the agent reported it.
    Tool,
    /// What the operator decided.
    Decision,
}

impl EvidenceKind {
    pub fn label(self) -> &'static str {
        match self {
            EvidenceKind::Conclusion => "Agent conclusion",
            EvidenceKind::Tool => "Tool result",
            EvidenceKind::Decision => "Your decision",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: EvidenceKind,
    /// Who: a Step's name, "The agent", or "You".
    pub by: String,
    pub text: String,
    /// The PR revision current when it was given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u32>,
    pub at_ms: i64,
}

/// What a merge Step's Worker reports: six distinct results, of which
/// only `merged` may complete the Task.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MergeOutcome {
    Merged,
    Conflict,
    Pending,
    Denied,
    HeadChanged,
    Unknown,
}

impl MergeOutcome {
    pub const ALL: [MergeOutcome; 6] = [
        MergeOutcome::Merged,
        MergeOutcome::Conflict,
        MergeOutcome::Pending,
        MergeOutcome::Denied,
        MergeOutcome::HeadChanged,
        MergeOutcome::Unknown,
    ];

    /// Its wire word: `merged`, `conflict`, `pending`, `denied`,
    /// `head_changed`, `unknown`.
    pub fn word(self) -> &'static str {
        match self {
            MergeOutcome::Merged => "merged",
            MergeOutcome::Conflict => "conflict",
            MergeOutcome::Pending => "pending",
            MergeOutcome::Denied => "denied",
            MergeOutcome::HeadChanged => "head_changed",
            MergeOutcome::Unknown => "unknown",
        }
    }

    /// As a route's row names it.
    pub fn label(self) -> &'static str {
        match self {
            MergeOutcome::Merged => "Merge confirmed",
            MergeOutcome::Conflict => "Merge conflicts",
            MergeOutcome::Pending => "Checks pending",
            MergeOutcome::Denied => "Access denied",
            MergeOutcome::HeadChanged => "PR changed since approval",
            MergeOutcome::Unknown => "Result unconfirmed",
        }
    }

    /// What the agent is to report it for.
    pub fn help(self) -> &'static str {
        match self {
            MergeOutcome::Merged => "you confirmed the PR is merged",
            MergeOutcome::Conflict => "the PR has merge conflicts",
            MergeOutcome::Pending => "required checks have not finished",
            MergeOutcome::Denied => "your tools lack permission to merge",
            MergeOutcome::HeadChanged => {
                "the PR's head is not the approved revision: new commits arrived"
            }
            MergeOutcome::Unknown => "you could not confirm what happened",
        }
    }
}

/// A merge Step's route for each result: they replace its "When
/// finished" route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeRoutes {
    pub merged: Destination,
    pub conflict: Destination,
    pub pending: Destination,
    pub denied: Destination,
    pub head_changed: Destination,
    pub unknown: Destination,
}

impl Default for MergeRoutes {
    /// Merged completes, pending keeps waiting, the rest need you.
    fn default() -> Self {
        Self {
            merged: Destination::Complete,
            conflict: Destination::Human,
            pending: Destination::Wait,
            denied: Destination::Human,
            head_changed: Destination::Human,
            unknown: Destination::Human,
        }
    }
}

impl MergeRoutes {
    pub fn get(&self, outcome: MergeOutcome) -> &Destination {
        match outcome {
            MergeOutcome::Merged => &self.merged,
            MergeOutcome::Conflict => &self.conflict,
            MergeOutcome::Pending => &self.pending,
            MergeOutcome::Denied => &self.denied,
            MergeOutcome::HeadChanged => &self.head_changed,
            MergeOutcome::Unknown => &self.unknown,
        }
    }

    pub fn set(&mut self, outcome: MergeOutcome, to: Destination) {
        *match outcome {
            MergeOutcome::Merged => &mut self.merged,
            MergeOutcome::Conflict => &mut self.conflict,
            MergeOutcome::Pending => &mut self.pending,
            MergeOutcome::Denied => &mut self.denied,
            MergeOutcome::HeadChanged => &mut self.head_changed,
            MergeOutcome::Unknown => &mut self.unknown,
        } = to;
    }
}

impl Task {
    /// The PR's current revision, if one was reported.
    pub fn revision_now(&self) -> Option<u32> {
        self.delivery
            .pr
            .as_ref()
            .and_then(|pr| pr.current())
            .map(|each| each.revision)
    }

    /// Keep `text` as evidence of `kind`, about the current revision.
    pub(crate) fn evidence(
        &mut self,
        kind: EvidenceKind,
        by: impl Into<String>,
        text: impl Into<String>,
        at_ms: i64,
    ) {
        let revision = self.revision_now();
        self.delivery.evidence.push(Evidence {
            kind,
            by: by.into(),
            text: text.into(),
            revision,
            at_ms,
        });
    }
}
