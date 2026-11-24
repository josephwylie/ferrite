//! A CLI session file a window offers to import (ADR 0022): which
//! vendor's root it was found under, and what it says of itself.
//! Finding and adopting one are `import`'s.

use std::path::PathBuf;
use std::time::SystemTime;

use crate::provider::Provider;

/// One session file discovery found: which vendor's root it was under, and
/// when the vendor last wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub provider: Provider,
    pub path: PathBuf,
    pub modified: Option<SystemTime>,
    pub title: Option<String>,
    pub cwd: Option<PathBuf>,
    pub session_id: Option<String>,
}
