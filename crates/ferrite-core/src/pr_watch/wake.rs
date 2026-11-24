//! A report in words: the prompt that wakes the agent, and the line its
//! notification shows. T3 Code's `pullRequestWatchMessage`,
//! `pullRequestWatchEndMessage`, its give-up text and
//! `combinePullRequestWatchWakes`, word for word with "Ferrite" in place of
//! "T3 Code".

use super::{Change, Check, CheckStatus, End, Report, READ_FAILURE_LIMIT, WAKE_LIMIT};
use crate::pull_request::Watch;
use crate::wake::{Outcome, Source, Wake};

const LISTED_ITEMS: usize = 10;
/// A quoted remark's length, in UTF-16 units as T3 counts it.
const SNIPPET_LENGTH: usize = 200;

const WATCHING_CLOSING: &str = "Look into each item and act on it as your task requires. This PR stays linked and watched, so do not link, watch, or list it again. End your turn when you are done, and Ferrite wakes you on the next change. When you hand the work back to the user, call unwatch_pull_request first so the thread returns to their inbox.";
const WATCHING_CLOSING_PLURAL: &str = "Look into each item and act on it as your task requires. Each PR Ferrite still watches stays linked and watched, so do not link, watch, or list them again. End your turn when you are done, and Ferrite wakes you on the next change. When you hand the work back to the user, call unwatch_pull_request for each first so the thread returns to their inbox.";

/// The wake for one watch's report; `None` when it has no news and does
/// not end.
pub fn wake_text(watch: &Watch, report: &Report) -> Option<Wake> {
    let (number, url) = (watch.number, &watch.url);
    match &report.end {
        Some(End::Merged { base, by, commit }) => {
            let by = by
                .as_ref()
                .map(|by| format!(" by {by}"))
                .unwrap_or_default();
            let commit = commit
                .as_ref()
                .map(|commit| format!(" as {}", short(commit)))
                .unwrap_or_default();
            return Some(Wake {
                source: Source::PrWatch,
                text: format!(
                    "Pull request #{number} ({url}) was merged into {base}{by}{commit}, so Ferrite stopped watching it."
                ),
                outcome: Outcome::Completed,
                summary: format!("#{number}: merged, stopped watching"),
            });
        }
        Some(End::Closed) => {
            return Some(Wake {
                source: Source::PrWatch,
                text: format!(
                    "Pull request #{number} ({url}) was closed without merging, so Ferrite stopped watching it. If it is reopened, call watch_pull_request to watch it again."
                ),
                outcome: Outcome::Updated,
                summary: format!("#{number}: closed, stopped watching"),
            });
        }
        Some(End::Unreadable { problem }) => {
            return Some(match problem {
                None => Wake {
                    source: Source::PrWatch,
                    text: format!(
                        "Ferrite stopped watching pull request #{number} ({url}) because it failed to read it from the host {READ_FAILURE_LIMIT} times in a row. Check it yourself, and call watch_pull_request only if it is still open."
                    ),
                    outcome: Outcome::Failed,
                    summary: format!("#{number}: stopped watching, could not read it"),
                },
                Some(problem) => Wake {
                    source: Source::PrWatch,
                    text: format!(
                        "Ferrite stopped watching pull request #{number} ({url}) because the Ferrite server cannot use the host: {problem} Fix that on the server, then call watch_pull_request only if it is still open."
                    ),
                    outcome: Outcome::Failed,
                    summary: format!("#{number}: stopped watching, host not set up on the server"),
                },
            });
        }
        Some(End::CommentLimit) | None => {}
    }
    if report.changes.is_empty() {
        return None;
    }
    let exhausted = report.end == Some(End::CommentLimit);
    let commit = report
        .next
        .head
        .as_ref()
        .map(|head| format!(" on {}", short(head)))
        .unwrap_or_default();
    let mut lines = vec![format!(
        "Update on pull request #{number} ({url}), which Ferrite is watching for you:"
    )];
    for change in &report.changes {
        lines.extend(change_lines(change, &commit));
    }
    lines.push(String::new());
    lines.push(if exhausted {
        format!(
            "Ferrite stopped watching after {WAKE_LIMIT} comment-only updates in a row. Call watch_pull_request to watch it again."
        )
    } else {
        WATCHING_CLOSING.to_string()
    });
    let failed = report.changes.iter().any(|change| {
        matches!(
            change,
            Change::ChecksFailed(_)
                | Change::ChecksFinished { .. }
                | Change::Conflicting { .. }
                | Change::MergeQueueRemoved { .. }
        )
    });
    let mut summary: Vec<&str> = report.changes.iter().map(summary_word).collect();
    if exhausted {
        summary.push("stopped watching");
    }
    Some(Wake {
        source: Source::PrWatch,
        text: lines.join("\n"),
        outcome: if failed {
            Outcome::Failed
        } else if report
            .changes
            .iter()
            .all(|change| matches!(change, Change::ChecksPassed { .. }))
        {
            Outcome::Completed
        } else {
            Outcome::Updated
        },
        summary: format!("#{number}: {}", summary.join(", ")),
    })
}

