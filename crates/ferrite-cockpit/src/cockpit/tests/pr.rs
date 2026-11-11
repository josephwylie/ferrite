//! A Thread's PR reading (`crate::pr`): what it holds, when `gh` is
//! asked, and the two actions on it.
use super::*;
use crate::palette::{Act, Command};

fn asked(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<(ThreadId, bool)> {
    view.update(cx, |view, _| std::mem::take(&mut view.pr_asked))
}

/// The number of the PR the Thread's last reading names.
fn reading(
    view: &Entity<CockpitView>,
    thread: ThreadId,
    cx: &mut gpui::VisualTestContext,
) -> Option<u64> {
    view.read_with(cx, |view, _| view.facts.pr(thread).map(|pr| pr.number))
}

fn land(
    view: &Entity<CockpitView>,
    thread: ThreadId,
    reading: Option<ferrite_core::workspace::PullRequest>,
    cx: &mut gpui::VisualTestContext,
) {
    view.update(cx, |view, cx| view.pr_landed(thread, reading, cx));
}

fn run(view: &Entity<CockpitView>, command: Command, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_palette_act(Act::Command(command), false, window, cx)
        })
    });
}

/// Opening a Thread asks for its PR; the answer is its reading, kept while
/// the next one is out, and gone once `gh` has nothing to say.
#[gpui::test]
fn a_thread_keeps_its_prs_reading(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("pr-label", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.cockpit.threads()[0]);
    assert_eq!(asked(&view, cx), [(thread, false)], "opened, asked once");
    assert_eq!(reading(&view, thread, cx), None, "no PR known yet");

    land(&view, thread, Some(pr_with_checks()), cx);
    assert_eq!(reading(&view, thread, cx), Some(48));

    run(&view, Command::RefreshPr, cx);
    assert_eq!(
        asked(&view, cx),
        [(thread, true)],
        "a manual ask waits for nothing"
    );
    assert_eq!(reading(&view, thread, cx), Some(48), "kept while asking");

    land(&view, thread, None, cx);
    assert_eq!(reading(&view, thread, cx), None, "no gh, no PR: no reading");
}

/// Focusing a Thread asks for its PR, at most once per thirty seconds.
#[gpui::test]
fn focusing_a_thread_asks_at_most_every_thirty_seconds(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("pr-focus", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let threads = view.read_with(cx, |view, _| view.cockpit.threads());
    let mut opened = asked(&view, cx);
    opened.sort();
    assert_eq!(opened, [(threads[0], false), (threads[1], false)]);
    for thread in &threads {
        land(&view, *thread, None, cx);
    }
    let focus = |thread: ThreadId, cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| view.focus_thread(thread, cx));
        tick(cx);
    };

    focus(threads[0], cx);
    focus(threads[1], cx);
    assert!(asked(&view, cx).is_empty(), "both were asked just now");

    leap(cx, crate::pr::AUTO_EVERY);
    focus(threads[0], cx);
    assert_eq!(asked(&view, cx), [(threads[0], false)]);
}

/// The Thread's right-click menu refreshes its PR and, once one is known,
/// opens it; the palette offers both for the focused Thread.
#[gpui::test]
fn the_menu_and_the_palette_offer_the_pr_actions(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("pr-actions", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.cockpit.threads()[0]);
    let menu = |view: &CockpitView| -> Vec<String> {
        view.context_rows(MenuTarget::Thread(thread))
            .into_iter()
            .flatten()
            .map(|(item, _)| item.label.to_string())
            .collect()
    };
    let palette = |cx: &mut gpui::VisualTestContext| -> Vec<String> {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.palette_command_rows(window, cx)
                    .into_iter()
                    .map(|row| row.name.to_string())
                    .collect()
            })
        })
    };
    let refresh = "Refresh PR status".to_string();
    let open = "Open PR in browser".to_string();
    view.read_with(cx, |view, _| {
        assert!(menu(view).contains(&refresh));
        assert!(!menu(view).contains(&open), "no PR to open yet");
    });
    assert!(palette(cx).contains(&refresh));
    assert!(!palette(cx).contains(&open));

    land(&view, thread, Some(pr_with_checks()), cx);
    view.read_with(cx, |view, _| assert!(menu(view).contains(&open)));
    assert!(palette(cx).contains(&open));

    asked(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_menu_verb(MenuTarget::Thread(thread), MenuVerb::RefreshPr, window, cx)
        })
    });
    assert_eq!(asked(&view, cx), [(thread, true)]);
}

