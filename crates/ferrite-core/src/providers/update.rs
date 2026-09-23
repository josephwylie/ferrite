//! Keeping the provider CLIs current.
//!
//! A provider's model menu is whatever its CLI announces, so a new model
//! reaches Ferrite only through a newer CLI. This module answers three
//! questions without a window: what the newest release is (the npm
//! registry, which every install channel tracks), how the copy Ferrite runs
//! was installed (read off its path — npm, Homebrew, volta, bun, or the
//! vendor's own installer), and the one command that upgrades it through
//! that same channel. Installing through a different channel would leave a
//! second copy that discovery might not prefer, so a copy whose channel is
//! unknown gets no command; the operator is told what to run instead.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use super::discover::{self, Located};
use crate::spawn::NoConsoleWindow;
use crate::store::Provider;

/// Generous for one small JSON document; a check that overruns it is
/// simply retried at the next interval.
const REGISTRY_TIMEOUT: Duration = Duration::from_secs(10);

/// Enough installer output to explain a failure.
const OUTPUT_TAIL_LINES: usize = 12;

/// The npm package each provider publishes its CLI as.
pub fn package(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "@anthropic-ai/claude-code",
        Provider::Codex => "@openai/codex",
    }
}

/// Homebrew's name for each CLI (a cask for Claude, `codex` for Codex;
/// `brew upgrade` takes either kind by name).
fn brew_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "claude-code",
        Provider::Codex => "codex",
    }
}

/// How an installed copy got there, which is how it must be upgraded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Channel {
    /// A global npm package. `bin` is the directory npm itself should be
    /// found in first: the node that installed the package.
    Npm {
        bin: PathBuf,
    },
    Brew,
    Volta,
    Bun,
    /// The vendor's own installer, whose CLI upgrades itself: `claude
    /// update`, or `codex update` for Codex's standalone install.
    SelfUpdate,
}

/// The channel behind `found`, whose symlinks resolve to `real`. `None`
/// when the path says nothing Ferrite can act on.
pub fn channel(provider: Provider, found: &Path, real: &Path) -> Option<Channel> {
    let resolved = real.to_string_lossy().replace('\\', "/").to_lowercase();
    let listed = found.to_string_lossy().replace('\\', "/").to_lowercase();
    let bin = found.parent().map(Path::to_path_buf).unwrap_or_default();
    // Windows npm shims (`%APPDATA%\npm\codex.cmd`) are not links: the
    // package sits beside them under `node_modules`.
    let beside_modules = bin
        .join("node_modules")
        .join(package(provider).replace('/', std::path::MAIN_SEPARATOR_STR))
        .is_dir();
    if listed.contains("/.volta/") || resolved.contains("/.volta/") {
        return Some(Channel::Volta);
    }
    if listed.contains("/.bun/") || resolved.contains("/.bun/") {
        return Some(Channel::Bun);
    }
    // Before Homebrew: a Homebrew node's global packages live under its
    // prefix too, and those belong to npm.
    if resolved.contains("/node_modules/") || beside_modules {
        return Some(Channel::Npm { bin });
    }
    if resolved.contains("/cellar/")
        || resolved.contains("/caskroom/")
        || resolved.starts_with("/opt/homebrew/")
        || resolved.starts_with("/home/linuxbrew/")
    {
        return Some(Channel::Brew);
    }
    // Codex's standalone installer keeps each release under
    // `~/.codex/packages/standalone/releases/`, behind a `current` link.
    if resolved.contains("/.codex/packages/standalone/") {
        return Some(Channel::SelfUpdate);
    }
    match provider {
        Provider::Claude => Some(Channel::SelfUpdate),
        Provider::Codex => None,
    }
}

/// One upgrade, ready to run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Upgrade {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Put ahead of PATH for the child: an nvm npm is a node script, and
    /// the app's PATH may not hold that node.
    pub path_prefix: Option<PathBuf>,
}

