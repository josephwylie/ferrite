//! How a copy's numbers, and what it left, read on the card and in its
//! toasts.

use ferrite_core::tree::{self, Refusal, Sent, Side};
use std::time::Duration;

/// A count as the operator reads it: `12,500`.
pub(crate) fn count(count: usize) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// A size as the operator reads it: `340 KB`, `1.2 GB`.
pub(crate) fn bytes(size: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if size < 1000 {
        return format!("{size} B");
    }
    let mut value = size as f64 / 1000.0;
    let mut unit = 0;
    while value >= 999.5 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    if value < 9.95 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

/// How long a copy took, to the second: `40 s`, `3 min 40 s`, `1 h 5 min`.
pub(crate) fn took(took: Duration) -> String {
    let seconds = took.as_secs_f64().round().max(1.0) as u64;
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    match (hours, minutes, seconds) {
        (0, 0, seconds) => format!("{seconds} s"),
        (0, minutes, 0) => format!("{minutes} min"),
        (0, minutes, seconds) => format!("{minutes} min {seconds} s"),
        (hours, 0, _) => format!("{hours} h"),
        (hours, minutes, _) => format!("{hours} h {minutes} min"),
    }
}

/// How many alike are listed one by one under Not copied before they are
/// told as one line.
const LISTED: usize = 3;

/// What a copy to `machine` left as it was, in the operator's words: each
/// by its path and why, or, past [`LISTED`] of one kind left for one
/// reason, how many and the folder they share.
pub(crate) fn not_copied(sent: &Sent, changed: &[String], machine: &str) -> Vec<String> {
    // Each one left: its path, what it is, and what is said of one and of
    // several.
    let mut left: Vec<(&str, &str, String, String)> = Vec::new();
    for problem in &sent.problems {
        let (one, many) = match problem.side {
            Side::Here => ("in use here".to_string(), "were in use here".to_string()),
            Side::There => (format!("unreadable on {machine}"), format!("could not be read on {machine}")),
        };
        left.push((&problem.path, "files", one, many));
    }
    for unmade in &sent.unmade {
        let what = match unmade.entry {
            tree::Entry::Dir => "folders",
            tree::Entry::Symlink { .. } => "links",
            _ => "files",
        };
        let (one, many) = match &unmade.why {
            Refusal::LeadsOut => (
                format!("a link outside the Project, not made on {machine}"),
                format!("lead outside the Project and were not made on {machine}"),
            ),
            Refusal::LeadsNowhere => (
                format!("a link to nothing on {machine}, not made"),
                format!("lead to nothing on {machine} and were not made"),
            ),
            Refusal::Name => (
                format!("a name {machine} cannot hold"),
                format!("have names {machine} cannot hold"),
            ),
            Refusal::Failed { error } => {
                let why = plainly(error);
                (format!("not made on {machine}: {why}"), format!("could not be made on {machine}: {why}"))
            }
            Refusal::Unknown => (format!("not made on {machine}"), format!("could not be made on {machine}")),
        };
        left.push((&unmade.path, what, one, many));
    }
    for path in changed {
        left.push((path, "files", format!("changed on {machine}"), format!("were changed on {machine}")));
    }
    let mut alike: Vec<((&str, String), Vec<(&str, String)>)> = Vec::new();
    for (path, what, one, many) in left {
        match alike.iter_mut().find(|((kind, said), _)| *kind == what && *said == many) {
            Some((_, these)) => these.push((path, one)),
            None => alike.push(((what, many), vec![(path, one)])),
        }
    }
    alike
        .into_iter()
        .flat_map(|((what, many), these)| {
            if these.len() <= LISTED {
                return these.into_iter().map(|(path, one)| format!("{path} \u{b7} {one}")).collect();
            }
            let folder = shared_folder(these.iter().map(|(path, _)| *path));
            let within = if folder.is_empty() { String::new() } else { format!(" in {folder}") };
            vec![format!("{} {what}{within} {many}", count(these.len()))]
        })
        .collect()
}

/// The deepest folder that holds every one of `paths`: `""` for the
/// Project's own.
fn shared_folder<'a>(paths: impl Iterator<Item = &'a str>) -> String {
    let mut shared: Option<Vec<&str>> = None;
    for path in paths {
        let mut folder: Vec<&str> = path.split('/').collect();
        folder.pop();
        shared = Some(match shared {
            None => folder,
            Some(shared) => shared.into_iter().zip(folder).take_while(|(one, other)| one == other).map(|(one, _)| one).collect(),
        });
    }
    shared.unwrap_or_default().join("/")
}

