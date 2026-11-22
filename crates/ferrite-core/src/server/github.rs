//! A Factory's GitHub starts as the Server reads them (#225): with the
//! operator's `gh` login, on the path the Pane header's PR reads use, off
//! the pump (ADR 0025), with conditional requests so an unchanged poll
//! costs nothing. Ferrite writes nothing to GitHub.
//!
//! [`GitHub`] is the seam: the Server's harness hands it a scripted
//! reader, as it hands it a scripted Session spawner. A watched PR (ADR
//! 0046) is read through the same seam, with `gh api graphql`.

use std::path::Path;

use crate::factory::{GitHubItem, GitHubQuery, GitHubRead, StartKind, Trigger};
use crate::pr_watch::{Ask, Fingerprint, ReadError, Reading};

/// What the Server reads from GitHub.
pub trait GitHub: Send + Sync {
    /// The open issues or PRs `query` matches, or that nothing changed
    /// since its tag.
    fn matches(&self, query: &GitHubQuery) -> Result<GitHubRead, String>;
    /// Every repository the login can reach, `owner/name`.
    fn repositories(&self) -> Result<Vec<String>, String>;
    /// A repository's labels.
    fn labels(&self, repository: &str) -> Result<Vec<String>, String>;
    /// A pull request's head commit.
    fn head(&self, repository: &str, number: u64) -> Result<String, String>;
    /// A watched PR's cheap change check (`pr_watch::fingerprint`).
    fn pr_fingerprint(&self, repository: &str, number: u64) -> Result<Fingerprint, ReadError>;
    /// A watched PR whole (`pr_watch::reading`), its remarks too when
    /// `remarks`.
    fn pr_reading(&self, repository: &str, number: u64, remarks: bool) -> Result<Reading, ReadError>;
}

/// The operator's own `gh`.
pub struct Gh;

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

/// One GraphQL query through `gh api graphql`, its body on stdin.
fn graphql(ask: Ask) -> Result<serde_json::Value, ReadError> {
    let body = serde_json::json!({ "query": ask.query, "variables": ask.variables });
    gh_json(&["api", "graphql", "--input", "-"], Some(body.to_string()))
}

/// One REST read through `gh api`.
fn rest(path: &str) -> Result<serde_json::Value, ReadError> {
    gh_json(&["api", path], None)
}

/// `gh`'s JSON answer, or why there is none, as the watcher treats it.
fn gh_json(args: &[&str], input: Option<String>) -> Result<serde_json::Value, ReadError> {
    use crate::spawn::NoConsoleWindow;
    use std::io::Write;
    use std::process::{Command, Stdio};
    let spawned = Command::new("gh")
        .args(args)
        .current_dir(dirs_home())
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .no_console_window()
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ReadError::Unusable {
                setup: "Install the GitHub CLI (gh) on the server and sign in with `gh auth login`."
                    .into(),
            })
        }
        Err(error) => return Err(ReadError::Failed(format!("the GitHub CLI could not run: {error}"))),
    };
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        let _ = stdin.write_all(input.as_bytes());
    }
    let output = child
        .wait_with_output()
        .map_err(|error| ReadError::Failed(format!("the GitHub CLI could not run: {error}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        return serde_json::from_str(&stdout)
            .map_err(|error| ReadError::Failed(format!("GitHub's answer: {error}")));
    }
    // A GraphQL error still prints its answer: a missing PR is GitHub's
    // word, not a failure of the read.
    if let Ok(answer) = serde_json::from_str::<serde_json::Value>(&stdout) {
        if answer["data"].is_object() && !rate_limited(&stdout) {
            return Ok(answer);
        }
    }
    Err(classify(&format!("{} {stdout}", String::from_utf8_lossy(&output.stderr))))
}

fn rate_limited(said: &str) -> bool {
    let said = said.to_ascii_lowercase();
    said.contains("rate limit") || said.contains("rate_limited")
}

/// What a failed `gh` call's words say about it.
pub(crate) fn classify(said: &str) -> ReadError {
    let said = said.trim();
    let lower = said.to_ascii_lowercase();
    if rate_limited(said) {
        ReadError::RateLimited
    } else if lower.contains("gh auth login") || lower.contains("http 401") || lower.contains("bad credentials") {
        ReadError::Unusable {
            setup: "Sign in to GitHub on the server with `gh auth login`.".into(),
        }
    } else if lower.contains("http 404")
        || lower.contains("http 403")
        || lower.contains("not_found")
        || lower.contains("forbidden")
        || lower.contains("could not resolve to")
    {
        ReadError::Refused(said.to_string())
    } else {
        ReadError::Failed(if said.is_empty() {
            "the GitHub CLI failed".into()
        } else {
            said.to_string()
        })
    }
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

    fn head(&self, repository: &str, number: u64) -> Result<String, String> {
        let path = format!("repos/{repository}/pulls/{number}");
        names(&["api", &path, "-q", ".head.sha"])?
            .into_iter()
            .next()
            .ok_or_else(|| format!("GitHub said nothing of {repository}#{number}'s head"))
    }

    fn pr_fingerprint(&self, repository: &str, number: u64) -> Result<Fingerprint, ReadError> {
        crate::pr_watch::fingerprint(repository, number, &mut graphql)
    }

    fn pr_reading(&self, repository: &str, number: u64, remarks: bool) -> Result<Reading, ReadError> {
        crate::pr_watch::reading(repository, number, remarks, &mut graphql, &mut rest)
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
    fn a_failed_read_is_told_by_what_gh_says() {
        assert_eq!(
            classify("gh: API rate limit exceeded for user ID 1. (HTTP 403)"),
            ReadError::RateLimited
        );
        assert!(matches!(
            classify("To get started with GitHub CLI, please run:  gh auth login"),
            ReadError::Unusable { .. }
        ));
        assert!(matches!(
            classify("GraphQL: Could not resolve to a Repository with the name 'o/r'. (repository)"),
            ReadError::Refused(_)
        ));
        assert!(matches!(
            classify("error connecting to api.github.com"),
            ReadError::Failed(_)
        ));
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
