//! The `/` menu after the handshake: the MCP prompts.
//!
//! Spawn's initialize answer carries the CLI's effective slash menu, and
//! it comes within a second of launch — before the MCP servers have
//! connected (a remote one takes a few seconds), and the prompts those
//! servers publish become commands only once they have. The CLI never
//! re-announces: `commands_changed` fires for skill reloads, not MCP
//! connects, and `mcp_status` names servers, not prompts. Asked again
//! later, initialize does list them (capture, 2.1.275: 102 commands at
//! one second, 106 with the four `reui:… (MCP)` prompts at three, once
//! that server read `connected`). So after the handshake this polls
//! `mcp_status` until no server is still pending, then asks initialize
//! once more and announces the menu if it changed. Every `system:init`
//! line — one per turn — names the typeable commands as well; one naming
//! an `mcp__…` the menu lacks asks initialize again, which covers a server
//! that connected late, was toggled on with `/mcp`, or was reconnected.
//!
//! Request ids here are `ferrite_mcp_…`, apart from the Session's own
//! `req_N` counter, so a stub reading the pipe positionally can tell them
//! apart and the Session's numbering stays what its tests recorded.

use std::collections::HashMap;
use std::process::ChildStdin;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use super::{wire, write_stdin_line};
use crate::{SessionCommand, SessionEvent};

/// How long the servers get: one `mcp_status` a second, this many times.
/// A remote server that has not connected in half a minute has failed
/// and says so — the poll is not what is keeping the Session up.
const SETTLE_ATTEMPTS: u32 = 30;
const SETTLE_INTERVAL: Duration = Duration::from_secs(1);

/// One of this module's own requests, awaiting its answer.
pub(super) enum Step {
    Status { attempt: u32 },
    Initialize,
}

#[derive(Default)]
pub(super) struct McpMenu {
    /// The menu as last announced — what a fresh answer is compared to.
    known: Vec<SessionCommand>,
    pending: HashMap<String, Step>,
    serial: u32,
    /// One initialize in flight at a time: a burst of `system:init` lines
    /// must not become a burst of requests.
    initializing: bool,
}

impl McpMenu {
    /// The handshake answered: remember its menu and start the settle.
    pub fn begin(&mut self, commands: &[SessionCommand], stdin: &Arc<Mutex<ChildStdin>>) {
        self.known = commands.to_vec();
        self.poll(1, stdin, false);
    }

    /// The CLI announced a menu on its own (`commands_changed`).
    pub fn announced(&mut self, commands: &[SessionCommand]) {
        self.known = commands.to_vec();
    }

    /// Whether this response answers one of this module's requests.
    pub fn take(&mut self, request_id: &str) -> Option<Step> {
        self.pending.remove(request_id)
    }

    /// Fold the answer to a taken step in; the menu event it earns, if any.
    pub fn answered(
        &mut self,
        step: Step,
        line: &str,
        value: &Value,
        stdin: &Arc<Mutex<ChildStdin>>,
    ) -> Option<SessionEvent> {
        let response = &value["response"];
        match step {
            Step::Status { attempt } => {
                if response["subtype"] != "success" {
                    return None;
                }
                let servers = response["response"]["mcpServers"].as_array()?;
                // No MCP servers: nothing will ever join the menu.
                if servers.is_empty() {
                    return None;
                }
                if still_connecting(servers) && attempt < SETTLE_ATTEMPTS {
                    self.poll(attempt + 1, stdin, true);
                } else {
                    self.initialize(stdin);
                }
                None
            }
            Step::Initialize => {
                self.initializing = false;
                let id = response["request_id"].as_str()?;
                let capabilities = wire::parse_capabilities(line, id)?;
                if capabilities.commands.is_empty() || capabilities.commands == self.known {
                    return None;
                }
                self.known = capabilities.commands.clone();
                Some(SessionEvent::Commands {
                    commands: capabilities.commands,
                })
            }
        }
    }

