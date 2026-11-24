//! A provider's subscription windows as a window warns of them (ADR
//! 0022): which of the two, and its use rising past one of the
//! operator's thresholds. Reading and remembering them are the Server's
//! (`providers::limits`).

use crate::provider::Provider;

/// One of a provider's two subscription windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitWindow {
    FiveHour,
    Weekly,
}

/// A window's use rising across one of the operator's thresholds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AllowanceWarning {
    pub provider: Provider,
    pub window: LimitWindow,
    /// The highest threshold crossed, in percent.
    pub threshold: u8,
    pub used_fraction: f32,
}
