//! Event-sourced Thread log: what makes a Thread outlive every process.
//!
//! Files live under a directory the caller passes — one subdirectory per
//! Thread, one JSONL log inside. The persisted schema is the store's own,
//! versioned from day one and converted internally from `SessionEvent`:
//! live-model churn is never a data migration. Writers buffer in memory and
//! flush on boundary marks (turn end, close) or a timeout — a durable write
//! per delta is impossible by interface shape.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::workspace::registry::ProjectId;
use crate::workspace::WorkspaceBinding;
use crate::SessionEvent;
use crate::{transcript::Input, ThreadId};

/// The schema this store writes. Every log names the schema it was written
/// at in its header line; `load` accepts this version and every version
/// before it, and refuses anything newer — a log from Ferrite's future must
/// fail loudly, not half-read.
///
/// History:
/// - **1** — header + event records converted from the Session stream.
/// - **2** — `prompt` records (the operator's own lines are history too) and
///   the structured `result` on `tool_completed` (a diff card cannot be
///   redrawn from prose). A v1 log loads with no prompts and every result
///   `Opaque` — exactly what v1 recorded, nothing invented.
/// - **3** — the workspace binding in the header (the checkout a Thread
///   works in must survive a restart). A v1/v2 log loads with no binding —
///   those Threads never recorded where they worked.
/// - **4** — the session project root in the header (#24): the git repo
///   inside the binding where the Thread's work happens. A v1–v3 log loads
///   with none — those Threads work in the binding itself, which is also
///   what `None` means today.
/// - **5** — the chosen model in the header (#25): what a pre-first-prompt
///   pick recorded, verbatim as the provider announced it. A v1–v4 log
///   loads with none — the provider's default, which is also what absent
///   means.
/// - **6** — the registered project in the header (#29): the registry id of
///   the project the Thread's CWD choice named. A v1–v5 log loads with none
///   — the resolved binding paths stay the durable truth either way, so a
///   Thread without one loses nothing but a grouping key.
/// - **7** — an optional operator-chosen Thread title. Older Threads keep
///   their generated `thread-NN` label until renamed.
/// - **8** — the chosen reasoning effort in the header, beside the model,
///   and the `handover` record: a provider switch after the first prompt,
///   which tells a revive that the last Init belongs to a provider no
///   longer serving. A v1–v7 log loads with no effort — the provider's
///   default, which is also what absent means — and no handovers.
/// How far `peek_first_prompt` reads before giving up: the first prompt
/// is normally the second line, and a log whose first prompt sits past
/// this much preamble is named by its number instead.
const FIRST_PROMPT_SCAN: usize = 64 * 1024;

const SCHEMA_VERSION: u32 = 8;

/// Which agent backend serves this Thread — persisted so a restart knows
/// which provider to revive the Thread on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Codex,
}

impl std::fmt::Display for Provider {
    /// The provider's name as a Notice says it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Provider::Claude => "Claude",
            Provider::Codex => "Codex",
        })
    }
}

/// Loading one Thread failed. Errors are per-Thread by design: one damaged
/// log must never take the rest of the store down with it.
#[derive(Debug)]
pub enum LoadError {
    /// The log was written by a newer Ferrite than this one. Refused whole:
    /// half-reading a schema from the future would show the operator a
    /// Thread that quietly is not theirs. The operator upgrades Ferrite —
    /// the log is fine.
    FutureSchema {
        found: u32,
        supported: u32,
    },
    /// The log's own header could not be read — the file was damaged at or
    /// before the first line, which only a crash inside `create` can leave
    /// behind. Everything after the header heals silently; the header is the
    /// one line with nothing before it to recover to.
    Corrupt {
        detail: String,
    },
    Io(io::Error),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::FutureSchema { found, supported } => write!(
                f,
                "thread log is schema {found}, newer than the supported {supported}; \
                 upgrade Ferrite"
            ),
            LoadError::Corrupt { detail } => write!(f, "thread log corrupt: {detail}"),
            LoadError::Io(e) => write!(f, "io error loading thread: {e}"),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<io::Error> for LoadError {
    fn from(e: io::Error) -> Self {
        LoadError::Io(e)
    }
}

/// The first line of every log: what wrote it, for what provider, working
/// where.
#[derive(Serialize, Deserialize)]
struct Header {
    schema: u32,
    provider: Provider,
    /// Schema 3+; a v1/v2 header loads as `None` — those Threads never
    /// recorded a binding.
    #[serde(default)]
    workspace: Option<PersistedBinding>,
    /// Schema 4+; the git repo inside the binding where work happens.
    /// `None` — and every v1–v3 header — means work in the binding itself.
    #[serde(default)]
    session_project_root: Option<PathBuf>,
    /// Schema 5+; the model this Thread chose before its first prompt.
    /// `None` — and every v1–v4 header — means the provider's default.
    #[serde(default)]
    model: Option<String>,
    /// Schema 6+; the registry id of the project this Thread's CWD choice
    /// named (#29). `None` — and every v1–v5 header — means unregistered:
    /// the binding's resolved paths still say where work happens.
    #[serde(default)]
    project_id: Option<ProjectId>,
    /// Schema 7+; absent means the generated `thread-NN` label.
    #[serde(default)]
    title: Option<String>,
    /// Schema 8+; the reasoning effort this Thread chose. `None` — and
    /// every v1–v7 header — means the provider's default.
    #[serde(default)]
    effort: Option<String>,
}

/// The persisted form of a Thread's workspace binding, mirroring
/// `workspace::WorkspaceBinding` shape for shape — but the store's own type,
/// so the live vocabulary can change without rewriting anyone's history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PersistedBinding {
    Main { checkout: PathBuf },
    Worktree { repo: PathBuf, path: PathBuf },
}

impl PersistedBinding {
    fn from_live(binding: &WorkspaceBinding) -> Self {
        match binding {
            WorkspaceBinding::Main { checkout } => PersistedBinding::Main {
                checkout: checkout.clone(),
            },
            WorkspaceBinding::Worktree { repo, path } => PersistedBinding::Worktree {
                repo: repo.clone(),
                path: path.clone(),
            },
        }
    }

    fn live(&self) -> WorkspaceBinding {
        match self {
            PersistedBinding::Main { checkout } => WorkspaceBinding::Main {
                checkout: checkout.clone(),
            },
            PersistedBinding::Worktree { repo, path } => WorkspaceBinding::Worktree {
                repo: repo.clone(),
                path: path.clone(),
            },
        }
    }
}

/// One line of the log body: the persisted schema, owned by the store.
/// Converted from `SessionEvent`, never `SessionEvent` itself — the live
/// event vocabulary may grow any day, and this one changes only with a
/// schema bump. Text deltas are coalesced into one record per run: the log
/// stores what was said, not how the wire chopped it.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Record {
    Init {
        session_id: String,
        model: String,
    },
    /// A line the operator sent (schema 2+).
    Prompt {
        text: String,
    },
    Text {
        text: String,
    },
    Thinking {
        text: String,
    },
    ReasoningSummary {
        text: String,
        summary_index: u64,
    },
    TokenUsage {
        total_tokens: u64,
        input_tokens: u64,
        cached_input_tokens: u64,
        output_tokens: u64,
        reasoning_output_tokens: u64,
        context_window: Option<u64>,
    },
    ToolStarted {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolCompleted {
        id: String,
        output: String,
        is_error: bool,
        /// Schema 2+; a v1 record loads as `Opaque`, which is also what v1
        /// wrote by never recording one.
        #[serde(default)]
        result: PersistedToolResult,
        /// How long the call ran, as the cockpit clocked it live. Absent
        /// from any log written before the clock was persisted, and absent
        /// for a call whose start this cockpit never saw — the row then
        /// draws no duration, exactly as it did.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    TurnEnded {
        outcome: Outcome,
        cost_usd: Option<f64>,
    },
    Closed {
        reason: String,
    },
    /// The Thread moved to another provider after its first prompt (schema
    /// 8+). Everything before this line was said to `from`; the Session
    /// that follows is `to`'s, started fresh, so the Init before this line
    /// is nobody's to resume and the conversation before it travels as
    /// context in the next prompt instead.
    Handover {
        from: Provider,
        to: Provider,
        model: Option<String>,
    },
}

/// The Notice a provider switch leaves in the transcript, live and on
/// replay alike.
pub(crate) fn handover_notice(to: Provider, model: Option<&str>) -> String {
    let label = crate::providers::models::label(model.unwrap_or("default"), &[]);
    format!("continued on {to} · {label} — the earlier conversation is handed over as context")
}

/// How a persisted turn ended: `"completed"`, `"interrupted"`, or
/// `{"error":"…"}`.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Completed,
    Interrupted,
    Error(String),
}

/// The structured half of a persisted tool result, mirroring
/// `crate::ToolResult` shape for shape — but its own type, so the live model
/// can change without rewriting anyone's history.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PersistedToolResult {
    #[default]
    Opaque,
    Command {
        stdout: String,
        stderr: String,
    },
    FileEdit {
        path: String,
        hunks: Vec<PersistedHunk>,
    },
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct PersistedHunk {
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
    lines: Vec<String>,
}

impl PersistedToolResult {
    fn from_live(result: &crate::ToolResult) -> Self {
        match result {
            crate::ToolResult::Opaque => PersistedToolResult::Opaque,
            crate::ToolResult::Command { stdout, stderr } => PersistedToolResult::Command {
                stdout: stdout.clone(),
                stderr: stderr.clone(),
            },
            crate::ToolResult::FileEdit { path, hunks } => PersistedToolResult::FileEdit {
                path: path.clone(),
                hunks: hunks
                    .iter()
                    .map(|hunk| PersistedHunk {
                        old_start: hunk.old_start,
                        old_lines: hunk.old_lines,
                        new_start: hunk.new_start,
                        new_lines: hunk.new_lines,
                        lines: hunk.lines.clone(),
                    })
                    .collect(),
            },
        }
    }

