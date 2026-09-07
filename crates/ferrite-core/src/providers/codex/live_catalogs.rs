//! Live catalog invalidation and pagination. Only this reader owns follow-up
//! IDs; ordinary Session requests and child history remain independently routed.
use std::{collections::HashSet, path::Path};

use serde_json::{json, Value};

use crate::SessionEvent;

use super::{requests::notice, wire, MODELS_REQUEST_ID, SKILLS_REQUEST_ID};

const MAX_MODEL_PAGES: usize = 256;

#[derive(Default)]
pub(super) struct Update {
    pub events: Vec<SessionEvent>,
    pub request: Option<Value>,
}

pub(super) struct Catalogs {
    skills_params: Value,
    skills_pending: Option<Value>,
    skills_dirty: bool,
    models_pending: Option<Value>,
    model_rows: Vec<Value>,
    model_cursors: HashSet<String>,
    model_pages: usize,
    serial: u64,
}

impl Catalogs {
    pub fn new(cwd: Option<&Path>) -> Self {
        let mut skills_params = json!({});
        if let Some(cwd) = cwd {
            skills_params["cwds"] = json!([cwd.display().to_string()]);
        }
        Self {
            skills_params,
            skills_pending: Some(json!(SKILLS_REQUEST_ID)),
            skills_dirty: false,
            models_pending: Some(json!(MODELS_REQUEST_ID)),
            model_rows: Vec::new(),
            model_cursors: HashSet::new(),
            model_pages: 0,
            serial: 0,
        }
    }

    /// Startup owns the first skills/list exchange because Session creation
    /// waits for it. Later skill invalidations and all model pages remain in
    /// this reader-owned catalog lifecycle.
    pub fn after_startup(cwd: Option<&Path>) -> Self {
        let mut catalogs = Self::new(cwd);
        catalogs.skills_pending = None;
        catalogs
    }

    fn next_id(&mut self) -> Result<Value, String> {
        self.serial = self
            .serial
            .checked_add(1)
            .ok_or_else(|| "Codex catalog request IDs exhausted".to_owned())?;
        Ok(json!(format!("ferrite:catalog:{}", self.serial)))
    }

    fn refresh_skills(&mut self, update: &mut Update) {
        self.skills_dirty = false;
        match self.next_id() {
            Ok(id) => {
                let mut params = self.skills_params.clone();
                params["forceReload"] = true.into();
                update.request = Some(json!({"jsonrpc":"2.0", "id":id,
                    "method":"skills/list", "params":params}));
                self.skills_pending = Some(id);
            }
            Err(error) => update.events.push(notice(error)),
        }
    }

    /// None leaves unrelated notifications, server requests and responses to
    /// their owners. At most one read of each catalog can be outstanding.
    pub fn observe(&mut self, frame: &Value) -> Option<Update> {
        let mut update = Update::default();
        if frame["method"] == "skills/changed" && frame.get("id").is_none() {
            if self.skills_pending.is_some() {
                self.skills_dirty = true;
            } else {
                self.refresh_skills(&mut update);
            }
            return Some(update);
        }
        if frame.get("method").is_some()
            || (frame.get("result").is_none() && frame.get("error").is_none())
        {
            return None;
        }
        let id = frame.get("id")?;
        if self.skills_pending.as_ref() == Some(id) {
            self.skills_pending = None;
            if let Some(error) = native_error(frame) {
                update.events.push(notice(error));
            } else if let Some(entries) = frame["result"]["data"].as_array() {
                for entry in entries {
                    if let Some(errors) = entry["errors"].as_array() {
                        for error in errors {
                            if let Some(message) = error["message"].as_str() {
                                let text = match error["path"].as_str() {
                                    Some(path) => format!("{path}: {message}"),
                                    None => message.to_owned(),
                                };
                                update.events.push(notice(text));
                            }
                        }
                    }
                }
                update.events.push(SessionEvent::Commands {
                    commands: wire::parse_skills(&frame["result"]),
                });
            } else {
                update
                    .events
                    .push(notice("Codex skills/list carried no data".into()));
            }
            if self.skills_dirty {
                self.refresh_skills(&mut update);
            }
            Some(update)
        } else if self.models_pending.as_ref() == Some(id) {
            self.models_pending = None;
            if let Err(error) = self.model_page(frame, &mut update) {
                update.events.push(notice(error));
            }
            if self.models_pending.is_none() {
                self.model_rows.clear();
                self.model_cursors.clear();
                self.model_pages = 0;
            }
            Some(update)
        } else {
            None
        }
    }

    fn model_page(&mut self, frame: &Value, update: &mut Update) -> Result<(), String> {
        if let Some(error) = native_error(frame) {
            return Err(error);
        }
        let page = &frame["result"];
        let rows = page["data"]
            .as_array()
            .ok_or_else(|| "Codex model/list carried no data".to_owned())?;
        self.model_pages += 1;
        self.model_rows.extend(rows.iter().cloned());
        if let Some(cursor) = page["nextCursor"]
            .as_str()
            .filter(|cursor| !cursor.is_empty())
        {
            if !self.model_cursors.insert(cursor.to_owned()) {
                return Err("Codex model/list repeated its cursor".into());
            }
            if self.model_pages >= MAX_MODEL_PAGES {
                return Err("Codex model/list exceeded 256 pages".into());
            }
            let id = self.next_id()?;
            update.request = Some(json!({"jsonrpc":"2.0", "id":id,
                "method":"model/list", "params":{"cursor":cursor}}));
            self.models_pending = Some(id);
        } else {
            update.events.push(SessionEvent::Models {
                models: wire::parse_models(&json!({"data":self.model_rows})),
            });
        }
        Ok(())
    }

    pub fn write_failed(&mut self, request: &Value, error: &std::io::Error) -> SessionEvent {
        let id = &request["id"];
        if self.skills_pending.as_ref() == Some(id) {
            self.skills_pending = None;
            self.skills_dirty = false;
        }
        if self.models_pending.as_ref() == Some(id) {
            self.models_pending = None;
            self.model_rows.clear();
            self.model_cursors.clear();
            self.model_pages = 0;
        }
        notice(format!("Could not read Codex catalog: {error}"))
    }
}

fn native_error(frame: &Value) -> Option<String> {
    frame.get("error").map(|error| {
        error["message"]
            .as_str()
            .unwrap_or("Codex rejected the catalog request")
            .to_owned()
    })
}
