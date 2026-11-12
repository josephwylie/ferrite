//! Notices, toasts, sounds and system notifications.
#[allow(unused_imports)]
use super::*;

/// A rising allowance window toasts once per threshold crossed, and a
/// repeat of the same reading toasts nothing.
#[gpui::test]
fn a_window_crossing_its_threshold_toasts_once(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("allowance-warning", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let report = |used_fraction: f32| SessionEvent::RateLimits {
        five_hour: Some(ferrite_core::RateLimitWindow {
            used_fraction,
            resets_at: None,
        }),
        weekly: None,
    };
    let toasts = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            use gpui::component::WindowExt as _;
            window.notifications(cx).len()
        })
    };
    let before = toasts(cx);
    for used in [0.5, 0.82, 0.83] {
        fake.streams.borrow()[0].send(report(used)).unwrap();
        tick(cx);
    }
    assert_eq!(toasts(cx), before + 1, "80% crossed once");
    fake.streams.borrow()[0].send(report(0.96)).unwrap();
    tick(cx);
    assert_eq!(toasts(cx), before + 2, "then 95%");
}

#[gpui::test]
fn cmd_i_holds_the_list_until_escape_gives_the_keys_back(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("needs-you-escape", 1);
    core.send(core.threads()[0], "keep working".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    cx.simulate_keystrokes("cmd-i");
    tick(cx);
    // An unrelated render must leave the Popover's dismiss handler focused.
    view.update(cx, |_, cx| cx.notify());
    tick(cx);
    view.read_with(cx, |view, _| assert!(view.nav_keys_held));
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(
        *fake.interrupts.borrow(),
        0,
        "Escape only gives the list's keys back"
    );
    view.read_with(cx, |view, _| assert!(!view.nav_keys_held));
    cx.simulate_keystrokes("escape");
    assert_eq!(
        *fake.interrupts.borrow(),
        1,
        "Composer regains its interrupt key"
    );
}

#[gpui::test]
fn the_list_scrolls_to_and_opens_its_last_card(cx: &mut TestAppContext) {
    // One finish per Thread: a Thread's repeats fold into one row, so
    // a long list is many Threads.
    const THREADS: usize = 24;
    let (mut core, fake) = cockpit("needs-you-scroll", THREADS);
    for index in 0..THREADS {
        core.send(core.threads()[index], "go".into());
        fake.streams.borrow()[index]
            .send(SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
        core.pump();
    }
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(1080.)));
    tick(cx);
    let active: Vec<ThreadId> = view.read_with(cx, |view, _| {
        view.nav_state().active.iter().map(|row| row.thread).collect()
    });
    assert_eq!(active.len(), THREADS, "one card per Thread");
    let card = |thread: ThreadId| -> &'static str {
        format!("nav-thread-{}", thread.get()).leak()
    };
    let first = cx.debug_bounds(card(active[0])).unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: first.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-10000.))),
        modifiers: gpui::Modifiers::none(),
        touch_phase: gpui::TouchPhase::default(),
    });
    cx.run_until_parked();
    let last_thread = active[THREADS - 1];
    let last = cx.debug_bounds(card(last_thread)).unwrap();
    assert!(
        last.bottom() < px(1080.),
        "the last card must scroll into the window: {last:?}"
    );
    cx.simulate_click(last.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.focused_thread(), Some(last_thread));
    });
}

/// A Thread that finishes off the screen needs you: an unread
/// Notice until the operator lands on it — which reads it. The nav is
/// open, so nothing toasts (C8): the tree says it.
#[gpui::test]
fn a_finished_thread_needs_you_until_it_is_seen(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("finished-needs-you", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let threads = view.read_with(cx, |view, _| view.cockpit.threads());
    view.update(cx, |view, cx| view.focus_thread(threads[0], cx));
    fake.streams.borrow()[1]
        .send(SessionEvent::TextDelta {
            text: "done".into(),
        })
        .unwrap();
    fake.streams.borrow()[1]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        let notifications = view.cockpit.notifications();
        assert_eq!(notifications.unread(), 1);
        assert!(notifications.attention(threads[1]), "off the screen");
        assert!(!notifications.attention(threads[0]));
        assert!(
            view.needs_you.toasts().is_empty(),
            "with the nav open, a finish toasts nothing"
        );
    });

    view.update(cx, |view, cx| view.focus_thread(threads[1], cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        let notifications = view.cockpit.notifications();
        assert_eq!(notifications.unread(), 0, "landing on the Pane reads it");
        assert!(!notifications.attention(threads[1]));
        assert_eq!(
            notifications.notices().count(),
            1,
            "the Inbox still lists it"
        );
    });
}