    fn live(&self) -> crate::ToolResult {
        match self {
            PersistedToolResult::Opaque => crate::ToolResult::Opaque,
            PersistedToolResult::Command { stdout, stderr } => crate::ToolResult::Command {
                stdout: stdout.clone(),
                stderr: stderr.clone(),
            },
            PersistedToolResult::FileEdit { path, hunks } => crate::ToolResult::FileEdit {
                path: path.clone(),
                hunks: hunks
                    .iter()
                    .map(|hunk| crate::Hunk {
                        old_start: hunk.old_start,
                        old_lines: hunk.old_lines,
                        new_start: hunk.new_start,
                        new_lines: hunk.new_lines,
                        lines: hunk.lines.clone(),
                    })
                    .collect(),
            },
        }
    }
}

impl Record {
    /// The persisted form of one live event, or `None` for events that are
    /// Session state rather than durable history (a pending Decision dies
    /// with its Session; there is nothing to replay it into). `duration` is
    /// the wall clock the cockpit measured for a settled tool call — the one
    /// thing in the record no event carries.
    fn from_event(event: &SessionEvent, duration: Option<std::time::Duration>) -> Option<Record> {
        Some(match event {
            SessionEvent::Init { session_id, model } => Record::Init {
                session_id: session_id.clone(),
                model: model.clone(),
            },
            SessionEvent::TextDelta { text } => Record::Text { text: text.clone() },
            SessionEvent::ThinkingDelta { text } => Record::Thinking { text: text.clone() },
            SessionEvent::ToolStarted { id, name, input } => Record::ToolStarted {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            },
            SessionEvent::ToolCompleted {
                id,
                output,
                is_error,
                result,
            } => Record::ToolCompleted {
                id: id.clone(),
                output: output.clone(),
                is_error: *is_error,
                result: PersistedToolResult::from_live(result),
                duration_ms: duration.map(|total| total.as_millis() as u64),
            },
            SessionEvent::TurnEnded { outcome, cost_usd } => Record::TurnEnded {
                outcome: match outcome {
                    crate::TurnOutcome::Completed => Outcome::Completed,
                    crate::TurnOutcome::Interrupted => Outcome::Interrupted,
                    crate::TurnOutcome::Error(message) => Outcome::Error(message.clone()),
                },
                cost_usd: *cost_usd,
            },
            SessionEvent::Closed { reason } => Record::Closed {
                reason: reason.clone(),
            },
            SessionEvent::ReasoningSummaryDelta {
                text,
                summary_index,
            } => Record::ReasoningSummary {
                text: text.clone(),
                summary_index: *summary_index,
            },
            SessionEvent::TokenUsage {
                total_tokens,
                input_tokens,
                cached_input_tokens,
                output_tokens,
                reasoning_output_tokens,
                context_window,
            } => Record::TokenUsage {
                total_tokens: *total_tokens,
                input_tokens: *input_tokens,
                cached_input_tokens: *cached_input_tokens,
                output_tokens: *output_tokens,
                reasoning_output_tokens: *reasoning_output_tokens,
                context_window: *context_window,
            },
            SessionEvent::DecisionRequested { .. } => return None,
            // The command menu, the permission mode and the model menu are
            // the live Session's, like a Decision: a replay has no Session
            // to serve them and the next one announces its own.
            SessionEvent::Commands { .. } => return None,
            SessionEvent::PermissionMode { .. } => return None,
            SessionEvent::Models { .. } => return None,
        })
    }

    /// Replay this record as the transcript input it stands for.
    fn input(&self) -> Input {
        match self {
            Record::Init { session_id, model } => Input::Event(SessionEvent::Init {
                session_id: session_id.clone(),
                model: model.clone(),
            }),
            Record::Prompt { text } => Input::Prompt(text.clone()),
            Record::Text { text } => Input::Event(SessionEvent::TextDelta { text: text.clone() }),
            Record::Thinking { text } => {
                Input::Event(SessionEvent::ThinkingDelta { text: text.clone() })
            }
            Record::ToolStarted { id, name, input } => Input::Event(SessionEvent::ToolStarted {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            }),
            Record::ToolCompleted {
                id,
                output,
                is_error,
                result,
                // The clock is not transcript vocabulary — it replays
                // through `tool_durations`, into the cockpit's own timings.
                duration_ms: _,
            } => Input::Event(SessionEvent::ToolCompleted {
                id: id.clone(),
                output: output.clone(),
                is_error: *is_error,
                result: result.live(),
            }),
            Record::TurnEnded { outcome, cost_usd } => Input::Event(SessionEvent::TurnEnded {
                outcome: match outcome {
                    Outcome::Completed => crate::TurnOutcome::Completed,
                    Outcome::Interrupted => crate::TurnOutcome::Interrupted,
                    Outcome::Error(message) => crate::TurnOutcome::Error(message.clone()),
                },
                cost_usd: *cost_usd,
            }),
            Record::ReasoningSummary {
                text,
                summary_index,
            } => Input::Event(SessionEvent::ReasoningSummaryDelta {
                text: text.clone(),
                summary_index: *summary_index,
            }),
            Record::TokenUsage {
                total_tokens,
                input_tokens,
                cached_input_tokens,
                output_tokens,
                reasoning_output_tokens,
                context_window,
            } => Input::Event(SessionEvent::TokenUsage {
                total_tokens: *total_tokens,
                input_tokens: *input_tokens,
                cached_input_tokens: *cached_input_tokens,
                output_tokens: *output_tokens,
                reasoning_output_tokens: *reasoning_output_tokens,
                context_window: *context_window,
            }),
            Record::Closed { reason } => Input::Event(SessionEvent::Closed {
                reason: reason.clone(),
            }),
            Record::Handover { to, model, .. } => {
                Input::Notice(handover_notice(*to, model.as_deref()))
            }
        }
    }

    /// Whether the log is consistent here — the transcript's boundary marks,
    /// seen from the store's side of the conversion. A handover is one too:
    /// the header rewrite that follows it must find it on disk.
    fn is_boundary(&self) -> bool {
        matches!(
            self,
            Record::TurnEnded { .. } | Record::Closed { .. } | Record::Handover { .. }
        )
    }

    /// Extend this record with a later delta of the same kind, if the two
    /// coalesce. A run of deltas is one record; anything else keeps its line.
    fn coalesce(&mut self, next: &Record) -> bool {
        match (self, next) {
            (Record::Text { text }, Record::Text { text: more }) => {
                text.push_str(more);
                true
            }
            (Record::Thinking { text }, Record::Thinking { text: more }) => {
                text.push_str(more);
                true
            }
            (
                Record::ReasoningSummary {
                    text,
                    summary_index,
                },
                Record::ReasoningSummary {
                    text: more,
                    summary_index: part,
                },
            ) if summary_index == part => {
                text.push_str(more);
                true
            }
            _ => false,
        }
    }
}

/// How long a writer lets buffered records sit before an append makes them
/// durable anyway. Bounds what a crash can cost during a long-streaming turn;
/// short turns never reach it, flushing on their boundary instead.
const DEFAULT_FLUSH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// A directory of Thread logs.
pub struct Store {
    dir: PathBuf,
    flush_interval: std::time::Duration,
    #[cfg(test)]
    fail_create: bool,
    #[cfg(test)]
    fail_delete: bool,
}

impl Store {
    /// Bind a store to `dir`, creating it if it does not exist.
    pub fn open(dir: impl AsRef<Path>) -> io::Result<Self> {
        Self::with_flush_interval(dir, DEFAULT_FLUSH_INTERVAL)
    }

