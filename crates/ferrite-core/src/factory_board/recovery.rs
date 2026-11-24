//! What a Task remembers of interruptions and time (#226). Reviving its
//! Worker is the Factory's (`factory::recovery`).

use serde::{Deserialize, Serialize};

/// What a Task remembers of interruptions and time.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keeping {
    /// When the current run's Worker was revived; a second loss in the
    /// same run stops the Task. A new run (another Step, a return, the
    /// operator's Resume) starts without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revived_ms: Option<i64>,
    /// When it completed, as the step first saw it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_ms: Option<i64>,
    /// When it left the board for the Archive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_ms: Option<i64>,
    /// Its Workers were parked once it completed and they were at rest.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub parked: bool,
}
