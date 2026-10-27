//! Binding a thread to a checkout, a branch or a worktree.
#[allow(unused_imports)]
use super::*;

/// Leg 1, through the draft flow (#29): cmd-shift-n drafts straight
/// onto "new worktree"; the first send bootstraps, and the Thread
/// really lands in a worktree of its own — isolation is the whole
/// point of the binding.
#[gpui::test]
fn a_thread_can_be_opened_in_its_own_worktree(cx: &mut TestAppContext) {
    // One scratch for both halves: `scratch` wipes its directory, so a
    // second call for the store would delete the repo just made.
    let base = scratch("worktree-key");
    let repo = repo_in(&base);
    let fake = Fake::default();
    let store = Store::open(base.join("threads")).unwrap();
    let core = Cockpit::new(store, Box::new(fake.clone()));
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-shift-n", NewWorktreeThread, None),
            KeyBinding::new("enter", Submit, None),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.aim_launch(&repo));
    tick(cx);

    cx.simulate_keystrokes("cmd-shift-n");
    view.read_with(cx, |view, _| {
        let draft = view.panes[view.focused()].draft().expect("a draft Pane");
        assert_eq!(
            *draft.binding.target(),
            DraftTarget::New,
            "aimed at a worktree"
        );
        assert!(fake.streams.borrow().is_empty(), "nothing spawned yet");
    });

    cx.simulate_input("set up the branch");
    cx.simulate_keystrokes("enter");

    view.read_with(cx, |view, _| {
        let thread = view.panes[view.focused()]
            .thread()
            .expect("the first send made a Thread of the draft");
        let binding = view
            .cockpit
            .thread(thread)
            .and_then(|open| open.workspace())
            .expect("a binding");
        assert!(
            matches!(binding, WorkspaceBinding::Worktree { .. }),
            "expected a worktree, got {binding:?}"
        );
        // And it is somewhere of its own, not the operator's checkout.
        assert_ne!(binding.cwd(), repo);
    });
}

/// `a` is option 2 at wall range too (`2 always`): a request that
/// offered no standing answer is allowed for this thread out loud — the
/// answer goes, and Ferrite keeps the thread rule it names.
#[gpui::test]
fn always_at_the_wall_keeps_a_thread_rule_when_nothing_was_offered(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("wall-always", 24);
    // Twenty-four Threads tile the board as one Group's wall.
    let group = group_all(&mut core);
    core.enter_group(group).unwrap();
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("a", Always, Some("Wall"))]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    // Wall range, as above: the "Wall" key context only exists there.
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    // `decision()` offers no standing answer.
    fake.streams.borrow()[3].send(decision("perm_04")).unwrap();
    tick(cx);
    let flagged = view.read_with(cx, |view, _| view.panes[3].thread().unwrap());

    cx.simulate_keystrokes("a");
    tick(cx);

    assert!(
        matches!(
            fake.answered.borrow().as_slice(),
            [(id, DecisionAnswer::Allow { .. })] if id == "perm_04"
        ),
        "{:?}",
        fake.answered.borrow()
    );
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.decisions.rules().len(),
            1,
            "the thread rule option 2 names is kept"
        );
        assert_eq!(view.decisions.rules()[0].thread, flagged);
    });
}