/// One wake for news on several PRs, with the closing instructions said
/// once; `None` for no wakes.
pub fn combine(mut wakes: Vec<Wake>) -> Option<Wake> {
    if wakes.len() <= 1 {
        return wakes.pop();
    }
    let closing = format!("\n\n{WATCHING_CLOSING}");
    let watching = wakes.iter().any(|wake| wake.text.ends_with(&closing));
    let mut parts: Vec<&str> = wakes
        .iter()
        .map(|wake| wake.text.strip_suffix(&closing).unwrap_or(&wake.text))
        .collect();
    if watching {
        parts.push(WATCHING_CLOSING_PLURAL);
    }
    let outcome = if wakes.iter().any(|wake| wake.outcome == Outcome::Failed) {
        Outcome::Failed
    } else if wakes.iter().all(|wake| wake.outcome == Outcome::Completed) {
        Outcome::Completed
    } else {
        Outcome::Updated
    };
    Some(Wake {
        source: Source::PrWatch,
        text: parts.join("\n\n"),
        outcome,
        summary: wakes
            .iter()
            .map(|wake| wake.summary.as_str())
            .collect::<Vec<_>>()
            .join("; "),
    })
}

fn short(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

fn summary_word(change: &Change) -> &'static str {
    match change {
        Change::ChecksFailed(_) => "checks failed",
        Change::ChecksPassed { .. } => "checks passed",
        Change::ChecksFinished { .. } => "checks finished",
        Change::AwaitingApproval(_) => "checks await approval",
        Change::Remarks(_) => "new comments",
        Change::NoChecks { .. } => "no checks reported",
        Change::Conflicting { .. } => "merge conflict",
        Change::MergeQueueRemoved { .. } => "removed from merge queue",
    }
}

fn change_lines(change: &Change, commit: &str) -> Vec<String> {
    match change {
        Change::ChecksFailed(failed) => {
            let mut lines = vec![format!("- Checks failed{commit}:")];
            lines.extend(listed(failed, failed_check_line));
            lines
        }
        Change::ChecksFinished { passed, failing } => {
            let mut lines = vec![format!(
                "- Every check has finished{commit}: {passed} passed, {} still failing:",
                failing.len()
            )];
            lines.extend(listed(failing, failed_check_line));
            lines
        }
        Change::AwaitingApproval(checks) => {
            let mut lines = vec![format!(
                "- Checks are waiting for approval or action from a maintainer{commit}:"
            )];
            lines.extend(listed(checks, |check| {
                format!("  - {}{}", check.name, with_url(&check.url))
            }));
            lines
        }
        Change::ChecksPassed { count, required } => vec![format!(
            "- All {count} {}{} passed{commit}.",
            if *required { "required " } else { "" },
            if *count == 1 { "check" } else { "checks" },
        )],
        Change::Remarks(remarks) => {
            let mut lines = vec![format!(
                "- {} new {}:",
                remarks.len(),
                if remarks.len() == 1 { "comment" } else { "comments" }
            )];
            lines.extend(listed(remarks, |remark| {
                let author = remark.author.as_deref().unwrap_or("someone");
                let place = remark
                    .path
                    .as_ref()
                    .map(|path| format!(" on {path}"))
                    .unwrap_or_default();
                let body = snippet(&remark.body);
                let said = if body.is_empty() {
                    remark.review_state.as_deref().unwrap_or("reviewed").to_string()
                } else {
                    format!("\"{body}\"")
                };
                format!("  - {author}{place}: {said}{}", with_url(&remark.url))
            }));
            lines
        }
        Change::NoChecks { minutes } => vec![format!(
            "- No checks have reported{commit} after {minutes} minutes. None may be configured to run on this change."
        )],
        Change::Conflicting { base } => vec![format!("- The branch now conflicts with {base}.")],
        Change::MergeQueueRemoved { reason } => vec![format!(
            "- It was removed from the merge queue without merging{}.",
            reason
                .as_ref()
                .map(|reason| format!(": {reason}"))
                .unwrap_or_default()
        )],
    }
}

