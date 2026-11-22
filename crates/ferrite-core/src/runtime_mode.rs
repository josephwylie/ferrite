//! How far an agent goes without asking, on one scale over every
//! provider's own words (T3 Code's runtime modes): what a delegated
//! `thread.start` caps its Thread at, so a Thread on another
//! machine never runs broader than the one that started it.
//!
//! Under a cap the Server alone sets how far the agent goes: a capped
//! launch carries no argument, environment or launcher of its own that
//! could set or widen it ([`widening`]), and every Session of the Thread
//! is started with the narrower of its cap and this machine's mode
//! ([`RuntimeMode::hold_claude`], [`RuntimeMode::hold_codex`]).

use serde::{Deserialize, Serialize};

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

    /// What a Thread on `provider` starts at on a machine whose defaults
    /// are `defaults`.
    pub fn of_defaults(provider: Provider, defaults: &SessionDefaults) -> Self {
        match provider {
            Provider::Claude => Self::of_claude(
                defaults
                    .claude_permission_mode
                    .as_deref()
                    .unwrap_or(CLAUDE_DEFAULT),
            ),
            Provider::Codex => Self::of_codex(
                defaults
                    .codex_approval_policy
                    .as_deref()
                    .unwrap_or(CODEX_DEFAULT),
                defaults.codex_sandbox.as_deref(),
            ),
        }
    }

    /// What a Thread on `provider` may do now: what its provider announced
    /// (Claude's permission mode, Codex's approval policy and sandbox),
    /// else what it starts at under `defaults`, and never more than its own
    /// `cap`. What a Delegate it starts is capped at (G-A6).
    pub fn of_thread(
        provider: Provider,
        announced: Option<&str>,
        sandbox: Option<&str>,
        defaults: &SessionDefaults,
        cap: Option<Self>,
    ) -> Self {
        let mode = match (provider, announced) {
            (_, None) => Self::of_defaults(provider, defaults),
            (Provider::Claude, Some(mode)) => Self::of_claude(mode),
            (Provider::Codex, Some(policy)) => {
                Self::of_codex(policy, sandbox.or(defaults.codex_sandbox.as_deref()))
            }
        };
        cap.map_or(mode, |cap| mode.min(cap))
    }

    /// Claude's permission mode, by its word. A word this scale does not
    /// know counts as full access, so a cap still narrows it.
    fn of_claude(mode: &str) -> Self {
        match mode {
            "default" | "manual" | "plan" | "dontAsk" => Self::ApprovalRequired,
            "acceptEdits" => Self::AutoAcceptEdits,
            "auto" => Self::Auto,
            _ => Self::FullAccess,
        }
    }

    /// Codex's approval policy and sandbox. `untrusted` asks before
    /// anything it does not trust, whatever the sandbox; otherwise the
    /// sandbox bounds what runs without asking, and one left to Codex's
    /// own config counts as full access.
    fn of_codex(policy: &str, sandbox: Option<&str>) -> Self {
        match (policy, sandbox) {
            ("untrusted", _) | (_, Some("read-only")) => Self::ApprovalRequired,
            ("never", Some("workspace-write")) => Self::Auto,
            ("on-request" | "on-failure", Some("workspace-write")) => Self::AutoAcceptEdits,
            _ => Self::FullAccess,
        }
    }

    /// The permission mode a capped Claude Session starts in: `mode`
    /// (Claude's default when `None`) when it is no broader than this
    /// cap, else this cap's own. Always a word, so Claude's settings
    /// files cannot pick a broader one.
    pub fn hold_claude(self, mode: Option<String>) -> String {
        let mode = mode.unwrap_or_else(|| CLAUDE_DEFAULT.into());
        if Self::of_claude(&mode) <= self {
            return mode;
        }
        match self {
            Self::ApprovalRequired => "default",
            Self::AutoAcceptEdits => "acceptEdits",
            Self::Auto => "auto",
            Self::FullAccess => "bypassPermissions",
        }
        .into()
    }

    /// The approval policy and sandbox a capped Codex Session starts
    /// with: these when they are no broader than this cap, else this
    /// cap's own.
    pub fn hold_codex(self, policy: String, sandbox: Option<String>) -> (String, Option<String>) {
        if Self::of_codex(&policy, sandbox.as_deref()) <= self {
            return (policy, sandbox);
        }
        let (policy, sandbox) = match self {
            Self::ApprovalRequired => ("untrusted", "read-only"),
            Self::AutoAcceptEdits => ("on-request", "workspace-write"),
            Self::Auto => ("never", "workspace-write"),
            Self::FullAccess => ("never", "danger-full-access"),
        };
        (policy.into(), Some(sandbox.into()))
    }
}

