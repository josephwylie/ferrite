//! The sample pages an agent might draw: what the demo shows and the
//! visual captures draw.

/// A dashboard: token usage over 30 days.
pub const STATS: &str = include_str!("../../assets/visuals/stats.html");
/// A mockup with options to choose between.
#[cfg(feature = "visual-reference")]
pub const MOCKUPS: &str = include_str!("../../assets/visuals/mockups.html");
/// A treemap.
#[cfg(feature = "visual-reference")]
pub const TREEMAP: &str = include_str!("../../assets/visuals/treemap.html");