/// The tree names the Thread the toast was about: a finish off the
/// screen is **done** — the teal check, the word `done` — and landing
/// on its Pane, which shows it, reads it back to idle. A finish on the
/// screen is seen as it lands, unless the window is away: then it is
/// done until the window comes back.
#[gpui::test]
fn a_finish_is_done_until_it_is_seen(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("finished-nav-row", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let threads = view.read_with(cx, |view, _| view.cockpit.threads());
    view.update(cx, |view, cx| view.focus_thread(threads[0], cx));
    let finish = |at: usize| {
        fake.streams.borrow()[at]
            .send(SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
    };
    finish(1);
    tick(cx);
    view.read_with(cx, |view, _| {
        let finished = view.thread_row(threads[1]);
        assert_eq!(finished.status, nav::RowStatus::Done, "off the screen");
        assert_eq!(finished.tail, nav::NavTail::Done);
        assert_eq!(
            finished.status.ink(),
            ferrite_ui::theme::INK_SUBTLE,
            "a dim check, never ochre"
        );
        assert_eq!(view.thread_row(threads[0]).status, nav::RowStatus::Idle);
    });

    view.update(cx, |view, cx| view.land_on_thread(threads[1], cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        let read = view.thread_row(threads[1]);
        assert_eq!(read.status, nav::RowStatus::Idle, "seen on its Pane");
        assert_ne!(read.tail, nav::NavTail::Done);
    });

    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.deactivate_window();
    finish(1);
    tick(cx);
    let status = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.thread_row(threads[1]).status)
    };
    assert_eq!(
        status(&view, cx),
        nav::RowStatus::Done,
        "on the screen, but the window is away"
    );
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    assert_eq!(status(&view, cx), nav::RowStatus::Idle, "seen on return");
}

/// A window that opens without focus has seen nothing yet: a finish on
/// its one Pane is done until the window is focused.
#[gpui::test]
fn a_window_opened_unfocused_sees_nothing_until_focused(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("finished-unfocused", 1);
    let (view, cx) = add_unfocused_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    let status = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.thread_row(view.cockpit.threads()[0]).status
        })
    };
    assert_eq!(status(&view, cx), nav::RowStatus::Done, "not yet seen");
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    assert_eq!(status(&view, cx), nav::RowStatus::Idle, "seen once focused");
}

/// A finish on the Pane the operator is already on is read as it is
/// born: it joins the Inbox, but nothing shouts.
#[gpui::test]
fn a_finish_on_the_focused_pane_is_read_at_once_and_never_toasts(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("finished-focused", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        let notifications = view.cockpit.notifications();
        assert_eq!(notifications.notices().count(), 1);
        assert_eq!(notifications.unread(), 0);
    });
    view.read_with(cx, |view, _| {
        assert!(
            view.needs_you.toasts().is_empty(),
            "no toast for the Pane in view"
        );
    });
}

/// Opening a Notice — a toast, or a row of the Inbox — lands the
/// operator on its Pane and preserves Inbox mode.
#[gpui::test]
fn opening_a_notice_lands_on_its_pane_and_reads_it(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("open-notice", 4);
    let group = group_all(&mut core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    fake.streams.borrow()[2]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Error("rate limited".into()),
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    let id = view.read_with(cx, |view, _| {
        assert_eq!(view.focused(), 0);
        view.cockpit.notifications().newest().unwrap()
    });
    view.update(cx, |view, cx| view.notice_verb(Verb::Open(id), cx));
    view.read_with(cx, |view, _| {
        assert_eq!(view.focused(), 2, "focus landed on the finished Pane");
        assert!(view.cockpit.notifications().get(id).unwrap().read);
    });
    view.update(cx, |view, cx| view.notice_verb(Verb::Dismiss(id), cx));
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.notifications().notices().count(), 0);
    });
}

