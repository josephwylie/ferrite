//! A Thread's prompts on their way into a turn as the Composer shows
//! them: what became of one sent, each one queued, and where it stands.
//! The queue that holds them is the Engine's.

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

/// What became of a prompt the operator sent, as far as the Engine can
/// tell at once. The caller keeps the whole draft until it knows.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "a refused or unconfirmed send leaves the draft with the caller"]
pub enum Admission {
    /// On the wire: the Session has it.
    Delivered,
    /// Waiting for the Session to start. If it never does, the prompt comes
    /// back through `take_retrieved_prompt`.
    Held,
    /// Not sent, and nothing will send it: the words say why.
    Refused(String),
    /// The Server did not say whether it took the prompt (it never answered
    /// or went away mid-act): sending again may duplicate it.
    Unconfirmed(String),
}