    /// `open`, with the writers' flush interval chosen by the caller.
    pub fn with_flush_interval(
        dir: impl AsRef<Path>,
        flush_interval: std::time::Duration,
    ) -> io::Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            flush_interval,
            #[cfg(test)]
            fail_create: false,
            #[cfg(test)]
            fail_delete: false,
        })
    }

    #[cfg(test)]
    pub(crate) fn refuse_create(mut self) -> Self {
        self.fail_create = true;
        self
    }

    #[cfg(test)]
    pub(crate) fn refuse_delete(mut self) -> Self {
        self.fail_delete = true;
        self
    }

    /// Mint a new Thread from an already-resolved workspace binding. The
    /// Thread is durable before this returns: a crash immediately after
    /// still shows it. The store runs no git and places no paths — worktree
    /// placement is the registry's (#29) — it only writes the resolved
    /// truth down.
    pub fn create(
        &self,
        provider: Provider,
        project: Option<ProjectId>,
        binding: WorkspaceBinding,
    ) -> io::Result<(ThreadId, ThreadWriter)> {
        self.create_with_model(provider, None, project, binding)
    }

    pub(crate) fn create_with_model(
        &self,
        provider: Provider,
        model: Option<String>,
        project: Option<ProjectId>,
        binding: WorkspaceBinding,
    ) -> io::Result<(ThreadId, ThreadWriter)> {
        #[cfg(test)]
        if self.fail_create {
            return Err(io::Error::other("stub refused Thread creation"));
        }
        let mut next = self.thread_ids()?.last().map_or(1, |id| id.get() + 1);
        loop {
            match fs::create_dir(self.dir.join(next.to_string())) {
                Ok(()) => break,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => next += 1,
                Err(e) => return Err(e),
            }
        }
        let id = ThreadId::new(next);

        let mut file = OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(self.log_path(id))?;
        let header = Header {
            schema: SCHEMA_VERSION,
            provider,
            workspace: Some(PersistedBinding::from_live(&binding)),
            // Work starts in the binding itself; a root is picked later.
            session_project_root: None,
            // And on the provider's default model; a choice is picked later.
            model,
            project_id: project,
            title: None,
            // And on its default effort.
            effort: None,
        };
        file.write_all(line(&header)?.as_bytes())?;
        file.sync_data()?;
        Ok((
            id,
            ThreadWriter {
                file,
                buffer: Vec::new(),
                flush_interval: self.flush_interval,
                buffered_since: None,
            },
        ))
    }

    /// Where the store keeps its files — what the registry binds to, so the
    /// registry file and the central worktree layout live beside the Thread
    /// logs (#29).
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Remove one Thread entirely: its log, its directory, everything in it.
    /// The caller settles the worktree's fate first — anything still under
    /// the Thread's directory goes with it.
    pub fn delete(&self, id: ThreadId) -> io::Result<()> {
        #[cfg(test)]
        if self.fail_delete {
            return Err(io::Error::other("stub refused Thread deletion"));
        }
        fs::remove_dir_all(self.dir.join(id.to_string()))
    }

    /// Every Thread in the store, sorted by creation.
    pub fn thread_ids(&self) -> io::Result<Vec<ThreadId>> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            if let Ok(number) = entry.file_name().to_string_lossy().parse::<u64>() {
                ids.push(ThreadId::new(number));
            }
        }
        ids.sort_unstable();
        Ok(ids)
    }

    /// Header-only read of one Thread: what wrote it, for what provider,
    /// working where — without replaying its log. This exists for render
    /// paths (#21's nav lists parked Threads): `read_until` pulls buffered
    /// chunks only up to the first newline, so the records after the header
    /// are never read off the disk, and a huge log peeks at the same cost
    /// as an empty one.
    pub fn peek(&self, id: ThreadId) -> Result<ThreadMeta, LoadError> {
        use std::io::BufRead;
        let mut first = Vec::new();
        io::BufReader::new(File::open(self.log_path(id))?).read_until(b'\n', &mut first)?;
        let header: Header = serde_json::from_slice(&first).map_err(|_| LoadError::Corrupt {
            detail: format!("thread {id} has no readable header"),
        })?;
        if header.schema > SCHEMA_VERSION {
            return Err(LoadError::FutureSchema {
                found: header.schema,
                supported: SCHEMA_VERSION,
            });
        }
        Ok(ThreadMeta {
            provider: header.provider,
            workspace: header.workspace.as_ref().map(PersistedBinding::live),
            session_project_root: header.session_project_root,
            model: header.model,
            project_id: header.project_id,
            title: header.title,
            effort: header.effort,
        })
    }

    /// The first prompt the operator sent, without loading the log: the
    /// records are read only until the first `prompt`, and never past
    /// `FIRST_PROMPT_SCAN` bytes — a huge log with no early prompt costs
    /// that much and no more. `None` when no prompt was found in reach.
    pub fn peek_first_prompt(&self, id: ThreadId) -> Result<Option<String>, LoadError> {
        use std::io::BufRead;
        let mut reader = io::BufReader::new(File::open(self.log_path(id))?);
        let mut line = Vec::new();
        let mut read = 0usize;
        // The header first; it is not a record.
        reader.read_until(b'\n', &mut line)?;
        loop {
            line.clear();
            let n = reader.read_until(b'\n', &mut line)?;
            if n == 0 {
                return Ok(None);
            }
            read += n;
            if let Ok(serde_json::Value::Object(record)) =
                serde_json::from_slice::<serde_json::Value>(&line)
            {
                if record.get("type").and_then(serde_json::Value::as_str) == Some("prompt") {
                    return Ok(record
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string));
                }
            }
            if read > FIRST_PROMPT_SCAN {
                return Ok(None);
            }
        }
    }

    /// Load one Thread's snapshot: its history and resume metadata.
    pub fn load(&self, id: ThreadId) -> Result<ThreadSnapshot, LoadError> {
        // Bytes, not a String: a crash can tear the tail mid-character, and
        // a loader that insists the whole file is UTF-8 would lose the
        // Thread over its last three bytes.
        let bytes = fs::read(self.log_path(id))?;
        let mut lines = bytes.split(|byte| *byte == b'\n');
        let header: Header = lines
            .next()
            .and_then(|first| serde_json::from_slice(first).ok())
            .ok_or_else(|| LoadError::Corrupt {
                detail: format!("thread {id} has no readable header"),
            })?;
        if header.schema > SCHEMA_VERSION {
            return Err(LoadError::FutureSchema {
                found: header.schema,
                supported: SCHEMA_VERSION,
            });
        }
        // Recover to the last complete record: a crash tears at most the
        // final line, so the first unreadable line is where the log ends.
        let records = lines
            .map_while(|body_line| serde_json::from_slice(body_line).ok())
            .collect();
        Ok(ThreadSnapshot {
            id,
            provider: header.provider,
            schema: header.schema,
            workspace: header.workspace,
            session_project_root: header.session_project_root,
            model: header.model,
            project_id: header.project_id,
            title: header.title,
            effort: header.effort,
            records,
        })
    }

    /// Record where inside the binding this Thread's work happens — or
    /// `None` to work in the binding itself. The header is the log's first
    /// line, so the change rewrites the log whole (written beside, renamed
    /// over — the same crash safety as any upgrade). `writer` is the
    /// Thread's open writer, if one exists: the rename would leave its
    /// handle on the replaced inode, where appends vanish silently — so it
    /// is flushed before the rewrite reads the log and swapped onto the new
    /// file after. The swap happens only once the rename has succeeded: on
    /// any error the caller's writer is untouched and still valid.
    #[cfg(test)]
    pub(crate) fn set_session_project_root(
        &self,
        id: ThreadId,
        root: Option<PathBuf>,
        mut writer: Option<&mut ThreadWriter>,
    ) -> Result<(), LoadError> {
        if let Some(w) = writer.as_mut() {
            w.flush()?;
        }
        let mut snapshot = self.load(id)?;
        snapshot.session_project_root = root;
        let file = self.rewrite(&snapshot)?;
        if let Some(w) = writer {
            *w = ThreadWriter {
                file,
                buffer: Vec::new(),
                flush_interval: self.flush_interval,
                buffered_since: None,
            };
        }
        Ok(())
    }

    /// Record which provider serves this Thread, and the model and effort
    /// it chose — `None` for the provider's default of either (#25). The
    /// same header rewrite as `set_session_project_root`, with the same
    /// writer contract: the Thread's open writer, if one exists, is flushed
    /// before and swapped onto the new file after, and on any error it is
    /// untouched and still valid on the old one.
    pub fn set_provider(
        &self,
        id: ThreadId,
        provider: Provider,
        model: Option<String>,
        effort: Option<String>,
        mut writer: Option<&mut ThreadWriter>,
    ) -> Result<(), LoadError> {
        if let Some(w) = writer.as_mut() {
            w.flush()?;
        }
        let mut snapshot = self.load(id)?;
        snapshot.provider = provider;
        snapshot.model = model;
        snapshot.effort = effort;
        let file = self.rewrite(&snapshot)?;
        if let Some(w) = writer {
            *w = ThreadWriter {
                file,
                buffer: Vec::new(),
                flush_interval: self.flush_interval,
                buffered_since: None,
            };
        }
        Ok(())
    }

    /// Commit the new header and its Handover together. Before the rename,
    /// every refusal leaves the old log and writer authoritative; afterwards
    /// no fallible read remains between persistence and Session adoption.
    pub(crate) fn hand_over(
        &self,
        id: ThreadId,
        provider: Provider,
        model: Option<String>,
        writer: &mut ThreadWriter,
    ) -> Result<Handover, LoadError> {
        writer.flush()?;
        let mut snapshot = self.load(id)?;
        snapshot.records.push(Record::Handover {
            from: snapshot.provider,
            to: provider,
            model: model.clone(),
        });
        snapshot.provider = provider;
        snapshot.model = model;
        snapshot.effort = None;
        let handover = snapshot.last_handover().expect("just added");
        let file = self.rewrite(&snapshot)?;
        *writer = ThreadWriter {
            file,
            buffer: Vec::new(),
            flush_interval: self.flush_interval,
            buffered_since: None,
        };
        Ok(handover)
    }

    pub fn set_title(
        &self,
        id: ThreadId,
        title: String,
        mut writer: Option<&mut ThreadWriter>,
    ) -> Result<(), LoadError> {
        if let Some(w) = writer.as_mut() {
            w.flush()?;
        }
        let mut snapshot = self.load(id)?;
        snapshot.title = Some(title);
        let file = self.rewrite(&snapshot)?;
        if let Some(w) = writer {
            *w = ThreadWriter {
                file,
                buffer: Vec::new(),
                flush_interval: self.flush_interval,
                buffered_since: None,
            };
        }
        Ok(())
    }

    /// Reopen one Thread's log for appending — how a revived Thread's next
    /// turns reach the same history after a restart.
    ///
    /// A log written at an older schema is upgraded whole before the first
    /// append: new records under an old header would make the file a lie,
    /// and a reader of that older schema would stop dead at the first record
    /// it cannot know.
    ///
    /// A log a crash left torn is likewise rewritten from what `load`
    /// recovers: appending straight after the tear would concatenate the
    /// first new record onto the fragment — one unreadable line where the
    /// loader stops, hiding every turn after the crash.
    pub fn writer(&self, id: ThreadId) -> Result<ThreadWriter, LoadError> {
        let snapshot = self.load(id)?;
        let file =
            if snapshot.schema < SCHEMA_VERSION || has_torn_tail(&fs::read(self.log_path(id))?) {
                self.rewrite(&snapshot)?
            } else {
                OpenOptions::new().append(true).open(self.log_path(id))?
            };
        Ok(ThreadWriter {
            file,
            buffer: Vec::new(),
            flush_interval: self.flush_interval,
            buffered_since: None,
        })
    }

    /// Replace a Thread's log with the loaded snapshot at the current
    /// schema. Written beside and renamed over, so a crash mid-rewrite
    /// leaves the original log untouched. Answers an append handle to the
    /// new log — taken on the file before the rename and riding it, so any
    /// failure here happens while the original log (and every handle on it)
    /// is still the real one.
    fn rewrite(&self, snapshot: &ThreadSnapshot) -> io::Result<File> {
        let path = self.log_path(snapshot.id);
        let mut contents = line(&Header {
            schema: SCHEMA_VERSION,
            provider: snapshot.provider,
            workspace: snapshot.workspace.clone(),
            session_project_root: snapshot.session_project_root.clone(),
            model: snapshot.model.clone(),
            project_id: snapshot.project_id,
            title: snapshot.title.clone(),
            effort: snapshot.effort.clone(),
        })?;
        for record in &snapshot.records {
            contents.push_str(&line(record)?);
        }
        let tmp = path.with_extension("jsonl.tmp");
        let mut file = File::create(&tmp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_data()?;
        let handle = OpenOptions::new().append(true).open(&tmp)?;
        fs::rename(&tmp, &path)?;
        Ok(handle)
    }

    fn log_path(&self, id: ThreadId) -> PathBuf {
        self.dir.join(id.to_string()).join("log.jsonl")
    }
}