/// FL-18, ADR 0017: a request from a Thread off the board toasts once,
/// a second after it arrives, with the nav open or folded; pumping
/// never duplicates it, one cancelled within the second never toasts,
/// and a newer request from the same Thread replaces it.
#[gpui::test]
fn contract_request_attention_toasts_once_and_cancellation_removes_it(cx: &mut TestAppContext) {
    use ferrite_core::activity::ActivityEvent;
    // Solo on the first Thread: the second is off the board.
    let (core, fake) = cockpit("request-toast-contract", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let toasts = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.needs_you.toasts().len())
    };
    let request = |name: &str| SessionEvent::DecisionRequested {
        decision: ferrite_core::Decision {
            delivery: Default::default(),
            kind: Default::default(),
            policy: Default::default(),
            id: name.into(),
            tool_use_id: name.into(),
            tool_name: name.into(),
            description: "Needs your input".into(),
            input: serde_json::json!({}),
            suggestions: vec![],
        },
    };
    let cancelled =
        |name: &str| SessionEvent::Activity(ActivityEvent::DecisionCancelled { id: name.into() });
    let off = view.read_with(cx, |view, _| {
        view.cockpit
            .threads()
            .into_iter()
            .position(|thread| Some(thread) != view.focused_thread())
            .unwrap()
    });
    fake.streams.borrow()[off].send(request("Early")).unwrap();
    tick(cx);
    assert_eq!(toasts(cx), 0, "it waits its second");
    ring(cx);
    assert_eq!(toasts(cx), 1, "with the nav open, a request still toasts");
    tick(cx);
    assert_eq!(toasts(cx), 1, "pumping must not duplicate a request toast");
    fake.streams.borrow()[off].send(cancelled("Early")).unwrap();
    tick(cx);
    assert_eq!(toasts(cx), 0, "a cancelled request takes its toast down");

    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    fake.streams.borrow()[off].send(request("Brief")).unwrap();
    tick(cx);
    fake.streams.borrow()[off].send(cancelled("Brief")).unwrap();
    ring(cx);
    assert_eq!(toasts(cx), 0, "gone within its second, it never toasts");

    for name in ["AskUserQuestion", "Bash"] {
        fake.streams.borrow()[off].send(request(name)).unwrap();
    }
    tick(cx);
    ring(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.notifications().unread(), 2);
        assert_eq!(view.needs_you.toasts().len(), 1, "one toast per Thread");
    });
    for name in ["AskUserQuestion", "Bash"] {
        fake.streams.borrow()[off].send(cancelled(name)).unwrap();
    }
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.notifications().unread(), 0)
    });
    assert_eq!(toasts(cx), 0, "cancelled requests leave no stale toast");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.chimed,
            [Sound::NeedsYou, Sound::NeedsYou],
            "each toast rang once; the brief request never did"
        );
    });
}

/// ADR 0017: a toast goes on its own, a request's after 8s.
#[gpui::test]
fn a_request_toast_expires(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("request-toast-expiry", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let off = view.read_with(cx, |view, _| {
        view.cockpit
            .threads()
            .into_iter()
            .position(|thread| Some(thread) != view.focused_thread())
            .unwrap()
    });
    fake.streams.borrow()[off].send(decision("waits")).unwrap();
    tick(cx);
    ring(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.needs_you.toasts().len()),
        1
    );
    cx.executor()
        .advance_clock(crate::notifications::NEEDS_YOU_TOAST);
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.needs_you.toasts().is_empty(), "it went on its own");
        assert_eq!(
            view.cockpit.notifications().unread(),
            1,
            "the request still waits in the Inbox"
        );
    });
}

/// ADR 0005's update (focus-notify's rule): with the window frontmost, a
/// request from a Thread whose Pane is not the focused one posts a
/// system notification beside its toast.
#[gpui::test]
fn a_request_off_the_board_posts_while_the_window_is_frontmost(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("system-notification-frontmost", 2);
    cx.update(|cx| cx.set_app_identity("com.github.josephwylie.ferrite", "Ferrite"));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let threads = view.read_with(cx, |view, _| view.cockpit.threads());
    let off = view.read_with(cx, |view, _| {
        threads
            .iter()
            .position(|thread| Some(*thread) != view.focused_thread())
            .unwrap()
    });
    fake.streams.borrow()[off].send(decision("waits")).unwrap();
    tick(cx);
    ring(cx);
    view.read_with(cx, |view, _| {
        assert!(view.cockpit.window_active());
        assert_eq!(view.needs_you.toasts().len(), 1);
    });
    assert_eq!(
        cx.delivered_system_notifications()
            .iter()
            .map(|notification| notification.tag.to_string())
            .collect::<Vec<_>>(),
        [system_tag(&crate::machines::MachineKey(None), threads[off])]
    );
}