/// Claude's permission mode when nothing names one.
const CLAUDE_DEFAULT: &str = "default";
/// Codex's approval policy when nothing names one (`spawner`).
const CODEX_DEFAULT: &str = "on-request";

/// Claude's arguments that set or widen how far it goes without asking,
/// each with how (`claude --help`, 2.1.330).
const CLAUDE: &[(&str, &str)] = &[
    ("--dangerously-skip-permissions", "bypasses every check"),
    (
        "--allow-dangerously-skip-permissions",
        "offers to bypass every check",
    ),
    ("--permission-mode", "sets the permission mode"),
    (
        "--permission-prompt-tool",
        "names who answers its permission prompts",
    ),
    (
        "--permission-prompts",
        "names who answers its permission prompts",
    ),
    ("--allowedTools", "runs tools without asking"),
    ("--allowed-tools", "runs tools without asking"),
    (
        "--settings",
        "loads settings, which allow tools, set the mode and add hooks",
    ),
    ("--setting-sources", "leaves out settings that may deny"),
    ("--add-dir", "opens more directories to it"),
    ("--mcp-config", "starts the MCP servers it names"),
    (
        "--plugin-dir",
        "loads a plugin, with its hooks and MCP servers",
    ),
    (
        "--plugin-url",
        "loads a plugin, with its hooks and MCP servers",
    ),
    (
        "--agents",
        "defines agents, which set their own permission mode",
    ),
    (
        "--agent",
        "runs as an agent, which sets its own permission mode",
    ),
    ("--bare", "skips the hooks settings define"),
    ("--safe-mode", "skips the hooks settings define"),
];

/// Codex's arguments that set or widen how far it goes without asking
/// (`codex --help`, 0.171.0).
const CODEX: &[(&str, &str)] = &[
    ("--yolo", "bypasses approvals and the sandbox"),
    (
        "--dangerously-bypass-approvals-and-sandbox",
        "bypasses approvals and the sandbox",
    ),
    ("-s", "sets the sandbox"),
    ("--sandbox", "sets the sandbox"),
    ("-a", "sets the approval policy"),
    ("--ask-for-approval", "sets the approval policy"),
    ("--approve-for-me", "routes approvals to automatic review"),
    ("--dangerously-bypass-hook-trust", "runs untrusted hooks"),
    (
        "-p",
        "layers a profile, which sets approvals and the sandbox",
    ),
    (
        "--profile",
        "layers a profile, which sets approvals and the sandbox",
    ),
    ("--add-dir", "opens more directories to it"),
];

/// The `-c` / `--config` keys, by their first part, that set or widen it.
const CODEX_CONFIG: &[&str] = &[
    "approval_policy",
    "approvals_reviewer",
    "sandbox_mode",
    "sandbox_workspace_write",
    "sandbox_permissions",
    "default_permissions",
    "permissions",
    "profile",
    "profiles",
    "mcp_servers",
];

/// What a capped launch carries that would set or widen how far its agent
/// goes, which only the Server sets under a cap: its message names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Widening(pub String);

