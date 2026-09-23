//! Codex's background terminals, read off the item stream.
//!
//! Codex runs shell commands through one unified exec tool: the tool
//! yields after a short wait, and a process still running when it does
//! keeps running as a *background terminal* — what Claude calls a
//! backgrounded task. The app-server never announces that moment. What it
//! does say is that the command's `commandExecution` item stays
//! `inProgress` (it completes only when the process ends, exit code and
//! all) while the model goes on to its next item, or the turn ends. The
//! tool is synchronous per item, so a command whose item is still open when
//! a later item starts, or when its turn completes, is exactly one the tool
//! yielded on. That is the reading here; `thread/backgroundTerminals/list`
//! (`itemId`, `processId`, `command`, `cwd`) agrees with it, verified live
//! against 0.153.4.
//!
//! Every change publishes the whole running set as a
//! `ProgressEvent::BackgroundSnapshot`, the shape Claude's
//! `background_tasks_changed` takes, so the cockpit's chips and controls
//! card read both providers alike. A task's id is the terminal's
//! `processId` — the handle `thread/backgroundTerminals/terminate` wants —
//! so a chip's `×` needs no translation.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::progress::{BackgroundTask, ProgressEvent, TaskStatus};
use crate::SessionEvent;

#[derive(Default)]
pub(super) struct BackgroundTerminals {
    /// Main's open `commandExecution` items with a process behind them, by
    /// item id: candidates until a later item or the turn's end proves the
    /// tool yielded on them.
    open: BTreeMap<String, Terminal>,
}

struct Terminal {
    process_id: String,
    command: String,
    backgrounded: bool,
}

impl BackgroundTerminals {
    /// Fold one server frame in; the new running set when it changed.
    pub fn observe(&mut self, frame: &Value, main_thread: Option<&str>) -> Option<SessionEvent> {
        let method = frame.get("method")?.as_str()?;
        let params = frame.get("params")?;
        // Only Main's own processes: a child's terminal is the child's, and
        // terminate is addressed by thread.
        if let Some(main) = main_thread {
            if params["threadId"]
                .as_str()
                .is_some_and(|thread| thread != main)
            {
                return None;
            }
        }
        let before = self.running();
        match method {
            "item/started" => {
                let item = &params["item"];
                let id = item["id"].as_str()?;
                // Everything still open when this item began was yielded on.
                for (open, terminal) in &mut self.open {
                    if open != id {
                        terminal.backgrounded = true;
                    }
                }
                if item["type"] == "commandExecution" {
                    if let Some(process) = item["processId"].as_str().filter(|p| !p.is_empty()) {
                        self.open.insert(
                            id.to_string(),
                            Terminal {
                                process_id: process.to_string(),
                                command: command_of(item),
                                backgrounded: false,
                            },
                        );
                    }
                }
            }
            "item/completed" => {
                let item = &params["item"];
                if item["type"] == "commandExecution" {
                    if let Some(id) = item["id"].as_str() {
                        self.open.remove(id);
                    }
                }
            }
            "turn/completed" => {
                for terminal in self.open.values_mut() {
                    terminal.backgrounded = true;
                }
            }
            _ => return None,
        }
        let after = self.running();
        if after == before {
            return None;
        }
        Some(SessionEvent::Progress {
            event: ProgressEvent::BackgroundSnapshot { tasks: after },
        })
    }

    fn running(&self) -> Vec<BackgroundTask> {
        self.open
            .values()
            .filter(|terminal| terminal.backgrounded)
            .map(|terminal| BackgroundTask {
                id: terminal.process_id.clone(),
                label: terminal.command.clone(),
                status: TaskStatus::Working,
                detail: "shell".into(),
            })
            .collect()
    }
}

