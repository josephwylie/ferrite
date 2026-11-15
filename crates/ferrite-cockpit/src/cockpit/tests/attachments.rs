//! A draft's files are only as sendable as they look. Each is dropped, pasted
//! or picked into the rendered Composer and sent with the production Enter;
//! what stops a send is drawn in the file's own place, and what is sent is
//! the bytes the draft kept.
use super::sends::WORDS;
use super::*;
use ferrite_core::attachment::Destination;
use std::path::PathBuf;

const PNG: [u8; 70] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn files_dir(name: &str) -> PathBuf {
    let dir = scratch(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn prefs_in(dir: &std::path::Path) -> Preferences {
    Preferences {
        dir: dir.to_path_buf(),
        ..Preferences::ephemeral()
    }
}

/// Drop `paths` on the Pane's Composer, as the platform delivers them.
fn drop_files(paths: &[PathBuf], cx: &mut gpui::VisualTestContext) {
    let at = cx
        .debug_bounds("focused-prompt-editor")
        .expect("the Composer")
        .center();
    cx.simulate_event(gpui::FileDropEvent::Entered {
        position: at,
        paths: gpui::ExternalPaths(paths.to_vec().into()),
    });
    tick(cx);
    cx.simulate_event(gpui::FileDropEvent::Pending { position: at });
    cx.simulate_event(gpui::FileDropEvent::Submit { position: at });
    tick(cx);
}

fn window_on<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    Fake,
    PathBuf,
    &'a mut gpui::VisualTestContext,
) {
    let (core, fake) = cockpit(name, 1);
    let dir = files_dir(&format!("{name}-prefs"));
    bind_production_keys(cx);
    let prefs = prefs_in(&dir);
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    (view, fake, dir, cx)
}

fn held_files(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<PathBuf> {
    view.read_with(cx, |view, cx| {
        view.panes[0].composer.read(cx).files().to_vec()
    })
}

fn sent_with(fake: &Fake, needle: &str) -> usize {
    fake.sent
        .borrow()
        .iter()
        .filter(|text| text.contains(needle))
        .count()
}

#[gpui::test]
fn a_dropped_image_that_is_gone_stops_the_send_until_it_is_repaired(cx: &mut TestAppContext) {
    let (view, fake, dir, cx) = window_on("attach-repair", cx);
    let shot = dir.join("shot.png");
    drop_files(std::slice::from_ref(&shot), cx);
    cx.simulate_input(WORDS);
    tick(cx);
    assert!(
        cx.debug_bounds("attachment-failed").is_some(),
        "the tile says it failed"
    );

    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(sent_with(&fake, WORDS), 0, "nothing was sent");
    assert_eq!(
        held_files(&view, cx),
        [shot.clone()],
        "the file stays in the draft"
    );
    assert!(
        cx.debug_bounds("attachment-failed").is_some(),
        "and still says why"
    );

    std::fs::write(&shot, PNG).unwrap();
    let tile = cx.debug_bounds("tray-image-0").unwrap();
    cx.simulate_click(tile.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        cx.debug_bounds("attachment-failed").is_none(),
        "Retry on the tile finds the repaired file"
    );

    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(sent_with(&fake, WORDS), 1, "the repaired file sends");
}

#[gpui::test]
fn a_file_repaired_at_the_same_path_sends_without_a_retry(cx: &mut TestAppContext) {
    let (view, fake, dir, cx) = window_on("attach-repair-enter", cx);
    let notes = dir.join("notes.txt");
    drop_files(std::slice::from_ref(&notes), cx);
    cx.simulate_input(WORDS);
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(sent_with(&fake, WORDS), 0);
    assert_eq!(
        cx.debug_bounds("attachment-status")
            .map(|bounds| bounds.size.width > px(0.)),
        Some(true),
        "the chip says Missing"
    );

    std::fs::write(&notes, "now here").unwrap();
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(sent_with(&fake, WORDS), 1);
    assert!(view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).is_empty()));
}

#[gpui::test]
fn what_is_sent_is_the_bytes_the_draft_kept(cx: &mut TestAppContext) {
    let (_view, fake, dir, cx) = window_on("attach-retained", cx);
    let shot = dir.join("shot.png");
    std::fs::write(&shot, PNG).unwrap();
    drop_files(std::slice::from_ref(&shot), cx);
    cx.simulate_input(WORDS);
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);

    std::fs::write(&shot, b"replaced afterwards").unwrap();
    let sent = fake.sent.borrow().last().cloned().expect("it was sent");
    let [kept] = ferrite_core::prompt_files::paths(&sent, None)
        .try_into()
        .unwrap();
    assert_ne!(
        kept, shot,
        "the prompt names the kept copy, not the live path"
    );
    assert!(kept.starts_with(dir.join("attachments")));
    assert_eq!(
        std::fs::read(&kept).unwrap(),
        PNG,
        "retry would send the original bytes"
    );
}

