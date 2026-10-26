//! What a Thread's agent last said of itself (ADR 0032): its command menu,
//! models, permission modes, controls and MCP servers. A Thread with no
//! Session offers these as its own, and keeps them beside its log, so a
//! Thread reopened by a later Server offers them too without starting an
//! agent to ask.

use std::io;
use std::path::PathBuf;

use crate::providers::Session;
use crate::{
    ControlKind, McpServer, ModelInfo, PermissionModeChoice, SessionCommand, SessionEvent,
};

/// Every control a Session may offer, in the order the windows list them.
const CONTROLS: [ControlKind; 8] = [
    ControlKind::RefreshContext,
    ControlKind::RefreshMcp,
    ControlKind::ReconnectMcp,
    ControlKind::LoginMcp,
    ControlKind::ReloadMcp,
    ControlKind::StopTask,
    ControlKind::BackgroundTasks,
    ControlKind::SetPermissionMode,
];

#[derive(Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Said {
    commands: Vec<SessionCommand>,
    models: Vec<ModelInfo>,
    permission_mode: Option<String>,
    permission_modes: Vec<PermissionModeChoice>,
    controls: Vec<ControlKind>,
    mcp_servers: Vec<McpServer>,
}

pub(super) struct Announced {
    said: Said,
    path: PathBuf,
}

impl Announced {
    /// What was saved at `path`. A missing or unreadable file is nothing
    /// said yet: the next Session says it again.
    pub fn open(path: PathBuf) -> Self {
        let said = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { said, path }
    }

    pub fn commands(&self) -> &[SessionCommand] {
        &self.said.commands
    }

    pub fn models(&self) -> &[ModelInfo] {
        &self.said.models
    }

    pub fn permission_mode(&self) -> Option<&str> {
        self.said.permission_mode.as_deref()
    }

    pub fn permission_modes(&self) -> &[PermissionModeChoice] {
        &self.said.permission_modes
    }

    pub fn controls(&self) -> &[ControlKind] {
        &self.said.controls
    }

    pub fn supports(&self, kind: ControlKind) -> bool {
        self.said.controls.contains(&kind)
    }

    pub fn mcp_servers(&self) -> &[McpServer] {
        &self.said.mcp_servers
    }

    /// Take what `event` says of the Session, if it says any of it.
    pub fn hear(&mut self, event: &SessionEvent) {
        let said = &mut self.said;
        let changed = match event {
            SessionEvent::Commands { commands } => replace(&mut said.commands, commands),
            SessionEvent::Models { models } => replace(&mut said.models, models),
            SessionEvent::PermissionMode { mode } => {
                replace(&mut said.permission_mode, &Some(mode.clone()))
            }
            SessionEvent::McpServers { servers } => replace(&mut said.mcp_servers, servers),
            _ => false,
        };
        if changed {
            self.save();
        }
    }

    /// Take what a ready Session offers without saying it: its permission
    /// modes and controls. Answers whether that changed anything.
    pub fn ask(&mut self, session: &dyn Session) -> bool {
        let said = &mut self.said;
        let controls: Vec<ControlKind> = CONTROLS
            .into_iter()
            .filter(|kind| session.supports_control(*kind))
            .collect();
        let modes = replace(&mut said.permission_modes, &session.permission_modes());
        let changed = replace(&mut said.controls, &controls) | modes;
        if changed {
            self.save();
        }
        changed
    }

    /// Another Provider's agent said all this: none of it holds for the
    /// next one.
    pub fn forget(&mut self) {
        if self.said != Said::default() {
            self.said = Said::default();
            self.save();
        }
    }

    /// Best effort: what is lost is asked again of the next Session.
    fn save(&self) {
        let written = serde_json::to_vec(&self.said)
            .map_err(io::Error::from)
            .and_then(|bytes| crate::store::replace_file(&self.path, &bytes));
        if let Err(error) = written {
            crate::log!(
                "{}: what the agent said of itself could not be kept: {error}",
                self.path.display()
            );
        }
    }
}

/// `to`, unless `field` already holds it: whether it changed.
fn replace<T: Clone + PartialEq>(field: &mut T, to: &T) -> bool {
    if field == to {
        return false;
    }
    field.clone_from(to);
    true
}