/// A copy to `machine` cancelled while what it asked is on its way: the
/// card and its toast say so until `machine` has answered all of it.
pub(crate) fn stopping(machine: &str) -> String {
    format!("Cancelling\u{2026} waiting for {machine} to answer")
}

/// What a Server said went wrong, as a clause: `Access is denied. (os
/// error 5)` reads `access is denied`.
pub(crate) fn plainly(said: &str) -> String {
    let why = said.split(" (os error").next().unwrap_or(said).trim_end_matches('.');
    let mut letters = why.chars();
    match letters.next() {
        Some(first) => first.to_lowercase().chain(letters).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::tree::{Problem, Unmade};

    fn unmade(path: &str, entry: tree::Entry, why: Refusal) -> Unmade {
        Unmade {
            path: path.into(),
            entry,
            why,
        }
    }

    fn link() -> tree::Entry {
        tree::Entry::Symlink { target: "../x".into() }
    }

    /// A few left are each named by path; many of one kind left for one
    /// reason are one line, with how many and the folder they share.
    #[test]
    fn what_a_copy_left_is_said_plainly_and_many_alike_as_one_line() {
        let mut sent = Sent::default();
        sent.problems.push(Problem {
            path: "server.log".into(),
            side: Side::Here,
            error: "The process cannot access the file. (os error 32)".into(),
        });
        for at in 0..312 {
            let path = format!("apps/web/node_modules/@scope/name-{at}");
            sent.unmade.push(unmade(&path, link(), Refusal::LeadsOut));
        }
        sent.unmade.push(unmade("apps/docs/node_modules/x", link(), Refusal::LeadsOut));
        sent.unmade.push(unmade(".env", link(), Refusal::LeadsNowhere));
        sent.unmade.push(unmade("src/aux", tree::Entry::Dir, Refusal::Name));
        let failed = Refusal::Failed {
            error: "Access is denied. (os error 5)".into(),
        };
        sent.unmade.push(unmade("dist/app.js", tree::Entry::File { size: 1, sha256: String::new(), mode: None, modified: None, text: None }, failed));
        assert_eq!(
            not_copied(&sent, &["notes.md".into()], "josephpc"),
            [
                "server.log \u{b7} in use here",
                "313 links in apps lead outside the Project and were not made on josephpc",
                ".env \u{b7} a link to nothing on josephpc, not made",
                "src/aux \u{b7} a name josephpc cannot hold",
                "dist/app.js \u{b7} not made on josephpc: access is denied",
                "notes.md \u{b7} changed on josephpc",
            ]
        );
        sent.unmade.truncate(312);
        assert_eq!(
            not_copied(&Sent { problems: Vec::new(), ..sent }, &[], "josephpc"),
            ["312 links in apps/web/node_modules/@scope lead outside the Project and were not made on josephpc"]
        );
    }

    #[test]
    fn counts_are_grouped_in_thousands() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(18_402), "18,402");
        assert_eq!(count(1_234_567), "1,234,567");
    }

    #[test]
    fn sizes_read_in_the_largest_unit_under_a_thousand() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(143_000), "143 KB");
        assert_eq!(bytes(1_500_000), "1.5 MB");
        assert_eq!(bytes(2_200_000_000), "2.2 GB");
        // Rounding never says 1000 of a unit.
        assert_eq!(bytes(999_700), "1.0 MB");
        assert_eq!(bytes(9_970), "10 KB");
    }

    #[test]
    fn how_long_reads_to_the_second_in_the_two_largest_units() {
        assert_eq!(took(Duration::from_millis(200)), "1 s");
        assert_eq!(took(Duration::from_secs(40)), "40 s");
        assert_eq!(took(Duration::from_secs(220)), "3 min 40 s");
        assert_eq!(took(Duration::from_secs(180)), "3 min");
        assert_eq!(took(Duration::from_secs(3900)), "1 h 5 min");
        assert_eq!(took(Duration::from_secs(7215)), "2 h");
    }
}
