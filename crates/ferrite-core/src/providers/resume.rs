//! Why a start that asked to resume a provider-native conversation failed,
//! told apart at the provider boundary: the two failures have opposite
//! remedies, so the Engine must not read them from words.

use std::fmt;
use std::io;

/// A refused resume the Engine acts on. Anything else a start can fail on
/// (a missing CLI, a handshake timeout) carries no variant: it is a plain
/// failure to try again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResumeFailure {
    /// The provider holds no conversation under the saved id. Retrying
    /// meets the same refusal; only a fresh conversation goes on.
    SessionGone,
    /// Another process holds the conversation. A refusal that clears when
    /// that process lets go: starting a fresh conversation beside it would
    /// fork the Thread silently.
    InUse,
}

impl ResumeFailure {
    /// What a provider said when it refused a resume. `None` for any
    /// other failure.
    pub(crate) fn classify(said: &str) -> Option<Self> {
        let said = said.to_ascii_lowercase();
        if said.contains("already has an active writer") || said.contains("is already in use") {
            Some(Self::InUse)
        } else if said.contains("no rollout found") || said.contains("no conversation found") {
            Some(Self::SessionGone)
        } else {
            None
        }
    }

    /// The failure `error` carries, when a spawn classified it.
    pub fn of(error: &io::Error) -> Option<Self> {
        error
            .get_ref()?
            .downcast_ref::<ResumeRefused>()
            .map(|refused| refused.failure)
    }
}

/// A spawn's failure, with its classification, as the `io::Error` the
/// Spawner speaks. It reads as the provider's own words.
#[derive(Debug)]
pub struct ResumeRefused {
    failure: ResumeFailure,
    said: String,
}

impl ResumeRefused {
    /// `said` as an error, classified when the start asked to `resume`.
    pub fn error(resuming: bool, said: String) -> io::Error {
        match resuming.then(|| ResumeFailure::classify(&said)).flatten() {
            Some(failure) => io::Error::other(Self { failure, said }),
            None => io::Error::other(said),
        }
    }
}

impl fmt::Display for ResumeRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.said)
    }
}

impl std::error::Error for ResumeRefused {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_conversation_is_told_from_one_in_use() {
        let gone = |said: &str| ResumeRefused::error(true, said.into());
        assert_eq!(
            ResumeFailure::of(&gone(
                "codex app-server handshake failed: no rollout found for thread id t1"
            )),
            Some(ResumeFailure::SessionGone)
        );
        assert_eq!(
            ResumeFailure::of(&gone(
                "claude CLI exited: No conversation found with session ID: s1"
            )),
            Some(ResumeFailure::SessionGone)
        );
        assert_eq!(
            ResumeFailure::of(&gone("thread resumed already has an active writer")),
            Some(ResumeFailure::InUse)
        );
        assert_eq!(ResumeFailure::of(&gone("CLI not found")), None);
    }

    #[test]
    fn only_a_start_that_resumes_classifies() {
        let error = ResumeRefused::error(false, "no rollout found for thread id t1".into());
        assert_eq!(ResumeFailure::of(&error), None);
        assert_eq!(error.to_string(), "no rollout found for thread id t1");
    }
}