#[gpui::test]
fn a_file_attached_from_the_workspace_is_sent_as_its_own_path(cx: &mut TestAppContext) {
    let (view, fake, dir, cx) = window_on("attach-workspace", cx);
    let notes = files_dir("attach-workspace-repo").join("notes.txt");
    std::fs::write(&notes, "one").unwrap();
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.add_files(std::slice::from_ref(&notes), cx)
        })
    });
    cx.simulate_input(WORDS);
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);

    let sent = fake.sent.borrow().last().cloned().expect("it was sent");
    assert_eq!(
        ferrite_core::prompt_files::paths(&sent, None),
        [notes],
        "the agent is given the workspace's file, not a copy of it"
    );
    assert!(!dir.join("attachments").exists(), "nothing was copied");
}

#[gpui::test]
fn a_workspace_file_that_goes_missing_stops_the_send(cx: &mut TestAppContext) {
    let (view, fake, _dir, cx) = window_on("attach-workspace-gone", cx);
    let notes = files_dir("attach-workspace-gone-repo").join("notes.txt");
    std::fs::write(&notes, "one").unwrap();
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.add_files(std::slice::from_ref(&notes), cx)
        })
    });
    std::fs::remove_file(&notes).unwrap();
    cx.simulate_input(WORDS);
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);

    assert_eq!(sent_with(&fake, WORDS), 0);
    assert_eq!(held_files(&view, cx), [notes]);
    assert!(cx.debug_bounds("attachment-status").is_some());
}

#[gpui::test]
fn a_pasted_screenshot_outlives_the_launch(cx: &mut TestAppContext) {
    let (view, _fake, dir, cx) = window_on("attach-paste", cx);
    let image = gpui::Image::from_bytes(gpui::ImageFormat::Png, PNG.to_vec());
    cx.write_to_clipboard(gpui::ClipboardItem::new_image(&image));
    cx.simulate_keystrokes("cmd-v");
    tick(cx);
    let [pasted] = held_files(&view, cx).try_into().unwrap();
    assert!(
        pasted.starts_with(dir.join("attachments")),
        "kept in the app's store: {pasted:?}"
    );
    view.update(cx, |view, cx| view.save_drafts(cx));
    drop(view);

    let (core, _fake) = cockpit("attach-paste-again", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs_in(&dir), cx)
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    assert_eq!(held_files(&view, cx), [pasted.clone()]);
    assert!(
        cx.debug_bounds("attachment-failed").is_none(),
        "and still readable"
    );
    assert_eq!(std::fs::read(&pasted).unwrap(), PNG);
}

#[gpui::test]
fn a_local_file_bound_for_a_machine_is_refused_and_a_workspace_file_is_not(
    cx: &mut TestAppContext,
) {
    let (view, fake, dir, cx) = window_on("attach-remote", cx);
    view.update(cx, |view, cx| {
        let store = dir.clone();
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.aim_files(Destination::Remote, store, cx)
        });
    });
    let local = dir.join("mine.png");
    std::fs::write(&local, PNG).unwrap();
    drop_files(std::slice::from_ref(&local), cx);
    cx.simulate_input(WORDS);
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(
        sent_with(&fake, WORDS),
        0,
        "it is not sent as a path the machine lacks"
    );
    assert!(cx.debug_bounds("attachment-failed").is_some());
    assert_eq!(held_files(&view, cx), [local.clone()]);
    view.update(cx, |view, cx| view.save_drafts(cx));
    assert!(
        ferrite_core::drafts::Drafts::load(&dir)
            .thread(view.read_with(cx, |view, _| view.panes[0].thread().unwrap()))
            .is_some_and(|draft| draft.local == [local.clone()]),
        "that it is local is saved with the draft"
    );

    let cx_remove = cx.debug_bounds("tray-image-0").unwrap();
    cx.simulate_mouse_move(cx_remove.center(), None, gpui::Modifiers::none());
    tick(cx);
    let remove = cx.debug_bounds("tray-image-0-remove").unwrap();
    cx.simulate_click(remove.center(), gpui::Modifiers::none());
    tick(cx);
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.add_files(&[dir.join("on-machine.txt")], cx)
        });
    });
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(
        sent_with(&fake, WORDS),
        1,
        "a path of the machine goes as it is"
    );
}

#[gpui::test]
fn an_image_over_the_inline_limit_goes_as_a_path_and_says_so(cx: &mut TestAppContext) {
    let (_view, fake, dir, cx) = window_on("attach-large", cx);
    let big = dir.join("big.png");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(ferrite_core::attachment::NATIVE_LIMIT + 1)
        .unwrap();
    drop_files(std::slice::from_ref(&big), cx);
    cx.simulate_input(WORDS);
    tick(cx);
    assert!(
        cx.debug_bounds("attachment-failed").is_none(),
        "it is not an error"
    );
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(sent_with(&fake, WORDS), 1);
}

#[gpui::test]
fn an_image_the_header_reader_does_not_know_is_still_drawn(cx: &mut TestAppContext) {
    let (_view, _fake, dir, cx) = window_on("attach-svg", cx);
    let svg = dir.join("mark.svg");
    std::fs::write(
        &svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8"/></svg>"#,
    )
    .unwrap();
    drop_files(std::slice::from_ref(&svg), cx);
    assert!(cx.debug_bounds("tray-image-0").is_some());
    assert!(cx.debug_bounds("attachment-failed").is_none());
}
