//! Limits: the longest one run of an agent may take (`limit.time`) and the most tokens a task may use
//! (`limit.tokens`). One area per file. In plain words: docs/limits.md.
pub mod reached;
pub mod run;
pub mod shown;
pub mod spent;
pub mod tick;
