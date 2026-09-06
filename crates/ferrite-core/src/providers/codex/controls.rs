//! Native controls, their replies, and the Session's MCP connection inventory.
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;

use serde_json::{json, Value};

use crate::{McpServer, McpStatus, SessionControl, SessionEvent};

use super::requests::notice;

const MAX_PENDING: usize = 128;
const MAX_PAGES: usize = 256;

enum Purpose {
    Status,
    Login { server: String, completed: bool },
    Reload,
    Permission,
}

#[derive(Default)]
struct Listing {
    rows: BTreeMap<String, McpServer>,
    /// Notifications observed during a read are newer than its snapshot.
    updates: BTreeMap<String, McpServer>,
    cursors: HashSet<String>,
    pages: usize,
}

#[derive(Default)]
pub(super) struct Controls {
    pending: HashMap<String, Purpose>,
    listing: Option<Listing>,
    refresh_dirty: bool,
    servers: BTreeMap<String, McpServer>,
    serial: u64,
}

#[derive(Default)]
pub(super) struct Update {
    pub events: Vec<SessionEvent>,
    pub request: Option<Value>,
}

impl Controls {
    fn register(&mut self, id: &Value, purpose: Purpose) -> io::Result<()> {
        if self.pending.len() >= MAX_PENDING {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "too many Codex controls are pending",
            ));
        }
        self.pending.insert(id.to_string(), purpose);
        Ok(())
    }

    pub fn begin(
        &mut self,
        id: Value,
        action: SessionControl,
        thread: &str,
    ) -> io::Result<Option<Value>> {
        let (method, params, purpose) = match action {
            SessionControl::RefreshMcp => {
                if self.listing.is_some() {
                    self.refresh_dirty = true;
                    return Ok(None);
                }
                (
                    "mcpServerStatus/list",
                    Some(status_params(thread)),
                    Purpose::Status,
                )
            }
            SessionControl::LoginMcp { server } => (
                "mcpServer/oauth/login",
                Some(json!({"name":server,"threadId":thread})),
                Purpose::Login {
                    server,
                    completed: false,
                },
            ),
            SessionControl::ReloadMcp => ("config/mcpServer/reload", None, Purpose::Reload),
            SessionControl::SetPermissionMode { mode } => {
                if !matches!(mode.as_str(), "untrusted" | "on-request" | "never") {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "unsupported Codex approval policy",
                    ));
                }
                (
                    "thread/settings/update",
                    Some(json!({"threadId":thread,"approvalPolicy":mode})),
                    Purpose::Permission,
                )
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Codex does not support this control",
                ))
            }
        };
        let listing = matches!(purpose, Purpose::Status);
        self.register(&id, purpose)?;
        if listing {
            self.listing = Some(Listing::default());
        }
        let mut request = json!({"jsonrpc":"2.0","id":id,"method":method});
        if let Some(params) = params {
            request["params"] = params;
        }
        Ok(Some(request))
    }

    fn next_id(&mut self) -> io::Result<Value> {
        self.serial = self
            .serial
            .checked_add(1)
            .ok_or_else(|| io::Error::other("Codex control request IDs exhausted"))?;
        Ok(json!(format!("ferrite:control:{}", self.serial)))
    }

    fn refresh(&mut self, thread: &str, update: &mut Update) {
        if self.listing.is_some() {
            self.refresh_dirty = true;
            return;
        }
        self.refresh_dirty = false;
        match self
            .next_id()
            .and_then(|id| self.begin(id, SessionControl::RefreshMcp, thread))
        {
            Ok(request) => update.request = request,
            Err(error) => update.events.push(notice(error.to_string())),
        }
    }

    fn snapshot(&self) -> SessionEvent {
        SessionEvent::McpServers {
            servers: self.servers.values().cloned().collect(),
        }
    }

    pub fn observe(&mut self, frame: &Value, thread: &str) -> Option<Update> {
        let mut update = Update::default();
        if let Some(method) = frame["method"].as_str() {
            if !matches!(
                method,
                "mcpServer/startupStatus/updated" | "mcpServer/oauthLogin/completed"
            ) || frame.get("id").is_some()
            {
                return None;
            }
            let params = &frame["params"];
            // Null is an unscoped server observation; a child's connection is
            // never the main Thread's inventory or authorization lifecycle.
            if !params["threadId"].is_null() && params["threadId"].as_str() != Some(thread) {
                return Some(update);
            }
            let Some(name) = params["name"].as_str() else {
                return Some(update);
            };
            if method == "mcpServer/startupStatus/updated" {
                let status = match params["status"].as_str() {
                    Some("starting") => McpStatus::Connecting,
                    Some("ready") => McpStatus::Connected,
                    Some("failed") if params["failureReason"] == "reauthenticationRequired" => {
                        McpStatus::NeedsAuth
                    }
                    Some("failed") => McpStatus::Failed,
                    _ => McpStatus::Unknown,
                };
                let server = McpServer {
                    name: name.into(),
                    status,
                    error: params["error"].as_str().map(str::to_owned),
                };
                self.servers.insert(name.into(), server.clone());
                if let Some(listing) = &mut self.listing {
                    listing.updates.insert(name.into(), server);
                }
                update.events.push(self.snapshot());
            } else {
                for purpose in self.pending.values_mut() {
                    if let Purpose::Login { server, completed } = purpose {
                        if server == name {
                            *completed = true;
                        }
                    }
                }
                update.events.push(SessionEvent::McpAuthorization {
                    server: name.into(),
                    url: None,
                });
                if params["success"] == false {
                    update.events.push(notice(
                        params["error"]
                            .as_str()
                            .unwrap_or("Codex MCP authorization failed")
                            .into(),
                    ));
                }
                self.refresh(thread, &mut update);
            }
            return Some(update);
        }
        if frame.get("method").is_some()
            || (frame.get("result").is_none() && frame.get("error").is_none())
        {
            return None;
        }
        let purpose = self.pending.remove(&frame.get("id")?.to_string())?;
        let error = frame.get("error").map(|error| {
            error["message"]
                .as_str()
                .unwrap_or("Codex rejected the control request")
                .to_owned()
        });
        match purpose {
            Purpose::Status => {
                let result = match error {
                    Some(error) => Err(io::Error::other(error)),
                    None => self.status_page(&frame["result"], thread, &mut update),
                };
                if let Err(error) = result {
                    self.listing = None;
                    update.events.push(notice(error.to_string()));
                }
                if self.listing.is_none() && self.refresh_dirty {
                    self.refresh(thread, &mut update);
                }
            }
            Purpose::Login { server, completed } => {
                if let Some(error) = error {
                    update
                        .events
                        .push(SessionEvent::McpAuthorization { server, url: None });
                    update.events.push(notice(error));
                } else if !completed {
                    if let Some(url) = frame["result"]["authorizationUrl"].as_str() {
                        update.events.push(SessionEvent::McpAuthorization {
                            server,
                            url: Some(url.into()),
                        });
                    } else {
                        update
                            .events
                            .push(notice("Codex MCP authorization returned no URL".into()));
                    }
                }
            }
            Purpose::Reload | Purpose::Permission => {
                if let Some(error) = error {
                    update.events.push(notice(error));
                } else if matches!(purpose, Purpose::Reload) {
                    update.events.push(notice(
                        "Codex MCP configuration reloaded; changes apply on the next turn.".into(),
                    ));
                }
                // Only thread/settings/updated establishes effective policy.
            }
        }
        Some(update)
    }

    fn status_page(&mut self, page: &Value, thread: &str, update: &mut Update) -> io::Result<()> {
        let rows = page["data"]
            .as_array()
            .ok_or_else(|| io::Error::other("Codex MCP status carried no data"))?;
        let listing = self
            .listing
            .as_mut()
            .expect("status request owns a listing");
        listing.pages += 1;
        for row in rows {
            if let Some(name) = row["name"].as_str() {
                listing.rows.insert(
                    name.into(),
                    McpServer {
                        name: name.into(),
                        status: runtime_status(row),
                        error: None,
                    },
                );
            }
        }
        if let Some(cursor) = page["nextCursor"]
            .as_str()
            .filter(|cursor| !cursor.is_empty())
        {
            if !listing.cursors.insert(cursor.into()) {
                return Err(io::Error::other("Codex MCP status repeated its cursor"));
            }
            if listing.pages >= MAX_PAGES {
                return Err(io::Error::other("Codex MCP status exceeded 256 pages"));
            }
            let id = self.next_id()?;
            self.register(&id, Purpose::Status)?;
            let mut params = status_params(thread);
            params["cursor"] = cursor.into();
            update.request = Some(
                json!({"jsonrpc":"2.0","id":id,"method":"mcpServerStatus/list","params":params}),
            );
        } else {
            let mut listing = self.listing.take().expect("listing exists");
            listing.rows.extend(listing.updates);
            self.servers = listing.rows;
            update.events.push(self.snapshot());
        }
        Ok(())
    }

    pub fn discard(&mut self, request: &Value) {
        if matches!(
            self.pending.remove(&request["id"].to_string()),
            Some(Purpose::Status)
        ) {
            self.listing = None;
            self.refresh_dirty = false;
        }
    }
}

fn status_params(thread: &str) -> Value {
    json!({"threadId":thread,"detail":"toolsAndAuthOnly"})
}

fn runtime_status(row: &Value) -> McpStatus {
    match row["runtimeStatus"].as_str() {
        Some("connected") => McpStatus::Connected,
        Some("starting") => McpStatus::Connecting,
        Some("authenticationRequired") => McpStatus::NeedsAuth,
        Some("failed") => McpStatus::Failed,
        Some("disabled") => McpStatus::Disabled,
        _ if row["authStatus"] == "notLoggedIn" => McpStatus::NeedsAuth,
        _ => McpStatus::Unknown,
    }
}