/// cmd-n drafts onto the checkout the operator is already in — the
/// plain case, beside cmd-shift-n's worktree — and the zero-keystroke
/// path holds: typing and sending with the band untouched binds to the
/// launch project's main checkout.
#[gpui::test]
fn a_new_thread_binds_to_the_main_checkout(cx: &mut TestAppContext) {
    let root = scratch("new-main");
    let repo = repo_in(&root);
    let fake = Fake::default();
    let store = Store::open(root.join("threads")).unwrap();
    let core = Cockpit::new(store, Box::new(fake));
    cx.update(|cx| {
        cx.bind_keys([
            KeyBinding::new("cmd-n", NewThread, None),
            KeyBinding::new("enter", Submit, None),
        ]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.aim_launch(&repo));
    tick(cx);

    cx.simulate_keystrokes("cmd-n");
    cx.simulate_input("hello");
    cx.simulate_keystrokes("enter");

    view.read_with(cx, |view, _| {
        let binding = view
            .cockpit
            .thread(view.panes[view.focused()].thread().unwrap())
            .and_then(|open| open.workspace())
            .expect("a binding");
        assert!(matches!(binding, WorkspaceBinding::Main { .. }));
        // The registry canonicalizes what it registers; the binding is
        // the registered root.
        assert_eq!(binding.cwd(), repo.canonicalize().unwrap());
    });
}

/// The branch selector names the checkout honestly, offers the branch
/// the repo was on as an existing branch, and the two ways to start
/// fresh: beside the operator or in an isolated worktree.
#[gpui::test]
fn the_branch_chip_offers_the_checkout_its_other_branch_and_both_new_branch_modes(
    cx: &mut TestAppContext,
) {
    let base = scratch("branch-menu");
    let repo = repo_in(&base);
    // Whatever this machine's `git init` called its first branch.
    let default = ferrite_core::workspace::checkout_branch(&repo).expect("a branch");
    let switched = std::process::Command::new("git")
        .args(["switch", "-q", "-c", "feature/checked-out"])
        .current_dir(&repo)
        .output()
        .expect("git switch");
    assert!(switched.status.success(), "git switch: {switched:?}");
    let fake = Fake::default();
    let store = Store::open(base.join("threads")).unwrap();
    let core = Cockpit::new(store, Box::new(fake));
    bind_band_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.aim_launch(&repo));
    tick(cx);

    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("enter");

    view.read_with(cx, |view, _| {
        let popover = view.popover.as_ref().expect("the branch popover is open");
        assert!(matches!(
            popover.kind,
            Kind::Band(pane::BandChip::Workspace)
        ));
        let rows: Vec<(&str, &str, bool)> = popover
            .rows
            .iter()
            .map(|row| (row.name.as_ref(), row.detail.as_ref(), row.active))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("feature/checked-out", "checked out", true),
                (default.as_str(), "existing branch", false),
                ("New branch", "in the project checkout", false),
                ("New branch + worktree", "in an isolated checkout", false),
            ]
        );
    });
}