    /// A `system:init` line: its `slash_commands` are the typeable names,
    /// MCP prompts as `mcp__server__prompt`. One the menu lacks means the
    /// menu is stale.
    pub fn init_line(&mut self, value: &Value, stdin: &Arc<Mutex<ChildStdin>>) {
        if self.initializing {
            return;
        }
        let Some(names) = value["slash_commands"].as_array() else {
            return;
        };
        if names_an_unknown_prompt(names, &self.known) {
            self.initialize(stdin);
        }
    }

    fn poll(&mut self, attempt: u32, stdin: &Arc<Mutex<ChildStdin>>, delayed: bool) {
        let id = self.next("status");
        self.pending.insert(id.clone(), Step::Status { attempt });
        let request = json!({
            "type": "control_request",
            "request_id": id,
            "request": {"subtype": "mcp_status"},
        });
        if delayed {
            // The reader must not sleep — it is the only thing draining the
            // CLI's stdout — so the wait happens beside it. A write that
            // fails is a Session already gone.
            let stdin = Arc::clone(stdin);
            thread::spawn(move || {
                thread::sleep(SETTLE_INTERVAL);
                let _ = write_stdin_line(&stdin, &request);
            });
        } else {
            let _ = write_stdin_line(stdin, &request);
        }
    }

    fn initialize(&mut self, stdin: &Arc<Mutex<ChildStdin>>) {
        let id = self.next("init");
        self.pending.insert(id.clone(), Step::Initialize);
        self.initializing = true;
        let request = json!({
            "type": "control_request",
            "request_id": id,
            "request": {"subtype": "initialize"},
        });
        if write_stdin_line(stdin, &request).is_err() {
            self.initializing = false;
            self.pending.remove(&id);
        }
    }

    fn next(&mut self, kind: &str) -> String {
        self.serial += 1;
        format!("ferrite_mcp_{kind}_{}", self.serial)
    }
}

/// Whether any server is still on its way: `pending` is what the CLI says
/// before a server answers; `connecting` is kept for the same reading.
fn still_connecting(servers: &[Value]) -> bool {
    servers
        .iter()
        .any(|server| matches!(server["status"].as_str(), Some("pending" | "connecting")))
}

fn names_an_unknown_prompt(names: &[Value], known: &[SessionCommand]) -> bool {
    names
        .iter()
        .filter_map(Value::as_str)
        .filter(|name| name.starts_with("mcp__"))
        .any(|name| !known.iter().any(|command| command.name == name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(name: &str) -> SessionCommand {
        SessionCommand {
            name: name.into(),
            description: String::new(),
            path: None,
        }
    }

    #[test]
    fn a_pending_server_is_still_connecting_and_every_other_state_is_settled() {
        assert!(still_connecting(&[
            json!({"name": "a", "status": "connected"}),
            json!({"name": "b", "status": "pending"}),
        ]));
        assert!(still_connecting(&[
            json!({"name": "b", "status": "connecting"})
        ]));
        for settled in ["connected", "failed", "needs-auth", "disabled"] {
            assert!(!still_connecting(&[
                json!({"name": "a", "status": settled})
            ]));
        }
    }

    #[test]
    fn only_a_missing_mcp_prompt_makes_the_menu_stale() {
        let known = [command("compact"), command("mcp__reui__improve")];
        let names = |list: &[&str]| list.iter().map(|s| json!(s)).collect::<Vec<_>>();
        assert!(!names_an_unknown_prompt(
            &names(&["compact", "mcp__reui__improve"]),
            &known
        ));
        assert!(names_an_unknown_prompt(
            &names(&["compact", "mcp__reui__build"]),
            &known
        ));
        // Built-ins the handshake never lists are not MCP prompts.
        assert!(!names_an_unknown_prompt(
            &names(&["doctor", "color"]),
            &known
        ));
    }
}
