//! Session import: adopt a Claude Code or Codex session file as a Thread.
//!
//! "Your CLI session, continued here": the vendor's own on-disk session file
//! — `~/.claude/projects/<slug>/<session>.jsonl` or
//! `~/.codex/sessions/<date>/rollout-*.jsonl` — is parsed into a new Thread
//! log: prompts, text, tool runs, turn ends, in the same records a live
//! Session would have written. The file's own session id becomes the
//! Thread's resume target, so the Thread's next prompt continues the
//! conversation the CLI was having (both providers reload their side from
//! their own files; proven by the live import probes).
//!
//! The parsers read the shapes the committed import fixtures record and
//! ignore everything else — vendor session files carry far more (queue
//! bookkeeping, attachments, scaffolding messages) than a transcript is made
//! of, and an unknown line must cost nothing. A file that matches neither
//! vendor, or names no session to resume, is refused whole with a reason fit
//! for the operator: import exists to continue a conversation, and a Thread
//! that could not would be a lie.

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use crate::store::{Provider, Store};
use crate::workspace::WorkspaceBinding;
use crate::{SessionEvent, ThreadId, ToolResult, TurnOutcome};

/// Importing failed; no Thread was created.
#[derive(Debug)]
pub enum ImportError {
    /// The file is not a session file Ferrite knows how to adopt — foreign,
    /// damaged, empty, or missing the session id that resume needs. The
    /// detail says what was seen, in words fit for the operator.
    Unrecognized {
        detail: String,
    },
    Io(io::Error),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Unrecognized { detail } => {
                write!(f, "not an importable session file: {detail}")
            }
            ImportError::Io(e) => write!(f, "io error importing session file: {e}"),
        }
    }
}

impl std::error::Error for ImportError {}

impl From<io::Error> for ImportError {
    fn from(e: io::Error) -> Self {
        ImportError::Io(e)
    }
}

/// What one session file parses into: everything the Thread log is written
/// from.
struct ParsedSession {
    provider: Provider,
    session_id: String,
    model: String,
    /// The directory the session was working in — the Thread's workspace
    /// binding, when the file records one.
    cwd: Option<PathBuf>,
    entries: Vec<Entry>,
}

/// One line of history, in the order the log will hold it.
enum Entry {
    Prompt(String),
    Event(SessionEvent),
}

/// Where the vendors write session files, per the layouts the import
/// fixtures capture: `~/.claude/projects/<slug>/<session>.jsonl` and
/// `~/.codex/sessions/<date dirs>/rollout-*.jsonl`. Windows spells the
/// home directory USERPROFILE, not HOME.
pub fn default_roots() -> Vec<(Provider, PathBuf)> {
    let home = PathBuf::from(
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".into()),
    );
    vec![
        (Provider::Claude, home.join(".claude").join("projects")),
        (Provider::Codex, home.join(".codex").join("sessions")),
    ]
}

/// One session file discovery found: which vendor's root it was under, and
/// when the vendor last wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub provider: Provider,
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
}

/// Candidate session files under the vendors' roots: every `.jsonl` in
/// either tree — the layouts differ (per-project slugs vs per-date
/// directories), so the walk recurses instead of assuming a depth, the
/// same read the live import probes use — newest first, capped. A missing
/// root lists nothing: that vendor was simply never run here. Whether a
/// candidate really is an adoptable session stays the parser's verdict,
/// not the filename's.
pub fn candidates(roots: &[(Provider, PathBuf)], cap: usize) -> Vec<Candidate> {
    fn walk(provider: Provider, dir: &Path, into: &mut Vec<Candidate>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(provider, &path, into);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
            {
                into.push(Candidate {
                    provider,
                    modified: std::fs::metadata(&path)
                        .and_then(|metadata| metadata.modified())
                        .ok(),
                    path,
                });
            }
        }
    }
    let mut found = Vec::new();
    for (provider, root) in roots {
        walk(*provider, root, &mut found);
    }
    // Newest first; a file with no readable mtime sorts oldest rather than
    // vanishing.
    found.sort_by_key(|candidate| std::cmp::Reverse(candidate.modified));
    found.truncate(cap);
    found
}

/// Adopt the session file at `path` as a new Thread in `store`. The Thread
/// is durable when this returns, and `store.load(id).resume_target()` names
/// the imported session — the id the next Session resumes from.
pub fn import(store: &Store, path: &Path) -> Result<ThreadId, ImportError> {
    let mut registry = crate::workspace::registry::Registry::open(store.dir())?;
    import_registered(store, &mut registry, path)
}