impl std::fmt::Display for Widening {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The first thing in a capped launch on `provider` that would set or
/// widen how far its agent goes: an argument of the provider's table, any
/// `env` (a variable can point the CLI at other settings, as
/// `CLAUDE_CONFIG_DIR` and `CODEX_HOME` do, or at another program, as
/// `PATH` does), or any `launcher` (a program the CLI is run through can
/// change its arguments).
pub fn widening(
    provider: Provider,
    args: &[String],
    env: &[(String, String)],
    launcher: &[String],
) -> Option<Widening> {
    let flags = match provider {
        Provider::Claude => CLAUDE,
        Provider::Codex => CODEX,
    };
    for arg in args {
        if let Some((flag, how)) = flags.iter().find(|(flag, _)| names(arg, flag)) {
            return Some(Widening(format!(
                "`{flag}` {how}: a capped start takes no such argument (its `mode` sets how far \
                 the agent goes)"
            )));
        }
    }
    if provider == Provider::Codex {
        if let Some(key) = codex_configs(args).find(|key| CODEX_CONFIG.contains(key)) {
            return Some(Widening(format!(
                "`-c {key}` sets how far Codex goes without asking: a capped start takes no such \
                 setting (its `mode` sets it)"
            )));
        }
    }
    if let Some((name, _)) = env.first() {
        return Some(Widening(format!(
            "`env` `{name}`: a capped start takes no environment of its own, which can point the \
             CLI at other settings or another program"
        )));
    }
    if !launcher.is_empty() {
        return Some(Widening(
            "`launcher`: a capped start runs its CLI through no other program, which could change \
             its arguments"
                .into(),
        ));
    }
    None
}

/// Whether `arg` is `flag`: bare, `--flag=value`, or a short flag with its
/// value attached (`-sread-only`).
fn names(arg: &str, flag: &str) -> bool {
    arg.strip_prefix(flag).is_some_and(|rest| {
        rest.is_empty()
            || rest.starts_with('=')
            || (!flag.starts_with("--") && !rest.starts_with('-'))
    })
}

/// The first part of each key Codex's `-c` / `--config` arguments set
/// (`sandbox_workspace_write` of `sandbox_workspace_write.network_access=true`).
fn codex_configs(args: &[String]) -> impl Iterator<Item = &str> {
    let mut words = args.iter().map(String::as_str);
    std::iter::from_fn(move || loop {
        let word = words.next()?;
        let setting = match word {
            "-c" | "--config" => words.next()?,
            _ => match word
                .strip_prefix("--config=")
                .or_else(|| word.strip_prefix("-c").filter(|rest| !rest.is_empty()))
            {
                Some(setting) => setting,
                None => continue,
            },
        };
        let key = setting.split(['=', '.']).next().unwrap_or_default();
        return Some(key.trim().trim_matches('"'));
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn a_threads_mode_is_what_it_announced_else_its_defaults_and_never_past_its_cap() {
        let defaults = SessionDefaults::default();
        let claude = |announced, cap| {
            RuntimeMode::of_thread(Provider::Claude, announced, None, &defaults, cap)
        };
        assert_eq!(claude(Some("acceptEdits"), None), RuntimeMode::AutoAcceptEdits);
        assert_eq!(claude(None, None), RuntimeMode::ApprovalRequired);
        assert_eq!(
            claude(Some("bypassPermissions"), Some(RuntimeMode::Auto)),
            RuntimeMode::Auto
        );
        let codex = RuntimeMode::of_thread(
            Provider::Codex,
            Some("never"),
            Some("workspace-write"),
            &defaults,
            None,
        );
        assert_eq!(codex, RuntimeMode::Auto);
    }

    #[test]
    fn a_cap_holds_a_session_to_the_narrower_of_it_and_this_machines_mode() {
        let claude = |cap: RuntimeMode, mode: Option<&str>| cap.hold_claude(mode.map(Into::into));
        assert_eq!(
            claude(RuntimeMode::ApprovalRequired, Some("bypassPermissions")),
            "default"
        );
        assert_eq!(claude(RuntimeMode::Auto, Some("bypassPermissions")), "auto");
        assert_eq!(claude(RuntimeMode::Auto, Some("someNewMode")), "auto");
        // Never broadened, and always named.
        assert_eq!(claude(RuntimeMode::FullAccess, Some("plan")), "plan");
        assert_eq!(claude(RuntimeMode::FullAccess, None), "default");

        let codex = |cap: RuntimeMode, policy: &str, sandbox: Option<&str>| {
            cap.hold_codex(policy.into(), sandbox.map(Into::into))
        };
        let words = |policy: &str, sandbox: &str| (policy.to_string(), Some(sandbox.to_string()));
        // Codex's own sandbox is not known here: a cap pins it.
        assert_eq!(
            codex(RuntimeMode::AutoAcceptEdits, "on-request", None),
            words("on-request", "workspace-write")
        );
        assert_eq!(
            codex(
                RuntimeMode::ApprovalRequired,
                "never",
                Some("danger-full-access")
            ),
            words("untrusted", "read-only")
        );
        assert_eq!(
            codex(RuntimeMode::FullAccess, "untrusted", Some("read-only")),
            words("untrusted", "read-only")
        );
    }

    #[test]
    fn each_cap_holds_a_full_access_session_at_that_cap() {
        for cap in RuntimeMode::ALL {
            assert_eq!(
                RuntimeMode::of_claude(&cap.hold_claude(Some("bypassPermissions".into()))),
                cap
            );
            let (policy, sandbox) = cap.hold_codex("never".into(), None);
            assert_eq!(RuntimeMode::of_codex(&policy, sandbox.as_deref()), cap);
        }
    }

    #[test]
    fn a_machines_defaults_say_how_far_its_threads_go() {
        let defaults = SessionDefaults::default();
        assert_eq!(
            RuntimeMode::of_defaults(Provider::Claude, &defaults),
            RuntimeMode::ApprovalRequired
        );
        assert_eq!(
            RuntimeMode::of_defaults(Provider::Codex, &defaults),
            RuntimeMode::FullAccess
        );
        let accepting = SessionDefaults {
            claude_permission_mode: Some("acceptEdits".into()),
            codex_sandbox: Some("workspace-write".into()),
            ..SessionDefaults::default()
        };
        assert_eq!(
            RuntimeMode::of_defaults(Provider::Claude, &accepting),
            RuntimeMode::AutoAcceptEdits
        );
        assert_eq!(
            RuntimeMode::of_defaults(Provider::Codex, &accepting),
            RuntimeMode::AutoAcceptEdits
        );
    }

    #[test]
    fn what_sets_no_permission_is_no_widening() {
        let none = |provider, args: &[&str]| widening(provider, &words(args), &[], &[]);
        assert_eq!(
            none(Provider::Claude, &["--model", "haiku", "--effort", "high"]),
            None
        );
        assert_eq!(
            none(
                Provider::Claude,
                &["--agent-teams", "--disallowedTools", "Bash"]
            ),
            None
        );
        assert_eq!(
            none(
                Provider::Codex,
                &[
                    "-m",
                    "gpt-5.5",
                    "-c",
                    "model_reasoning_effort=high",
                    "--search",
                    "--no-daemon"
                ]
            ),
            None
        );
        // Codex's flags are not Claude's.
        assert_eq!(none(Provider::Claude, &["-c"]), None);
    }

    #[test]
    fn a_widening_names_what_widens() {
        let named = |provider, args: &[&str]| {
            widening(provider, &words(args), &[], &[]).map(|widening| widening.0)
        };
        assert!(named(Provider::Claude, &["--agents", "{}"])
            .is_some_and(|message| message.starts_with("`--agents`")));
        assert!(named(Provider::Codex, &["-csandbox_mode=x"])
            .is_some_and(|message| message.contains("`-c sandbox_mode`")));
        assert!(
            named(Provider::Codex, &["-c", "\"approval_policy\"=\"never\""])
                .is_some_and(|message| message.contains("approval_policy"))
        );
    }
}