/// AC (#29): the workspace chip is scoped to the chosen project's repo
/// alone. Driven entirely at the keyboard — tab to the project chip, ↵
/// opens its popover, arrows pick the other project — and the workspace
/// popover's rows follow: the second repo's registered worktree is a
/// row there, and vanishes when the project flips back.
#[gpui::test]
fn the_workspace_chip_scopes_to_the_chosen_project(cx: &mut TestAppContext) {
    let base = scratch("band-scope");
    let repo_one = repo_in(&base);
    // A second repo with a distinct leaf name, so the project rows can
    // be told apart.
    let repo_two = base.join("second");
    std::fs::create_dir_all(&repo_two).unwrap();
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "operator@example.invalid"],
        vec!["config", "user.name", "operator"],
        vec!["commit", "-q", "--allow-empty", "-m", "root"],
    ] {
        let out = std::process::Command::new("git")
            .args(&args)
            .current_dir(&repo_two)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }
    // Whatever this machine's `git init` called each first branch.
    let default_one = ferrite_core::workspace::checkout_branch(&repo_one).expect("a branch");
    let default_two = ferrite_core::workspace::checkout_branch(&repo_two).expect("a branch");
    let fake = Fake::default();
    let store = Store::open(base.join("threads")).unwrap();
    let core = Cockpit::new(store, Box::new(fake));
    bind_band_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| {
        view.aim_launch(&repo_one);
        // The second project holds one registered worktree — made
        // through the core's own bootstrap, then parked away.
        let thread = view
            .cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::NewWorktree {
                    repo: repo_two.clone(),
                },
            )
            .unwrap();
        view.cockpit.park(thread).unwrap();
        cx.notify();
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(
            view.panes[view.focused()].draft().is_some(),
            "an empty store launches as a draft Pane"
        );
    });

    // tab tab tab: provider, effort, then the project chip; ↵ opens
    // its popover (bare enter — no popover is up yet, so Submit routes
    // it to the focused chip).
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        let band = view.popover.as_ref().expect("the project popover is open");
        assert!(matches!(band.kind, Kind::Band(pane::BandChip::Project)));
        let labels: Vec<&str> = band.rows.iter().map(|row| row.name.as_ref()).collect();
        assert!(labels.contains(&"repo"), "rows: {labels:?}");
        assert!(labels.contains(&"second"), "rows: {labels:?}");
    });

    // The arrows start on the standing choice (repo); down reaches the
    // project registered after it — "second", the one with a worktree.
    cx.simulate_keystrokes("down");
    cx.simulate_keystrokes("enter");
    let chosen = view.read_with(cx, |view, _| {
        let draft = view.panes[view.focused()].draft().expect("still a draft");
        assert_eq!(
            *draft.binding.target(),
            DraftTarget::Main,
            "changing the project resets the workspace chip"
        );
        assert_eq!(
            view.cockpit
                .registry()
                .project(draft.binding.project())
                .unwrap()
                .title,
            "second"
        );
        draft.binding.project()
    });

    // tab: the workspace chip; ↵ opens its popover, scoped to the
    // chosen project only — its one worktree between the checkout
    // and the two ways to start fresh.
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        let band = view
            .popover
            .as_ref()
            .expect("the workspace popover is open");
        assert!(matches!(band.kind, Kind::Band(pane::BandChip::Workspace)));
        let labels: Vec<&str> = band.rows.iter().map(|row| row.name.as_ref()).collect();
        assert_eq!(
            labels,
            vec![
                default_two.as_str(),
                "ferrite/wt-1",
                "New branch",
                "New branch + worktree"
            ],
            "the chosen repo's checkout, its worktree, then the fresh starts"
        );
    });

    // Flip the project back and the workspace rows follow — no global
    // list anywhere.
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("tab"); // prompt → provider
    cx.simulate_keystrokes("tab"); // → effort
    cx.simulate_keystrokes("tab"); // → project
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("up");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("tab"); // → workspace
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        let band = view.popover.as_ref().expect("the workspace popover again");
        assert!(matches!(band.kind, Kind::Band(pane::BandChip::Workspace)));
        let draft = view.panes[view.focused()].draft().expect("still a draft");
        assert_ne!(
            draft.binding.project(),
            chosen,
            "the arrows flipped the project"
        );
        let labels: Vec<&str> = band.rows.iter().map(|row| row.name.as_ref()).collect();
        assert_eq!(
            labels,
            vec![default_one.as_str(), "New branch", "New branch + worktree"],
            "the other repo's own checkout, and nothing from the first"
        );
    });
}

/// The workspace chip lists what the repo actually holds: the checkout,
/// every other worktree git lists — hand-made ones included, named by
/// branch with the directory as detail — every other local branch, then
/// the fresh starts. A branch a worktree sits on is not offered twice.
#[gpui::test]
fn the_workspace_chip_lists_the_repos_other_worktrees_and_local_branches(cx: &mut TestAppContext) {
    let base = scratch("workspace-rows");
    let repo = repo_with_worktree_and_branches(&base);
    let fake = Fake::default();
    let store = Store::open(base.join("threads")).unwrap();
    let core = Cockpit::new(store, Box::new(fake));
    bind_band_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.aim_launch(&repo));
    tick(cx);

    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("enter");

    view.read_with(cx, |view, _| {
        let popover = view
            .popover
            .as_ref()
            .expect("the workspace popover is open");
        assert!(matches!(
            popover.kind,
            Kind::Band(pane::BandChip::Workspace)
        ));
        let rows: Vec<(&str, &str, bool)> = popover
            .rows
            .iter()
            .map(|row| (row.name.as_ref(), row.detail.as_ref(), row.active))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("trunk", "checked out", true),
                ("feature/wt", "worktree · wt-feature", false),
                ("main", "existing branch", false),
                ("topic/spare", "existing branch", false),
                ("New branch", "in the project checkout", false),
                ("New branch + worktree", "in an isolated checkout", false),
            ]
        );
    });
}

