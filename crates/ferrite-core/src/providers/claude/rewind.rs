//! Rewind and fork over Claude's control protocol
//! (docs/research/edit-rewind.md).
//!
//! A rewind is one or two control requests, answered on the reader thread:
//! `rewind_files` first when the files are to come back (the checkpoint is
//! keyed by the prompt's message uuid, which the cut removes), then
//! `rewind_conversation`, which cuts the live conversation in place and
//! persists the cut. A CLI that does not know `rewind_conversation` says
//! "Unsupported control request subtype"; the rewind then becomes a resume
//! at the entry before the prompt (`--resume-session-at`), read from the
//! CLI's own session file. A fork is always that resume, forked.

use std::io;
use std::path::{Path, PathBuf};
use std::process::ChildStdin;
use std::sync::mpsc::SyncSender;
use std::sync::Mutex;

use serde_json::{json, Value};

use crate::providers::{Cut, FilesRestored, Forked, RewindRequest, Rewound};

/// The rewind in flight.
#[derive(Default)]
pub(super) struct Rewinds {
    pending: Option<Pending>,
}

struct Pending {
    /// The control request whose answer moves it on.
    request_id: String,
    stage: Stage,
    request: RewindRequest,
    files: Option<FilesRestored>,
    reply: SyncSender<io::Result<Rewound>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Files,
    Conversation,
}

impl Rewinds {
    /// Start `request` under control request `request_id`: the line to
    /// write. A newer rewind takes the place of one still unanswered, whose
    /// ack then reads as never answered.
    pub(super) fn begin(
        &mut self,
        request_id: String,
        request: RewindRequest,
        reply: SyncSender<io::Result<Rewound>>,
    ) -> Value {
        let stage = if request.files {
            Stage::Files
        } else {
            Stage::Conversation
        };
        let line = control(&request_id, stage, &request);
        self.pending = Some(Pending {
            request_id,
            stage,
            request,
            files: None,
            reply,
        });
        line
    }

    /// Take in one stdout line. Answers whether it was a rewind's, which
    /// nothing else reads.
    pub(super) fn observe(&mut self, frame: &Value, stdin: &Mutex<ChildStdin>) -> bool {
        let response = &frame["response"];
        if frame["type"] != "control_response"
            || self
                .pending
                .as_ref()
                .map(|pending| pending.request_id.as_str())
                != response["request_id"].as_str()
        {
            return false;
        }
        let mut pending = self.pending.take().expect("matched above");
        let answer = match pending.stage {
            Stage::Files => match files_restored(response) {
                Err(refusal) => Ok(Rewound {
                    cut: Cut::Refused(format!("the files could not be restored: {refusal}")),
                    files: None,
                }),
                Ok(files) => {
                    // On to the conversation, under an id of its own.
                    pending.files = Some(files);
                    pending.stage = Stage::Conversation;
                    pending.request_id = format!("{}_conversation", pending.request_id);
                    let line = control(&pending.request_id, pending.stage, &pending.request);
                    match super::write_stdin_line(stdin, &line) {
                        Ok(()) => {
                            self.pending = Some(pending);
                            return true;
                        }
                        Err(error) => Err(error),
                    }
                }
            },
            Stage::Conversation => cut(response, &pending.request).map(|cut| Rewound {
                cut,
                files: pending.files.take(),
            }),
        };
        let _ = pending.reply.send(answer);
        true
    }
}

/// Where a fork before the prompt `request` names starts.
pub(super) fn fork(request: &RewindRequest) -> io::Result<Forked> {
    let session = session(request)?;
    Ok(
        match entry_parent(&projects_root(), &session, &request.prompt)? {
            Some(at) => Forked::ResumeAt { session, at },
            None => Forked::Fresh,
        },
    )
}

/// What `rewind_conversation` answered.
fn cut(response: &Value, request: &RewindRequest) -> io::Result<Cut> {
    if response["subtype"] != "success" {
        let error = response["error"].as_str().unwrap_or("rewind refused");
        if !error.contains("Unsupported control request subtype") {
            return Ok(Cut::Refused(error.to_string()));
        }
        // An older CLI: resume at the entry before the prompt instead.
        let session = session(request)?;
        return Ok(
            match entry_parent(&projects_root(), &session, &request.prompt)? {
                Some(at) => Cut::ResumeAt { session, at },
                None => Cut::Refused(
                    "this Claude Code cannot rewind to its first prompt; \
                 start a new Thread instead"
                        .into(),
                ),
            },
        );
    }
    let body = &response["response"];
    Ok(if body["rewound"] == true {
        Cut::InPlace
    } else {
        Cut::Refused(refusal(body))
    })
}

fn session(request: &RewindRequest) -> io::Result<String> {
    request
        .session
        .clone()
        .ok_or_else(|| io::Error::other("Claude has not named its session yet"))
}

/// The control request for `stage` of `request`.
fn control(request_id: &str, stage: Stage, request: &RewindRequest) -> Value {
    let body = match stage {
        Stage::Files => json!({
            "subtype": "rewind_files",
            "user_message_id": request.prompt,
            "dry_run": false,
        }),
        Stage::Conversation => {
            let mut body = json!({
                "subtype": "rewind_conversation",
                "target_message_uuid": request.prompt,
            });
            if let Some(latest) = &request.latest {
                body["last_seen_user_message_uuid"] = json!(latest);
            }
            body
        }
    };
    json!({"type": "control_request", "request_id": request_id, "request": body})
}