/// ADR 0017: with the window in the background, a finish on a Pane
/// left on screen is news. It chimes, toasts and posts a system
/// notification; clicking that brings the window forward and lands on
/// the Thread, and everything on screen is read.
#[gpui::test]
fn away_from_the_window_a_finish_posts_a_system_notification(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("system-notification", 2);
    let group = group_all(&mut core);
    cx.update(|cx| cx.set_app_identity("com.github.josephwylie.ferrite", "Ferrite"));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    let threads = view.read_with(cx, |view, _| view.cockpit.threads());
    let home = crate::machines::MachineKey(None);
    cx.deactivate_window();
    for at in [0, 1] {
        fake.streams.borrow()[at]
            .send(SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
    }
    tick(cx);
    ring(cx);
    let tags = |cx: &mut gpui::VisualTestContext| {
        cx.delivered_system_notifications()
            .iter()
            .map(|notification| notification.tag.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        tags(cx),
        [system_tag(&home, threads[0]), system_tag(&home, threads[1])],
        "both Panes are on the board, but nobody is looking"
    );
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.chimed,
            [Sound::Done],
            "ringing together, they chime once"
        );
        assert_eq!(view.needs_you.toasts().len(), 2);
    });

    cx.simulate_system_notification_response(gpui::SystemNotificationResponse {
        tag: system_tag(&home, threads[1]).into(),
        action_id: None,
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.focused(), 1, "the click landed on its Thread");
        assert!(!view.cockpit.notifications().attention(threads[1]));
        assert!(view.needs_you.toasts().is_empty(), "both are in view now");
    });
    assert!(
        tags(cx).is_empty(),
        "read, they leave the notification centre"
    );
}

/// ADR 0017: a request waiting while the window is away posts a system
/// notification too; clicking it lands on the request, and once read
/// it leaves the notification centre.
#[gpui::test]
fn clicking_a_request_s_system_notification_opens_it(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("system-notification-request", 2);
    let group = group_all(&mut core);
    cx.update(|cx| cx.set_app_identity("com.github.josephwylie.ferrite", "Ferrite"));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    let threads = view.read_with(cx, |view, _| view.cockpit.threads());
    let home = crate::machines::MachineKey(None);
    cx.deactivate_window();
    fake.streams.borrow()[1].send(decision("away")).unwrap();
    tick(cx);
    ring(cx);
    let tags = |cx: &mut gpui::VisualTestContext| {
        cx.delivered_system_notifications()
            .iter()
            .map(|notification| notification.tag.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(tags(cx), [system_tag(&home, threads[1])]);
    view.read_with(cx, |view, _| assert_eq!(view.chimed, [Sound::NeedsYou]));

    cx.simulate_system_notification_response(gpui::SystemNotificationResponse {
        tag: system_tag(&home, threads[1]).into(),
        action_id: None,
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.focused(), 1, "the click landed on its Thread");
        let request = view.cockpit.notifications().decisions().next().unwrap();
        assert!(request.read);
        assert!(view.needs_you.toasts().is_empty());
    });
    assert!(
        tags(cx).is_empty(),
        "read, it leaves the notification centre"
    );
}

/// ADR 0017: alerts ringing together chime once, needs-you over done;
/// and with the window focused, a request on a Pane that is on the
/// board but not focused rings nothing — it is seen where it is asked.
#[gpui::test]
fn alerts_chime_once_and_a_request_in_view_rings_nothing(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("alerts-together", 2);
    let group = group_all(&mut core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    cx.deactivate_window();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    fake.streams.borrow()[1].send(decision("away")).unwrap();
    tick(cx);
    ring(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.chimed, [Sound::NeedsYou]);
        assert_eq!(view.needs_you.toasts().len(), 2);
    });

    cx.update(|window, _| window.activate_window());
    view.update(cx, |view, _| view.chimed.clear());
    tick(cx);
    let unfocused = view.read_with(cx, |view, _| {
        assert!(view.needs_you.toasts().is_empty(), "both are in view now");
        view.cockpit
            .threads()
            .into_iter()
            .position(|thread| Some(thread) != view.focused_thread())
            .unwrap()
    });
    fake.streams.borrow()[unfocused]
        .send(decision("here"))
        .unwrap();
    tick(cx);
    ring(cx);
    view.read_with(cx, |view, _| {
        assert!(view.needs_you.toasts().is_empty());
        assert!(view.chimed.is_empty());
    });
}