/// Picking a worktree row binds the draft to that worktree's path, and
/// the first send lands the Thread there — adopting the standing tree:
/// nothing minted, nothing registered, nothing created.
#[gpui::test]
fn picking_an_existing_worktree_binds_the_thread_to_its_path(cx: &mut TestAppContext) {
    let base = scratch("workspace-pick");
    let repo = repo_with_worktree_and_branches(&base);
    let worktree = std::fs::canonicalize(base.join("wt-feature")).unwrap();
    let fake = Fake::default();
    let store = Store::open(base.join("threads")).unwrap();
    let core = Cockpit::new(store, Box::new(fake));
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.aim_launch(&repo));
    tick(cx);

    // tab ×4 to the workspace chip, ↵ opens it; down reaches the
    // worktree row, ↵ picks it.
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("down");
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.popover.is_none(), "the pick closed the popover");
        let draft = view.panes[view.focused()].draft().expect("still a draft");
        match draft.binding.target() {
            DraftTarget::Existing { branch, path } => {
                assert_eq!(branch, "feature/wt");
                assert_eq!(std::fs::canonicalize(path).unwrap(), worktree);
            }
            other => panic!("the pick bound the worktree: {other:?}"),
        }
    });

    // escape returns to the prompt; the first send bootstraps.
    cx.simulate_keystrokes("escape");
    cx.simulate_input("run the tests");
    cx.simulate_keystrokes("enter");
    tick(cx);

    view.read_with(cx, |view, _| {
        let thread = view.panes[view.focused()]
            .thread()
            .expect("the draft became a Thread");
        let cwd = view
            .cockpit
            .thread(thread)
            .unwrap()
            .workspace()
            .unwrap()
            .cwd()
            .to_path_buf();
        assert_eq!(std::fs::canonicalize(cwd).unwrap(), worktree);
        let project = view.cockpit.registry().projects()[0].id;
        assert!(
            view.cockpit.registry().worktrees(project).is_empty(),
            "adoption minted nothing"
        );
        assert_eq!(
            ferrite_core::workspace::worktree_paths(&repo)
                .unwrap()
                .len(),
            2,
            "adoption created nothing"
        );
    });
}

/// `main` is a row only where the repo has one: a repo whose sole
/// branch is `trunk` offers the checkout and the fresh starts, and
/// nothing named `main` — a row that would only fail at first send.
#[gpui::test]
fn a_repo_without_a_main_branch_offers_no_main_row(cx: &mut TestAppContext) {
    let base = scratch("workspace-no-main");
    let repo = repo_in(&base);
    let renamed = std::process::Command::new("git")
        .args(["branch", "-M", "trunk"])
        .current_dir(&repo)
        .output()
        .expect("git branch");
    assert!(renamed.status.success(), "git branch: {renamed:?}");
    let fake = Fake::default();
    let store = Store::open(base.join("threads")).unwrap();
    let core = Cockpit::new(store, Box::new(fake));
    bind_band_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.aim_launch(&repo));
    tick(cx);

    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("tab");
    cx.simulate_keystrokes("enter");

    view.read_with(cx, |view, _| {
        let popover = view
            .popover
            .as_ref()
            .expect("the workspace popover is open");
        let rows: Vec<(&str, &str)> = popover
            .rows
            .iter()
            .map(|row| (row.name.as_ref(), row.detail.as_ref()))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("trunk", "checked out"),
                ("New branch", "in the project checkout"),
                ("New branch + worktree", "in an isolated checkout"),
            ]
        );
    });
}