/// A push, a PR or background work handed back can move the workspace
/// behind the agent's own edits: its PR is read again at once, so Changes
/// follows (HD27). Any other command asks nothing.
#[gpui::test]
fn a_push_reads_the_pr_again(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("pr-after-push", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.cockpit.threads()[0]);
    land(&view, thread, None, cx);
    asked(&view, cx);
    let send = |event: SessionEvent| fake.streams.borrow()[0].send(event).unwrap();
    for (id, command) in [("ls", "ls -la"), ("push", "git push -u origin feature")] {
        send(SessionEvent::ToolStarted {
            id: id.into(),
            name: "Bash".into(),
            input: serde_json::json!({"command": command}),
        });
        send(SessionEvent::ToolCompleted {
            id: id.into(),
            output: String::new(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        });
        tick(cx);
        let expected: &[(ThreadId, bool)] = if id == "push" { &[(thread, true)] } else { &[] };
        assert_eq!(asked(&view, cx), expected, "after {command}");
    }
}

/// The branch's PR whole, as its Surface reads it.
fn snapshot(state: ferrite_core::protocol::WirePrState) -> ferrite_core::protocol::WirePr {
    use ferrite_core::protocol::*;
    WirePr {
        number: 48,
        title: "Nav rows keep their height".into(),
        url: "https://github.com/o/r/pull/48".into(),
        body: "Live rows reserve their status line.".into(),
        state,
        draft: false,
        head: "fix/nav".into(),
        base: "main".into(),
        merge: WireMerge::Clean,
        sync: WireSync::InSync,
        checks: vec![WireCheck {
            name: "fmt".into(),
            status: WireCheckStatus::Failure,
        }],
        comments: vec![WirePrComment {
            kind: WirePrCommentKind::Comment,
            author: "ada".into(),
            anchor: "comment".into(),
            body: "Looks right.".into(),
            snippet: None,
            created_at: "2026-11-10".into(),
            resolved: false,
            outdated: false,
            replies: Vec::new(),
        }],
        truncated: false,
        checks_truncated: false,
        comments_truncated: false,
    }
}

/// A window on a checkout whose branch has PR #48, read whole as `state`.
fn with_pr<'a>(
    name: &str,
    state: ferrite_core::protocol::WirePrState,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, ThreadId, Fake, &'a mut gpui::VisualTestContext) {
    let (core, fake, _checkout) = bound_cockpit(name, Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    land(&view, thread, Some(pr_with_checks()), cx);
    view.update(cx, |view, _| view.fixture_pr_snapshot(thread, snapshot(state)));
    tick(cx);
    (view, thread, fake, cx)
}

fn click(selector: &'static str, cx: &mut gpui::VisualTestContext) {
    let at = cx.debug_bounds(selector).unwrap_or_else(|| panic!("{selector}"));
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    tick(cx);
}

/// The PR row in Overview and the `+` menu's Pull request open the
/// branch's PR as one tab, shown, however often they are pressed.
#[gpui::test]
fn the_pr_row_and_the_plus_menu_open_one_pr_surface(cx: &mut TestAppContext) {
    use crate::cockpit::surfaces::SurfaceId;
    let (view, thread, _fake, cx) =
        with_pr("pr-surface-open", ferrite_core::protocol::WirePrState::Open, cx);
    let id = SurfaceId::PullRequest(48);
    opens_once(&view, thread, id.clone(), cx, |_, cx| {
        if cx.debug_bounds("panel-pr").is_none() {
            click("panel-tab-Overview", cx);
        }
        click("panel-pr", cx);
    });
    opens_once(&view, thread, id, cx, |view, cx| {
        view.update(cx, |view, cx| {
            view.run_opener(thread, crate::cockpit::panel::Opener::PullRequest, cx)
        });
    });
    assert!(cx.debug_bounds("panel-tab-PullRequest-48").is_some(), "its tab");
    assert!(cx.debug_bounds("pr-header").is_some(), "its body");
    assert!(cx.debug_bounds("pr-title").is_some(), "on Summary");
}

/// The views are a segmented group; the one chosen is the Surface's, kept
/// when another tab shows and the PR's comes back.
#[gpui::test]
fn the_pr_surface_keeps_its_view(cx: &mut TestAppContext) {
    use crate::cockpit::surfaces::{PrTab, SurfaceId};
    let (view, thread, _fake, cx) =
        with_pr("pr-surface-views", ferrite_core::protocol::WirePrState::Open, cx);
    view.update(cx, |view, cx| view.open_pr_surface(thread, cx));
    tick(cx);
    for tab in ["Summary", "Guide", "Timeline", "Code"] {
        assert!(debug_bounds(cx, format!("pr-tab-{tab}")).is_some(), "{tab}");
    }
    let segment = cx.debug_bounds("pr-tab-Summary").unwrap();
    assert_eq!(segment.size.height, px(ferrite_ui::theme::SURFACE_TAB_H));
    click("pr-tab-Timeline", cx);
    assert!(cx.debug_bounds("pr-timeline").is_some(), "the conversation");
    view.update(cx, |view, cx| view.upsert_surface(thread, SurfaceId::Overview, cx));
    tick(cx);
    view.update(cx, |view, cx| view.open_pr_surface(thread, cx));
    tick(cx);
    assert!(cx.debug_bounds("pr-timeline").is_some(), "kept on the Surface");
    let kept = view.read_with(cx, |view, _| view.panel.surfaces.get(thread).unwrap().pr_tab(48));
    assert_eq!(kept, PrTab::Timeline);
    click("pr-tab-Guide", cx);
    assert!(cx.debug_bounds("pr-write-guide").is_some(), "no guide yet: the way to ask");
}

/// An open PR's primary action is Merge, which asks once more before it
/// hands the merge to the agent; a merged one shows its pill instead, and
/// its `⋯` offers what a merged PR can still take.
#[gpui::test]
fn merge_asks_twice_and_a_merged_pr_shows_its_pill(cx: &mut TestAppContext) {
    use ferrite_core::protocol::WirePrState;
    let (view, thread, fake, cx) = with_pr("pr-surface-merge", WirePrState::Open, cx);
    view.update(cx, |view, cx| view.open_pr_surface(thread, cx));
    tick(cx);
    assert!(cx.debug_bounds("pr-pill").is_none());
    let before = fake.sent.borrow().len();
    click("pr-primary", cx);
    assert_eq!(fake.sent.borrow().len(), before, "armed, not sent");
    click("pr-primary", cx);
    let sent = fake.sent.borrow().clone();
    assert_eq!(sent.len(), before + 1, "{sent:?}");
    assert!(sent.last().unwrap().contains("gh pr merge 48"));

    view.update(cx, |view, cx| {
        view.fixture_pr_snapshot(thread, snapshot(WirePrState::Merged));
        cx.notify();
    });
    tick(cx);
    assert!(cx.debug_bounds("pr-pill").is_some(), "merged: the pill");
    assert!(cx.debug_bounds("pr-primary").is_none(), "in the action's place");
    let rows: Vec<String> = view.read_with(cx, |view, _| {
        view.context_rows(MenuTarget::PrMore(thread))
            .into_iter()
            .flatten()
            .map(|(item, _)| item.label.to_string())
            .collect()
    });
    for row in ["Explain this PR", "Revert changes", "Delete branch", "Copy link"] {
        assert!(rows.contains(&row.to_string()), "{row} in {rows:?}");
    }
    assert!(!rows.contains(&"Merge now".to_string()));
}

/// The nav's reading says the PR merged (the agent ran `gh pr merge`): the
/// open Surface follows, its pill in Merge's place.
#[gpui::test]
fn an_open_pr_surface_follows_a_merge(cx: &mut TestAppContext) {
    use ferrite_core::protocol::WirePrState;
    use ferrite_core::workspace::{PrState, PullRequest};
    let (view, thread, _fake, cx) = with_pr("pr-follows", WirePrState::Open, cx);
    view.update(cx, |view, cx| view.open_pr_surface(thread, cx));
    tick(cx);
    assert!(cx.debug_bounds("pr-primary").is_some(), "open: Merge");
    land(
        &view,
        thread,
        Some(PullRequest {
            state: PrState::Merged,
            ..pr_with_checks()
        }),
        cx,
    );
    tick(cx);
    assert!(cx.debug_bounds("pr-pill").is_some(), "merged: the pill");
    assert!(cx.debug_bounds("pr-primary").is_none(), "no Merge to press again");
    let rows: Vec<String> = view.read_with(cx, |view, _| {
        view.context_rows(MenuTarget::PrMore(thread))
            .into_iter()
            .flatten()
            .map(|(item, _)| item.label.to_string())
            .collect()
    });
    assert!(!rows.contains(&"Merge now".to_string()), "{rows:?}");
}

/// Merge arms on its first press; leaving its Surface (another tab, the
/// Panel hidden) disarms it, so one press later does not merge.
#[gpui::test]
fn an_armed_merge_does_not_outlive_its_surface(cx: &mut TestAppContext) {
    use crate::cockpit::surfaces::SurfaceId;
    use ferrite_core::protocol::WirePrState;
    let (view, thread, fake, cx) = with_pr("pr-armed", WirePrState::Open, cx);
    let back = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| view.activate_surface(thread, &SurfaceId::PullRequest(48), cx));
        tick(cx);
    };
    view.update(cx, |view, cx| view.open_pr_surface(thread, cx));
    tick(cx);
    let before = fake.sent.borrow().len();

    click("pr-primary", cx);
    view.update(cx, |view, cx| view.upsert_surface(thread, SurfaceId::Overview, cx));
    tick(cx);
    back(&view, cx);
    click("pr-primary", cx);
    assert_eq!(fake.sent.borrow().len(), before, "another tab disarmed it");

    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    tick(cx);
    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    tick(cx);
    back(&view, cx);
    click("pr-primary", cx);
    assert_eq!(fake.sent.borrow().len(), before, "hiding the Panel disarmed it");
}