fn with_url(url: &Option<String>) -> String {
    url.as_ref()
        .map(|url| format!(" {url}"))
        .unwrap_or_default()
}

fn failed_check_line(check: &Check) -> String {
    let status = if check.status == CheckStatus::Failure {
        String::new()
    } else {
        format!(" ({})", check.status.word())
    };
    format!("  - {}{status}{}", check.name, with_url(&check.url))
}

fn listed<T>(items: &[T], line: impl Fn(&T) -> String) -> Vec<String> {
    let mut lines: Vec<String> = items.iter().take(LISTED_ITEMS).map(line).collect();
    if items.len() > LISTED_ITEMS {
        lines.push(format!("  - and {} more", items.len() - LISTED_ITEMS));
    }
    lines
}

/// A remark's prose on one line, without HTML comments or tags, cut to
/// `SNIPPET_LENGTH`.
pub(super) fn snippet(body: &str) -> String {
    let text = without_markup(&without_html_comments(body));
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.encode_utf16().count() <= SNIPPET_LENGTH {
        return text;
    }
    // Cut on a character, never through an emoji's surrogate pair: half of
    // one serializes to a lone surrogate, which provider JSON parsers
    // reject.
    let mut units = 0;
    let kept: String = text
        .chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= SNIPPET_LENGTH - 3
        })
        .collect();
    format!("{kept}...")
}

fn without_html_comments(body: &str) -> String {
    let mut text = String::new();
    let mut rest = body;
    while let Some(open) = rest.find("<!--") {
        let Some(close) = rest[open + 4..].find("-->") else {
            break;
        };
        text.push_str(&rest[..open]);
        text.push(' ');
        rest = &rest[open + 4 + close + 3..];
    }
    text.push_str(rest);
    text
}

/// Drops HTML tags from Markdown prose. Fenced and indented code blocks
/// pass through untouched.
fn without_markup(body: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut prose: Vec<&str> = Vec::new();
    let mut fence: Option<&str> = None;
    let mut after_text = false;
    let code = |line: &str, prose: &mut Vec<&str>, parts: &mut Vec<String>| {
        if !prose.is_empty() {
            parts.push(without_tags(&prose.join("\n")));
        }
        prose.clear();
        parts.push(line.to_string());
    };
    for line in body.split('\n') {
        if let Some(open) = fence {
            let closer = line.trim();
            let mark = open.chars().next().unwrap_or('`');
            if closer.len() >= open.len() && closer.chars().all(|c| c == mark) {
                fence = None;
            }
            code(line, &mut prose, &mut parts);
            after_text = false;
            continue;
        }
        if let Some(opener) = fence_opener(line) {
            fence = Some(opener);
            code(line, &mut prose, &mut parts);
            after_text = false;
        } else if !after_text && indented_code(line) {
            // Indented code needs a blank line before it; after text the
            // indent is a continuation.
            code(line, &mut prose, &mut parts);
            after_text = false;
        } else {
            prose.push(line);
            after_text = !line.trim().is_empty();
        }
    }
    if !prose.is_empty() {
        parts.push(without_tags(&prose.join("\n")));
    }
    parts.join("\n")
}

/// The fence a line opens: up to three spaces, then three or more
/// backticks or tildes. A backtick fence's info string holds no backtick.
fn fence_opener(line: &str) -> Option<&str> {
    let indent = line.bytes().take_while(|&b| b == b' ').count();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let mark = rest.chars().next().filter(|&c| c == '`' || c == '~')?;
    let run = rest.bytes().take_while(|&b| b == mark as u8).count();
    if run < 3 || (mark == '`' && rest[run..].contains('`')) {
        return None;
    }
    Some(&rest[..run])
}

/// Four spaces, or up to three and a tab, then something.
fn indented_code(line: &str) -> bool {
    let spaces = line.bytes().take_while(|&b| b == b' ').count();
    let rest = if spaces >= 4 {
        &line[4..]
    } else if line[spaces..].starts_with('\t') {
        &line[spaces + 1..]
    } else {
        return false;
    };
    rest.chars().any(|c| !c.is_whitespace())
}

