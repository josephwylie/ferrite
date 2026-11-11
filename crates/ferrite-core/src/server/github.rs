//! A Factory's GitHub starts as the Server reads them (#225): with the
//! operator's `gh` login, on the path the Pane header's PR reads use, off
//! the pump (ADR 0025), with conditional requests so an unchanged poll
//! costs nothing. Ferrite writes nothing to GitHub.
//!
//! [`GitHub`] is the seam: the Server's harness hands it a scripted
//! reader, as it hands it a scripted Session spawner.

use std::path::Path;
use std::sync::mpsc;
use std::sync::Arc;

use crate::factory::{Command, GitHubItem, GitHubQuery, GitHubRead, StartKind, Trigger};

use super::Server;

/// What the Server reads from GitHub.
pub trait GitHub: Send + Sync {
    /// The open issues or PRs `query` matches, or that nothing changed
    /// since its tag.
    fn matches(&self, query: &GitHubQuery) -> Result<GitHubRead, String>;
    /// Every repository the login can reach, `owner/name`.
    fn repositories(&self) -> Result<Vec<String>, String>;
    /// A repository's labels.
    fn labels(&self, repository: &str) -> Result<Vec<String>, String>;
}

/// The operator's own `gh`.
pub struct Gh;

/// A read still out: whose, of which version, and its answer to come.
pub(super) type Reading = (String, u32, mpsc::Receiver<Result<GitHubRead, String>>);

impl Server {
    /// Read a GitHub start on a thread of its own; the next pass hears it.
    pub(super) fn read_github(&mut self, workflow: String, version: u32, query: GitHubQuery) {
        let (answer, answered) = mpsc::channel();
        let github = Arc::clone(&self.github);
        std::thread::spawn(move || {
            crate::spawn::serve_the_operator();
            let _ = answer.send(github.matches(&query));
        });
        self.github_reads.push((workflow, version, answered));
    }

    /// What each finished read found, as the commands they are.
    pub(super) fn github_answers(&mut self) -> Vec<Command> {
        let mut heard = Vec::new();
        self.github_reads
            .retain(|(workflow, version, answer)| match answer.try_recv() {
                Ok(result) => {
                    heard.push(Command::GitHubRead {
                        workflow: workflow.clone(),
                        version: *version,
                        result,
                    });
                    false
                }
                Err(mpsc::TryRecvError::Empty) => true,
                Err(mpsc::TryRecvError::Disconnected) => {
                    heard.push(Command::GitHubRead {
                        workflow: workflow.clone(),
                        version: *version,
                        result: Err("the read ended".into()),
                    });
                    false
                }
            });
        heard
    }
}

/// A window's picker list, as its reply.
pub(super) fn list(
    github: &dyn GitHub,
    ask: &crate::protocol::GitHubAsk,
) -> Result<Option<crate::protocol::Done>, crate::protocol::WireError> {
    use crate::protocol::{ErrorCode, GitHubAsk, WireError};
    let names = match ask {
        GitHubAsk::Repositories => github.repositories(),
        GitHubAsk::Labels { repository } => github.labels(repository),
        GitHubAsk::Unknown => {
            return Err(WireError {
                code: ErrorCode::Unsupported,
                message: "this server does not know that GitHub list".into(),
            })
        }
    };
    names
        .map(|names| Some(crate::protocol::Done::Names { names }))
        .map_err(|message| WireError {
            code: ErrorCode::Failed,
            message,
        })
}

/// `owner/name` safe to put in a path, or refused.
fn repository(query: &GitHubQuery) -> Result<&str, String> {
    let repository = query
        .source
        .repository
        .as_deref()
        .ok_or("no repository is chosen")?;
    let fine = repository.split('/').count() == 2
        && repository
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'));
    fine.then_some(repository)
        .ok_or_else(|| format!("{repository} is not a repository name"))
}

/// Percent-encode a query value.
fn encoded(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// The REST endpoint a query reads, and whether its answer is a search's
/// (`{"items": [...]}`).
pub(crate) fn endpoint(query: &GitHubQuery) -> Result<(String, bool), String> {
    let repository = repository(query)?;
    let label = || {
        query
            .source
            .label
            .as_deref()
            .map(encoded)
            .ok_or("no label is chosen")
    };
    Ok(match (query.kind, query.source.trigger) {
        (StartKind::GithubPr, Trigger::ReviewRequested) => (
            format!(
                "search/issues?q={}&per_page=100",
                encoded(&format!(
                    "repo:{repository} is:pr is:open review-requested:@me"
                ))
            ),
            true,
        ),
        (StartKind::GithubPr, Trigger::Opened) => (
            format!("repos/{repository}/pulls?state=open&per_page=100"),
            false,
        ),
        (_, Trigger::Labelled) => (
            format!(
                "repos/{repository}/issues?state=open&labels={}&per_page=100",
                label()?
            ),
            false,
        ),
        (_, _) => (
            format!("repos/{repository}/issues?state=open&per_page=100"),
            false,
        ),
    })
}

/// `gh api --include`'s answer: the status, the ETag, the body.
pub(crate) fn parse_included(out: &str) -> Option<(u16, Option<String>, &str)> {
    let (head, body) = out
        .split_once("\r\n\r\n")
        .or_else(|| out.split_once("\n\n"))
        .unwrap_or((out, ""));
    let mut lines = head.lines();
    let status = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    let etag = lines.find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("etag")
            .then(|| value.trim().to_string())
    });
    Some((status, etag, body))
}

