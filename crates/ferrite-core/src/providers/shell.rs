//! Shell wrappers a provider puts around the command it actually runs.
//!
//! Codex reports a command as its login shell saw it — `/bin/zsh -lc 'cargo
//! test'` or `/bin/zsh -lc "gh issue list --state open"` — while the operator
//! asked for, and reads, the inner command. One unwrap for every surface that
//! names a command: the transcript's `Bash(…)` row, its copy, the background
//! chips.

use std::borrow::Cow;

/// The command a `sh`/`bash`/`zsh`/`fish`/`dash` `-c`/`-lc` wrapper runs, or
/// `raw` itself when it is not one. A single-quoted body is taken verbatim;
/// a double-quoted one has its backslash escapes (`\"`, `\\`, `\$`, `` \` ``)
/// undone.
pub fn unwrap_shell(raw: &str) -> Cow<'_, str> {
    let trimmed = raw.trim();
    for flag in [" -lc ", " -c "] {
        let Some(at) = trimmed.find(flag) else {
            continue;
        };
        let shell = &trimmed[..at];
        let is_shell = shell
            .rsplit('/')
            .next()
            .is_some_and(|name| matches!(name, "sh" | "bash" | "zsh" | "fish" | "dash"));
        if !is_shell {
            continue;
        }
        let body = &trimmed[at + flag.len()..];
        if let Some(inner) = body
            .strip_prefix('\'')
            .and_then(|body| body.strip_suffix('\''))
        {
            return Cow::Borrowed(inner);
        }
        if let Some(inner) = body
            .strip_prefix('"')
            .and_then(|body| body.strip_suffix('"'))
        {
            if !inner.contains('\\') {
                return Cow::Borrowed(inner);
            }
            let mut out = String::with_capacity(inner.len());
            let mut chars = inner.chars().peekable();
            while let Some(ch) = chars.next() {
                if ch == '\\' {
                    if let Some(&next) = chars.peek() {
                        if matches!(next, '"' | '\\' | '$' | '`') {
                            out.push(next);
                            chars.next();
                            continue;
                        }
                    }
                }
                out.push(ch);
            }
            return Cow::Owned(out);
        }
    }
    Cow::Borrowed(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_shell_wrappers_unwrap_to_the_command() {
        assert_eq!(
            unwrap_shell("/bin/zsh -lc 'cargo test -p ferrite nav::'"),
            "cargo test -p ferrite nav::"
        );
        assert_eq!(
            unwrap_shell("/bin/zsh -lc \"gh issue list --state open --label stale\""),
            "gh issue list --state open --label stale"
        );
        assert_eq!(
            unwrap_shell(r#"bash -c "echo \"hi\" \$HOME""#),
            r#"echo "hi" $HOME"#
        );
        assert_eq!(unwrap_shell("sh -c 'ls'"), "ls");
        // Not a shell wrapper: left alone.
        assert_eq!(unwrap_shell("cargo test"), "cargo test");
        assert_eq!(unwrap_shell("python -c 'print(1)'"), "python -c 'print(1)'");
        assert_eq!(
            unwrap_shell("/bin/zsh -lc unquoted"),
            "/bin/zsh -lc unquoted"
        );
    }
}