/// One Thread's header facts, read without its history — what a nav row can
/// say about a parked Thread (#21). Everything here is the log's first line.
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadMeta {
    pub provider: Provider,
    /// `None` for a log from before schema 3, which never recorded one.
    pub workspace: Option<WorkspaceBinding>,
    /// `None` — including every pre-v4 log — means work in the binding
    /// itself.
    pub session_project_root: Option<PathBuf>,
    /// `None` — including every pre-v5 log — means the provider's default
    /// model.
    pub model: Option<String>,
    /// `None` — including every pre-v6 log — means unregistered (#29): the
    /// binding's resolved paths still say where work happens.
    pub project_id: Option<ProjectId>,
    pub title: Option<String>,
    /// `None` — including every pre-v8 log — means the provider's default
    /// reasoning effort.
    pub effort: Option<String>,
}

/// One Thread as loaded from disk: everything a restart needs.
pub struct ThreadSnapshot {
    pub id: ThreadId,
    provider: Provider,
    /// The schema the log on disk declares — what tells `writer` an old log
    /// needs upgrading before anything lands after it.
    schema: u32,
    workspace: Option<PersistedBinding>,
    session_project_root: Option<PathBuf>,
    model: Option<String>,
    project_id: Option<ProjectId>,
    title: Option<String>,
    effort: Option<String>,
    records: Vec<Record>,
}

/// The last provider switch a log records, and what the next prompt on
/// the new provider owes it.
pub(crate) struct Handover {
    /// The provider the conversation before the switch was held with.
    pub from: Provider,
    /// Every prompt before the switch with the answer text that followed
    /// it — the material of the carry digest.
    pub exchanges: Vec<(String, String)>,
    /// Whether a prompt has gone out since the switch: the carry travelled
    /// with it, and a revive must not send it again.
    pub delivered: bool,
}

impl ThreadSnapshot {
    pub fn provider(&self) -> Provider {
        self.provider
    }

    /// The checkout this Thread works in. `None` for a log from before
    /// schema 3, which never recorded one.
    pub fn workspace(&self) -> Option<WorkspaceBinding> {
        self.workspace.as_ref().map(PersistedBinding::live)
    }

    /// The git repo inside the binding where this Thread's work happens.
    /// `None` — including every log from before schema 4 — means work in
    /// the binding itself.
    pub fn session_project_root(&self) -> Option<PathBuf> {
        self.session_project_root.clone()
    }

    /// The model this Thread chose before its first prompt. `None` —
    /// including every log from before schema 5 — means the provider's
    /// default.
    pub fn model(&self) -> Option<String> {
        self.model.clone()
    }

