//! A Thread's queued prompts as the Composer shows them: each one, and
//! where it stands on its way into a turn. The queue that holds them is
//! the Engine's.

/// Where a prompt the operator queued stands on its way into a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    /// Ferrite holds it: no Session can take it yet.
    Holding,
    /// Handed to the provider, which has not acknowledged it yet.
    Sending,
    /// In the provider's queue, for after the running turn.
    Queued,
    /// Folded into the running turn at its next tool boundary (Codex).
    Steering,
    /// In the provider's queue while a turn runs, joining that turn at its
    /// next tool round and still to be taken back until then (Claude,
    /// `Session::queue_folds`). The strip says `Steering`.
    Folding,
    /// A state this build does not know, or a Server too old to say.
    #[serde(other)]
    Unknown,
}

/// One queued prompt, as the Composer shows it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QueueItem {
    /// Stable from the moment it is queued until a turn picks it up: the
    /// client id it is held and handed to the provider under. What cancel
    /// and edit name it by.
    pub id: String,
    pub text: String,
    pub delivery: Delivery,
}
