use super::*;
use crate::ferrite_update::State;
use ferrite_core::self_update::{Asset, Release, Staged};

fn staged(version: &str, notes: &str) -> Staged {
    serde_json::from_value(serde_json::json!({
        "version": version,
        "notes": notes,
        "installer": "ferrite.dmg",
    }))
    .unwrap()
}

/// The window's update made ready as a check and a download would.
fn make_ready(view: &mut CockpitView, version: &str) {
    assert!(view.ferrite_update.begin_check());
    let release = Release {
        version: version.into(),
        notes: String::new(),
        asset: Asset {
            name: "ferrite.dmg".into(),
            url: "https://example.invalid/ferrite.dmg".into(),
            sha256: "0".repeat(64),
        },
    };
    view.ferrite_update.checked(Ok(Some(release)));
    view.ferrite_update.downloaded(Ok(staged(version, "")));
}

/// The first launch of an installed release opens on its notes, once.
#[gpui::test]
fn whats_new_opens_on_the_first_frame(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("whats-new", 1);
    let prefs = Preferences {
        whats_new: Some(staged("0.5.0", "## What's Changed\n* Self-update")),
        ..Preferences::ephemeral()
    };
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    cx.run_until_parked();
    assert!(cx.update(|window, cx| {
        use gpui::component::WindowExt as _;
        window.has_active_dialog(cx)
    }));
    view.read_with(cx, |view, _| assert!(view.prefs.whats_new.is_none()));
}

/// Restarting onto a ready release never cuts a turn off: with an agent
/// working it waits for idle, and nothing is asked or ended meanwhile.
#[gpui::test]
fn a_ready_release_waits_for_working_agents(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("update-waits", 1);
    let thread = core.threads()[0];
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| {
        view.cockpit.send(thread, "Run the checks".into())
    });
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "Running them.".into(),
        })
        .unwrap();
    tick(cx);

    view.update(cx, |view, cx| {
        make_ready(view, "0.5.0");
        assert_eq!(view.ferrite_update.take_offer().as_deref(), Some("0.5.0"));
        view.restart_to_update(cx);
    });
    assert!(!cx.has_pending_prompt());
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.ferrite_update.state(),
            &State::Ready {
                version: "0.5.0".into(),
                when_idle: true,
            }
        );
        let busy = view.in_flight();
        assert_eq!(
            view.ferrite_update.describe(busy.as_deref()).0.as_ref(),
            "0.5.0 installs once no agent is working or waiting (1 agent is working now)"
        );
        assert_eq!(
            view.cockpit.threads(),
            vec![thread],
            "the Thread stays live"
        );
    });
}
