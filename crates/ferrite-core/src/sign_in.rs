//! Whether a turn's failure says its Provider's CLI has no sign-in to use,
//! so a window offers to sign that CLI in on its Machine (ADR 0051).

use crate::provider::Provider;

/// What each CLI says, lowercased, when it has no sign-in or one gone bad.
fn phrases(provider: Provider) -> &'static [&'static str] {
    match provider {
        Provider::Claude => &[
            "not logged in",
            "please run /login",
            "authentication_failed",
            "oauth token has expired",
            "oauth token revoked",
        ],
        Provider::Codex => &[
            "401 unauthorized",
            "not logged in",
            "sign in again",
            "log in again",
        ],
    }
}

/// Whether `failure`, a turn's failure as `provider`'s CLI said it, means
/// that CLI is signed out.
pub fn signed_out(provider: Provider, failure: &str) -> bool {
    let failure = failure.to_lowercase();
    phrases(provider)
        .iter()
        .any(|phrase| failure.contains(phrase))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_clis_own_signed_out_failure_reads_as_signed_out() {
        assert!(signed_out(
            Provider::Claude,
            "api_error: Not logged in \u{b7} Please run /login"
        ));
        assert!(signed_out(
            Provider::Claude,
            "API Error: 401 {\"type\":\"error\",\"error\":{\"type\":\"authentication_error\",\"message\":\"OAuth token has expired.\"}}"
        ));
        assert!(signed_out(Provider::Claude, "authentication_failed"));
        assert!(signed_out(
            Provider::Codex,
            "unexpected status 401 Unauthorized: Missing bearer or basic authentication in header"
        ));
        assert!(signed_out(
            Provider::Codex,
            "Your access token could not be refreshed. Please log out and sign in again."
        ));
    }

    #[test]
    fn any_other_failure_does_not() {
        assert!(!signed_out(Provider::Claude, "API Error: 529 overloaded"));
        assert!(!signed_out(Provider::Codex, "stream disconnected before completion"));
        assert!(
            !signed_out(Provider::Claude, "unexpected status 401 Unauthorized"),
            "a Codex phrase is not Claude's"
        );
    }
}