/// AC (#29): the first send is the lock — the draft becomes a Thread,
/// the band is gone, the transcript opens with a Notice naming the
/// checkout, and the prompt follows it.
#[gpui::test]
fn the_first_send_bootstraps_the_thread_and_drops_the_band(cx: &mut TestAppContext) {
    let base = scratch("band-lock");
    let repo = repo_in(&base);
    let fake = Fake::default();
    let store = Store::open(base.join("threads")).unwrap();
    let core = Cockpit::new(store, Box::new(fake.clone()));
    bind_band_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.aim_launch(&repo));
    tick(cx);

    cx.simulate_input("run the tests");
    cx.simulate_keystrokes("enter");

    view.read_with(cx, |view, cx| {
        let pane = &view.panes[view.focused()];
        let thread = pane.thread().expect("the draft became a Thread");
        assert!(
            view.cockpit
                .thread(thread)
                .is_some_and(|open| open.first_prompt_sent()),
            "the first send armed the lock"
        );
        assert!(
            pane.composer.read(cx).is_empty(),
            "the sent prompt left the line"
        );
        let blocks = view.cockpit.thread(thread).unwrap().transcript().blocks();
        assert!(
            matches!(
                &blocks[0].body,
                Body::Notice(line) if line.starts_with("opened in ")
            ),
            "the first line names the checkout: {:?}",
            blocks[0].body
        );
        assert!(
            matches!(&blocks[1].body, Body::Prompt(line) if line == "run the tests"),
            "the prompt follows: {:?}",
            blocks[1].body
        );
    });
}

/// Background readiness must settle the submitted Pane, preserving any
/// later edits and the operator's current focus/picker elsewhere.
#[gpui::test]
fn delayed_bootstrap_preserves_edited_composer_focus_and_another_drafts_picker(
    cx: &mut TestAppContext,
) {
    use ferrite_core::session::SessionLifecycle;
    use std::sync::{Arc, Mutex};
    struct Ready {
        events: Receiver<SessionEvent>,
        sent: Arc<Mutex<Vec<String>>>,
    }
    impl Session for Ready {
        fn events(&self) -> &Receiver<SessionEvent> {
            &self.events
        }
        fn send(&mut self, text: &str) -> std::io::Result<()> {
            self.sent.lock().unwrap().push(text.into());
            Ok(())
        }
        fn interrupt(&mut self) -> std::io::Result<()> {
            Ok(())
        }
        fn respond_to_decision(&mut self, _: &str, _: DecisionAnswer) -> std::io::Result<()> {
            Ok(())
        }
    }
    struct Delayed {
        gates: std::collections::VecDeque<Receiver<()>>,
        sent: Arc<Mutex<Vec<String>>>,
    }
    impl Spawner for Delayed {
        fn spawn(
            &mut self,
            _: ferrite_core::cockpit::SpawnRequest,
        ) -> std::io::Result<Box<dyn Session>> {
            panic!("use start")
        }
        fn start(
            &mut self,
            _: ferrite_core::cockpit::SpawnRequest,
        ) -> std::io::Result<SessionLifecycle> {
            let gate = self.gates.pop_front().expect("one start per submission");
            let sent = self.sent.clone();
            SessionLifecycle::background(move || {
                gate.recv().unwrap();
                let (_, events) = mpsc::channel();
                Ok(Box::new(Ready { events, sent }) as Box<dyn Session + Send>)
            })
        }
    }
    let base = scratch("delayed-bootstrap-edits");
    let repo = repo_in(&base);
    let sent = Arc::new(Mutex::new(Vec::new()));
    let (first_tx, first_rx) = mpsc::channel();
    let (cancel_tx, cancel_rx) = mpsc::channel();
    let (escape_tx, escape_rx) = mpsc::channel();
    let core = Cockpit::new(
        Store::open(base.join("threads")).unwrap(),
        Box::new(Delayed {
            gates: [first_rx, cancel_rx, escape_rx].into(),
            sent: sent.clone(),
        }),
    );
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let (submitted, other) = view.update(cx, |view, cx| {
        view.aim_launch(&repo);
        let submitted = view.panes[view.focused()].identity.draft().unwrap();
        view.panes[view.focused()]
            .composer
            .update(cx, |composer, cx| composer.set("first".into(), cx));
        view.bootstrap_draft(cx);
        assert!(view.cockpit.draft_starting(submitted));
        view.panes[view.focused()]
            .composer
            .update(cx, |composer, cx| {
                composer.set("edited while starting".into(), cx)
            });
        view.open_draft(DraftTarget::Main, cx);
        let other = view.panes[view.focused()].identity.draft().unwrap();
        view.open_band_popover(pane::BandChip::Provider, cx);
        (submitted, other)
    });
    first_tx.send(()).unwrap();
    ferrite_core::doorbell::until("startup did not settle", || {
        view.update(cx, |view, cx| {
            view.pump(cx);
            !view.cockpit.draft_starting(submitted)
        })
    });
    view.update(cx, |view, cx| {
        assert_eq!(
            view.cockpit.roster().focused(),
            Some(PaneIdentity::Draft(other))
        );
        assert_eq!(
            view.popover.as_ref().unwrap().pane,
            PaneIdentity::Draft(other)
        );
        let first = view
            .panes
            .iter()
            .find(|pane| pane.thread().is_some())
            .unwrap();
        assert_eq!(first.composer.read(cx).text(), "edited while starting");
        assert_eq!(sent.lock().unwrap().as_slice(), ["first"]);
        // A changed draft choice cancels the submitted startup before
        // applying the new choice, so the old request can never send.
        view.popover = None;
        let composer = view.panes[view.focused()].composer.clone();
        composer.update(cx, |composer, cx| {
            composer.set("cancel on choice".into(), cx)
        });
        view.bootstrap_draft(cx);
        assert!(view.cockpit.draft_starting(other));
        view.pick_band(
            &BandChoice::Provider(ProviderChoice {
                provider: Provider::Codex,
                model: None,
            }),
            &composer,
            cx,
        );
        assert!(!view.cockpit.draft_starting(other));
        assert_eq!(composer.read(cx).text(), "cancel on choice");
    });
    cancel_tx.send(()).unwrap();
    view.update(cx, |view, cx| {
        view.bootstrap_draft(cx);
        assert!(view.cockpit.draft_starting(other));
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.interrupt(&Interrupt, window, cx)));
    view.read_with(cx, |view, cx| {
        assert!(!view.cockpit.draft_starting(other));
        assert_eq!(
            view.panes[view.focused()].composer.read(cx).text(),
            "cancel on choice"
        );
    });
    escape_tx.send(()).unwrap();
    assert_eq!(sent.lock().unwrap().as_slice(), ["first"]);
}

