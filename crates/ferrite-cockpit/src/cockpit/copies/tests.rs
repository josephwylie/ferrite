//! A copy's state machine, heard as its thread tells it, on a clock of the
//! test's own.

use super::*;
use ferrite_core::chunked::Progress;
use ferrite_core::project::ProjectId;
use ferrite_core::roster::{DraftId, PaneIdentity};
use ferrite_core::tree::{self, Problem, Refusal, Sent, Side, Unmade};

const TARGET: &str = "/home/me/Projects/app";

fn order(resumes: bool) -> Order {
    Order {
        placing: Placing {
            draft: PaneIdentity::Draft(DraftId::new(1)),
            project: ProjectKey::fixture(0, ProjectId(1)),
            title: "app".into(),
            root: "/here/app".into(),
            machine: ProjectKey::fixture(1, ProjectId(0)).machine(),
            machine_name: "box".into(),
            projects_dir: "~/Projects".into(),
            kept: "/kept/box".into(),
        },
        dest: "~/Projects/app".into(),
        target: TARGET.into(),
        skip: Vec::new(),
        resumes,
        home: Some("/home/me".into()),
    }
}

fn sending(sent: u64, total: u64, file: &str) -> Heard {
    Heard::Copying(Copying::Sending {
        progress: Progress { sent, total },
        file: Some(file.into()),
    })
}

fn stopped(error: io::Error) -> Heard {
    Heard::Copied {
        copied: Err(Stopped {
            error,
            sent: Sent::default(),
        }),
        unfinished: vec![TARGET.into()],
    }
}

#[test]
fn a_copy_lists_sends_and_finishes_and_says_what_it_left() {
    let start = Instant::now();
    let mut copy = Entry::new(order(false), None, start);
    let key = copy.key();
    assert!(matches!(copy.phase, Phase::Listing { files: 0 }));
    assert_eq!(copy.phase.done(), None, "how far is not known while listing");
    assert_eq!(copy.hear(Heard::Copying(Copying::Listing(18_402)), start, true), None);
    assert!(matches!(copy.phase, Phase::Listing { files: 18_402 }));

    let at = |seconds| start + Duration::from_secs(seconds);
    for second in 1..=6 {
        assert_eq!(copy.hear(sending(second * 10, 100, "src/main.rs"), at(second), true), None);
    }
    let Phase::Copying { sent, total, file, reading, .. } = &copy.phase else {
        panic!("copying");
    };
    assert_eq!((*sent, *total, file.as_deref()), (60, 100, Some("src/main.rs")));
    assert_eq!(reading.rate, Some(10), "ten bytes a second");
    assert_eq!(copy.phase.done(), Some(0.6));
    assert!(copy.unfinished.contains(&TARGET.into()), "once it sends it is an unfinished copy");
    // A count with no file keeps the last file named.
    let heard = Heard::Copying(Copying::Sending {
        progress: Progress { sent: 70, total: 100 },
        file: None,
    });
    copy.hear(heard, at(7), true);
    assert!(matches!(&copy.phase, Phase::Copying { file: Some(file), .. } if file == "src/main.rs"));

    let sent = Sent {
        bytes: 100,
        problems: vec![Problem {
            path: "big.lock".into(),
            side: Side::Here,
            error: "in use".into(),
        }],
        unmade: vec![Unmade {
            path: "web/link".into(),
            entry: tree::Entry::Symlink { target: "../../x".into() },
            why: Refusal::LeadsOut,
        }],
        ..Sent::default()
    };
    let heard = Heard::Copied {
        copied: Ok(Copied::Sent {
            sent,
            changed: vec!["notes.md".into()],
        }),
        unfinished: Vec::new(),
    };
    assert_eq!(copy.hear(heard, at(8), true), Some(Landed::Sent(key)), "the window adds it");
    let Phase::Finishing { bytes, skipped, used } = &copy.phase else {
        panic!("finishing");
    };
    assert_eq!((*bytes, *used), (100, false));
    assert_eq!(
        skipped.lines,
        [
            "big.lock \u{b7} in use here",
            "web/link \u{b7} a link outside the Project, not made on box",
            "notes.md \u{b7} changed on box",
        ]
    );
    assert_eq!(skipped.entries, 3);
    assert!(copy.phase.live(), "it runs until it is a Project there");
    assert_eq!(copy.phase.done(), Some(1.0));
}

#[test]
fn a_destination_holding_what_it_did_not_put_there_takes_nothing() {
    let now = Instant::now();
    let mut copy = Entry::new(order(false), None, now);
    let key = copy.key();
    let heard = Heard::Copied {
        copied: Ok(Copied::Occupied),
        unfinished: Vec::new(),
    };
    assert_eq!(copy.hear(heard, now, true), Some(Landed::Ended(key)));
    assert!(matches!(copy.phase, Phase::Occupied { of: None }));
    assert_eq!(copy.why().as_deref(), Some("app is not empty. Nothing there is changed."));

    let mut copy = Entry::new(order(false), None, now);
    let heard = Heard::Copied {
        copied: Ok(Copied::AnotherCopy {
            of: "/here/web".into(),
        }),
        unfinished: Vec::new(),
    };
    copy.hear(heard, now, true);
    assert_eq!(
        copy.why().as_deref(),
        Some("app holds an unfinished copy of web. Nothing there is changed.")
    );
    assert!(!copy.phase.live());
}

#[test]
fn a_copy_that_stops_says_why_in_the_operators_words() {
    let now = Instant::now();
    let mut copy = Entry::new(order(false), None, now);
    copy.hear(
        Heard::Unreached {
            error: io::ErrorKind::ConnectionRefused.into(),
            unfinished: Vec::new(),
        },
        now,
        false,
    );
    assert!(matches!(
        &copy.phase,
        Phase::Failed { reason, resumable: false, when_back: true } if reason == "Can't reach box."
    ));

    let mut copy = Entry::new(order(false), None, now);
    copy.hear(sending(5, 10, "a"), now, true);
    copy.hear(stopped(io::ErrorKind::BrokenPipe.into()), now, true);
    assert!(matches!(
        &copy.phase,
        Phase::Failed { reason, resumable: true, when_back: true }
            if reason == "The connection to box was lost."
    ));

    let mut copy = Entry::new(order(false), None, now);
    let denied = io::Error::other("src/lib.rs: Permission denied (os error 13)");
    copy.hear(stopped(denied), now, true);
    assert_eq!(copy.why().as_deref(), Some("Can't write in ~/Projects/app/src: permission denied."));

    // The Machine gone under the copy is lost, whatever the error says.
    let mut copy = Entry::new(order(false), None, now);
    copy.hear(stopped(io::Error::other("x: Disk full")), now, false);
    assert_eq!(copy.why().as_deref(), Some("The connection to box was lost."));
}
