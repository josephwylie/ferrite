use super::*;
use crate::workspace::{Check, PrState, PullRequest, Remark};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ferrite-coord-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn project() -> ProjectId {
    serde_json::from_value(serde_json::json!(3)).unwrap()
}

fn coordination(coordinator: u64) -> Coordination {
    Coordination {
        project: project(),
        coordinator: ThreadId::new(coordinator),
        goal: "Ship billing".into(),
        instructions: String::new(),
        follow_prs: false,
        workers: Vec::new(),
    }
}

/// The book survives a restart: a Project's coordinator, its brief, its
/// workers and the feedback each was sent. A second coordinator for the
/// same Project replaces the first.
#[test]
fn the_book_round_trips_and_one_project_has_one_coordinator() {
    let dir = scratch("book");
    let mut book = Coordinators::load(&dir).unwrap();
    assert_eq!(book.iter().count(), 0);
    book.insert(coordination(7)).unwrap();
    assert!(book
        .update(ThreadId::new(7), |entry| {
            entry.follow_prs = true;
            entry.workers.push(Worker {
                thread: ThreadId::new(8),
                task: "Add invoices".into(),
                sent: BTreeSet::new(),
                merged: None,
            });
        })
        .unwrap());
    assert!(!book.update(ThreadId::new(99), |_| {}).unwrap());
    book.update_worker(ThreadId::new(8), |worker| {
        worker.sent.insert("remark:IC_1".into());
    })
    .unwrap();

    let reloaded = Coordinators::load(&dir).unwrap();
    let entry = reloaded.led_by(ThreadId::new(7)).expect("its coordinator");
    assert!(entry.follow_prs);
    assert_eq!(entry.workers[0].task, "Add invoices");
    assert!(entry.workers[0].sent.contains("remark:IC_1"));
    assert_eq!(reloaded.of_project(project()), Some(entry));

    book.insert(coordination(9)).unwrap();
    let reloaded = Coordinators::load(&dir).unwrap();
    assert_eq!(reloaded.iter().count(), 1);
    assert_eq!(
        reloaded.of_project(project()).unwrap().coordinator,
        ThreadId::new(9)
    );
}

/// Lessons join the memory once each, after whatever the operator wrote
/// there by hand.
#[test]
fn the_memory_keeps_each_lesson_once() {
    let dir = scratch("memory");
    let book = Coordinators::load(&dir).unwrap();
    assert_eq!(book.memory(project()), "");
    let added = book
        .remember(
            project(),
            &["Run cargo check".into(), "Run cargo check".into()],
        )
        .unwrap();
    assert_eq!(added, ["Run cargo check"]);
    fs::write(
        book.memory_path(project()),
        "- Run cargo check\nThe operator's own note",
    )
    .unwrap();
    let added = book
        .remember(
            project(),
            &["Run cargo check".into(), "Never touch main".into()],
        )
        .unwrap();
    assert_eq!(added, ["Never touch main"]);
    assert_eq!(
        book.memory(project()),
        "- Run cargo check\nThe operator's own note\n- Never touch main\n"
    );
}

/// A coordinator's Group reads what needs the operator first, then what
/// is ready for review — finished, seen or not — then what is working.
#[test]
fn workers_are_read_needs_you_then_ready_for_review_then_working() {
    let mut needs = [
        Attention::Working,
        Attention::Idle,
        Attention::Blocked,
        Attention::Done,
    ]
    .map(Need::of);
    needs.sort();
    assert_eq!(
        needs,
        [
            Need::NeedsYou,
            Need::ReadyForReview,
            Need::ReadyForReview,
            Need::Working
        ]
    );
}

/// The lessons are the bullets under `## Remember`, as `plain_text` writes
/// an answer: its heading at any level, its bullets as `•`; the section
/// ends at the next heading or tool row.
#[test]
fn lessons_are_the_bullets_under_remember() {
    let answer = "Done: invoices render.\n\n\
                  ## Report\n\n\
                  • Added the table\n\n\
                  ### Remember\n\n\
                  • The billing tests need `STRIPE_KEY=test`\n\n\
                  - Invoices live in `app/billing`\n\n\
                  Some closing words.\n\n\
                  •  \n\n\
                  ## Next\n\n\
                  • Not a lesson\n\n\
                  ## remember\n\n\
                  • A second section counts too\n\n\
                  ● Bash (ls)\n\n\
                  • Not after a tool";
    assert_eq!(
        remembered(answer),
        [
            "The billing tests need `STRIPE_KEY=test`",
            "Invoices live in `app/billing`",
            "A second section counts too",
        ]
    );
    assert!(remembered("No lessons here.\n\n• a bullet").is_empty());
}