/// AC (#29): a failed bootstrap is a no-op with words — no Thread, the
/// Pane stays draft, the error shows at the band, and the prompt stays
/// in the Composer for the retry.
#[gpui::test]
fn a_failed_bootstrap_keeps_the_draft_and_the_prompt(cx: &mut TestAppContext) {
    let base = scratch("band-refused");
    let repo = repo_in(&base);
    let fake = Fake::default();
    let store = Store::open(base.join("threads")).unwrap();
    let core = Cockpit::new(store, Box::new(fake.clone()));
    bind_band_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.aim_launch(&repo));
    tick(cx);
    *fake.fail_send.borrow_mut() = true;

    cx.simulate_input("precious words");
    cx.simulate_keystrokes("enter");

    view.read_with(cx, |view, cx| {
        let pane = &view.panes[view.focused()];
        let draft = pane.draft().expect("the Pane stays draft");
        assert!(
            draft
                .error
                .as_ref()
                .is_some_and(|error| error.contains("stub refused first prompt")),
            "the failure shows its words: {:?}",
            draft.error
        );
        assert_eq!(
            pane.composer.read(cx).text(),
            "precious words",
            "the prompt is preserved for the retry"
        );
        assert_eq!(
            view.cockpit.parked().unwrap(),
            vec![],
            "no half-born Thread"
        );
    });

    // The retry goes through once the Session accepts the first prompt.
    *fake.fail_send.borrow_mut() = false;
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(
            view.panes[view.focused()].thread().is_some(),
            "the same draft bootstraps on the retry"
        );
    });
}