/// GitHub renders these, so in a remark's prose they are presentation, not
/// what the reviewer said. Lowercase only: `Promise<T>` is a type argument.
const TAG_NAMES: &[&str] = &[
    "a",
    "abbr",
    "b",
    "blockquote",
    "br",
    "caption",
    "cite",
    "code",
    "dd",
    "del",
    "details",
    "div",
    "dl",
    "dt",
    "em",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "i",
    "img",
    "ins",
    "kbd",
    "li",
    "mark",
    "ol",
    "p",
    "picture",
    "pre",
    "q",
    "s",
    "samp",
    "small",
    "source",
    "span",
    "strike",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "tt",
    "ul",
    "var",
];

/// One markup match in prose, as T3's `PROSE_MARKUP` finds them: a code
/// span (whose tags stay literal) or a known tag.
enum Markup<'a> {
    Code,
    Tag { name: &'a str, closing: bool },
}

/// T3's `withoutTags`: each tag becomes a space, except one glued to an
/// identifier (`Vec<code>`), which is a type argument and stays, with its
/// closer.
fn without_tags(prose: &str) -> String {
    let mut held: Vec<(&str, usize)> = Vec::new();
    let mut text = String::new();
    let mut from = 0;
    let mut at = 0;
    while at < prose.len() {
        let Some((markup, end)) = markup_at(prose, at) else {
            at += prose[at..].chars().next().map_or(1, char::len_utf8);
            continue;
        };
        let start = at;
        at = end;
        let Markup::Tag { name, closing } = markup else {
            continue;
        };
        let index = held
            .iter()
            .position(|(held, _)| *held == name)
            .unwrap_or_else(|| {
                held.push((name, 0));
                held.len() - 1
            });
        let count = &mut held[index].1;
        if closing {
            if *count > 0 {
                *count -= 1;
                continue;
            }
        } else if prose[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            *count += 1;
            continue;
        }
        text.push_str(&prose[from..start]);
        text.push(' ');
        from = end;
    }
    text.push_str(&prose[from..]);
    text
}

/// The markup starting at byte `at`, and where it ends.
fn markup_at(prose: &str, at: usize) -> Option<(Markup<'_>, usize)> {
    let bytes = prose.as_bytes();
    match bytes[at] {
        b'`' => code_span_at(prose, at).map(|end| (Markup::Code, end)),
        b'<' => tag_at(prose, at),
        _ => None,
    }
}

/// A code span: a run of backticks, then the nearest run of exactly as
/// many, never across a blank line.
fn code_span_at(prose: &str, at: usize) -> Option<usize> {
    let bytes = prose.as_bytes();
    if at > 0 && bytes[at - 1] == b'`' {
        return None;
    }
    let ticks = bytes[at..].iter().take_while(|&&b| b == b'`').count();
    let mut index = at + ticks;
    while index < bytes.len() {
        if starts_blank_line(&bytes[index..]) {
            return None;
        }
        if bytes[index] == b'`' && bytes[index - 1] != b'`' {
            let run = bytes[index..].iter().take_while(|&&b| b == b'`').count();
            if run == ticks {
                return Some(index + run);
            }
            index += run;
            continue;
        }
        index += 1;
    }
    None
}

/// `\n[ \t\r]*\n`.
fn starts_blank_line(bytes: &[u8]) -> bool {
    bytes.first() == Some(&b'\n')
        && bytes[1..]
            .iter()
            .find(|&&b| !matches!(b, b' ' | b'\t' | b'\r'))
            == Some(&b'\n')
}

/// A known tag: `<`, an optional `/`, a listed name followed by space, `/`
/// or `>`, then attributes (quoted values may hold anything) up to `>`.
fn tag_at(prose: &str, at: usize) -> Option<(Markup<'_>, usize)> {
    let closing = prose[at + 1..].starts_with('/');
    let name_at = at + 1 + usize::from(closing);
    let name_len = prose[name_at..].find(|c: char| c.is_whitespace() || c == '/' || c == '>')?;
    let name = &prose[name_at..name_at + name_len];
    if !TAG_NAMES.contains(&name) {
        return None;
    }
    let mut chars = prose[name_at + name_len..].char_indices();
    while let Some((offset, c)) = chars.next() {
        match c {
            '>' => {
                return Some((
                    Markup::Tag { name, closing },
                    name_at + name_len + offset + 1,
                ));
            }
            '<' => return None,
            '"' | '\'' => {
                chars.find(|&(_, close)| close == c)?;
            }
            _ => {}
        }
    }
    None
}