/// The Notifications page: sounds on and off, a provider's sounds, a
/// chime played and a chosen file put back to the built-in, system
/// notifications — each press a saved setting.
#[gpui::test]
fn the_notifications_page_sets_every_alert_setting(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("notifications-page", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    view.update(cx, |view, cx| {
        view.settings_open = true;
        view.settings_page = settings::PageKey::Notifications;
        cx.notify();
    });
    tick(cx);
    let click = |selector: &'static str, cx: &mut gpui::VisualTestContext| {
        let at = cx.debug_bounds(selector).expect(selector).center();
        cx.simulate_click(at, gpui::Modifiers::none());
        tick(cx);
    };
    let settings = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.prefs.settings.clone())
    };
    click("settings-codex-sounds", cx);
    assert!(!settings(cx).sounds_for(Provider::Codex));
    assert!(settings(cx).sounds_for(Provider::Claude));
    click("settings-sounds", cx);
    assert!(!settings(cx).sounds_for(Provider::Claude));
    click("settings-claude-sounds", cx);
    assert!(
        !settings(cx).sounds && !settings(cx).provider_sounds(Provider::Claude),
        "a provider's switch is its own: sounds stay off"
    );
    click("settings-sounds", cx);
    assert!(!settings(cx).sounds_for(Provider::Claude));
    assert!(!settings(cx).sounds_for(Provider::Codex));
    click("settings-claude-sounds", cx);
    assert!(settings(cx).sounds_for(Provider::Claude));
    click("settings-system-notifications", cx);
    assert!(!settings(cx).system_notifications);

    click("settings-needs-you-sound-0", cx);
    view.read_with(cx, |view, _| assert_eq!(view.chimed, [Sound::NeedsYou]));
    view.update(cx, |view, cx| {
        view.change_settings(
            |s| s.set_sound_file(Sound::Done, Some("/sounds/bell.aiff".into())),
            cx,
        )
    });
    tick(cx);
    let shown = cx
        .debug_bounds("settings-fact-Finished sound")
        .expect("the chosen file's name");
    assert!(shown.size.width > px(0.));
    click("settings-done-sound-2", cx);
    assert_eq!(settings(cx).sound_file(Sound::Done), None, "built-in again");
    let saved = view.read_with(cx, |view, _| {
        ferrite_core::settings::Settings::load(&view.prefs.dir).unwrap()
    });
    assert_eq!(saved, settings(cx), "every press was saved");
}

/// Settings decide what an alert may do: sounds off, or this
/// provider muted, keeps it quiet; system notifications off posts
/// nothing. The toast stands regardless.
#[gpui::test]
fn settings_silence_sounds_and_system_notifications(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("alert-settings", 2);
    cx.update(|cx| cx.set_app_identity("com.github.josephwylie.ferrite", "Ferrite"));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| {
        view.prefs
            .settings
            .set_provider_sounds(Provider::Claude, false);
        view.prefs.settings.system_notifications = false;
    });
    cx.deactivate_window();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    ring(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.needs_you.toasts().len(), 1);
        assert!(view.chimed.is_empty(), "Claude is muted");
    });
    assert!(cx.delivered_system_notifications().is_empty());
}