fn git_in(repo: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// The draft's workspace chip names its Project's checkout every frame,
/// yet no frame asks git: a Project the chip has not named yet is looked
/// up off the UI thread, and opening the menu re-reads the checkout
/// there too — the menu names the last answer (never a stand-in `main`)
/// until the fresh one lands.
#[gpui::test]
fn the_workspace_chip_never_asks_git_on_the_ui_thread(cx: &mut TestAppContext) {
    let spawned = ferrite_core::workspace::git_spawns_on_this_thread;
    let base = scratch("chip-git-off-ui");
    let repo = repo_in(&base);
    git_in(&repo, &["switch", "-q", "-c", "feature/drafted"]);
    let store = Store::open(base.join("threads")).unwrap();
    let core = Cockpit::new(store, Box::new(Fake::default()));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    let before = spawned();
    view.update(cx, |view, cx| {
        view.aim_launch(&repo);
        cx.notify();
    });
    assert_eq!(spawned() - before, 0, "a frame asked git for the chip");
    cx.run_until_parked();
    // The menu's first row is the checkout exactly as the chip names it.
    let checkout_row = |view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            let open = view.popover.as_ref().expect("the workspace menu is open");
            assert!(matches!(open.kind, Kind::Band(pane::BandChip::Workspace)));
            (
                open.rows[0].name.to_string(),
                open.rows[0].detail.to_string(),
            )
        })
    };
    let toggle_menu = |view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| {
            view.open_band_popover(pane::BandChip::Workspace, cx)
        })
    };
    toggle_menu(&view, cx);
    assert_eq!(
        checkout_row(&view, cx),
        ("feature/drafted".to_string(), "checked out".to_string())
    );
    cx.run_until_parked();
    toggle_menu(&view, cx);

    // The operator switches branches in a terminal, then opens the menu.
    git_in(&repo, &["switch", "-q", "-c", "feature/moved"]);
    let before = spawned();
    toggle_menu(&view, cx);
    assert_eq!(spawned() - before, 0, "opening the menu asked git");
    assert_eq!(
        checkout_row(&view, cx),
        ("feature/drafted".to_string(), "checked out".to_string()),
        "the last answer stands until the fresh one lands"
    );
    cx.run_until_parked();
    assert_eq!(
        checkout_row(&view, cx),
        ("feature/moved".to_string(), "checked out".to_string())
    );
}

/// A Main turn ending is a moment the checkout may have moved (#29) —
/// but the pump that notices never waits on git. The label is re-read
/// off the UI thread and follows the repo once the answer lands.
#[gpui::test]
fn a_turn_end_rereads_the_checkout_off_the_ui_thread(cx: &mut TestAppContext) {
    let spawned = ferrite_core::workspace::git_spawns_on_this_thread;
    let base = scratch("turn-end-git");
    let repo = repo_in(&base);
    let fake = Fake::default();
    let store = Store::open(base.join("threads")).unwrap();
    let mut core = Cockpit::new(store, Box::new(fake.clone()));
    let thread = core
        .open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: repo.clone(),
            },
        )
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "Moving the checkout".into(),
        })
        .unwrap();
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| view.cockpit.thread(thread).unwrap().busy()),
        "the premise: a turn is under way"
    );
    let branch = |view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.facts
                .get(thread)
                .and_then(|facts| facts.branch.as_ref())
                .map(|branch| branch.to_string())
        })
    };
    assert!(branch(&view, cx).is_some_and(|branch| branch != "agent-moved"));

    // The agent switches branches during its turn; then the turn ends.
    git_in(&repo, &["switch", "-q", "-c", "agent-moved"]);
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    let before = spawned();
    view.update(cx, |view, cx| view.pump(cx));
    assert_eq!(
        spawned() - before,
        0,
        "the turn end asked git on the UI thread"
    );
    assert!(
        view.read_with(cx, |view, _| !view.cockpit.thread(thread).unwrap().busy()),
        "the pump saw the turn end"
    );
    cx.run_until_parked();
    assert_eq!(branch(&view, cx).as_deref(), Some("agent-moved"));
}