pub fn import_registered(
    store: &Store,
    registry: &mut crate::workspace::registry::Registry,
    path: &Path,
) -> Result<ThreadId, ImportError> {
    let bytes = std::fs::read(path)?;
    let parsed = parse(&bytes)?;

    // The session file knows where it was working; the Thread binds to that
    // checkout. A file recording none is the degenerate case — every
    // committed capture records one — and binds to the file's own directory:
    // the nearest true thing, and visible rather than invented.
    let checkout = parsed
        .cwd
        .clone()
        .or_else(|| path.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    let project = registry.register_recorded(&checkout)?;
    let (id, mut writer) = store.create(
        parsed.provider,
        Some(project),
        WorkspaceBinding::Main { checkout },
    )?;

    // A failure past this point must not leave a half-written Thread
    // claiming to be the imported conversation (the error type says no
    // Thread was created, and it must not lie). The writer drops first so
    // its file handle is closed before the directory goes; the deletion is
    // best-effort, which is enough — it can only fail for the same kind of
    // io trouble that failed the write, the import still reports that
    // original error, and a directory the deletion could not remove loads as
    // an ordinary (short) Thread rather than corruption.
    let written = write_history(&mut writer, parsed);
    drop(writer);
    if let Err(e) = written {
        let _ = store.delete(id);
        return Err(e.into());
    }
    Ok(id)
}

fn write_history(writer: &mut crate::store::ThreadWriter, parsed: ParsedSession) -> io::Result<()> {
    writer.record_event(
        &SessionEvent::Init {
            session_id: parsed.session_id,
            model: parsed.model,
        },
        None,
    )?;
    for entry in &parsed.entries {
        match entry {
            Entry::Prompt(text) => writer.record_prompt(text)?,
            Entry::Event(event) => writer.record_event(event, None)?,
        }
    }
    writer.flush()
}

/// Parse the file as whichever vendor's session it is. Detection reads the
/// evidence, not the filename: a Codex rollout opens with a `session_meta`
/// line; a Claude Code session's lines carry a `sessionId`. The first
/// parseable line that shows either marker decides — a file showing neither
/// is foreign.
fn parse(bytes: &[u8]) -> Result<ParsedSession, ImportError> {
    for line in bytes.split(|byte| *byte == b'\n') {
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) == Some("session_meta") {
            return parse_codex(bytes);
        }
        if value.get("sessionId").and_then(Value::as_str).is_some() {
            return parse_claude(bytes);
        }
    }
    Err(ImportError::Unrecognized {
        detail: "neither a Claude Code session (no line carries a sessionId) \
                 nor a Codex rollout (no session_meta line)"
            .into(),
    })
}

fn unreadable(which: &'static str) -> ImportError {
    ImportError::Unrecognized {
        detail: format!("recognised a {which} session file, but no session id could be read"),
    }
}

/// A marker line is recognition, not trust: a file whose body yielded no
/// history at all is refused — a Thread with no conversation in it would be
/// nothing to continue.
fn no_conversation(which: &'static str) -> ImportError {
    ImportError::Unrecognized {
        detail: format!("recognised a {which} session file, but no conversation could be read"),
    }
}

