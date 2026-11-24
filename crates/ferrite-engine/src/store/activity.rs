//! A child's Activity read back off its log: the aliases its key was given,
//! and the bounded projection a Subagent's history replays.

use ferrite_core::activity::{
    ActivityEvent, AgentInfo, AgentKey, AgentStatus, ExecutionEvent, Subject, TranscriptCoverage,
};
use ferrite_core::thread_log::activity::{Execution, PersistedActivity, PersistedSubject};
use ferrite_core::thread_log::Outcome;
use std::time::Duration;

#[derive(Default)]
pub(super) struct Aliases(std::collections::HashMap<String, String>);

impl Aliases {
    pub(super) fn new(records: &[super::Record]) -> Self {
        Self(
            records
                .iter()
                .filter_map(|record| match record {
                    super::Record::Activity {
                        observation: PersistedActivity::Alias { from, to },
                    } => Some((from.clone(), to.clone())),
                    _ => None,
                })
                .collect(),
        )
    }

    fn resolve(&self, start: &str) -> String {
        let mut key = start;
        let mut seen = std::collections::HashSet::new();
        while seen.insert(key) {
            let Some(next) = self.0.get(key) else {
                break;
            };
            key = next;
        }
        key.to_string()
    }

    pub(super) fn subject(&self, subject: Subject) -> Subject {
        match subject {
            Subject::Main => Subject::Main,
            Subject::Subagent(key) => {
                Subject::Subagent(AgentKey::from_stored(self.resolve(key.as_str())))
            }
        }
    }
}

/// Reads a single stable inode twice: aliases first, then the bounded child
/// projection. The checkpoint limits both scans before JSON decoding.
/// Regular Thread loading deliberately keeps its existing snapshot behavior.
pub(super) fn read_agent_inputs(
    mut file: std::fs::File,
    thread: ferrite_core::ThreadId,
    requested: &AgentKey,
    through: u64,
    limits: ferrite_core::activity::ActivityLimits,
) -> Result<Vec<ferrite_core::activity::ActivityInput>, ferrite_core::header::LoadError> {
    use std::io::Seek;
    if file.metadata()?.len() < through {
        return Err(ferrite_core::header::LoadError::Corrupt {
            detail: format!("thread {thread} is shorter than its history checkpoint"),
        });
    }
    let mut aliases = Aliases::default();
    for record in RecordReader::new(&mut file, thread, through)? {
        if let super::Record::Activity {
            observation: PersistedActivity::Alias { from, to },
        } = record?
        {
            aliases.0.insert(from, to);
        }
    }
    file.rewind()?;
    project_agent(
        RecordReader::new(&mut file, thread, through)?,
        aliases,
        requested,
        limits,
    )
}

/// A crash may leave a torn UTF-8/JSON tail. Match Store::load by stopping at
/// the first unreadable body record; malformed/future headers still fail.
/// Peak temporary data is one encoded line and one decoded record. The line
/// itself is not capped, preserving old logs and unusually large tool output.
struct RecordReader<'a> {
    reader: std::io::BufReader<std::io::Take<&'a mut std::fs::File>>,
    line: Vec<u8>,
    ended: bool,
}