/// The command as the operator would type it: the server's parsed actions
/// when it parsed any, else the raw line with the login-shell wrapper
/// (`/bin/zsh -lc '…'`) Codex runs it under taken off.
fn command_of(item: &Value) -> String {
    let actions: Vec<&str> = item["commandActions"]
        .as_array()
        .map(|actions| {
            actions
                .iter()
                .filter_map(|action| action["command"].as_str())
                .map(str::trim)
                .filter(|command| !command.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if !actions.is_empty() {
        return actions.join(" · ");
    }
    unwrap_shell(item["command"].as_str().unwrap_or("").trim()).to_string()
}

fn unwrap_shell(raw: &str) -> &str {
    for flag in [" -lc '", " -c '"] {
        if let Some(at) = raw.find(flag) {
            let shell = &raw[..at];
            let inner = &raw[at + flag.len()..];
            if shell
                .rsplit('/')
                .next()
                .is_some_and(|name| matches!(name, "sh" | "bash" | "zsh" | "fish" | "dash"))
            {
                if let Some(inner) = inner.strip_suffix('\'') {
                    return inner;
                }
            }
        }
    }
    raw
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn exec(id: &str, process: &str, status: &str) -> Value {
        json!({
            "type": "commandExecution",
            "id": id,
            "command": format!("/bin/zsh -lc 'sleep 600 #{id}'"),
            "cwd": "/workspace",
            "processId": process,
            "source": "unifiedExecStartup",
            "status": status,
            "commandActions": [{"type": "unknown", "command": format!("sleep 600 #{id}")}],
            "aggregatedOutput": null,
            "exitCode": null,
            "durationMs": null
        })
    }

    fn started(thread: &str, item: Value) -> Value {
        json!({"method": "item/started", "params": {"threadId": thread, "turnId": "t1", "item": item}})
    }

    fn completed(thread: &str, item: Value) -> Value {
        json!({"method": "item/completed", "params": {"threadId": thread, "turnId": "t1", "item": item}})
    }

    fn running(event: &Option<SessionEvent>) -> Vec<(String, String)> {
        match event {
            Some(SessionEvent::Progress {
                event: ProgressEvent::BackgroundSnapshot { tasks },
            }) => tasks
                .iter()
                .map(|task| {
                    assert_eq!(task.status, TaskStatus::Working);
                    assert_eq!(task.detail, "shell");
                    (task.id.clone(), task.label.clone())
                })
                .collect(),
            Some(other) => panic!("not a background snapshot: {other:?}"),
            None => panic!("no snapshot"),
        }
    }

    #[test]
    fn a_command_still_open_when_the_next_item_starts_was_backgrounded() {
        let mut terminals = BackgroundTerminals::default();
        assert!(terminals
            .observe(
                &started("root", exec("exec-1", "63014", "inProgress")),
                Some("root")
            )
            .is_none());
        let event = terminals.observe(
            &started(
                "root",
                json!({"type": "agentMessage", "id": "msg-1", "text": ""}),
            ),
            Some("root"),
        );
        assert_eq!(
            running(&event),
            vec![("63014".to_string(), "sleep 600 #exec-1".to_string())]
        );
        // Its own completion — the process ended — clears it.
        let event = terminals.observe(
            &completed("root", exec("exec-1", "63014", "failed")),
            Some("root"),
        );
        assert_eq!(running(&event), vec![]);
        assert!(terminals
            .observe(
                &completed("root", exec("exec-1", "63014", "failed")),
                Some("root")
            )
            .is_none());
    }

    #[test]
    fn a_command_that_finishes_before_anything_else_starts_never_shows() {
        let mut terminals = BackgroundTerminals::default();
        assert!(terminals
            .observe(
                &started("root", exec("exec-1", "1", "inProgress")),
                Some("root")
            )
            .is_none());
        assert!(terminals
            .observe(
                &completed("root", exec("exec-1", "1", "completed")),
                Some("root")
            )
            .is_none());
        assert!(terminals
            .observe(
                &started("root", exec("exec-2", "2", "inProgress")),
                Some("root")
            )
            .is_none());
    }

    #[test]
    fn the_turn_ending_backgrounds_whatever_is_still_open() {
        let mut terminals = BackgroundTerminals::default();
        terminals.observe(
            &started("root", exec("exec-1", "1", "inProgress")),
            Some("root"),
        );
        terminals.observe(
            &started("root", exec("exec-2", "2", "inProgress")),
            Some("root"),
        );
        let event = terminals.observe(
            &json!({"method": "turn/completed", "params": {"threadId": "root", "turn": {"id": "t1", "status": "completed"}}}),
            Some("root"),
        );
        let ids: Vec<String> = running(&event).into_iter().map(|(id, _)| id).collect();
        assert_eq!(ids, vec!["1".to_string(), "2".to_string()]);
    }

    #[test]
    fn a_childs_terminal_is_not_mains() {
        let mut terminals = BackgroundTerminals::default();
        terminals.observe(
            &started("child", exec("exec-1", "9", "inProgress")),
            Some("root"),
        );
        assert!(terminals
            .observe(
                &started(
                    "child",
                    json!({"type": "agentMessage", "id": "m", "text": ""})
                ),
                Some("root")
            )
            .is_none());
    }

    #[test]
    fn a_command_without_a_process_is_not_a_terminal() {
        let mut terminals = BackgroundTerminals::default();
        let mut item = exec("exec-1", "", "inProgress");
        item["processId"] = Value::Null;
        terminals.observe(&started("root", item), Some("root"));
        assert!(terminals
            .observe(
                &started(
                    "root",
                    json!({"type": "agentMessage", "id": "m", "text": ""})
                ),
                Some("root")
            )
            .is_none());
    }

    #[test]
    fn the_label_is_the_parsed_command_or_the_unwrapped_line() {
        let mut item = exec("exec-1", "1", "inProgress");
        assert_eq!(command_of(&item), "sleep 600 #exec-1");
        item["commandActions"] = json!([]);
        assert_eq!(command_of(&item), "sleep 600 #exec-1");
        item["command"] = json!("npm run dev");
        assert_eq!(command_of(&item), "npm run dev");
        assert_eq!(unwrap_shell("/usr/bin/bash -c 'make -j'"), "make -j");
        assert_eq!(unwrap_shell("python -c 'print(1)'"), "python -c 'print(1)'");
    }
}