#[gpui::test]
fn contract_request_notice_opens_its_owning_child(cx: &mut TestAppContext) {
    use ferrite_core::activity::{ActivityEvent, AgentInfo, AgentKey, Subject};
    let (mut core, fake) = cockpit("request-child-contract", 2);
    let group = group_all(&mut core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    let key = AgentKey::new(Provider::Claude, "native", "child");
    let mut info = AgentInfo::new(key.clone());
    info.parent = Some(Subject::Main);
    fake.streams.borrow()[1]
        .send(SessionEvent::Activity(ActivityEvent::Discovered(info)))
        .unwrap();
    fake.streams.borrow()[1]
        .send(SessionEvent::Activity(ActivityEvent::Decision {
            subject: Some(Subject::Subagent(key.clone())),
            decision: ferrite_core::Decision {
                delivery: Default::default(),
                kind: Default::default(),
                policy: Default::default(),
                id: "child-request".into(),
                tool_use_id: "tool".into(),
                tool_name: "Bash".into(),
                description: "Allow command".into(),
                input: serde_json::json!({}),
                suggestions: vec![],
            },
        }))
        .unwrap();
    tick(cx);
    let id = view.read_with(cx, |view, _| {
        view.cockpit
            .notifications()
            .decisions()
            .next()
            .unwrap()
            .id
            .clone()
    });
    view.update(cx, |view, cx| {
        view.notice_verb(Verb::OpenDecision(id.clone()), cx)
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.focused(), 1);
        assert_eq!(
            view.panes[view.focused()].selected,
            Subject::Subagent(key.clone())
        );
        assert!(view.cockpit.notifications().decision(&id).unwrap().read);
        assert!(!view.nav_keys_held);
    });
}

#[gpui::test]
fn inbox_tracks_open_threads_independently_of_notices(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("inbox-lifecycle", 2);
    let threads = core.threads();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let inbox = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.needs_you_rows(cx)
                .iter()
                .map(|row| row.thread)
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(
        inbox(cx).len(),
        2,
        "idle open Threads need handling without Notices"
    );
    view.update(cx, |view, _| view.facts.fixture_forget_last_used(threads[0]));
    assert_eq!(inbox(cx).len(), 2, "an unknown age cannot hide Inbox work");
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "working".into(),
        })
        .unwrap();
    tick(cx);
    assert_eq!(inbox(cx), vec![threads[1]]);
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    view.update(cx, |view, cx| {
        view.focus_thread(threads[0], cx);
        view.notice_verb(Verb::MarkAllRead, cx);
        view.focus_thread(threads[1], cx);
    });
    tick(cx);
    assert_eq!(inbox(cx).len(), 2, "reading and leaving is not finishing");
    view.update(cx, |view, cx| view.park_listed(&threads, cx));
    tick(cx);
    assert!(inbox(cx).is_empty(), "Park finishes handling");
}

#[gpui::test]
fn the_list_empty_over_working_then_a_decision_parks_from_the_keyboard(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("inbox-empty-working", 3);
    let threads = core.threads();
    for thread in &threads {
        core.send(*thread, "go".into());
    }
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    cx.simulate_keystrokes("cmd-i");
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.needs_you_rows(cx).is_empty());
        assert_eq!(view.nav_state().working.len(), 3);
    });
    assert!(cx.debug_bounds("nav-empty").is_some());
    assert!(cx.debug_bounds("nav-shelf-working").is_some());
    fake.streams.borrow()[1]
        .send(decision("inbox-approval"))
        .unwrap();
    tick(cx);
    view.update(cx, |view, cx| {
        let rows = view.needs_you_rows(cx);
        assert_eq!(rows.len(), 1, "Decision takes precedence over work");
        assert_eq!(rows[0].thread, threads[1]);
        if let RowTarget::Decision(id) = &rows[0].target {
            view.notice_verb(Verb::DismissDecision(id.clone()), cx);
        }
    });
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, cx| view.needs_you_rows(cx).len()),
        1
    );
    // Park the only Active card; the two agents still working remain.
    cx.simulate_keystrokes("home down cmd-w");
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.needs_you_rows(cx).is_empty());
        assert_eq!(view.nav_state().working.len(), 2);
        assert_eq!(view.cockpit.parked().unwrap(), vec![threads[1]]);
    });
    assert!(cx.debug_bounds("nav-empty").is_some());
    assert!(cx.debug_bounds("nav-shelf-working").is_some());
}