struct EncodedSize(usize);
impl std::io::Write for EncodedSize {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.saturating_add(bytes.len());
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn encoded_size(observation: &PersistedActivity) -> usize {
    let mut size = EncodedSize(0);
    match serde_json::to_writer(&mut size, observation) {
        Ok(()) => size.0,
        Err(_) => usize::MAX,
    }
}
impl<'a> RecordReader<'a> {
    fn new(
        file: &'a mut std::fs::File,
        thread: ferrite_core::ThreadId,
        through: u64,
    ) -> Result<Self, ferrite_core::header::LoadError> {
        use std::io::{BufRead, Read};
        let mut reader = std::io::BufReader::new(file.take(through));
        let mut line = Vec::new();
        reader.read_until(b'\n', &mut line)?;
        let header: super::Header =
            serde_json::from_slice(&line).map_err(|_| ferrite_core::header::LoadError::Corrupt {
                detail: format!("thread {thread} has no readable header"),
            })?;
        if header.schema > super::SCHEMA_VERSION {
            return Err(ferrite_core::header::LoadError::FutureSchema {
                found: header.schema,
                supported: super::SCHEMA_VERSION,
            });
        }
        Ok(Self {
            reader,
            line,
            ended: false,
        })
    }
}
impl Iterator for RecordReader<'_> {
    type Item = Result<super::Record, ferrite_core::header::LoadError>;
    fn next(&mut self) -> Option<Self::Item> {
        use std::io::BufRead;
        if self.ended {
            return None;
        }
        self.line.clear();
        match self.reader.read_until(b'\n', &mut self.line) {
            Ok(0) => {
                self.ended = true;
                None
            }
            Ok(_) => match serde_json::from_slice(&self.line) {
                Ok(record) => Some(Ok(record)),
                Err(_) => {
                    self.ended = true;
                    None
                }
            },
            Err(error) => {
                self.ended = true;
                Some(Err(error.into()))
            }
        }
    }
}

#[cfg(test)]
pub(super) fn agent_inputs(
    records: &[super::Record],
    requested: &AgentKey,
    limits: ferrite_core::activity::ActivityLimits,
) -> Vec<ferrite_core::activity::ActivityInput> {
    project_agent(
        records.iter().map(Ok),
        Aliases::new(records),
        requested,
        limits,
    )
    .expect("infallible record slice")
}