impl Upgrade {
    /// The command as an operator would type it.
    pub fn display(&self) -> String {
        let program = self
            .program
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.program.display().to_string());
        std::iter::once(program)
            .chain(self.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The first `program` Ferrite can find, preferring `near`.
fn tool(program: &str, near: Option<&Path>) -> Option<PathBuf> {
    let path = std::env::var_os("PATH");
    let near = near.map(|dir| std::env::join_paths([dir]).unwrap_or_default());
    near.and_then(|dir| discover::candidates(program, Some(&dir)).into_iter().next())
        .or_else(|| {
            discover::candidates(program, path.as_deref())
                .into_iter()
                .next()
        })
}

/// The upgrade for `located` through `channel`, to the newest release.
/// `None` when the channel's own tool cannot be found.
pub fn upgrade(provider: Provider, located: &Located, channel: &Channel) -> Option<Upgrade> {
    let latest = format!("{}@latest", package(provider));
    let (program, args, path_prefix) = match channel {
        Channel::Npm { bin } => {
            let npm = tool("npm", Some(bin))?;
            let prefix = npm.parent().map(Path::to_path_buf);
            (npm, vec!["install".into(), "-g".into(), latest], prefix)
        }
        Channel::Brew => (
            tool("brew", None)?,
            vec!["upgrade".into(), brew_name(provider).into()],
            None,
        ),
        Channel::Volta => (tool("volta", None)?, vec!["install".into(), latest], None),
        Channel::Bun => (
            tool("bun", None)?,
            vec!["add".into(), "-g".into(), latest],
            None,
        ),
        Channel::SelfUpdate => (located.path.clone(), vec!["update".into()], None),
    };
    Some(Upgrade {
        program,
        args,
        path_prefix,
    })
}

/// The version in an npm registry `latest` document.
pub fn parse_latest(provider: Provider, body: &str) -> Option<(String, [u64; 3])> {
    let document: serde_json::Value = serde_json::from_str(body).ok()?;
    discover::parser(provider)(document.get("version")?.as_str()?)
}

/// The newest published release of `provider`'s CLI. Blocks on the
/// network; call it off the UI thread.
pub fn latest(provider: Provider) -> io::Result<(String, [u64; 3])> {
    let url = format!("https://registry.npmjs.org/{}/latest", package(provider));
    let body = ureq::get(&url)
        .header("Accept", "application/json")
        .config()
        .timeout_global(Some(REGISTRY_TIMEOUT))
        .build()
        .call()
        .map_err(io::Error::other)?
        .body_mut()
        .read_to_string()
        .map_err(io::Error::other)?;
    parse_latest(provider, &body)
        .ok_or_else(|| io::Error::other(format!("no version in the {url} answer")))
}

/// What a check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// No copy is installed; there is nothing to upgrade.
    Missing,
    UpToDate {
        current: String,
    },
    Available {
        current: String,
        latest: String,
        /// `None`: Ferrite cannot tell how this copy was installed, and
        /// the operator upgrades it by hand.
        upgrade: Option<Upgrade>,
    },
}

/// Compare the copy Ferrite runs against the newest release. Blocks on a
/// version probe and the network. Looks for copies afresh, so an upgrade
/// the operator ran in a terminal counts.
pub fn check(provider: Provider) -> io::Result<Status> {
    discover::rediscover();
    let Some(located) = discover::located(provider) else {
        return Ok(Status::Missing);
    };
    let (latest, parsed) = latest(provider)?;
    if parsed <= located.parsed {
        return Ok(Status::UpToDate {
            current: located.version,
        });
    }
    let real = std::fs::canonicalize(&located.path).unwrap_or_else(|_| located.path.clone());
    let upgrade = channel(provider, &located.path, &real)
        .and_then(|channel| upgrade(provider, &located, &channel));
    Ok(Status::Available {
        current: located.version,
        latest,
        upgrade,
    })
}

/// Run `upgrade` to completion, then look again for the newest copy. The
/// answer is the version now installed. Blocks for as long as the
/// installer takes.
pub fn run(provider: Provider, upgrade: &Upgrade) -> Result<String, String> {
    let mut command = Command::new(&upgrade.program);
    command
        .args(&upgrade.args)
        .stdin(Stdio::null())
        .no_console_window();
    if let Some(prefix) = &upgrade.path_prefix {
        let rest = std::env::var_os("PATH").unwrap_or_default();
        let joined = std::env::join_paths(
            std::iter::once(prefix.clone()).chain(std::env::split_paths(&rest)),
        )
        .map_err(|error| error.to_string())?;
        command.env("PATH", joined);
    }
    let output = command
        .output()
        .map_err(|error| format!("could not run `{}`: {error}", upgrade.display()))?;
    discover::rediscover();
    if !output.status.success() {
        let mut text = String::from_utf8_lossy(&output.stderr).into_owned();
        if text.trim().is_empty() {
            text = String::from_utf8_lossy(&output.stdout).into_owned();
        }
        let lines: Vec<&str> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        let tail = lines[lines.len().saturating_sub(OUTPUT_TAIL_LINES)..].join("\n");
        return Err(format!(
            "`{}` {}{}",
            upgrade.display(),
            output.status,
            if tail.is_empty() {
                String::new()
            } else {
                format!(":\n{tail}")
            }
        ));
    }
    discover::located(provider)
        .map(|found| found.version)
        .ok_or_else(|| "the upgrade finished, but no copy answers `--version` now".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(provider: Provider, found: &str, real: &str) -> Option<Channel> {
        channel(provider, Path::new(found), Path::new(real))
    }

    #[test]
    fn an_npm_link_is_upgraded_by_the_npm_beside_it() {
        let found = "/home/op/.nvm/versions/node/v22.1.0/bin/codex";
        assert_eq!(
            classify(
                Provider::Codex,
                found,
                "/home/op/.nvm/versions/node/v22.1.0/lib/node_modules/@openai/codex/bin/codex.js",
            ),
            Some(Channel::Npm {
                bin: PathBuf::from("/home/op/.nvm/versions/node/v22.1.0/bin"),
            })
        );
    }

    /// A Homebrew node's globals sit under Homebrew's prefix but are npm's.
    #[test]
    fn a_homebrew_nodes_global_package_is_npms() {
        assert!(matches!(
            classify(
                Provider::Codex,
                "/opt/homebrew/bin/codex",
                "/opt/homebrew/lib/node_modules/@openai/codex/bin/codex.js",
            ),
            Some(Channel::Npm { .. })
        ));
    }

    #[test]
    fn a_cask_or_formula_is_homebrews() {
        assert_eq!(
            classify(
                Provider::Claude,
                "/opt/homebrew/bin/claude",
                "/opt/homebrew/Caskroom/claude-code/2.1.259/claude",
            ),
            Some(Channel::Brew)
        );
        assert_eq!(
            classify(
                Provider::Codex,
                "/usr/local/bin/codex",
                "/usr/local/Cellar/codex/0.153.0/bin/codex",
            ),
            Some(Channel::Brew)
        );
    }

    #[test]
    fn volta_and_bun_keep_their_own_channels() {
        assert_eq!(
            classify(
                Provider::Codex,
                "/home/op/.volta/bin/codex",
                "/home/op/.volta/bin/volta-shim",
            ),
            Some(Channel::Volta)
        );
        assert_eq!(
            classify(
                Provider::Claude,
                "/home/op/.bun/bin/claude",
                "/home/op/.bun/install/global/node_modules/@anthropic-ai/claude-code/cli.js",
            ),
            Some(Channel::Bun)
        );
    }

    /// Claude's own installer is anything else; `claude update` knows it.
    #[test]
    fn an_unrecognised_claude_is_its_own_installer() {
        assert_eq!(
            classify(
                Provider::Claude,
                "/home/op/.local/bin/claude",
                "/home/op/.local/share/claude/versions/2.1.259",
            ),
            Some(Channel::SelfUpdate)
        );
    }

    /// Codex's standalone install upgrades itself, as Claude's does.
    #[test]
    fn a_standalone_codex_updates_itself() {
        assert_eq!(
            classify(
                Provider::Codex,
                "C:/Users/op/AppData/Local/Programs/OpenAI/Codex/bin/codex.exe",
                "C:/Users/op/.codex/packages/standalone/releases/0.153.4-x86_64-pc-windows-msvc/bin/codex.exe",
            ),
            Some(Channel::SelfUpdate)
        );
    }

    /// Codex has no self-upgrade to fall back on: an unknown copy is left
    /// to the operator rather than shadowed by a second install.
    #[test]
    fn an_unrecognised_codex_gets_no_channel() {
        assert_eq!(
            classify(Provider::Codex, "/home/op/bin/codex", "/home/op/bin/codex",),
            None
        );
    }

    #[test]
    fn the_registry_answer_is_read_by_the_providers_own_parser() {
        let body = r#"{"name":"@openai/codex","version":"0.160.2","bin":{"codex":"bin/codex.js"}}"#;
        assert_eq!(
            parse_latest(Provider::Codex, body),
            Some(("0.160.2".to_string(), [0, 160, 2]))
        );
        assert_eq!(parse_latest(Provider::Claude, r#"{"name":"x"}"#), None);
    }

    #[test]
    fn an_upgrade_displays_as_typed() {
        let upgrade = Upgrade {
            program: PathBuf::from("C:/Program Files/nodejs/npm.cmd"),
            args: vec!["install".into(), "-g".into(), "@openai/codex@latest".into()],
            path_prefix: None,
        };
        assert_eq!(upgrade.display(), "npm install -g @openai/codex@latest");
    }
}