/// What `rewind_files` answered: what it changed, or why it could not.
/// `filesChanged` is a list of paths; the counts may be absent.
fn files_restored(response: &Value) -> Result<FilesRestored, String> {
    if response["subtype"] != "success" {
        return Err(response["error"]
            .as_str()
            .unwrap_or("no checkpoint for that prompt")
            .to_string());
    }
    let body = &response["response"];
    if body["canRewind"] == false {
        return Err(body["error"]
            .as_str()
            .unwrap_or("no checkpoint for that prompt")
            .to_string());
    }
    Ok(FilesRestored {
        files: body["filesChanged"].as_array().map_or(0, Vec::len),
        insertions: body["insertions"].as_u64().unwrap_or(0),
        deletions: body["deletions"].as_u64().unwrap_or(0),
    })
}

/// A refused cut, as the operator reads it: the CLI's `reason` in words,
/// else its `error`.
fn refusal(body: &Value) -> String {
    let reason = body["reason"].as_str().unwrap_or_default();
    match reason {
        "turn_running" => "a turn is running".into(),
        "commands_queued" | "prompt_pending" => "prompts are waiting to be sent".into(),
        "target_not_found" => "Claude no longer has that prompt".into(),
        "stale_target" | "unseen_later_turn" | "state_changed" => {
            "the conversation changed since; try again".into()
        }
        "target_splits_tool_call" => "that prompt sits inside a tool call".into(),
        "persist_failed" => "Claude could not save the cut".into(),
        "usage_limited" => "the usage limit is reached".into(),
        _ => body["error"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| format!("refused ({reason})")),
    }
}

/// Where Claude keeps its session files (`import::default_roots`).
fn projects_root() -> PathBuf {
    crate::import::default_roots()
        .into_iter()
        .find(|(provider, _)| *provider == crate::store::Provider::Claude)
        .map(|(_, root)| root)
        .unwrap_or_default()
}

/// The entry before `uuid` in session `session`'s file under `root`
/// (`<root>/<project>/<session>.jsonl`): its `parentUuid`, `None` when it
/// is the conversation's first.
pub(super) fn entry_parent(root: &Path, session: &str, uuid: &str) -> io::Result<Option<String>> {
    let name = format!("{session}.jsonl");
    let file = std::fs::read_dir(root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(&name))
        .find(|path| path.is_file())
        .ok_or_else(|| io::Error::other(format!("Claude's file for session {session} is gone")))?;
    let text = std::fs::read_to_string(file)?;
    text.lines()
        .filter(|line| line.contains(uuid))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|entry| entry["uuid"] == uuid)
        .map(|entry| entry["parentUuid"].as_str().map(str::to_string))
        .ok_or_else(|| io::Error::other("Claude's session file does not hold that prompt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The replies as 2.1.317's stdin dispatcher builds them
    /// (`claude-rewind-2.1.317.jsonl`, written from the handler in the
    /// binary; a live capture is still owed): restored files, a cut, a
    /// refusal, an older CLI's unknown subtype, and a missing checkpoint.
    const FIXTURE: &str = include_str!("../../../tests/fixtures/claude-rewind-2.1.317.jsonl");

    fn frame(index: usize) -> Value {
        serde_json::from_str(FIXTURE.lines().nth(index).unwrap()).unwrap()
    }

    #[test]
    fn the_requests_name_the_prompt_by_its_message_uuid() {
        let request = RewindRequest {
            prompt: "p2".into(),
            latest: Some("p3".into()),
            files: true,
            session: None,
        };
        let files = control("req_7", Stage::Files, &request);
        assert_eq!(files["request"]["subtype"], "rewind_files");
        assert_eq!(files["request"]["user_message_id"], "p2");
        assert_eq!(files["request"]["dry_run"], false);
        let cut = control("req_7_conversation", Stage::Conversation, &request);
        assert_eq!(cut["request"]["subtype"], "rewind_conversation");
        assert_eq!(cut["request"]["target_message_uuid"], "p2");
        assert_eq!(cut["request"]["last_seen_user_message_uuid"], "p3");
    }

    #[test]
    fn the_captured_replies_read_as_restored_cut_and_refused() {
        let files = files_restored(&frame(0)["response"]).unwrap();
        assert_eq!(
            files,
            FilesRestored {
                files: 2,
                insertions: 3,
                deletions: 7
            }
        );
        let request = RewindRequest {
            prompt: "p2".into(),
            latest: None,
            files: false,
            session: None,
        };
        assert_eq!(cut(&frame(1)["response"], &request).unwrap(), Cut::InPlace);
        assert_eq!(
            cut(&frame(2)["response"], &request).unwrap(),
            Cut::Refused("a turn is running".into())
        );
        assert!(files_restored(&frame(4)["response"]).is_err());
    }

    /// An older CLI's refusal is no refusal: the rewind resumes at the
    /// prompt's parent entry, read off the session file.
    #[test]
    fn an_unknown_subtype_becomes_a_resume_at_the_entry_before() {
        let root = std::env::temp_dir().join(format!("ferrite-rewind-{}", std::process::id()));
        let project = root.join("-work");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("sess.jsonl"),
            concat!(
                r#"{"type":"user","uuid":"p1","parentUuid":null}"#,
                "\n",
                r#"{"type":"assistant","uuid":"a1","parentUuid":"p1"}"#,
                "\n",
                r#"{"type":"user","uuid":"p2","parentUuid":"a1"}"#,
                "\n"
            ),
        )
        .unwrap();
        assert_eq!(
            entry_parent(&root, "sess", "p2").unwrap(),
            Some("a1".into())
        );
        assert_eq!(entry_parent(&root, "sess", "p1").unwrap(), None);
        assert!(entry_parent(&root, "sess", "p9").is_err());
        assert!(frame(3)["response"]["error"]
            .as_str()
            .unwrap()
            .contains("Unsupported control request subtype"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