/// ⌘W while the list holds the keys parks the cursor's card, and the
/// cursor goes with its Thread into the folded Parked shelf, so the next
/// move starts over from the top.
#[gpui::test]
fn the_keyboard_parks_the_cursors_card_and_the_cursor_follows_it(cx: &mut TestAppContext) {
    let (core, _) = cockpit("inbox-park-cursor", 3);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let active: Vec<ThreadId> = view.read_with(cx, |view, _| {
        view.nav_state().active.iter().map(|row| row.thread).collect()
    });
    assert_eq!(active.len(), 3);
    cx.simulate_keystrokes("cmd-i down cmd-w");
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.cockpit.parked().unwrap(), vec![active[1]]);
        assert_eq!(view.needs_you_rows(cx).len(), 2);
        assert_eq!(
            view.nav_cursor.as_ref().map(|cursor| cursor.item),
            Some(nav::NavItem::Thread(active[1])),
            "the cursor follows its Thread into the Parked shelf"
        );
    });
    cx.simulate_keystrokes("up down cmd-w");
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, cx| view.needs_you_rows(cx).len()),
        1
    );
    cx.simulate_keystrokes("home down cmd-w");
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.needs_you_rows(cx).is_empty());
        assert_eq!(view.cockpit.parked().unwrap().len(), 3);
    });
}

#[gpui::test]
fn inbox_followup_returns_to_work_and_repeated_finishes_stay_one_row(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("inbox-followup", 1);
    cx.update(|cx| cx.bind_keys([KeyBinding::new("enter", Submit, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    for _ in 0..2 {
        cx.simulate_input("continue");
        cx.simulate_keystrokes("enter");
        tick(cx);
        assert!(view.read_with(cx, |view, cx| view.needs_you_rows(cx).is_empty()));
        fake.streams.borrow()[0]
            .send(SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
        tick(cx);
        assert_eq!(
            view.read_with(cx, |view, cx| view.needs_you_rows(cx).len()),
            1
        );
    }
    view.update(cx, |view, cx| {
        let id = view.cockpit.notifications().notices().next().unwrap().id;
        view.notice_verb(Verb::Dismiss(id), cx);
        assert_eq!(
            view.needs_you_rows(cx).len(),
            1,
            "dismissal is notification housekeeping"
        );
    });
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| view.needs_you.toasts().is_empty()),
        "watched results never alert"
    );
}

#[gpui::test]
fn stopped_failures_and_interruptions_remain_inbox(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("inbox-stopped", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    for (index, outcome) in [
        ferrite_core::TurnOutcome::Interrupted,
        ferrite_core::TurnOutcome::Error("provider failed".into()),
    ]
    .into_iter()
    .enumerate()
    {
        fake.streams.borrow()[index]
            .send(SessionEvent::TextDelta {
                text: "working".into(),
            })
            .unwrap();
        fake.streams.borrow()[index]
            .send(SessionEvent::TurnEnded {
                outcome,
                cost_usd: None,
            })
            .unwrap();
    }
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.needs_you_rows(cx).len(), 2);
        assert!(view.nav_state().working.is_empty());
    });
}

#[gpui::test]
fn main_completion_waits_for_working_descendants_before_inbox(cx: &mut TestAppContext) {
    use ferrite_core::activity::{ActivityEvent, AgentInfo, AgentKey, AgentStatus, Subject};
    let (core, fake) = cockpit("inbox-descendants", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let key = AgentKey::new(Provider::Claude, "native", "inbox-child");
    let mut info = AgentInfo::new(key.clone());
    info.parent = Some(Subject::Main);
    for event in [
        ActivityEvent::Discovered(info),
        ActivityEvent::Status {
            key: key.clone(),
            state: AgentStatus::Working,
        },
    ] {
        fake.streams.borrow()[0]
            .send(SessionEvent::Activity(event))
            .unwrap();
    }
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    assert!(view.read_with(cx, |view, cx| view.needs_you_rows(cx).is_empty()));
    fake.streams.borrow()[0]
        .send(SessionEvent::Activity(ActivityEvent::Status {
            key,
            state: AgentStatus::Idle,
        }))
        .unwrap();
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, cx| view.needs_you_rows(cx).len()),
        1
    );
}

#[gpui::test]
fn inbox_completion_keeps_keyboard_park_on_the_selected_thread(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("inbox-cursor-identity", 3);
    let threads = core.threads();
    core.send(threads[0], "go".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    cx.simulate_keystrokes("cmd-i down");
    tick(cx);
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    cx.simulate_keystrokes("cmd-w");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.parked().unwrap(),
            vec![threads[2]],
            "completion cannot redirect Park to another Thread"
        );
    });
}

