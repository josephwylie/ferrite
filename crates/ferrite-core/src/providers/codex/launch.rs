//! A Thread's launch arguments (`thread.start --args`, ADR 0030) are
//! written for Codex's terminal UI, as Herdr launches it: a profile's
//! `--model gpt-5.5` or `-c model_reasoning_effort="high"`, the safety
//! settings' `--dangerously-bypass-approvals-and-sandbox`. Ferrite drives
//! `codex app-server`, which refuses the UI's flags. Each one the
//! app-server has a setting for becomes that setting; the rest (`-c`,
//! `--enable`, `--disable`) go to `codex app-server` as they are.

/// The arguments, sorted into what the app-server takes.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Launch {
    pub model: Option<String>,
    pub effort: Option<String>,
    pub approval_policy: Option<String>,
    pub sandbox: Option<String>,
    /// For `codex app-server` itself.
    pub args: Vec<String>,
}

/// Sort `args`, written for `codex`, into what `codex app-server` takes.
/// A later flag wins over an earlier one, as on Codex's command line.
pub fn from_tui_args(args: &[String]) -> Launch {
    let mut launch = Launch::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = || args.next().cloned();
        match arg.as_str() {
            "-m" | "--model" => launch.model = value(),
            "-a" | "--ask-for-approval" => launch.approval_policy = value(),
            "-s" | "--sandbox" => launch.sandbox = value(),
            "--dangerously-bypass-approvals-and-sandbox" | "--yolo" => {
                launch.approval_policy = Some("never".into());
                launch.sandbox = Some("danger-full-access".into());
            }
            "-c" | "--config" => {
                let Some(setting) = value() else {
                    launch.args.push(arg.clone());
                    continue;
                };
                match setting.strip_prefix("model_reasoning_effort=") {
                    Some(effort) => launch.effort = Some(toml_string(effort)),
                    None => launch.args.extend([arg.clone(), setting]),
                }
            }
            _ => match arg.strip_prefix("--model=") {
                Some(model) => launch.model = Some(model.to_string()),
                None => launch.args.push(arg.clone()),
            },
        }
    }
    launch
}

/// A `-c` value is TOML; a bare word is taken literally, as Codex does.
fn toml_string(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn a_profiles_model_and_effort_become_the_sessions() {
        let launch = from_tui_args(&strings(&[
            "--model",
            "gpt-5.5",
            "-c",
            "model_reasoning_effort=\"high\"",
        ]));
        assert_eq!(launch.model.as_deref(), Some("gpt-5.5"));
        assert_eq!(launch.effort.as_deref(), Some("high"));
        assert!(launch.args.is_empty(), "{launch:?}");
        assert_eq!(
            from_tui_args(&strings(&["-m", "o3"])).model.as_deref(),
            Some("o3")
        );
        assert_eq!(
            from_tui_args(&strings(&["--model=o3"])).model.as_deref(),
            Some("o3")
        );
    }

    #[test]
    fn the_yolo_flag_is_never_ask_with_full_access() {
        for flag in ["--dangerously-bypass-approvals-and-sandbox", "--yolo"] {
            let launch = from_tui_args(&strings(&[flag]));
            assert_eq!(launch.approval_policy.as_deref(), Some("never"));
            assert_eq!(launch.sandbox.as_deref(), Some("danger-full-access"));
            assert!(launch.args.is_empty(), "{launch:?}");
        }
        let launch = from_tui_args(&strings(&["-a", "untrusted", "-s", "read-only"]));
        assert_eq!(launch.approval_policy.as_deref(), Some("untrusted"));
        assert_eq!(launch.sandbox.as_deref(), Some("read-only"));
    }

    #[test]
    fn what_the_app_server_takes_goes_to_it_unchanged() {
        let args = strings(&[
            "-c",
            "shell_environment_policy.inherit=all",
            "--enable",
            "x",
        ]);
        assert_eq!(from_tui_args(&args).args, args);
        assert_eq!(from_tui_args(&strings(&["-c"])).args, strings(&["-c"]));
    }

    /// Projects' turn-end hook (ADR 0031): Codex runs its `notify` at each
    /// turn end, and the app-server is handed it as written.
    #[test]
    fn projects_turn_end_notify_reaches_the_app_server() {
        let args = strings(&[
            "-c",
            r#"notify=["/d/bin/herdr-projects","turn-ended","--id","app/1","--role","agent","--token","t"]"#,
        ]);
        assert_eq!(from_tui_args(&args).args, args);
    }
}