/// Every brief carries the Project's goal, instructions and memory; the
/// worker's its task, the `## Remember` contract and, where PRs are
/// followed, a PR to open; the coordinator's the commands it delegates
/// with.
#[test]
fn briefs_carry_the_project_and_the_contract() {
    let brief = Brief {
        project: "app",
        goal: "Ship billing",
        instructions: "Never push to main.",
        memory: "- Run cargo check\n",
    };
    let worker = worker_prompt(&brief, ThreadId::new(7), " Add invoices \n", false);
    for part in [
        "\"app\"",
        "Thread 7",
        "## Goal\n\nShip billing",
        "## Standing instructions\n\nNever push to main.",
        "## Memory\n\n- Run cargo check",
        "## Your task\n\nAdd invoices\n",
        "`## Remember`",
    ] {
        assert!(worker.contains(part), "{part:?} in {worker}");
    }
    assert!(!worker.contains("gh pr create"), "{worker}");
    let following = worker_prompt(&brief, ThreadId::new(7), "Add invoices", true);
    assert!(
        following.contains("push your branch and open a pull request"),
        "{following}"
    );
    let changed = brief_changed_prompt(&brief);
    assert!(changed.contains("## Goal\n\nShip billing"), "{changed}");
    let coordinator = coordinator_prompt(&brief);
    for part in [
        "never do the work yourself",
        "worker start --current",
        "coordinator update --current",
        "coordinator get --current",
        "Propose the first tasks",
    ] {
        assert!(coordinator.contains(part), "{part:?} in {coordinator}");
    }
    let unset = coordinator_prompt(&Brief {
        project: "app",
        goal: " ",
        instructions: "",
        memory: "",
    });
    for part in ["Not set yet.", "None.", "Nothing yet.", "Ask the user"] {
        assert!(unset.contains(part), "{part:?} in {unset}");
    }
}

fn check(name: &str, state: CheckState, url: Option<&str>) -> Check {
    Check {
        name: name.into(),
        workflow: None,
        state,
        detail: match state {
            CheckState::Failing => "failure".into(),
            _ => "success".into(),
        },
        url: url.map(String::from),
    }
}

fn pr(state: PrState) -> PrFeedback {
    PrFeedback {
        pr: PullRequest {
            number: 4,
            state,
            draft: false,
            url: None,
            checks: Some(CheckState::Failing),
            runs: vec![
                check("build", CheckState::Passing, None),
                check("test", CheckState::Failing, Some("https://ci/run/1")),
                check("lint", CheckState::Failing, None),
            ],
        },
        author: "op".into(),
        head: "c0ffee".into(),
        remarks: vec![
            Remark {
                id: "PRR_1".into(),
                author: "ana".into(),
                body: " Rename `total`. ".into(),
                approves: false,
            },
            Remark {
                id: "PRR_2".into(),
                author: "ana".into(),
                body: String::new(),
                approves: false,
            },
            Remark {
                id: "PRR_3".into(),
                author: "ana".into(),
                body: "LGTM".into(),
                approves: true,
            },
            Remark {
                id: "IC_4".into(),
                author: "op".into(),
                body: "Renamed it.".into(),
                approves: false,
            },
        ],
    }
}

/// An open PR asks for each failing check and each remark with words in
/// it — never an approval, nor what its own author wrote — each under a
/// key a later reading recognises; a rerun's new run is a new key. A
/// merged or closed PR asks nothing.
#[test]
fn an_open_prs_feedback_is_its_failing_checks_and_remarks() {
    let items = feedback(&pr(PrState::Open));
    let keys: Vec<&str> = items.iter().map(|item| item.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "check:test:https://ci/run/1",
            "check:lint:failure",
            "remark:PRR_1"
        ]
    );
    let prompt = feedback_prompt(4, &items.iter().collect::<Vec<_>>());
    for part in [
        "pull request #4",
        "- Check `test` failed (failure): https://ci/run/1",
        "- Check `lint` failed (failure).",
        "- @ana wrote: Rename `total`.",
    ] {
        assert!(prompt.contains(part), "{part:?} in {prompt}");
    }
    assert!(feedback(&pr(PrState::Merged)).is_empty());
    assert!(feedback(&pr(PrState::Closed)).is_empty());
}
