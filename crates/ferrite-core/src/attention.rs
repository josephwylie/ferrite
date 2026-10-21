//! A Thread's Attention (ADR 0015): the one state every list ranks it by,
//! and what the API reports and waits on (ADR 0027).

/// The four states every list sorts, filters and rolls up by, least
/// urgent first (Herdr's model, read from the protocol rather than the
/// screen): **Blocked** waits on the operator, **Done** finished and has
/// not been seen, **Working** runs (Main, or only its background work),
/// **Idle** is everything else. Ferrite's own states (failing, failed,
/// parked, background) are faces within these; failed is a red idle.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Attention {
    Idle,
    Working,
    Done,
    Blocked,
}