/// A Claude Code session file: one JSONL line per conversation item, every
/// line stamped with the session id. Only `user` and `assistant` lines are
/// history; the rest — queue bookkeeping, attachments, prompt caches — is
/// the CLI's own business.
///
/// The file marks no turn boundaries, so the conversation's own rhythm
/// stands in: a new prompt closes the turn before it, and the end of the
/// file closes the last. Costs are unknown — the file records none — and an
/// empty thinking block (the CLI persists redacted thinking as an empty
/// string beside its signature) is nothing, not an empty thought.
fn parse_claude(bytes: &[u8]) -> Result<ParsedSession, ImportError> {
    let mut session_id = None;
    let mut model = String::new();
    let mut cwd = None;
    let mut entries = Vec::new();
    let mut turn_open = false;

    for line in bytes.split(|byte| *byte == b'\n') {
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if session_id.is_none() {
            session_id = value
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        // The CLI marks its own injected lines (caveats, command echoes) as
        // meta; they were never the operator speaking.
        if value.get("isMeta").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        // Sidechain lines are a subagent's conversation, not this Thread's.
        // On 2.1.241 they live in their own `subagents/` transcript files —
        // stamped with the parent's session id, so a subagent file offered
        // for import ends up refused here as having no conversation, instead
        // of adopting the parent session it is not.
        if value.get("isSidechain").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let message = value.get("message");
        match value.get("type").and_then(Value::as_str) {
            Some("user") => {
                if cwd.is_none() {
                    cwd = value.get("cwd").and_then(Value::as_str).map(PathBuf::from);
                }
                let Some(content) = message.and_then(|m| m.get("content")) else {
                    continue;
                };
                match content {
                    Value::String(text) => {
                        close_turn(&mut entries, &mut turn_open);
                        entries.push(Entry::Prompt(text.clone()));
                        turn_open = true;
                    }
                    Value::Array(blocks) => {
                        // A user line is either the operator's prompt or the
                        // CLI feeding a tool result back — never both.
                        let results: Vec<&Value> = blocks
                            .iter()
                            .filter(|block| block_type(block) == Some("tool_result"))
                            .collect();
                        if results.is_empty() {
                            let text: String = blocks
                                .iter()
                                .filter(|block| block_type(block) == Some("text"))
                                .filter_map(|block| block.get("text")?.as_str())
                                .collect();
                            if !text.is_empty() {
                                close_turn(&mut entries, &mut turn_open);
                                entries.push(Entry::Prompt(text));
                                turn_open = true;
                            }
                        }
                        for block in results {
                            let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else {
                                continue;
                            };
                            turn_open = true;
                            entries.push(Entry::Event(SessionEvent::ToolCompleted {
                                id: id.to_string(),
                                output: match block.get("content") {
                                    Some(Value::String(text)) => text.clone(),
                                    Some(other) => other.to_string(),
                                    None => String::new(),
                                },
                                is_error: block
                                    .get("is_error")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false),
                                result: ToolResult::Opaque,
                            }));
                        }
                    }
                    _ => {}
                }
            }
            Some("assistant") => {
                let Some(message) = message else { continue };
                if model.is_empty() {
                    if let Some(name) = message.get("model").and_then(Value::as_str) {
                        model = name.to_string();
                    }
                }
                for block in message
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let event = match block_type(block) {
                        Some("text") => block.get("text").and_then(Value::as_str).map(|text| {
                            SessionEvent::TextDelta {
                                text: text.to_string(),
                            }
                        }),
                        Some("thinking") => block
                            .get("thinking")
                            .and_then(Value::as_str)
                            .filter(|text| !text.is_empty())
                            .map(|text| SessionEvent::ThinkingDelta {
                                text: text.to_string(),
                            }),
                        Some("tool_use") => Some(SessionEvent::ToolStarted {
                            id: block
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            name: block
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            input: block.get("input").cloned().unwrap_or(Value::Null),
                        }),
                        _ => None,
                    };
                    if let Some(event) = event {
                        turn_open = true;
                        entries.push(Entry::Event(event));
                    }
                }
            }
            _ => {}
        }
    }

    close_turn(&mut entries, &mut turn_open);
    if entries.is_empty() {
        return Err(no_conversation("Claude Code"));
    }
    Ok(ParsedSession {
        provider: Provider::Claude,
        session_id: session_id.ok_or_else(|| unreadable("Claude Code"))?,
        model,
        cwd,
        entries,
    })
}

/// End an open turn the way the file's silence implies. Claude session files
/// mark no turns, so the conversation's own rhythm stands in: a turn that
/// got its reply ended well; one whose last word is the prompt itself, or a
/// tool call still waiting on its result, never finished — the session was
/// cut off there, and pretending it completed would leave a dead Thread
/// claiming otherwise. Cost unknown either way.
fn close_turn(entries: &mut Vec<Entry>, turn_open: &mut bool) {
    if std::mem::take(turn_open) {
        let outcome = match entries.last() {
            Some(Entry::Prompt(_)) | Some(Entry::Event(SessionEvent::ToolStarted { .. })) => {
                TurnOutcome::Interrupted
            }
            _ => TurnOutcome::Completed,
        };
        entries.push(Entry::Event(SessionEvent::TurnEnded {
            outcome,
            cost_usd: None,
        }));
    }
}