/// The items of a read's body: an issue list, a PR list or a search. An
/// issues list holds PRs too, so a start of one kind keeps only its own.
pub(crate) fn parse_items(
    kind: StartKind,
    body: &str,
    search: bool,
) -> Result<Vec<GitHubItem>, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|error| format!("GitHub's answer: {error}"))?;
    let list = if search { &value["items"] } else { &value };
    let list = list.as_array().ok_or("GitHub's answer is not a list")?;
    Ok(list
        .iter()
        .filter(|item| {
            let is_pr = item.get("pull_request").is_some() || item.get("head").is_some();
            is_pr == (kind == StartKind::GithubPr)
        })
        .filter_map(|item| {
            Some(GitHubItem {
                number: item["number"].as_u64()?,
                title: item["title"].as_str()?.to_string(),
                body: item["body"].as_str().unwrap_or_default().to_string(),
                url: item["html_url"].as_str()?.to_string(),
            })
        })
        .collect())
}

/// What `gh` said when it failed: its message, else a GitHub one.
fn failure(output: &std::process::Output) -> String {
    let said = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if said.is_empty() {
        "the GitHub CLI failed".into()
    } else {
        said
    }
}

fn gh(args: &[&str]) -> Result<std::process::Output, String> {
    let home = dirs_home();
    crate::workspace::gh_output(&home, args).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => "the GitHub CLI (gh) is not installed".to_string(),
        _ => format!("the GitHub CLI could not run: {error}"),
    })
}

/// Somewhere to run `gh` from that is not a checkout.
fn dirs_home() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(Into::into)
        .filter(|home: &std::path::PathBuf| home.is_dir())
        .unwrap_or_else(|| Path::new("/").to_path_buf())
}

fn names(args: &[&str]) -> Result<Vec<String>, String> {
    let output = gh(args)?;
    if !output.status.success() {
        return Err(failure(&output));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

impl GitHub for Gh {
    fn matches(&self, query: &GitHubQuery) -> Result<GitHubRead, String> {
        let (path, search) = endpoint(query)?;
        let condition = query
            .etag
            .as_ref()
            .map(|etag| format!("If-None-Match: {etag}"));
        let mut args = vec!["api", "--include"];
        if let Some(condition) = &condition {
            args.extend(["-H", condition.as_str()]);
        }
        args.push(&path);
        let output = gh(&args)?;
        let out = String::from_utf8_lossy(&output.stdout);
        match parse_included(&out) {
            Some((304, _, _)) => Ok(GitHubRead::Unchanged),
            Some((200..=299, etag, body)) => Ok(GitHubRead::Matches {
                etag,
                items: parse_items(query.kind, body, search)?,
            }),
            Some((status, _, body)) => {
                let message = serde_json::from_str::<serde_json::Value>(body)
                    .ok()
                    .and_then(|value| value["message"].as_str().map(str::to_string))
                    .unwrap_or_else(|| failure(&output));
                Err(format!("GitHub answered {status}: {message}"))
            }
            None => Err(failure(&output)),
        }
    }

    fn repositories(&self) -> Result<Vec<String>, String> {
        names(&[
            "api",
            "--paginate",
            "user/repos?per_page=100&sort=pushed",
            "-q",
            ".[].full_name",
        ])
    }

    fn labels(&self, repository: &str) -> Result<Vec<String>, String> {
        let path = format!("repos/{repository}/labels?per_page=100");
        names(&["api", "--paginate", &path, "-q", ".[].name"])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::GitHubSource;

    fn query(kind: StartKind, trigger: Trigger) -> GitHubQuery {
        GitHubQuery {
            kind,
            source: GitHubSource {
                repository: Some("acme/app".into()),
                trigger,
                label: Some("ready for agent".into()),
                extra_instructions: String::new(),
            },
            etag: None,
        }
    }

    #[test]
    fn each_trigger_reads_its_own_endpoint() {
        assert_eq!(
            endpoint(&query(StartKind::GithubIssue, Trigger::Labelled)).unwrap(),
            (
                "repos/acme/app/issues?state=open&labels=ready%20for%20agent&per_page=100".into(),
                false
            )
        );
        assert_eq!(
            endpoint(&query(StartKind::GithubPr, Trigger::Opened))
                .unwrap()
                .0,
            "repos/acme/app/pulls?state=open&per_page=100"
        );
        let (search, is_search) =
            endpoint(&query(StartKind::GithubPr, Trigger::ReviewRequested)).unwrap();
        assert!(is_search);
        assert!(search.contains("review-requested%3A%40me"), "{search}");
        let mut odd = query(StartKind::GithubIssue, Trigger::Opened);
        odd.source.repository = Some("acme/app/../x?y".into());
        assert!(endpoint(&odd).is_err());
    }

    #[test]
    fn an_included_answer_gives_its_status_tag_and_items() {
        let answer = "HTTP/2.0 200 OK\r\nContent-Type: application/json\r\nEtag: W/\"abc\"\r\n\r\n\
            [{\"number\":7,\"title\":\"Login fails\",\"body\":null,\"html_url\":\"https://github.com/acme/app/issues/7\"},\
             {\"number\":8,\"title\":\"A PR\",\"pull_request\":{},\"html_url\":\"https://github.com/acme/app/pull/8\"}]";
        let (status, etag, body) = parse_included(answer).unwrap();
        assert_eq!((status, etag.as_deref()), (200, Some("W/\"abc\"")));
        let issues = parse_items(StartKind::GithubIssue, body, false).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 7);
        assert_eq!(issues[0].body, "");
        let prs = parse_items(StartKind::GithubPr, body, false).unwrap();
        assert_eq!(prs[0].url, "https://github.com/acme/app/pull/8");
        let unchanged = "HTTP/2.0 304 Not Modified\r\nEtag: W/\"abc\"\r\n\r\n";
        assert_eq!(parse_included(unchanged).unwrap().0, 304);
    }
}