fn project_agent<R: std::borrow::Borrow<super::Record>>(
    records: impl IntoIterator<Item = Result<R, ferrite_core::header::LoadError>>,
    aliases: Aliases,
    requested: &AgentKey,
    limits: ferrite_core::activity::ActivityLimits,
) -> Result<Vec<ferrite_core::activity::ActivityInput>, ferrite_core::header::LoadError> {
    use ferrite_core::activity::ActivityInput;
    use std::collections::{HashMap, VecDeque};

    let canonical = aliases.resolve(requested.as_str());
    let key = AgentKey::from_stored(canonical.clone());
    let mut info = AgentInfo::new(key.clone());
    let mut coverage = TranscriptCoverage::Unavailable;
    let mut seen = false;
    let mut detached = false;
    let mut omitted = false;
    let mut bytes = 0usize;
    let mut last_status = AgentStatus::Unknown;
    let mut content: VecDeque<(ActivityEvent, usize, Option<(String, Duration)>)> = VecDeque::new();
    for record in records {
        let record = record?;
        let super::Record::Activity { observation } = record.borrow() else {
            continue;
        };
        let observed_key = match observation {
            PersistedActivity::Discovered { key, .. }
            | PersistedActivity::Content { key, .. }
            | PersistedActivity::HistoryContent { key, .. }
            | PersistedActivity::Status { key, .. }
            | PersistedActivity::Coverage { key, .. }
            | PersistedActivity::Detached { key } => key,
            PersistedActivity::CompletionObservation {
                subject: PersistedSubject::Subagent { key },
                ..
            } => key,
            _ => continue,
        };
        if aliases.resolve(observed_key) != canonical {
            continue;
        }
        seen = true;
        let size = encoded_size(observation);
        let historical_content = match observation {
            PersistedActivity::Discovered { .. } => {
                let ActivityEvent::Discovered(discovered) = observation.live() else {
                    unreachable!()
                };
                if discovered.parent.is_some() {
                    info.parent = discovered.parent.map(|parent| aliases.subject(parent));
                    detached = false;
                }
                if discovered.name.is_some() {
                    info.name = discovered.name;
                }
                if discovered.description.is_some() {
                    info.description = discovered.description;
                }
                if discovered.kind.is_some() {
                    info.kind = discovered.kind;
                }
                if discovered.coverage != TranscriptCoverage::Unavailable {
                    coverage = discovered.coverage;
                }
                None
            }
            PersistedActivity::Coverage {
                coverage: value, ..
            } => {
                coverage = value.live();
                None
            }
            PersistedActivity::Detached { .. } => {
                detached = true;
                None
            }
            PersistedActivity::Status { state, .. } => {
                let previous = last_status;
                last_status = state.live();
                // Replaying Status against the current live status can omit
                // an old interruption/failure row. Preserve the historical
                // boundary as content, without resurrecting lifecycle state.
                let outcome = (previous != last_status)
                    .then(|| last_status.turn_outcome())
                    .flatten();
                outcome.map(|outcome| ActivityEvent::HistoryContent {
                    key: key.clone(),
                    id: None,
                    event: ExecutionEvent::TurnEnded {
                        outcome,
                        cost_usd: None,
                    },
                })
            }
            PersistedActivity::Content { id, event, .. }
            | PersistedActivity::HistoryContent { id, event, .. } => {
                if matches!(observation, PersistedActivity::Content { .. }) {
                    match event {
                        Execution::TextDelta { .. }
                        | Execution::ThinkingDelta { .. }
                        | Execution::ReasoningSummaryDelta { .. }
                        | Execution::ReasoningSummaryPart {
                            snapshot: false, ..
                        }
                        | Execution::ToolOutputDelta { .. }
                        | Execution::Text { .. }
                        | Execution::Thinking { .. }
                        | Execution::TextSnapshot { .. }
                        | Execution::ThinkingSnapshot { .. }
                        | Execution::Prompt { .. }
                        | Execution::ToolStarted { .. } => last_status = AgentStatus::Working,
                        Execution::TurnEnded { outcome, .. } => {
                            last_status = match outcome {
                                Outcome::Completed => AgentStatus::Idle,
                                Outcome::Interrupted => AgentStatus::Interrupted,
                                Outcome::Error(_) => AgentStatus::Failed,
                            }
                        }
                        _ => {}
                    }
                }
                if coverage == TranscriptCoverage::Unavailable {
                    coverage = TranscriptCoverage::Live;
                }
                if size > limits.content_bytes_per_subject {
                    omitted = true;
                    continue;
                }
                Some(ActivityEvent::HistoryContent {
                    key: key.clone(),
                    id: id.clone(),
                    event: event.live(),
                })
            }
            PersistedActivity::CompletionObservation { .. } => Some(observation.live()),
            _ => None,
        };
        let Some(event) = historical_content else {
            continue;
        };
        // Serialization cost is a conservative proxy for retained strings/JSON.
        // A single oversized record stays only on disk.
        if size > limits.content_bytes_per_subject {
            omitted = true;
            continue;
        }
        let timing = observation
            .tool_duration()
            .map(|(_, id, duration)| (id, duration));
        content.push_back((event, size, timing));
        bytes = bytes.saturating_add(size);
        while bytes > limits.content_bytes_per_subject
            || content.len() > limits.blocks_per_subject.saturating_mul(8).max(1)
        {
            if let Some((_, removed, _)) = content.pop_front() {
                bytes = bytes.saturating_sub(removed);
                omitted = true;
            }
        }
    }
    if !seen {
        return Ok(Vec::new());
    }
    if omitted {
        coverage = TranscriptCoverage::Partial;
    }
    info.coverage = coverage;
    let subject = Subject::Subagent(key.clone());
    let mut inputs = vec![
        ActivityInput::Retain(subject.clone()),
        ActivityInput::ReplayEvent(ActivityEvent::Discovered(info)),
    ];
    let mut timings = HashMap::new();
    for (event, _, duration) in content {
        inputs.push(ActivityInput::ReplayEvent(event));
        if let Some((id, duration)) = duration {
            timings.insert(id, duration);
        }
    }
    inputs.push(ActivityInput::ReplayEvent(ActivityEvent::Coverage {
        key: key.clone(),
        coverage,
    }));
    inputs.push(ActivityInput::RestoreTimings { subject, timings });
    if detached {
        inputs.push(ActivityInput::ReplayEvent(ActivityEvent::Detached { key }));
    }
    Ok(inputs)
}