/// A PR tab restored from `panel.json` for #47, on a branch whose PR is now
/// #48, goes once the branch's PR is known: no tab acts on another PR than
/// the one it names.
#[gpui::test]
fn a_restored_tab_for_another_pr_goes(cx: &mut TestAppContext) {
    use crate::cockpit::surfaces::SurfaceId;
    use ferrite_core::protocol::WirePrState;
    let (core, fake, _checkout) = bound_cockpit("pr-restored", Provider::Claude);
    let thread = core.threads()[0];
    let dir = scratch("pr-restored-prefs");
    std::fs::create_dir_all(&dir).unwrap();
    let mut surfaces = crate::cockpit::surfaces::Surfaces::default();
    surfaces.entry(thread, true).upsert(SurfaceId::PullRequest(47));
    surfaces.save(&dir).unwrap();
    let prefs = Preferences {
        dir,
        ..Preferences::ephemeral()
    };
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    land(&view, thread, Some(pr_with_checks()), cx);
    view.update(cx, |view, _| view.fixture_pr_snapshot(thread, snapshot(WirePrState::Open)));
    tick(cx);
    let tabs = view.read_with(cx, |view, _| view.panel.surfaces.get(thread).unwrap().tabs.clone());
    assert!(!tabs.contains(&SurfaceId::PullRequest(47)), "{tabs:?}");
    assert!(cx.debug_bounds("panel-tab-PullRequest-47").is_none());
    assert!(
        !fake.sent.borrow().iter().any(|sent| sent.contains("gh pr merge")),
        "nothing merged"
    );
}

/// The PR Surface's Code view is the branch's whole diff, whatever the
/// Changes Surface was left on, and choosing it leaves Changes as it was.
#[gpui::test]
fn a_prs_code_view_is_the_branchs_diff(cx: &mut TestAppContext) {
    use crate::cockpit::surfaces::SurfaceId;
    use crate::cockpit::tools::Diff;
    use ferrite_core::protocol::WirePrState;
    let (view, thread, _fake, cx) = with_pr("pr-code", WirePrState::Open, cx);
    view.update(cx, |view, cx| view.upsert_surface(thread, SurfaceId::Changes, cx));
    tick(cx);
    view.update(cx, |view, cx| view.open_pr_surface(thread, cx));
    tick(cx);
    click("pr-tab-Code", cx);
    let scope = |diff: Diff, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.changes_scope_label(diff))
    };
    assert_eq!(scope(Diff::PullRequest(thread), cx).as_deref(), Some("All changes"));
    assert_eq!(
        scope(Diff::Changes(thread), cx).as_deref(),
        Some("Uncommitted changes"),
        "the Changes Surface keeps its scope"
    );
    assert!(cx.debug_bounds("changes-scope").is_none(), "no scope to pick in Code");
}