/// A Codex rollout: a `session_meta` line, then `event_msg` lines (the
/// display stream — prompts, agent text, token counts, turn verdicts) and
/// `response_item` lines (the model-context stream — read only for tool
/// calls and reasoning summaries; its user/developer/assistant messages
/// duplicate what `event_msg` already said).
///
/// Codex marks every turn's end in so many words — `task_complete` or
/// `turn_aborted` — so a turn still open when the file ends really was cut
/// off, and closes as interrupted rather than an invented completion.
fn parse_codex(bytes: &[u8]) -> Result<ParsedSession, ImportError> {
    let mut session_id = None;
    let mut model = String::new();
    let mut cwd = None;
    let mut entries = Vec::new();
    let mut turn_open = false;

    for line in bytes.split(|byte| *byte == b'\n') {
        let Ok(value) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        let Some(payload) = value.get("payload") else {
            continue;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("session_meta") => {
                if session_id.is_none() {
                    session_id = payload
                        .get("id")
                        .or_else(|| payload.get("session_id"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
                if cwd.is_none() {
                    cwd = payload
                        .get("cwd")
                        .and_then(Value::as_str)
                        .map(PathBuf::from);
                }
            }
            Some("turn_context") => {
                if model.is_empty() {
                    if let Some(name) = payload.get("model").and_then(Value::as_str) {
                        model = name.to_string();
                    }
                }
            }
            Some("event_msg") => {
                let event = match payload.get("type").and_then(Value::as_str) {
                    Some("user_message") => {
                        if let Some(text) = payload.get("message").and_then(Value::as_str) {
                            entries.push(Entry::Prompt(text.to_string()));
                            // The prompt opens its turn: a file that ends
                            // before any reply still closes as interrupted.
                            turn_open = true;
                        }
                        None
                    }
                    Some("agent_message") => {
                        payload.get("message").and_then(Value::as_str).map(|text| {
                            SessionEvent::TextDelta {
                                text: text.to_string(),
                            }
                        })
                    }
                    Some("token_count") => parse_token_count(payload),
                    Some("task_complete") => Some(SessionEvent::TurnEnded {
                        outcome: TurnOutcome::Completed,
                        cost_usd: None,
                    }),
                    Some("turn_aborted") => Some(SessionEvent::TurnEnded {
                        outcome: TurnOutcome::Interrupted,
                        cost_usd: None,
                    }),
                    _ => None,
                };
                if let Some(event) = event {
                    // A turn's own verdict closes it; anything else means it
                    // is (still) running.
                    turn_open = !matches!(event, SessionEvent::TurnEnded { .. });
                    entries.push(Entry::Event(event));
                }
            }
            Some("response_item") => {
                match payload.get("type").and_then(Value::as_str) {
                    Some("reasoning") => {
                        for (index, part) in payload
                            .get("summary")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .enumerate()
                        {
                            if let Some(text) = part.get("text").and_then(Value::as_str) {
                                turn_open = true;
                                entries.push(Entry::Event(SessionEvent::ReasoningSummaryDelta {
                                    text: text.to_string(),
                                    summary_index: index as u64,
                                }));
                            }
                        }
                    }
                    Some("function_call") => {
                        turn_open = true;
                        entries.push(Entry::Event(SessionEvent::ToolStarted {
                            id: payload
                                .get("call_id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            name: payload
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            // The rollout holds the arguments as a JSON
                            // string; the Thread holds them as the value the
                            // wire's ToolStarted would have carried.
                            input: payload
                                .get("arguments")
                                .and_then(Value::as_str)
                                .and_then(|args| serde_json::from_str(args).ok())
                                .unwrap_or(Value::Null),
                        }));
                    }
                    Some("function_call_output") => {
                        turn_open = true;
                        entries.push(Entry::Event(SessionEvent::ToolCompleted {
                            id: payload
                                .get("call_id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            output: payload
                                .get("output")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            // The rollout does not flag failures; the output
                            // text carries the exit code for whoever reads it.
                            is_error: false,
                            result: ToolResult::Opaque,
                        }));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    if turn_open {
        entries.push(Entry::Event(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Interrupted,
            cost_usd: None,
        }));
    }
    if entries.is_empty() {
        return Err(no_conversation("Codex"));
    }
    Ok(ParsedSession {
        provider: Provider::Codex,
        session_id: session_id.ok_or_else(|| unreadable("Codex"))?,
        model,
        cwd,
        entries,
    })
}

/// Match the live wire: the latest context size, with cumulative output
/// counters retained separately for the Transcript's turn accounting.
fn parse_token_count(payload: &Value) -> Option<SessionEvent> {
    let info = payload.get("info")?;
    let total = info.get("total_token_usage")?;
    let count = |key: &str| total.get(key).and_then(Value::as_u64).unwrap_or(0);
    Some(SessionEvent::TokenUsage {
        total_tokens: info
            .get("last_token_usage")?
            .get("total_tokens")?
            .as_u64()?,
        input_tokens: count("input_tokens"),
        cached_input_tokens: count("cached_input_tokens"),
        output_tokens: count("output_tokens"),
        reasoning_output_tokens: count("reasoning_output_tokens"),
        context_window: info.get("model_context_window").and_then(Value::as_u64),
    })
}

fn block_type(block: &Value) -> Option<&str> {
    block.get("type").and_then(Value::as_str)
}