    /// The registered project this Thread's CWD choice named (#29). `None`
    /// — including every log from before schema 6 — means unregistered.
    pub fn project_id(&self) -> Option<ProjectId> {
        self.project_id
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// The reasoning effort this Thread chose. `None` — including every
    /// log from before schema 8 — means the provider's default.
    pub fn effort(&self) -> Option<String> {
        self.effort.clone()
    }

    /// The provider-native id the next Session resumes with — the latest the
    /// provider announced, so a provider that renames its session on resume
    /// still resumes from the newest name. `None` before any Session spoke,
    /// and `None` again after a handover the new provider has not yet
    /// spoken past: the Init before the switch is the old provider's.
    pub fn resume_target(&self) -> Option<&str> {
        self.records.iter().rev().find_map(|record| match record {
            Record::Init { session_id, .. } => Some(Some(session_id.as_str())),
            Record::Handover { .. } => Some(None),
            _ => None,
        })?
    }

    /// The last provider switch in this log, with the conversation before
    /// it as prompt/answer pairs (answer = the assistant text that followed
    /// the prompt, tool runs and reasoning left out). `None` for a Thread
    /// that never switched.
    pub(crate) fn last_handover(&self) -> Option<Handover> {
        let at = self
            .records
            .iter()
            .rposition(|record| matches!(record, Record::Handover { .. }))?;
        let Record::Handover { from, .. } = &self.records[at] else {
            unreachable!("rposition matched a handover");
        };
        let mut exchanges: Vec<(String, String)> = Vec::new();
        for record in &self.records[..at] {
            match record {
                Record::Prompt { text } => exchanges.push((text.clone(), String::new())),
                Record::Text { text } => {
                    if let Some((_, answer)) = exchanges.last_mut() {
                        answer.push_str(text);
                    }
                }
                _ => {}
            }
        }
        Some(Handover {
            from: *from,
            exchanges,
            delivered: self.records[at + 1..]
                .iter()
                .any(|record| matches!(record, Record::Prompt { .. })),
        })
    }

    /// The history, in transcript vocabulary: replay these through a fresh
    /// `Transcript` and the Pane shows what it showed before the restart.
    pub fn inputs(&self) -> Vec<Input> {
        self.records.iter().map(Record::input).collect()
    }

    /// Every settled tool call's wall clock, keyed by the provider's call
    /// id — what the cockpit clocked when the call actually ran. Replayed
    /// into the cockpit's timings so a revived Thread's rows still carry
    /// the durations they were drawn with; calls logged before the clock
    /// was persisted are simply absent.
    pub(crate) fn tool_durations(&self) -> Vec<(String, std::time::Duration)> {
        self.records
            .iter()
            .filter_map(|record| match record {
                Record::ToolCompleted {
                    id,
                    duration_ms: Some(ms),
                    ..
                } => Some((id.clone(), std::time::Duration::from_millis(*ms))),
                _ => None,
            })
            .collect()
    }

    pub(crate) fn prompt_texts(&self) -> Vec<String> {
        self.records
            .iter()
            .filter_map(|record| match record {
                Record::Prompt { text } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }
}

/// Appends one Thread's records. Buffered: nothing reaches the disk until a
/// boundary (turn end, close), a timeout, or an explicit `flush`.
pub struct ThreadWriter {
    file: File,
    buffer: Vec<Record>,
    flush_interval: std::time::Duration,
    /// When the oldest unflushed record was buffered; `None` while empty.
    buffered_since: Option<std::time::Instant>,
}

impl ThreadWriter {
    /// Buffer one Session event, converted to the persisted schema. Flushes
    /// internally when the event is a boundary (turn end, close).
    /// `duration` is the wall clock a settled tool call took, where the
    /// caller measured one; `None` everywhere else.
    pub fn record_event(
        &mut self,
        event: &SessionEvent,
        duration: Option<std::time::Duration>,
    ) -> io::Result<()> {
        let Some(record) = Record::from_event(event, duration) else {
            return Ok(());
        };
        self.push(record)
    }

    /// Buffer one line the operator sent. Not a Session event: the prompt is
    /// Ferrite's own act, and no provider will ever echo it back.
    pub fn record_prompt(&mut self, text: &str) -> io::Result<()> {
        self.push(Record::Prompt { text: text.into() })
    }

    /// Record a provider switch after the first prompt: a boundary, so it
    /// is on disk before the header rewrite that follows reads the log.
    pub fn record_handover(
        &mut self,
        from: Provider,
        to: Provider,
        model: Option<String>,
    ) -> io::Result<()> {
        self.push(Record::Handover { from, to, model })
    }

    /// Everything buffered, durably on disk. The caller's lever for the
    /// moments the writer cannot see: parking a Thread, quitting the app.
    pub fn flush(&mut self) -> io::Result<()> {
        self.buffered_since = None;
        if self.buffer.is_empty() {
            return Ok(());
        }
        let mut lines = String::new();
        for record in self.buffer.drain(..) {
            lines.push_str(&line(&record)?);
        }
        self.file.write_all(lines.as_bytes())?;
        self.file.sync_data()
    }

    fn push(&mut self, record: Record) -> io::Result<()> {
        let coalesced = match self.buffer.last_mut() {
            Some(last) => last.coalesce(&record),
            None => false,
        };
        let flush_now = record.is_boundary();
        if !coalesced {
            self.buffer.push(record);
        }
        let since = *self
            .buffered_since
            .get_or_insert_with(std::time::Instant::now);
        if flush_now || since.elapsed() >= self.flush_interval {
            self.flush()?;
        }
        Ok(())
    }
}

/// Whether a loaded log's bytes end in anything but whole, newline-terminated,
/// readable records — the leavings of a crash, which an append must not build
/// on. The header is not judged here: `load` already required it.
fn has_torn_tail(bytes: &[u8]) -> bool {
    if !bytes.ends_with(b"\n") {
        // Even a fragment that happens to parse is dirty: the next append
        // would land on its line.
        return true;
    }
    let lines: Vec<&[u8]> = bytes.split(|byte| *byte == b'\n').collect();
    // First line is the header; last is the empty slice after the final
    // newline. Everything between must be a whole record.
    lines[1..lines.len() - 1]
        .iter()
        .any(|body_line| serde_json::from_slice::<Record>(body_line).is_err())
}

/// One record as one JSONL line.
fn line<T: Serialize>(record: &T) -> io::Result<String> {
    let mut text = serde_json::to_string(record).map_err(io::Error::other)?;
    text.push('\n');
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::Transcript;
    use crate::{ToolResult, TurnOutcome};

    /// A fresh per-test scratch directory.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrite-store-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// The binding for tests that are not about bindings.
    fn main_choice() -> WorkspaceBinding {
        WorkspaceBinding::Main {
            checkout: std::env::temp_dir(),
        }
    }

    /// A Thread's workspace binding is part of what a restart restores:
    /// both shapes round-trip exactly as the caller resolved them — since
    /// #29 the registry places worktrees, and the store only writes the
    /// resolved truth down. The project id rides the same header.
    #[test]
    fn a_thread_s_workspace_binding_survives_reopening_the_store() {
        let dir = scratch("binding");
        let store = Store::open(&dir).unwrap();
        let main_binding = WorkspaceBinding::Main {
            checkout: "/repos/project".into(),
        };
        let (main_id, _writer) = store
            .create(Provider::Claude, None, main_binding.clone())
            .unwrap();
        let wt_binding = WorkspaceBinding::Worktree {
            repo: "/repos/project".into(),
            path: "/store/worktrees/project-4f2a1c9e7b30/ferrite-wt-1".into(),
        };
        let project = registry_id(&dir, "project-a");
        let (wt_id, _writer) = store
            .create(Provider::Codex, Some(project), wt_binding.clone())
            .unwrap();

        // The fake restart: nothing survives but the directory.
        let store = Store::open(&dir).unwrap();
        assert_eq!(store.load(main_id).unwrap().workspace(), Some(main_binding));
        assert_eq!(store.load(main_id).unwrap().project_id(), None);
        assert_eq!(store.load(wt_id).unwrap().workspace(), Some(wt_binding));
        assert_eq!(store.load(wt_id).unwrap().project_id(), Some(project));
        assert_eq!(store.peek(wt_id).unwrap().project_id, Some(project));
    }

    /// A real ProjectId for header tests — minted by the registry, which is
    /// the only place one comes from.
    fn registry_id(dir: &Path, name: &str) -> ProjectId {
        let root = dir.join(name);
        fs::create_dir_all(&root).unwrap();
        crate::workspace::registry::Registry::open(dir)
            .unwrap()
            .register(&root)
            .unwrap()
    }

    /// Deletion is per-Thread and total: the log directory goes, the other
    /// Threads stay.
    #[test]
    fn a_deleted_thread_is_gone_and_its_neighbours_are_not() {
        let dir = scratch("delete");
        let store = Store::open(&dir).unwrap();
        let (first, _writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        let (second, _writer) = store.create(Provider::Codex, None, main_choice()).unwrap();

        store.delete(first).unwrap();

        assert_eq!(store.thread_ids().unwrap(), vec![second]);
        assert!(store.load(first).is_err(), "a deleted Thread must not load");
        assert!(store.load(second).is_ok());
    }

    /// #21: the nav's parked rows come from `peek`, which must answer from
    /// the header line alone. The body here is a megabyte of bytes that are
    /// not records at all — a peek that read or parsed past the first line
    /// would choke on them, and a `load` in a render path is exactly the
    /// whole-log replay the nav exists to avoid.
    #[test]
    fn peek_reads_the_header_line_and_never_the_records() {
        let dir = scratch("peek");
        let store = Store::open(&dir).unwrap();
        let (id, writer) = store
            .create(
                Provider::Codex,
                None,
                WorkspaceBinding::Main {
                    checkout: "/repos/project".into(),
                },
            )
            .unwrap();
        drop(writer);
        store
            .set_session_project_root(id, Some("/repos/project/api".into()), None)
            .unwrap();
        let mut log = OpenOptions::new()
            .append(true)
            .open(dir.join(id.to_string()).join("log.jsonl"))
            .unwrap();
        writeln!(log, "{}", "x".repeat(1024 * 1024)).unwrap();

        let meta = store.peek(id).unwrap();

        assert_eq!(meta.provider, Provider::Codex);
        assert_eq!(
            meta.workspace,
            Some(WorkspaceBinding::Main {
                checkout: "/repos/project".into(),
            })
        );
        assert_eq!(
            meta.session_project_root,
            Some(PathBuf::from("/repos/project/api"))
        );
    }

    /// A log from Ferrite's future refuses a peek exactly as it refuses a
    /// load: a nav row half-read from an unknown schema would claim a
    /// Thread that cannot actually be revived.
    #[test]
    fn peek_refuses_a_future_schema_like_load_does() {
        let dir = scratch("peek-future");
        let store = Store::open(&dir).unwrap();
        let (id, writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        drop(writer);
        let path = dir.join(id.to_string()).join("log.jsonl");
        let log = fs::read_to_string(&path).unwrap();
        fs::write(
            &path,
            log.replace(&format!("\"schema\":{SCHEMA_VERSION}"), "\"schema\":99"),
        )
        .unwrap();

        assert!(matches!(
            store.peek(id),
            Err(LoadError::FutureSchema { found: 99, .. })
        ));
    }

    /// The frozen contract for schema 2, byte for byte what its writer
    /// produced: prompts and structured results, but no workspace binding.
    /// Logs like this exist on disks; they must load forever.
    const V2_LOG: &str = concat!(
        r#"{"schema":2,"provider":"claude"}"#,
        "\n",
        r#"{"type":"init","session_id":"v2-era-4f2a","model":"claude-haiku-4-5"}"#,
        "\n",
        r#"{"type":"prompt","text":"fix the typo"}"#,
        "\n",
        r#"{"type":"text","text":"done"}"#,
        "\n",
        r#"{"type":"turn_ended","outcome":"completed","cost_usd":null}"#,
        "\n",
    );

    /// AC (schema story): loading a log written at schema v2 succeeds after
    /// the bump to v3 — with no binding, exactly what v2 recorded.
    #[test]
    fn a_log_written_at_schema_v2_still_loads_after_the_bump() {
        let dir = scratch("v2");
        plant_log(&dir, "9", V2_LOG);

        let thread = Store::open(&dir).unwrap().load(ThreadId::new(9)).unwrap();
        assert_eq!(thread.provider(), Provider::Claude);
        assert_eq!(thread.workspace(), None);
        assert_eq!(thread.resume_target(), Some("v2-era-4f2a"));
        assert_eq!(
            thread.inputs(),
            vec![
                Input::Event(SessionEvent::Init {
                    session_id: "v2-era-4f2a".into(),
                    model: "claude-haiku-4-5".into(),
                }),
                Input::Prompt("fix the typo".into()),
                Input::Event(SessionEvent::TextDelta {
                    text: "done".into(),
                }),
                Input::Event(SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    cost_usd: None,
                }),
            ]
        );
    }

    /// The frozen contract for schema 3, byte for byte what its writer
    /// produced: the workspace binding in the header, but no session project
    /// root. Logs like this exist on disks; they must load forever.
    const V3_LOG: &str = concat!(
        r#"{"schema":3,"provider":"claude","workspace":{"kind":"main","checkout":"/repos/project"}}"#,
        "\n",
        r#"{"type":"init","session_id":"v3-era-4f2a","model":"claude-haiku-4-5"}"#,
        "\n",
        r#"{"type":"turn_ended","outcome":"completed","cost_usd":null}"#,
        "\n",
    );

    /// AC (schema story): loading a log written at schema v3 succeeds after
    /// the bump to v4 — binding intact, and no session project root, exactly
    /// what v3 recorded: work happens in the binding itself.
    #[test]
    fn a_log_written_at_schema_v3_still_loads_after_the_bump() {
        let dir = scratch("v3");
        plant_log(&dir, "11", V3_LOG);

        let thread = Store::open(&dir).unwrap().load(ThreadId::new(11)).unwrap();
        assert_eq!(thread.provider(), Provider::Claude);
        assert_eq!(
            thread.workspace(),
            Some(WorkspaceBinding::Main {
                checkout: "/repos/project".into(),
            })
        );
        assert_eq!(thread.session_project_root(), None);
        assert_eq!(thread.resume_target(), Some("v3-era-4f2a"));
    }

    /// AC (#24): the session project root survives restart. Setting it
    /// rewrites the header; the Thread's open writer is passed through and
    /// comes back on the new log — the rename leaves the old handle on the
    /// replaced inode, where appends would vanish. Clearing the root hands
    /// back None, today's work-in-the-binding behavior.
    #[test]
    fn a_thread_s_session_project_root_survives_reopening_the_store() {
        let dir = scratch("session-root");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        assert_eq!(store.load(id).unwrap().session_project_root(), None);

        store
            .set_session_project_root(
                id,
                Some("/repos/project/apps/web".into()),
                Some(&mut writer),
            )
            .unwrap();
        // The writer rode the rewrite: this append must land where loads
        // look, not on the renamed-over inode.
        writer.record_prompt("after the pick").unwrap();
        writer.flush().unwrap();

        // The fake restart: nothing survives but the directory.
        let reopened = Store::open(&dir).unwrap();
        let thread = reopened.load(id).unwrap();
        assert_eq!(
            thread.session_project_root(),
            Some("/repos/project/apps/web".into())
        );
        assert!(
            thread
                .inputs()
                .contains(&Input::Prompt("after the pick".into())),
            "the swapped writer's append is history: {:?}",
            thread.inputs()
        );
        // The header declares the schema that wrote it.
        let log = fs::read_to_string(dir.join(id.to_string()).join("log.jsonl")).unwrap();
        assert!(
            log.lines()
                .next()
                .unwrap()
                .contains(&format!("\"schema\":{SCHEMA_VERSION}")),
            "header: {log}"
        );

        // Cleared — no writer open this time — the Thread works in the
        // binding again.
        reopened.set_session_project_root(id, None, None).unwrap();
        assert_eq!(reopened.load(id).unwrap().session_project_root(), None);
    }

    /// The frozen contract for schema 4, byte for byte what its writer
    /// produced: the session project root in the header, but no model.
    /// Logs like this exist on disks; they must load forever.
    const V4_LOG: &str = concat!(
        r#"{"schema":4,"provider":"claude","workspace":{"kind":"main","checkout":"/repos/project"},"session_project_root":"/repos/project/api"}"#,
        "\n",
        r#"{"type":"init","session_id":"v4-era-4f2a","model":"claude-haiku-4-5"}"#,
        "\n",
        r#"{"type":"turn_ended","outcome":"completed","cost_usd":null}"#,
        "\n",
    );

    /// AC (schema story): loading a log written at schema v4 succeeds after
    /// the bump to v5 — root intact, and no model, exactly what v4
    /// recorded: the provider's default.
    #[test]
    fn a_log_written_at_schema_v4_still_loads_after_the_bump() {
        let dir = scratch("v4");
        plant_log(&dir, "13", V4_LOG);

        let thread = Store::open(&dir).unwrap().load(ThreadId::new(13)).unwrap();
        assert_eq!(thread.provider(), Provider::Claude);
        assert_eq!(
            thread.session_project_root(),
            Some("/repos/project/api".into())
        );
        assert_eq!(thread.model(), None);
        assert_eq!(thread.resume_target(), Some("v4-era-4f2a"));
    }

    /// The frozen contract for schema 5, byte for byte what its writer
    /// produced: the chosen model in the header, but no project id. Logs
    /// like this exist on disks; they must load forever.
    const V5_LOG: &str = concat!(
        r#"{"schema":5,"provider":"codex","workspace":{"kind":"worktree","repo":"/repos/project","path":"/store/7/worktree"},"session_project_root":null,"model":"gpt-5.4-mini"}"#,
        "\n",
        r#"{"type":"init","session_id":"v5-era-4f2a","model":"gpt-5.4-mini"}"#,
        "\n",
        r#"{"type":"turn_ended","outcome":"completed","cost_usd":null}"#,
        "\n",
    );

    /// AC (schema story): loading a log written at schema v5 succeeds after
    /// the bump to v6 — binding and model intact, and no project id,
    /// exactly what v5 recorded: those Threads registered nothing.
    #[test]
    fn a_log_written_at_schema_v5_still_loads_after_the_bump() {
        let dir = scratch("v5");
        plant_log(&dir, "17", V5_LOG);

        let thread = Store::open(&dir).unwrap().load(ThreadId::new(17)).unwrap();
        assert_eq!(thread.provider(), Provider::Codex);
        assert_eq!(thread.model(), Some("gpt-5.4-mini".into()));
        assert_eq!(thread.project_id(), None);
        assert_eq!(
            thread.workspace(),
            Some(WorkspaceBinding::Worktree {
                repo: "/repos/project".into(),
                path: "/store/7/worktree".into(),
            })
        );
        assert_eq!(thread.resume_target(), Some("v5-era-4f2a"));
    }

    /// The frozen contract for schema 7, byte for byte what its writer
    /// produced: the title in the header, but no effort and no handovers.
    const V7_LOG: &str = concat!(
        r#"{"schema":7,"provider":"claude","workspace":{"kind":"main","checkout":"/repos/project"},"session_project_root":null,"model":"sonnet","project_id":null,"title":"Old title"}"#,
        "\n",
        r#"{"type":"init","session_id":"v7-era-4f2a","model":"claude-sonnet-5"}"#,
        "\n",
        r#"{"type":"prompt","text":"hello"}"#,
        "\n",
        r#"{"type":"turn_ended","outcome":"completed","cost_usd":null}"#,
        "\n",
    );

    /// AC (schema story): loading a log written at schema v7 succeeds after
    /// the bump to v8 — title and model intact, no effort (the provider's
    /// default) and no handover, exactly what v7 recorded.
    #[test]
    fn a_log_written_at_schema_v7_still_loads_after_the_bump() {
        let dir = scratch("v7");
        plant_log(&dir, "19", V7_LOG);

        let store = Store::open(&dir).unwrap();
        let thread = store.load(ThreadId::new(19)).unwrap();
        assert_eq!(thread.provider(), Provider::Claude);
        assert_eq!(thread.model(), Some("sonnet".into()));
        assert_eq!(thread.title(), Some("Old title"));
        assert_eq!(thread.effort(), None);
        assert!(thread.last_handover().is_none());
        assert_eq!(thread.resume_target(), Some("v7-era-4f2a"));
        assert_eq!(store.peek(ThreadId::new(19)).unwrap().effort, None);
    }

    /// AC (#25): the provider and model choice survives restart. Setting it
    /// rewrites the header; the Thread's open writer rides the rewrite the
    /// same way the root setter's does. Clearing the model hands back None,
    /// the provider's default.
    #[test]
    fn a_thread_s_provider_and_model_survive_reopening_the_store() {
        let dir = scratch("provider-model");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        assert_eq!(store.load(id).unwrap().model(), None);

        store
            .set_provider(
                id,
                Provider::Codex,
                Some("gpt-5.4-mini".into()),
                Some("high".into()),
                Some(&mut writer),
            )
            .unwrap();
        // The writer rode the rewrite: this append must land where loads
        // look, not on the renamed-over inode.
        writer.record_prompt("after the pick").unwrap();
        writer.flush().unwrap();

        // The fake restart: nothing survives but the directory.
        let reopened = Store::open(&dir).unwrap();
        let thread = reopened.load(id).unwrap();
        assert_eq!(thread.provider(), Provider::Codex);
        assert_eq!(thread.model(), Some("gpt-5.4-mini".into()));
        assert_eq!(thread.effort(), Some("high".into()));
        assert!(
            thread
                .inputs()
                .contains(&Input::Prompt("after the pick".into())),
            "the swapped writer's append is history: {:?}",
            thread.inputs()
        );
        let meta = reopened.peek(id).unwrap();
        assert_eq!(meta.provider, Provider::Codex);
        assert_eq!(meta.model, Some("gpt-5.4-mini".into()));
        assert_eq!(meta.effort, Some("high".into()));

        // Back on the provider's defaults — no writer open this time.
        reopened
            .set_provider(id, Provider::Codex, None, None, None)
            .unwrap();
        assert_eq!(reopened.load(id).unwrap().model(), None);
        assert_eq!(reopened.load(id).unwrap().effort(), None);
        assert_eq!(reopened.peek(id).unwrap().effort, None);
    }

    /// A handover is durable history: it replays as its Notice, it hides
    /// the old provider's Init from the resume target until the new one
    /// speaks, and it carries the conversation before it as exchanges —
    /// marked delivered once a prompt follows it.
    #[test]
    fn a_handover_is_replayed_and_shadows_the_old_providers_init() {
        let dir = scratch("handover");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        writer
            .record_event(
                &SessionEvent::Init {
                    session_id: "claude-sess".into(),
                    model: "claude-opus-5".into(),
                },
                None,
            )
            .unwrap();
        writer.record_prompt("first question").unwrap();
        writer
            .record_event(
                &SessionEvent::TextDelta {
                    text: "first ".into(),
                },
                None,
            )
            .unwrap();
        writer
            .record_event(
                &SessionEvent::TextDelta {
                    text: "answer".into(),
                },
                None,
            )
            .unwrap();
        writer
            .record_event(
                &SessionEvent::ToolCompleted {
                    id: "t1".into(),
                    output: "tool noise".into(),
                    is_error: false,
                    result: crate::ToolResult::Opaque,
                },
                None,
            )
            .unwrap();
        writer.record_prompt("second question").unwrap();
        writer
            .record_handover(
                Provider::Claude,
                Provider::Codex,
                Some("gpt-5.6-sol".into()),
            )
            .unwrap();
        assert!(!dir.join("1").join("log.jsonl.tmp").exists());
        assert_eq!(
            store.load(id).unwrap().resume_target(),
            None,
            "a boundary: on disk already"
        );
        store
            .set_provider(
                id,
                Provider::Codex,
                Some("gpt-5.6-sol".into()),
                None,
                Some(&mut writer),
            )
            .unwrap();

        let thread = store.load(id).unwrap();
        assert_eq!(thread.provider(), Provider::Codex);
        assert_eq!(
            thread.resume_target(),
            None,
            "the Init before the switch belongs to Claude"
        );
        let handover = thread.last_handover().expect("recorded");
        assert_eq!(handover.from, Provider::Claude);
        assert_eq!(
            handover.exchanges,
            [
                ("first question".to_string(), "first answer".to_string()),
                ("second question".to_string(), String::new()),
            ]
        );
        assert!(!handover.delivered);
        assert!(thread.inputs().contains(&Input::Notice(
            "continued on Codex · GPT-5.6 Sol — the earlier conversation is handed over as context"
                .into()
        )));

        // The new provider speaks and a prompt goes out: resume is its own
        // id again and the carry is spent.
        writer.record_prompt("third question").unwrap();
        writer
            .record_event(
                &SessionEvent::Init {
                    session_id: "codex-thread".into(),
                    model: "gpt-5.6-sol".into(),
                },
                None,
            )
            .unwrap();
        writer.flush().unwrap();
        let thread = Store::open(&dir).unwrap().load(id).unwrap();
        assert_eq!(thread.resume_target(), Some("codex-thread"));
        assert!(thread.last_handover().unwrap().delivered);
        assert!(store.create(Provider::Claude, None, main_choice()).is_ok());
        assert!(Store::open(&dir)
            .unwrap()
            .load(ThreadId::new(2))
            .unwrap()
            .last_handover()
            .is_none());
    }

    /// A realistic Claude-shaped turn: identity, thinking, markdown streamed
    /// in ragged deltas, a tool run, a paid ending.
    fn claude_turn() -> Vec<SessionEvent> {
        vec![
            SessionEvent::Init {
                session_id: "4f2a1c9e-7b30".into(),
                model: "claude-haiku-4-5".into(),
            },
            SessionEvent::ThinkingDelta {
                text: "weighing ".into(),
            },
            SessionEvent::ThinkingDelta {
                text: "options".into(),
            },
            SessionEvent::TextDelta {
                text: "## Plan\nfirst ".into(),
            },
            SessionEvent::TextDelta {
                text: "step\n\n".into(),
            },
            SessionEvent::ToolStarted {
                id: "toolu_1".into(),
                name: "Bash".into(),
                input: serde_json::json!({ "command": "cargo test" }),
            },
            SessionEvent::ToolCompleted {
                id: "toolu_1".into(),
                output: "42 passed".into(),
                is_error: false,
                result: ToolResult::Opaque,
            },
            SessionEvent::TextDelta {
                text: "done".into(),
            },
            SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: Some(0.02),
            },
        ]
    }

    /// Replay a snapshot's history into a fresh Transcript.
    fn restore(thread: &ThreadSnapshot) -> Transcript {
        let mut transcript = Transcript::default();
        for input in thread.inputs() {
            transcript.apply(input);
        }
        transcript
    }

    /// A deterministic stream of non-boundary Session events, `seed`-shaped.
    /// Plain LCG: the point is many varied interleavings, not randomness.
    fn arbitrary_mid_turn_events(seed: u64, count: usize) -> Vec<SessionEvent> {
        let mut state = seed;
        let mut next = move || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            state
        };
        (0..count)
            .map(|n| match next() % 6 {
                0 => SessionEvent::TextDelta {
                    text: format!("delta {n} "),
                },
                1 => SessionEvent::ThinkingDelta {
                    text: format!("thought {n} "),
                },
                2 => SessionEvent::ReasoningSummaryDelta {
                    text: format!("summary {n} "),
                    summary_index: next() % 3,
                },
                3 => SessionEvent::ToolStarted {
                    id: format!("toolu_{n}"),
                    name: "Bash".into(),
                    input: serde_json::json!({ "command": format!("step {n}") }),
                },
                4 => SessionEvent::ToolCompleted {
                    id: format!("toolu_{n}"),
                    output: format!("out {n}"),
                    is_error: n % 2 == 0,
                    result: ToolResult::Opaque,
                },
                _ => SessionEvent::TokenUsage {
                    total_tokens: next() % 10_000,
                    input_tokens: 1,
                    cached_input_tokens: 2,
                    output_tokens: 3,
                    reasoning_output_tokens: 4,
                    context_window: None,
                },
            })
            .collect()
    }

    /// AC: no durable write ever occurs per delta. The property, over many
    /// arbitrary mid-turn streams: appending any non-boundary event leaves
    /// the file untouched, and the boundary that ends the turn is the single
    /// moment the file grows.
    #[test]
    fn no_durable_write_ever_occurs_per_delta() {
        let dir = scratch("no-per-delta");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        let log = dir.join(id.to_string()).join("log.jsonl");

        for seed in 0..32u64 {
            let start = fs::metadata(&log).unwrap().len();
            let count = (seed % 40 + 5) as usize;
            for event in arbitrary_mid_turn_events(seed, count) {
                writer.record_event(&event, None).unwrap();
                assert_eq!(
                    fs::metadata(&log).unwrap().len(),
                    start,
                    "a durable write occurred mid-turn (seed {seed})"
                );
            }
            writer
                .record_event(
                    &SessionEvent::TurnEnded {
                        outcome: TurnOutcome::Completed,
                        cost_usd: None,
                    },
                    None,
                )
                .unwrap();
            assert!(
                fs::metadata(&log).unwrap().len() > start,
                "the boundary did not flush (seed {seed})"
            );
        }
    }

    /// The third flush trigger: a turn that streams for a long time hits the
    /// interval and its tail becomes durable mid-turn — still never one
    /// write per delta.
    #[test]
    fn a_long_turn_flushes_on_the_interval_not_per_delta() {
        let dir = scratch("interval");
        let store = Store::with_flush_interval(&dir, std::time::Duration::from_millis(50)).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        let log = dir.join(id.to_string()).join("log.jsonl");
        let header_len = fs::metadata(&log).unwrap().len();

        writer
            .record_event(
                &SessionEvent::TextDelta {
                    text: "mid-turn ".into(),
                },
                None,
            )
            .unwrap();
        assert_eq!(
            fs::metadata(&log).unwrap().len(),
            header_len,
            "the first delta must only buffer"
        );

        std::thread::sleep(std::time::Duration::from_millis(80));
        writer
            .record_event(
                &SessionEvent::TextDelta {
                    text: "still going".into(),
                },
                None,
            )
            .unwrap();
        assert!(
            fs::metadata(&log).unwrap().len() > header_len,
            "the interval passed and nothing became durable"
        );
    }

    /// AC: a crash mid-turn loses at most the unflushed tail, never the
    /// Thread. The property, at every possible tear: chop the log at any
    /// byte — mid-line, mid-record, mid-UTF-8-character — and the Thread
    /// still loads, with some prefix of the full history. Only a tear inside
    /// the header (a crash inside `create` itself) may fail the load.
    #[test]
    fn a_crash_torn_tail_never_loses_the_thread() {
        let dir = scratch("torn");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
        writer.record_prompt("try the café fix").unwrap();
        for event in claude_turn() {
            writer.record_event(&event, None).unwrap();
        }
        // Multi-byte characters on the final line: a tear can land inside
        // one, and a loader that insists on whole-file UTF-8 dies there.
        writer
            .record_event(
                &SessionEvent::TextDelta {
                    text: "naïve café ☕ résumé".into(),
                },
                None,
            )
            .unwrap();
        writer
            .record_event(
                &SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    cost_usd: None,
                },
                None,
            )
            .unwrap();
        drop(writer);

        let log = dir.join(id.to_string()).join("log.jsonl");
        let bytes = fs::read(&log).unwrap();
        let header_len = bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
        let full = store.load(id).unwrap().inputs();

        for cut in 0..=bytes.len() {
            fs::write(&log, &bytes[..cut]).unwrap();
            let recovered = match store.load(id) {
                Ok(thread) => thread,
                // Refusal is allowed only for a tear inside `create`'s own
                // header write; once the header's newline is durable, a
                // failed load is a lost Thread.
                Err(e) => {
                    assert!(cut < header_len, "the Thread was lost at cut {cut}: {e}");
                    continue;
                }
            };
            assert_eq!(recovered.provider(), Provider::Claude);
            let inputs = recovered.inputs();
            assert!(
                full.starts_with(&inputs),
                "cut {cut} recovered something the full log never held: {inputs:?}"
            );
        }
    }

    /// Write a raw log for one thread, bypassing the writer — how tests plant
    /// logs from other schema eras and torn files.
    fn plant_log(dir: &Path, thread: &str, contents: &str) {
        let thread_dir = dir.join(thread);
        fs::create_dir_all(&thread_dir).unwrap();
        fs::write(thread_dir.join("log.jsonl"), contents).unwrap();
    }

    /// The frozen contract for schema 1, byte for byte what its writer
    /// produced: no `prompt` records, no `result` on `tool_completed`. Logs
    /// like this exist on disks; they must load forever.
    const V1_LOG: &str = concat!(
        r#"{"schema":1,"provider":"claude"}"#,
        "\n",
        r#"{"type":"init","session_id":"legacy-4f2a","model":"claude-haiku-4-5"}"#,
        "\n",
        r#"{"type":"text","text":"running the suite\n\n"}"#,
        "\n",
        r#"{"type":"tool_started","id":"toolu_9","name":"Bash","input":{"command":"cargo test"}}"#,
        "\n",
        r#"{"type":"tool_completed","id":"toolu_9","output":"42 passed","is_error":false}"#,
        "\n",
        r#"{"type":"turn_ended","outcome":"completed","cost_usd":0.01}"#,
        "\n",
    );

    /// AC: loading a log written at schema v1 succeeds after the bump to v2.
    /// v1 recorded no prompts and no structured results, so the load carries
    /// exactly that — nothing lost, nothing invented.
    #[test]
    fn a_log_written_at_schema_v1_still_loads_after_the_bump() {
        let dir = scratch("v1");
        plant_log(&dir, "7", V1_LOG);

        let store = Store::open(&dir).unwrap();
        let thread = store.load(ThreadId::new(7)).unwrap();
        assert_eq!(thread.provider(), Provider::Claude);
        assert_eq!(thread.resume_target(), Some("legacy-4f2a"));
        assert_eq!(
            thread.inputs(),
            vec![
                Input::Event(SessionEvent::Init {
                    session_id: "legacy-4f2a".into(),
                    model: "claude-haiku-4-5".into(),
                }),
                Input::Event(SessionEvent::TextDelta {
                    text: "running the suite\n\n".into(),
                }),
                Input::Event(SessionEvent::ToolStarted {
                    id: "toolu_9".into(),
                    name: "Bash".into(),
                    input: serde_json::json!({ "command": "cargo test" }),
                }),
                Input::Event(SessionEvent::ToolCompleted {
                    id: "toolu_9".into(),
                    output: "42 passed".into(),
                    is_error: false,
                    result: ToolResult::Opaque,
                }),
                Input::Event(SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    cost_usd: Some(0.01),
                }),
            ]
        );
    }

