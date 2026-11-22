//! How far an agent goes without asking, on one scale over every
//! provider's own words (T3 Code's runtime modes): what a delegated
//! `thread.start` caps its Thread at, so a Thread on another
//! machine never runs broader than the one that started it.

use serde::{Deserialize, Serialize};

use crate::providers::codex_launch;
use crate::spawner::SessionDefaults;
use crate::store::Provider;

/// Narrowest first: a cap is the lesser of two.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    /// Asks before anything beyond reading.
    ApprovalRequired,
    /// Edits its checkout without asking; asks for the rest.
    AutoAcceptEdits,
    /// Goes on without asking, short of bypassing every check.
    Auto,
    /// Bypasses every check.
    FullAccess,
}

impl RuntimeMode {
    pub const ALL: [Self; 4] = [
        Self::ApprovalRequired,
        Self::AutoAcceptEdits,
        Self::Auto,
        Self::FullAccess,
    ];

    /// What a Session on `provider` runs at, started with `args` (as
    /// `thread.start` takes them) on a machine whose defaults are
    /// `defaults`. A word this scale does not know, and a Codex sandbox
    /// left to Codex's own config, count as full access, so a cap still
    /// narrows them.
    pub fn of_launch(provider: Provider, defaults: &SessionDefaults, args: &[String]) -> Self {
        match provider {
            Provider::Claude => {
                let mode =
                    claude_launch_mode(args).or_else(|| defaults.claude_permission_mode.clone());
                match mode.as_deref().unwrap_or("default") {
                    "default" | "plan" | "dontAsk" => Self::ApprovalRequired,
                    "acceptEdits" => Self::AutoAcceptEdits,
                    "auto" => Self::Auto,
                    _ => Self::FullAccess,
                }
            }
            Provider::Codex => {
                let launch = codex_launch::from_tui_args(args);
                let policy = launch
                    .approval_policy
                    .or_else(|| defaults.codex_approval_policy.clone());
                let sandbox = launch.sandbox.or_else(|| defaults.codex_sandbox.clone());
                // `untrusted` asks before anything it does not trust,
                // whatever the sandbox; otherwise the sandbox bounds what
                // runs without asking.
                match (
                    policy.as_deref().unwrap_or("on-request"),
                    sandbox.as_deref(),
                ) {
                    ("untrusted", _) | (_, Some("read-only")) => Self::ApprovalRequired,
                    ("never", Some("workspace-write")) => Self::Auto,
                    ("on-request" | "on-failure", Some("workspace-write")) => Self::AutoAcceptEdits,
                    _ => Self::FullAccess,
                }
            }
        }
    }

    /// The arguments that start a Session on `provider` at this mode,
    /// written as `thread.start --args` takes them; set after the others,
    /// they win over them.
    pub fn args(self, provider: Provider) -> Vec<String> {
        let words: &[&str] = match (provider, self) {
            (Provider::Claude, Self::ApprovalRequired) => &["--permission-mode", "default"],
            (Provider::Claude, Self::AutoAcceptEdits) => &["--permission-mode", "acceptEdits"],
            (Provider::Claude, Self::Auto) => &["--permission-mode", "auto"],
            (Provider::Claude, Self::FullAccess) => &["--permission-mode", "bypassPermissions"],
            (Provider::Codex, Self::ApprovalRequired) => &["-a", "untrusted", "-s", "read-only"],
            (Provider::Codex, Self::AutoAcceptEdits) => {
                &["-a", "on-request", "-s", "workspace-write"]
            }
            (Provider::Codex, Self::Auto) => &["-a", "never", "-s", "workspace-write"],
            (Provider::Codex, Self::FullAccess) => &["-a", "never", "-s", "danger-full-access"],
        };
        words.iter().map(|word| word.to_string()).collect()
    }
}

/// The permission mode Claude's own arguments start it in, the last one
/// winning as on its command line.
fn claude_launch_mode(args: &[String]) -> Option<String> {
    let mut mode = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--permission-mode" => mode = args.next().cloned(),
            "--dangerously-skip-permissions" => mode = Some("bypassPermissions".into()),
            _ => {
                if let Some(word) = arg.strip_prefix("--permission-mode=") {
                    mode = Some(word.into());
                }
            }
        }
    }
    mode
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> SessionDefaults {
        SessionDefaults {
            codex_approval_policy: Some("on-request".into()),
            ..SessionDefaults::default()
        }
    }

    #[test]
    fn each_modes_arguments_start_a_session_at_that_mode() {
        for provider in [Provider::Claude, Provider::Codex] {
            for mode in RuntimeMode::ALL {
                assert_eq!(
                    RuntimeMode::of_launch(provider, &defaults(), &mode.args(provider)),
                    mode,
                    "{provider:?} {mode:?}"
                );
            }
        }
    }

    #[test]
    fn a_machines_defaults_and_a_threads_arguments_say_how_far_it_goes() {
        let words = |words: &[&str]| {
            words
                .iter()
                .map(|word| word.to_string())
                .collect::<Vec<_>>()
        };
        let claude = |defaults: &SessionDefaults, args: &[&str]| {
            RuntimeMode::of_launch(Provider::Claude, defaults, &words(args))
        };
        let codex = |defaults: &SessionDefaults, args: &[&str]| {
            RuntimeMode::of_launch(Provider::Codex, defaults, &words(args))
        };
        assert_eq!(claude(&defaults(), &[]), RuntimeMode::ApprovalRequired);
        let accepting = SessionDefaults {
            claude_permission_mode: Some("acceptEdits".into()),
            ..defaults()
        };
        assert_eq!(claude(&accepting, &[]), RuntimeMode::AutoAcceptEdits);
        assert_eq!(
            claude(&accepting, &["--dangerously-skip-permissions"]),
            RuntimeMode::FullAccess
        );
        assert_eq!(
            claude(&accepting, &["--permission-mode=plan"]),
            RuntimeMode::ApprovalRequired
        );
        assert_eq!(
            claude(
                &SessionDefaults {
                    claude_permission_mode: Some("someNewMode".into()),
                    ..defaults()
                },
                &[]
            ),
            RuntimeMode::FullAccess
        );
        // Codex's own sandbox default is not known here.
        assert_eq!(codex(&defaults(), &[]), RuntimeMode::FullAccess);
        assert_eq!(
            codex(&defaults(), &["-s", "workspace-write"]),
            RuntimeMode::AutoAcceptEdits
        );
        assert_eq!(
            codex(&defaults(), &["--dangerously-bypass-approvals-and-sandbox"]),
            RuntimeMode::FullAccess
        );
        assert_eq!(
            codex(
                &defaults(),
                &["-a", "untrusted", "-s", "danger-full-access"]
            ),
            RuntimeMode::ApprovalRequired
        );
    }
}