#[gpui::test]
fn pressing_a_card_opens_its_thread_and_the_list_stays(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("inbox-stays-open", 2);
    let threads = core.threads();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let card = |thread: ThreadId| -> &'static str {
        format!("nav-thread-{}", thread.get()).leak()
    };
    cx.simulate_keystrokes("cmd-i");
    tick(cx);
    let row = cx.debug_bounds(card(threads[1])).unwrap();
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.focused_thread(), Some(threads[1]));
    });
    assert!(cx.debug_bounds(card(threads[0])).is_some(), "the list stays");
    fake.streams.borrow()[0]
        .send(decision("stay-in-inbox"))
        .unwrap();
    tick(cx);
    let row = cx.debug_bounds(card(threads[0])).unwrap();
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.focused_thread(), Some(threads[0]));
    });
    assert!(cx.debug_bounds(card(threads[1])).is_some(), "the list stays");
}

#[gpui::test]
fn snooze_hides_a_thread_until_woken_and_new_decisions_wake_it(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("inbox-snooze", 2);
    let threads = core.threads();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    view.update(cx, |view, _| {
        view.cockpit
            .snooze_thread(
                threads[0],
                Some(ferrite_core::clock::system_time() + Duration::from_secs(3600)),
            )
            .unwrap()
    });
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(!view
            .needs_you_rows(cx)
            .iter()
            .any(|row| row.thread == threads[0]));
        assert_eq!(view.nav_state().snoozed.len(), 1);
    });
    fake.streams.borrow()[0]
        .send(decision("wake-snoozed-thread"))
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view
            .needs_you_rows(cx)
            .iter()
            .any(|row| row.thread == threads[0]));
        assert!(view.nav_state().snoozed.is_empty());
    });
}

#[gpui::test]
fn working_shelf_starts_folded_and_opens_to_cards(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("working-shelf", 2);
    let thread = core.threads()[0];
    let other = core.threads()[1];
    core.send(thread, "work".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    without_checkouts(&view, cx);
    view.update(cx, |view, cx| view.focus_thread(other, cx));
    tick(cx);
    cx.simulate_keystrokes("cmd-i");
    tick(cx);
    let key: &'static str = format!("nav-thread-{}", thread.get()).leak();
    assert!(
        cx.debug_bounds(key).is_none(),
        "Working is hidden by default"
    );
    let heading = cx.debug_bounds("nav-shelf-working").unwrap();
    cx.simulate_click(heading.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        cx.debug_bounds(key).unwrap().size.height,
        px(ferrite_ui::theme::NAV_CARD_H_SHORT)
    );
    let heading = cx.debug_bounds("nav-shelf-working").unwrap();
    cx.simulate_click(heading.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds(key).is_none());
}

#[gpui::test]
fn snooze_expiry_returns_threads_without_an_alert(cx: &mut TestAppContext) {
    let clock = hold_wall_clock();
    let (core, _) = cockpit("snooze-expiry", 1);
    let thread = core.threads()[0];
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    view.update(cx, |view, cx| {
        view.cockpit
            .snooze_thread(
                thread,
                Some(ferrite_core::clock::system_time() + Duration::from_secs(3600)),
            )
            .unwrap();
        cx.notify();
    });
    tick(cx);
    assert!(view.read_with(cx, |view, cx| view.needs_you_rows(cx).is_empty()));
    clock.advance(Duration::from_secs(3601));
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.needs_you_rows(cx).len(), 1);
        assert!(view.needs_you.toasts().is_empty());
    });
}

#[gpui::test]
fn custom_snooze_keeps_invalid_input_and_closes_when_leaving_inbox(cx: &mut TestAppContext) {
    let (core, _) = cockpit("snooze-custom", 1);
    let thread = core.threads()[0];
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    cx.update(|window, cx| view.update(cx, |view, cx| view.edit_snooze(thread, window, cx)));
    tick(cx);
    view.update(cx, |view, cx| {
        view.snooze_editor
            .as_ref()
            .unwrap()
            .line
            .update(cx, |line, cx| line.set("nonsense".into(), cx));
    });
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(view.read_with(cx, |view, _| view
        .snooze_editor
        .as_ref()
        .unwrap()
        .error
        .is_some()));
    cx.simulate_keystrokes("cmd-b");
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.snooze_editor.is_none()));
    cx.update(|window, cx| view.update(cx, |view, cx| view.edit_snooze(thread, window, cx)));
    tick(cx);
    cx.simulate_keystrokes("cmd-i");
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.snooze_editor.is_none()));
}
