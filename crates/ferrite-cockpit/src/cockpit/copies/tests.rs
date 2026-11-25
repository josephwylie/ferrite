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
            saved_as: MachineKey(Some("box".into())),
            machine_name: "box".into(),
            projects_dir: "~/Projects".into(),
            kept: "/kept/box".into(),
        },
        dest: "~/Projects/app".into(),
        target: TARGET.into(),
        skip: Vec::new(),
        resumes,
        home: Some("/home/me".into()),
        waits: true,
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
        Phase::Failed { reason, again: Some(Again::Retry), when_back: true } if reason == "Can't reach box."
    ));

    let mut copy = Entry::new(order(false), None, now);
    copy.hear(sending(5, 10, "a"), now, true);
    copy.hear(stopped(io::ErrorKind::BrokenPipe.into()), now, true);
    assert!(matches!(
        &copy.phase,
        Phase::Failed { reason, again: Some(Again::Resume), when_back: true }
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

#[test]
fn a_resumed_copy_is_done_at_its_whole_size() {
    let now = Instant::now();
    let mut copy = Entry::new(order(true), None, now);
    // A resume finds 40 of the 100 bytes there from its first reading.
    copy.hear(sending(40, 100, "a"), now, true);
    copy.hear(sending(100, 100, "b"), now + Duration::from_secs(1), true);
    let heard = Heard::Copied {
        copied: Ok(Copied::Sent {
            sent: Sent {
                bytes: 60,
                ..Sent::default()
            },
            changed: Vec::new(),
        }),
        unfinished: Vec::new(),
    };
    copy.hear(heard, now + Duration::from_secs(1), true);
    assert!(
        matches!(copy.phase, Phase::Finishing { bytes: 100, .. }),
        "the whole copy, not what this run sent"
    );
}

#[test]
fn a_second_pass_never_takes_a_copy_back() {
    let now = Instant::now();
    let mut copy = Entry::new(order(false), None, now);
    copy.hear(sending(99, 100, "a"), now, true);
    // What could not be read is sent once more: that pass lists the
    // Project again, and reads less sent before it counts what is there.
    copy.hear(Heard::Copying(Copying::Listing(250)), now, true);
    copy.hear(sending(40, 100, "big.lock"), now, true);
    let Phase::Copying { sent, file, .. } = &copy.phase else {
        panic!("still copying, not listing");
    };
    assert_eq!((*sent, file.as_deref()), (99, Some("big.lock")));
    assert_eq!(copy.phase.done(), Some(0.99), "its percentage stays");
}

#[test]
fn a_machine_that_goes_stops_its_copy_for_a_resume_that_only_adds_what_landed() {
    let now = Instant::now();
    let mut copy = Entry::new(order(false), None, now);
    copy.hear(sending(5, 10, "a"), now, true);
    assert!(copy.machine_went("box was turned off.".into(), false));
    assert!(matches!(
        &copy.phase,
        Phase::Failed { reason, again: Some(Again::Resume), when_back: true } if reason == "box was turned off."
    ));
    assert!(!copy.machine_went("box was turned off.".into(), false), "it stopped already");

    let mut copy = Entry::new(order(false), None, now);
    let heard = Heard::Copied {
        copied: Ok(Copied::Sent {
            sent: Sent {
                bytes: 10,
                ..Sent::default()
            },
            changed: vec!["notes.md".into()],
        }),
        unfinished: Vec::new(),
    };
    copy.hear(heard, now, true);
    assert!(copy.machine_went("box was turned off.".into(), false));
    let Some(Again::Add { bytes, skipped }) = copy.phase.again() else {
        panic!("all of it landed: its Resume only adds it");
    };
    assert_eq!((*bytes, skipped.entries), (10, 1));
    assert_eq!(Again::Add { bytes: 0, skipped: NotCopied::default() }.verb(), "Resume");
    assert!(copy.add_again());
    assert!(matches!(copy.phase, Phase::Finishing { bytes: 10, used: false, .. }));
    assert!(copy.unadded("Copied, but not added on box: no space.".into(), false));
    assert!(matches!(copy.phase, Phase::Failed { again: Some(Again::Add { .. }), when_back: false, .. }));
}

#[test]
fn a_removed_machine_takes_its_copy_with_it() {
    let now = Instant::now();
    let run = run();
    let cancel = run.cancel.clone();
    let mut copy = Entry::new(order(false), Some(run), now);
    copy.hear(sending(5, 10, "a"), now, true);
    assert!(copy.machine_went("box was removed.".into(), true));
    assert!(cancel.check().is_err(), "its thread is told to stop");
    assert!(copy.run.is_none(), "nothing it says counts");
    assert!(matches!(&copy.phase, Phase::Failed { again: None, when_back: false, .. }));
    assert_eq!(copy.phase.again(), None, "no Resume, no Retry");
    for back in [false, true] {
        assert_eq!(copy.standing(back), Some(("box was removed.".into(), true)));
    }
    assert!(!copy.machine_went("box was removed.".into(), true), "it was told");

    // A copy that stopped for the Machine being off goes with it too.
    let mut copy = Entry::new(order(false), None, now);
    copy.hear(sending(5, 10, "a"), now, true);
    copy.machine_went("box was turned off.".into(), false);
    assert!(copy.machine_went("box was removed.".into(), true));
    assert_eq!(copy.phase.again(), None);
}

/// A copy's thread, running, none of what it says heard yet.
fn run() -> Run {
    let (_, hear) = ferrite_core::doorbell::channel();
    Run {
        hear,
        cancel: Cancellation::default(),
    }
}

/// Whether the copy's thread was told to stop.
fn cancelled(copy: &Entry) -> bool {
    copy.run.as_ref().is_some_and(|run| run.cancel.check().is_err())
}

/// All of the copy landed: 10 bytes, a file changed there left as it was.
fn landed() -> Heard {
    Heard::Copied {
        copied: Ok(Copied::Sent {
            sent: Sent {
                bytes: 10,
                ..Sent::default()
            },
            changed: vec!["notes.md".into()],
        }),
        unfinished: vec![TARGET.into()],
    }
}

#[test]
fn what_a_copy_says_after_its_machine_went_counts() {
    let now = Instant::now();
    // All of it landed as the Machine went: its Resume only adds it, and
    // the file it left as it was is still told of.
    let mut copy = Entry::new(order(false), Some(run()), now);
    copy.hear(sending(5, 10, "a"), now, true);
    assert!(copy.machine_went("box was turned off.".into(), false));
    assert!(cancelled(&copy), "its thread is told to stop");
    copy.hear(sending(10, 10, "a"), now, false);
    assert_eq!(copy.hear(landed(), now, false), None, "nothing is added while it is off");
    let Phase::Failed {
        reason,
        again: Some(Again::Add { bytes: 10, skipped }),
        when_back: true,
    } = &copy.phase
    else {
        panic!("its Resume only adds it");
    };
    assert_eq!(reason.as_ref(), "box was turned off.");
    assert_eq!(skipped.lines, ["notes.md \u{b7} changed on box"]);
    assert!(copy.run.is_none(), "it ended");
    assert_eq!(
        copy.standing(false),
        Some(("box was turned off. Resume adds the copy once it is back.".into(), true))
    );
    assert_eq!(copy.standing(true), Some(("box is back. Resume adds the copy.".into(), false)));

    // It stopped part way: its Resume finishes it, the Machine going why.
    let mut copy = Entry::new(order(false), Some(run()), now);
    copy.hear(sending(5, 10, "a"), now, true);
    copy.machine_went("box was turned off.".into(), false);
    copy.hear(stopped(io::ErrorKind::BrokenPipe.into()), now, false);
    assert!(matches!(
        &copy.phase,
        Phase::Failed { reason, again: Some(Again::Resume), when_back: true } if reason == "box was turned off."
    ));
    assert_eq!(copy.standing(true), Some(("box is back. Resume finishes the copy.".into(), false)));

    // It stopped before anything was kept: its Retry starts over.
    let mut copy = Entry::new(order(false), Some(run()), now);
    copy.machine_went("box was turned off.".into(), false);
    let ended = Heard::Copied {
        copied: Err(Stopped {
            error: io::ErrorKind::BrokenPipe.into(),
            sent: Sent::default(),
        }),
        unfinished: Vec::new(),
    };
    copy.hear(ended, now, false);
    assert_eq!(copy.phase.again(), Some(&Again::Retry));
}

#[test]
fn a_copy_that_lands_after_its_cancel_is_added_on_its_resume() {
    let now = Instant::now();
    let mut copy = Entry::new(order(false), Some(run()), now);
    assert!(copy.cancel());
    assert!(matches!(copy.phase, Phase::Cancelled { again: Again::Retry }), "nothing was sent");
    assert_eq!(copy.standing(false), Some(("Copy cancelled before anything was sent.".into(), false)));

    let mut copy = Entry::new(order(false), Some(run()), now);
    copy.hear(sending(5, 10, "a"), now, true);
    assert!(copy.cancel());
    assert!(cancelled(&copy));
    assert!(matches!(copy.phase, Phase::Cancelled { again: Again::Resume }), "what it sent stays");
    assert!(!copy.cancel(), "it stopped already");
    copy.hear(landed(), now, true);
    assert!(
        matches!(copy.phase, Phase::Cancelled { again: Again::Add { bytes: 10, .. } }),
        "it landed as it was cancelled: not found occupied by its own files"
    );
    assert_eq!(
        copy.standing(false),
        Some(("Copy cancelled once all of it had landed. Resume adds it.".into(), false))
    );
    assert!(copy.add_again());
    assert!(matches!(copy.phase, Phase::Finishing { bytes: 10, .. }));
}
