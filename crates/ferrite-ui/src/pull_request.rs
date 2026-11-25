//! How a pull request's reviews read: the PR reader's and the Review
//! slot's comment cards word a verdict alike.

use ferrite_core::protocol::WireReviewState;

use crate::theme::{GREEN, INK_SUBTLE, RED};

/// How a review ended, in words and the ink they take; `None` when the forge
/// gave no verdict.
pub fn review_verdict(state: WireReviewState) -> Option<(&'static str, u32)> {
    match state {
        WireReviewState::Approved => Some(("Approved", GREEN)),
        WireReviewState::ChangesRequested => Some(("Changes requested", RED)),
        WireReviewState::Commented => Some(("Commented", INK_SUBTLE)),
        WireReviewState::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_review_verdict_reads_in_words() {
        let words = |state| review_verdict(state).map(|(word, _)| word);
        assert_eq!(words(WireReviewState::Approved), Some("Approved"));
        assert_eq!(
            words(WireReviewState::ChangesRequested),
            Some("Changes requested")
        );
        assert_eq!(words(WireReviewState::Commented), Some("Commented"));
        assert_eq!(words(WireReviewState::Unknown), None);
    }
}
