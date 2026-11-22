//! Which PRs a shell command merges or closes: T3 Code's
//! `isChangeRequestCloseCommand` and `changeRequestsClosedBy`
//! (`sourceControlDefinitions.ts`, with GitHub's `changeRequestCloseCommand`).
//! The agent needs no wake for a merge or close it made itself.

/// What a merge or close command names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Closes {
    /// These PR numbers.
    Numbers(Vec<u64>),
    /// None by number, as a bare `gh pr merge` on the checked-out branch or
    /// `gh stack merge` does: any watched PR may be the one.
    Any,
}

impl Closes {
    pub fn names(&self, number: u64) -> bool {
        match self {
            Closes::Any => true,
            Closes::Numbers(numbers) => numbers.contains(&number),
        }
    }
}

/// The PRs `command` merges or closes; `None` when it is no such command.
pub fn closed_by(command: &str) -> Option<Closes> {
    let starts = close_commands(command);
    if starts.is_empty() {
        return None;
    }
    let mut named = Vec::new();
    for start in starts {
        let args = command[start..]
            .split([';', '&', '|', '\n'])
            .next()
            .unwrap_or_default();
        // Every merge and close takes one PR: the first word naming one.
        let mut target = None;
        for (word, bare) in shell_words(args) {
            // A comment ends the command.
            if bare && word.starts_with('#') && !is_number(&word[1..]) {
                break;
            }
            target = named_pr(&word);
            if target.is_some() {
                break;
            }
        }
        match target {
            Some(number) => named.push(number),
            None => return Some(Closes::Any),
        }
    }
    Some(Closes::Numbers(named))
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Where each `gh pr merge`, `gh pr close` or `gh stack merge` starts
/// (`\bgh\s+(?:pr\s+(?:merge|close)|stack\s+merge)\b`).
fn close_commands(command: &str) -> Vec<usize> {
    let mut starts = Vec::new();
    for (at, _) in command.match_indices("gh") {
        if command[..at].chars().next_back().is_some_and(word_char) {
            continue;
        }
        let rest = &command[at + 2..];
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let mut words = rest.split_whitespace();
        let close = match (words.next(), words.next()) {
            (Some("pr"), Some(verb)) => starts_word(verb, "merge") || starts_word(verb, "close"),
            (Some("stack"), Some(verb)) => starts_word(verb, "merge"),
            _ => false,
        };
        if close {
            starts.push(at);
        }
    }
    starts
}

/// `word` begins with the whole word `want` (`merge` in `merge;`, not in
/// `merged`).
fn starts_word(word: &str, want: &str) -> bool {
    word.strip_prefix(want)
        .is_some_and(|rest| !rest.starts_with(word_char))
}

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// A whole word naming a PR (`19`, `#19`, `--pr=19`, `…/pull/19`), so a
/// number inside `--subject "Release 20"` does not.
fn named_pr(word: &str) -> Option<u64> {
    let value = match word.strip_prefix('-') {
        Some(flag) => {
            let flag = flag.strip_prefix('-').unwrap_or(flag);
            let (name, value) = flag.split_once('=')?;
            if name.is_empty() || !name.chars().all(|c| word_char(c) || c == '-') {
                return None;
            }
            value
        }
        None => word,
    };
    let number = value.strip_prefix('#').unwrap_or(value);
    if is_number(number) {
        return number.parse().ok();
    }
    if word.contains(char::is_whitespace) {
        return None;
    }
    let path = word.strip_suffix('/').unwrap_or(word);
    let (head, last) = path.rsplit_once('/')?;
    let kind = head.rsplit('/').next()?;
    if matches!(kind, "pull" | "pulls" | "merge_requests") && is_number(last) {
        last.parse().ok()
    } else {
        None
    }
}

/// The words of a shell line, quoted or bare, an unclosed quote running to
/// the end (as Codex's `/bin/zsh -lc "gh pr merge 19"` leaves one). Each
/// says whether it was bare.
fn shell_words(line: &str) -> Vec<(String, bool)> {
    let mut words = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let mut word = String::new();
        if c == '"' || c == '\'' {
            chars.next();
            while let Some(next) = chars.next() {
                match next {
                    '\\' if c == '"' => word.extend(chars.next()),
                    next if next == c => break,
                    next => word.push(next),
                }
            }
            words.push((word, false));
            continue;
        }
        while let Some(&next) = chars.peek() {
            if next.is_whitespace() || next == '"' || next == '\'' {
                break;
            }
            chars.next();
            if next == '\\' {
                word.extend(chars.next());
            } else {
                word.push(next);
            }
        }
        words.push((word, true));
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(numbers: &[u64]) -> Option<Closes> {
        Some(Closes::Numbers(numbers.to_vec()))
    }

    #[test]
    fn names_the_prs_a_merge_or_close_ends() {
        assert_eq!(closed_by("gh pr merge 19 --squash"), numbers(&[19]));
        assert_eq!(closed_by("gh pr close #7"), numbers(&[7]));
        assert_eq!(
            closed_by("gh pr merge https://github.com/o/r/pull/440 --auto"),
            numbers(&[440])
        );
        assert_eq!(
            closed_by(r#"gh pr merge --subject "Release 20" 21"#),
            numbers(&[21])
        );
        assert_eq!(
            closed_by("gh pr merge 3 && gh pr close 4"),
            numbers(&[3, 4])
        );
        assert_eq!(closed_by("cd x && gh pr merge --squash"), Some(Closes::Any));
        assert_eq!(closed_by("gh stack merge"), Some(Closes::Any));
        assert_eq!(closed_by("gh pr merge # merge it"), Some(Closes::Any));
        assert_eq!(closed_by("gh pr view 19"), None);
        assert_eq!(closed_by("gh pr merged 19"), None);
        assert_eq!(closed_by("ngh pr merge 19"), None);
        assert!(Closes::Any.names(3));
        assert!(!Closes::Numbers(vec![2]).names(3));
    }
}
