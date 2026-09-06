//! What the Composer's idle line should offer.
//!
//! The suggestion itself is predicted by [`crate::suggest`] and arrives
//! asynchronously; this decides *whether* it is the right thing to show. A
//! Decision outranks it — that is what the Thread is actually waiting on — and
//! so does a closed Session, where the only follow-up there is is reviving.
//! Until a prediction lands, and whenever one is refused, the line is the
//! generic one it has always been.
//!
//! This module is wording-free on purpose. It names what the Thread is
//! waiting on; the renderer owns the sentence, as it does for every other
//! placeholder.

use crate::transcript::{Status, Transcript};

/// What the Composer's idle line should offer, most specific first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Followup {
    /// A Decision is pending; nothing else matters until it is answered.
    Decision,
    /// The Session is gone. Reviving is the only follow-up there is.
    Revive,
    /// A predicted next prompt, already in the operator's voice and already
    /// filtered. Shown verbatim, and the only variant Tab can accept —
    /// the others are descriptions of a state, not text anyone would send.
    Suggested(String),
    /// Nothing to offer: no prediction yet, or the one that came back was
    /// refused.
    Steer,
}

impl Followup {
    /// The text Tab would put in the box, if any.
    pub fn acceptable(&self) -> Option<&str> {
        match self {
            Followup::Suggested(text) => Some(text),
            _ => None,
        }
    }
}

/// The follow-up this Thread invites, given whether a Decision pends and
/// whatever prediction has landed for it.
///
/// A streaming Thread offers nothing: the response a prediction was made from
/// has been superseded by the one now being written, and ghost text that
/// answers a stale turn is worse than none.
pub fn suggest(
    pending: bool,
    transcript: Option<&Transcript>,
    suggestion: Option<&str>,
) -> Followup {
    if pending {
        return Followup::Decision;
    }
    let Some(transcript) = transcript else {
        return Followup::Steer;
    };
    match transcript.status() {
        Status::Closed => return Followup::Revive,
        Status::Streaming => return Followup::Steer,
        Status::Idle | Status::Blocked => {}
    }
    match suggestion.map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => Followup::Suggested(text.to_string()),
        None => Followup::Steer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::Input;
    use crate::{SessionEvent, TurnOutcome};

    fn answered(text: &str) -> Transcript {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("do the thing".into()));
        transcript.apply(Input::Event(SessionEvent::TextDelta { text: text.into() }));
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        }));
        transcript
    }

    #[test]
    fn a_prediction_becomes_the_line() {
        let transcript = answered("Fixed it.");
        assert_eq!(
            suggest(false, Some(&transcript), Some("Run the tests")),
            Followup::Suggested("Run the tests".into())
        );
    }

    #[test]
    fn a_decision_outranks_the_prediction() {
        let transcript = answered("Fixed it.");
        assert_eq!(
            suggest(true, Some(&transcript), Some("Run the tests")),
            Followup::Decision
        );
    }

    #[test]
    fn a_closed_session_outranks_the_prediction() {
        let mut transcript = answered("Fixed it.");
        transcript.apply(Input::Event(SessionEvent::Closed {
            reason: "the CLI exited".into(),
        }));
        assert_eq!(
            suggest(false, Some(&transcript), Some("Run the tests")),
            Followup::Revive
        );
    }

    /// The prediction was made from the previous response; the one being
    /// written now has not been read by anything.
    #[test]
    fn a_streaming_thread_shows_no_prediction() {
        let mut transcript = answered("Fixed it.");
        transcript.apply(Input::Event(SessionEvent::TextDelta {
            text: "Working on".into(),
        }));
        assert_eq!(transcript.status(), Status::Streaming);
        assert_eq!(
            suggest(false, Some(&transcript), Some("Run the tests")),
            Followup::Steer
        );
    }

    #[test]
    fn nothing_predicted_leaves_the_generic_line() {
        let transcript = answered("Fixed it.");
        assert_eq!(suggest(false, Some(&transcript), None), Followup::Steer);
        assert_eq!(
            suggest(false, Some(&transcript), Some("  ")),
            Followup::Steer
        );
        assert_eq!(suggest(false, None, Some("Run the tests")), Followup::Steer);
    }

    /// Only a prediction is text anyone would send; the rest describe a state.
    #[test]
    fn only_a_prediction_is_acceptable() {
        assert_eq!(
            Followup::Suggested("Run the tests".into()).acceptable(),
            Some("Run the tests")
        );
        for other in [Followup::Decision, Followup::Revive, Followup::Steer] {
            assert_eq!(other.acceptable(), None, "{other:?}");
        }
    }
}