    /// A resumed old Thread gets new turns. Appending schema-2 records under
    /// a schema-1 header would make the file a lie — a v1 reader would stop
    /// at the first record it cannot know and silently lose everything after
    /// it. So reopening a v1 Thread for writing upgrades the whole log to
    /// the current schema first.
    #[test]
    fn appending_to_a_v1_thread_upgrades_its_log_first() {
        let dir = scratch("v1-append");
        plant_log(&dir, "7", V1_LOG);
        let store = Store::open(&dir).unwrap();

        let mut writer = store.writer(ThreadId::new(7)).unwrap();
        writer.record_prompt("continue where you left off").unwrap();
        writer
            .record_event(
                &SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    cost_usd: None,
                },
                None,
            )
            .unwrap();
        drop(writer);

        let log = dir.join("7").join("log.jsonl");
        let first_line = fs::read_to_string(&log)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_string();
        assert!(
            first_line.contains(&format!("\"schema\":{SCHEMA_VERSION}")),
            "the log still declares the old schema: {first_line}"
        );

        let thread = store.load(ThreadId::new(7)).unwrap();
        let inputs = thread.inputs();
        assert_eq!(inputs.len(), 7, "history: {inputs:?}");
        assert_eq!(
            inputs[0],
            Input::Event(SessionEvent::Init {
                session_id: "legacy-4f2a".into(),
                model: "claude-haiku-4-5".into(),
            }),
            "the v1 history must survive the upgrade"
        );
        assert_eq!(
            inputs[5],
            Input::Prompt("continue where you left off".into())
        );
    }

    /// The turn after the crash: a torn final line must not swallow what
    /// comes next. Appending straight after the tear would concatenate the
    /// first new record onto the fragment — one unreadable line where the
    /// loader stops, hiding every turn after the crash. Reopening for write
    /// clears the tear first.
    #[test]
    fn appending_after_a_crash_never_hides_the_new_turn() {
        let dir = scratch("torn-append");
        plant_log(
            &dir,
            "4",
            concat!(
                r#"{"schema":2,"provider":"codex"}"#,
                "\n",
                r#"{"type":"init","session_id":"0199-thread","model":"gpt-5.4-mini"}"#,
                "\n",
                r#"{"type":"turn_ended","outcome":"completed","cost_usd":null}"#,
                "\n",
                // The crash: a record torn mid-write, no newline.
                r#"{"type":"text","te"#,
            ),
        );
        let store = Store::open(&dir).unwrap();

        let mut writer = store.writer(ThreadId::new(4)).unwrap();
        writer.record_prompt("are you still there").unwrap();
        writer
            .record_event(
                &SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    cost_usd: None,
                },
                None,
            )
            .unwrap();
        drop(writer);

        let inputs = store.load(ThreadId::new(4)).unwrap().inputs();
        assert_eq!(
            inputs,
            vec![
                Input::Event(SessionEvent::Init {
                    session_id: "0199-thread".into(),
                    model: "gpt-5.4-mini".into(),
                }),
                Input::Event(SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    cost_usd: None,
                }),
                Input::Prompt("are you still there".into()),
                Input::Event(SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Completed,
                    cost_usd: None,
                }),
            ]
        );
    }

    /// The version gate is what the header line is for: a log from a newer
    /// Ferrite is refused whole, never half-read into a lie.
    #[test]
    fn a_log_from_ferrites_future_is_refused_not_half_read() {
        let dir = scratch("future");
        plant_log(
            &dir,
            "3",
            concat!(
                r#"{"schema":9,"provider":"claude"}"#,
                "\n",
                r#"{"type":"init","session_id":"from-the-future","model":"m"}"#,
                "\n",
            ),
        );

        let store = Store::open(&dir).unwrap();
        match store.load(ThreadId::new(3)) {
            Err(LoadError::FutureSchema { found, supported }) => {
                assert_eq!(found, 9);
                assert_eq!(supported, SCHEMA_VERSION);
            }
            Ok(_) => panic!("a future schema must not load"),
            Err(other) => panic!("expected FutureSchema, got {other:?}"),
        }
    }

    /// What the operator typed and what the agent changed are both history:
    /// the restored Pane must show the prompt echo and the diff card, not
    /// just the prose between them.
    #[test]
    fn the_operator_prompt_and_the_diff_card_survive_the_round_trip() {
        let dir = scratch("prompt-diff");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();

        let mut live = Transcript::default();
        writer.record_prompt("fix the typo").unwrap();
        live.apply(Input::Prompt("fix the typo".into()));
        for event in [
            SessionEvent::TextDelta {
                text: "Editing now.\n\n".into(),
            },
            SessionEvent::ToolStarted {
                id: "toolu_edit".into(),
                name: "Edit".into(),
                input: serde_json::json!({ "file_path": "/workspace/x.txt" }),
            },
            SessionEvent::ToolCompleted {
                id: "toolu_edit".into(),
                output: "applied".into(),
                is_error: false,
                result: ToolResult::FileEdit {
                    path: "/workspace/x.txt".into(),
                    hunks: vec![crate::Hunk {
                        old_start: 1,
                        old_lines: 3,
                        new_start: 1,
                        new_lines: 3,
                        lines: vec![
                            " alpha".into(),
                            "-bravo".into(),
                            "+delta".into(),
                            " charlie".into(),
                        ],
                    }],
                },
            },
            SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            },
        ] {
            writer.record_event(&event, None).unwrap();
            live.apply(Input::Event(event));
        }
        drop(writer);

        let thread = Store::open(&dir).unwrap().load(id).unwrap();
        let restored = restore(&thread);
        assert_eq!(restored.blocks(), live.blocks());
    }

    /// Codex's own concepts survive the round trip: reasoning summaries keep
    /// their part structure, token accounting keeps its numbers, and the
    /// provider-native thread id is the resume target. Asserted on `inputs`
    /// rather than Blocks because nothing renders reasoning summaries yet —
    /// the store must not lose what a Pane does not yet draw.
    #[test]
    fn a_codex_turn_keeps_its_reasoning_and_token_accounting() {
        let dir = scratch("codex");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Codex, None, main_choice()).unwrap();

        let usage = SessionEvent::TokenUsage {
            total_tokens: 900,
            input_tokens: 600,
            cached_input_tokens: 100,
            output_tokens: 300,
            reasoning_output_tokens: 120,
            context_window: Some(272_000),
        };
        for event in [
            SessionEvent::Init {
                session_id: "0199a1b2-thread".into(),
                model: "gpt-5.4-mini".into(),
            },
            SessionEvent::ReasoningSummaryDelta {
                text: "planning ".into(),
                summary_index: 0,
            },
            SessionEvent::ReasoningSummaryDelta {
                text: "the fix".into(),
                summary_index: 0,
            },
            SessionEvent::ReasoningSummaryDelta {
                text: "running tests".into(),
                summary_index: 1,
            },
            usage.clone(),
            SessionEvent::TurnEnded {
                outcome: TurnOutcome::Interrupted,
                cost_usd: None,
            },
            SessionEvent::Closed {
                reason: "codex app-server exited: exit status: 0".into(),
            },
        ] {
            writer.record_event(&event, None).unwrap();
        }
        drop(writer);

        let thread = Store::open(&dir).unwrap().load(id).unwrap();
        assert_eq!(thread.provider(), Provider::Codex);
        assert_eq!(thread.resume_target(), Some("0199a1b2-thread"));
        assert_eq!(
            thread.inputs(),
            vec![
                Input::Event(SessionEvent::Init {
                    session_id: "0199a1b2-thread".into(),
                    model: "gpt-5.4-mini".into(),
                }),
                Input::Event(SessionEvent::ReasoningSummaryDelta {
                    text: "planning the fix".into(),
                    summary_index: 0,
                }),
                Input::Event(SessionEvent::ReasoningSummaryDelta {
                    text: "running tests".into(),
                    summary_index: 1,
                }),
                Input::Event(usage),
                Input::Event(SessionEvent::TurnEnded {
                    outcome: TurnOutcome::Interrupted,
                    cost_usd: None,
                }),
                Input::Event(SessionEvent::Closed {
                    reason: "codex app-server exited: exit status: 0".into(),
                }),
            ]
        );
    }

    #[test]
    fn a_flushed_turn_replays_identically_after_reopening() {
        let dir = scratch("roundtrip");
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Claude, None, main_choice()).unwrap();

        let mut live = Transcript::default();
        for event in claude_turn() {
            writer.record_event(&event, None).unwrap();
            live.apply(Input::Event(event));
        }
        drop(writer);
        drop(store);

        let thread = Store::open(&dir).unwrap().load(id).unwrap();
        assert_eq!(thread.resume_target(), Some("4f2a1c9e-7b30"));

        let restored = restore(&thread);
        assert_eq!(restored.blocks(), live.blocks());
        assert_eq!(restored.session_id(), live.session_id());
        assert_eq!(restored.model(), live.model());
        assert_eq!(restored.last_cost(), live.last_cost());
        assert_eq!(restored.status(), live.status());
    }

    #[test]
    fn a_created_thread_survives_reopening_the_store() {
        let dir = scratch("reopen");
        {
            let store = Store::open(&dir).unwrap();
            let (id, _writer) = store.create(Provider::Claude, None, main_choice()).unwrap();
            assert_eq!(id.to_string(), "1");
        }

        // The fake restart: nothing survives but the directory.
        let store = Store::open(&dir).unwrap();
        let ids = store.thread_ids().unwrap();
        assert_eq!(ids.len(), 1);
        let thread = store.load(ids[0]).unwrap();
        assert_eq!(thread.id, ids[0]);
        assert_eq!(thread.provider(), Provider::Claude);
        assert_eq!(thread.resume_target(), None);
        assert!(thread.inputs().is_empty());
    }
}
