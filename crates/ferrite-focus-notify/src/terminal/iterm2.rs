//! iTerm2: a Herdr client's container is the session in `ITERM_SESSION_ID`.
//!
//! iTerm2's reveal URL selects that session together with its tab and OS
//! window. It is handled by iTerm2 itself and needs no Python runtime or
//! scripting setup.

use super::{FocusCommand, HerdrClient, TerminalAdapter};

pub(super) struct Iterm2;

impl TerminalAdapter for Iterm2 {
    fn bundle_id(&self) -> &'static str {
        "com.googlecode.iterm2"
    }

    fn focus_command(&self, client: &HerdrClient) -> Option<FocusCommand> {
        // `open` succeeds once iTerm2 receives the URL, whether or not the
        // session still exists, so a stale session id would stop the core from
        // trying other clients. Skip clients that belong to another terminal
        // but inherited `ITERM_SESSION_ID` from a shell they were started in.
        if client.env("KITTY_WINDOW_ID").is_some()
            || client
                .env("TERM_PROGRAM")
                .is_some_and(|program| program != "iTerm.app")
        {
            return None;
        }
        let session_id = client.env("ITERM_SESSION_ID").filter(|id| {
            id.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-'))
        })?;
        Some(FocusCommand::new(
            "/usr/bin/open",
            [format!("iterm2:reveal?sessionid={session_id}")],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(session_id: Option<&str>) -> HerdrClient {
        client_with(session_id, &[])
    }

    fn client_with(session_id: Option<&str>, pairs: &[(&str, &str)]) -> HerdrClient {
        let mut pairs = pairs.to_vec();
        pairs.extend(session_id.map(|id| ("ITERM_SESSION_ID", id)));
        HerdrClient::from_pairs(&pairs)
    }

    #[test]
    fn reveals_the_client_iterm2_session() {
        assert_eq!(
            Iterm2.focus_command(&client(Some("w0t2p1:1234-ABCD"))),
            Some(FocusCommand::new(
                "/usr/bin/open",
                ["iterm2:reveal?sessionid=w0t2p1:1234-ABCD"]
            ))
        );
    }

    #[test]
    fn requires_a_safe_session_id() {
        assert_eq!(Iterm2.focus_command(&client(None)), None);
        assert_eq!(Iterm2.focus_command(&client(Some(""))), None);
        assert_eq!(
            Iterm2.focus_command(&client(Some("w0t0p0:id&unexpected=value"))),
            None
        );
    }

    #[test]
    fn skips_clients_of_other_terminals_with_an_inherited_session_id() {
        let session = Some("w0t0p0:1234-ABCD");
        assert!(Iterm2
            .focus_command(&client_with(session, &[("TERM_PROGRAM", "iTerm.app")]))
            .is_some());
        assert_eq!(
            Iterm2.focus_command(&client_with(session, &[("KITTY_WINDOW_ID", "3")])),
            None
        );
        assert_eq!(
            Iterm2.focus_command(&client_with(session, &[("TERM_PROGRAM", "Apple_Terminal")])),
            None
        );
    }
}
