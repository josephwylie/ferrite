//! The image viewer in the reader slot (`image_viewer.rs`): an image opens
//! beside its Pane from anywhere, swaps in place without ever blanking the
//! slot, replaces a text reader
//! and is replaced by one, closes on esc with the keyboard back on the
//! Pane, and zooms between fit and actual size — easing, asking for frames
//! only while it moves, and never rebuilding the Panes beside it.
use super::*;
use crate::attachment_preview::Body;

/// A solid PNG `width` × `height` at `path`.
fn png(path: &std::path::Path, width: u32, height: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbaImage::from_pixel(width, height, image::Rgba([90, 140, 200, 255]))
        .save(path)
        .unwrap();
}

/// A Solo board 1440 × 900 with the nav held open.
fn board<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    Fake,
    ThreadId,
    &'a mut gpui::VisualTestContext,
) {
    let (core, fake) = cockpit(name, 1);
    let thread = core.threads()[0];
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    (view, fake, thread, cx)
}

fn open_image(
    view: &Entity<CockpitView>,
    path: &std::path::Path,
    gallery: &[std::path::PathBuf],
    cx: &mut gpui::VisualTestContext,
) {
    step_to(view, path, gallery, cx);
    tick(cx);
}

/// Open `path` and draw the frame that follows, leaving every decode still
/// on the way where it is.
fn step_to(
    view: &Entity<CockpitView>,
    path: &std::path::Path,
    gallery: &[std::path::PathBuf],
    cx: &mut gpui::VisualTestContext,
) {
    view.update_in(cx, |view, window, cx| {
        view.panes[0]
            .preview
            .open_image(path.to_path_buf(), gallery, window, cx)
    });
}

/// The open document's path and the viewer showing it, if it is an image.
fn shown(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> Option<(std::path::PathBuf, Entity<crate::image_viewer::ImageViewer>)> {
    view.read_with(cx, |view, _| {
        let document = view.panes[0].preview.document()?;
        match document.body {
            Body::Image { viewer, .. } => Some((document.path, viewer)),
            _ => None,
        }
    })
}

fn picture(
    viewer: &Entity<crate::image_viewer::ImageViewer>,
    cx: &mut gpui::VisualTestContext,
) -> gpui::Bounds<Pixels> {
    viewer
        .read_with(cx, |viewer, _| viewer.picture())
        .expect("the picture is drawn")
}

/// A prompt's image, clicked under its band, opens in the reader slot
/// beside the Pane — the board halved, the picture fitted inside the
/// slot's inset, never past its own size — and the viewer holds the
/// keyboard. Nothing dims the window.
#[gpui::test]
fn a_sent_image_opens_in_the_reader_slot_beside_its_pane(cx: &mut TestAppContext) {
    let dir = scratch("viewer-sent-files");
    let (wide, small) = (dir.join("wide.png"), dir.join("small.png"));
    png(&wide, 1600, 1000);
    png(&small, 120, 80);
    let (core, _fake) = cockpit("viewer-sent", 1);
    let thread = core.threads()[0];
    let mut core = core;
    core.send(
        thread,
        ferrite_core::prompt_files::compose("compare these", &[wide.clone(), small.clone()]),
    );
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let whole = cx.update(|window, cx| view.read(cx).pane_rects(window)[0].1);
    let sent = cx.debug_bounds("sent-image").expect("a sent image");
    cx.simulate_click(sent.center(), gpui::Modifiers::none());
    tick(cx);
    let (path, viewer) = shown(&view, cx).expect("an image opened");
    assert!(path == wide || path == small, "{path:?}");
    let gallery = view.read_with(cx, |view, _| match view.panes[0].preview.document() {
        Some(crate::attachment_preview::Document {
            body: Body::Image { gallery, .. },
            ..
        }) => gallery.len(),
        _ => 0,
    });
    assert_eq!(gallery, 2, "it steps among the prompt's images");
    let pane = cx.update(|window, cx| view.read(cx).pane_rects(window)[0].1);
    assert!(pane.w < whole.w * 0.6, "the board halves: {pane:?}");
    let stage = cx.debug_bounds("image-viewer").expect("the viewer");
    assert!(stage.left() >= px(pane.x + pane.w), "beside the Pane");
    assert!(cx.debug_bounds("reader-head").is_some());
    assert!(cx.debug_bounds("open-original-image").is_some());
    assert!(cx
        .update(|window, cx| viewer.read(cx).held_focus(window))
        .is_some());
    // The decode lands; the picture lies centred in the stage.
    cx.run_until_parked();
    tick(cx);
    open_image(&view, &wide, &[wide.clone(), small.clone()], cx);
    cx.run_until_parked();
    tick(cx);
    let rect = picture(&viewer, cx);
    let inset = px(crate::theme::IMAGE_INSET);
    assert!(
        (rect.size.width - (stage.size.width - inset * 2.)).abs() <= px(1.)
            || (rect.size.height - (stage.size.height - inset * 2.)).abs() <= px(1.),
        "fitted inside the inset: {rect:?} in {stage:?}"
    );
    assert!(
        (rect.size.width / rect.size.height - 1.6).abs() < 0.01,
        "its own proportions"
    );
    assert!((rect.center().x - stage.center().x).abs() <= px(1.));
    assert!((rect.center().y - stage.center().y).abs() <= px(1.));
    open_image(&view, &small, &[wide.clone(), small.clone()], cx);
    cx.run_until_parked();
    tick(cx);
    let rect = picture(&viewer, cx);
    assert_eq!(rect.size, gpui::size(px(120.), px(80.)), "never upscaled");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A second image swaps in place — the same viewer in the same slot —
/// and ← → step along the gallery it was opened with, stopping at its
/// ends.
#[gpui::test]
fn a_second_image_swaps_in_place_and_arrows_step(cx: &mut TestAppContext) {
    let dir = scratch("viewer-swap-files");
    let images: Vec<_> = ["one.png", "two.png", "three.png"]
        .iter()
        .map(|name| dir.join(name))
        .collect();
    for image in &images {
        png(image, 400, 300);
    }
    let (view, _fake, _, cx) = board("viewer-swap", cx);
    open_image(&view, &images[0], &images, cx);
    let (_, first) = shown(&view, cx).unwrap();
    let slot = cx.debug_bounds("image-viewer").unwrap();
    open_image(&view, &images[1], &images, cx);
    let (path, second) = shown(&view, cx).unwrap();
    assert_eq!(path, images[1]);
    assert_eq!(first.entity_id(), second.entity_id(), "the same viewer");
    assert_eq!(
        cx.debug_bounds("image-viewer").unwrap(),
        slot,
        "no new slot"
    );
    cx.simulate_keystrokes("right");
    tick(cx);
    assert_eq!(shown(&view, cx).unwrap().0, images[2]);
    cx.simulate_keystrokes("right");
    tick(cx);
    assert_eq!(shown(&view, cx).unwrap().0, images[2], "the last stays");
    cx.simulate_keystrokes("left left left");
    tick(cx);
    assert_eq!(shown(&view, cx).unwrap().0, images[0]);
    let title = view.read_with(cx, |view, _| {
        view.panes[0].preview.document().unwrap().title
    });
    assert_eq!(title, "one.png", "the head names the image shown");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Stepping on never blanks the slot: before the next image is drawn the
/// picture on screen holds, and mid-crossfade whichever of the two shows
/// more holds, until the newest is all in.
#[gpui::test]
fn stepping_on_mid_crossfade_holds_the_picture_that_shows_more(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    let dir = scratch("viewer-step-files");
    let images: Vec<_> = ["one.png", "two.png", "three.png", "four.png"]
        .iter()
        .map(|name| dir.join(name))
        .collect();
    for image in &images {
        png(image, 400, 300);
    }
    let (view, _fake, _, cx) = board("viewer-step", cx);
    open_image(&view, &images[0], &images, cx);
    settle(cx);
    let (_, viewer) = shown(&view, cx).unwrap();
    let leaving = |cx: &mut gpui::VisualTestContext| viewer.read_with(cx, |v, _| v.leaving());
    assert_eq!(leaving(cx), None);
    // Two decodes on the way, a frame drawn between: the first holds.
    step_to(&view, &images[1], &images, cx);
    step_to(&view, &images[2], &images, cx);
    assert_eq!(leaving(cx), Some((images[0].clone(), 1.0)));
    // Three lands and fades in; past halfway, three holds for four.
    cx.run_until_parked();
    let fade = Duration::from_millis(crate::theme::MOTION_FADE_QUICK_MS);
    cx.executor().advance_clock(fade * 3 / 4);
    display_frames(cx);
    step_to(&view, &images[3], &images, cx);
    let (held, from) = leaving(cx).unwrap();
    assert_eq!(held, images[2]);
    assert!(from > 0.5 && from < 1.0, "{from}");
    // Four barely in, one comes back: three still shows more.
    cx.run_until_parked();
    cx.executor().advance_clock(fade / 10);
    display_frames(cx);
    step_to(&view, &images[0], &images, cx);
    assert_eq!(leaving(cx).unwrap().0, images[2]);
    settle(cx);
    assert_eq!(leaving(cx), None, "let go once the newest is all in");
    assert!(viewer.read_with(cx, |v, _| v.picture()).is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Esc closes the image — never interrupting the turn — and the keyboard
/// is the Pane's Composer's again; ⌘W in the viewer closes it rather than
/// its Pane.
#[gpui::test]
fn esc_closes_the_image_and_the_pane_takes_the_keyboard_back(cx: &mut TestAppContext) {
    let dir = scratch("viewer-esc-files");
    let image = dir.join("shot.png");
    png(&image, 300, 200);
    let (view, fake, _, cx) = board("viewer-esc", cx);
    open_image(&view, &image, &[], cx);
    assert!(cx.debug_bounds("image-viewer").is_some());
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(cx.debug_bounds("image-viewer").is_none());
    assert!(view.read_with(cx, |view, _| view.panes[0].preview.document().is_none()));
    assert_eq!(*fake.interrupts.borrow(), 0, "esc closed the image only");
    let composer = view.read_with(cx, |view, _| view.panes[0].composer.clone());
    assert!(cx.update(|window, cx| {
        composer
            .read(cx)
            .focus_target(window, cx)
            .contains_focused(window, cx)
    }));
    open_image(&view, &image, &[], cx);
    cx.simulate_keystrokes("cmd-w");
    tick(cx);
    assert!(
        cx.debug_bounds("image-viewer").is_none(),
        "the image closed"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.panes.len()),
        1,
        "its Pane stays"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// An image opening while a file is open replaces it in the same slot,
/// as another file would — ⌘-clicking an image's path goes there too —
/// and a file opening over an image replaces it back.
#[gpui::test]
fn an_image_and_a_text_reader_replace_each_other(cx: &mut TestAppContext) {
    let dir = scratch("viewer-replace-files");
    let (file, image) = (dir.join("notes.rs"), dir.join("shot.png"));
    png(&image, 300, 200);
    std::fs::write(&file, "fn one() {}\n").unwrap();
    let (view, _fake, _, cx) = board("viewer-replace", cx);
    let reader = |path: &std::path::Path| beside::Beside::Reader {
        path: path.to_path_buf(),
        line: None,
    };
    let open = |beside: beside::Beside, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| view.update(cx, |view, cx| view.open_beside(beside, window, cx)));
        tick(cx);
    };
    open(reader(&file), cx);
    let slot = cx
        .debug_bounds("markdown-reader")
        .expect("the file's reader");
    open(reader(&image), cx);
    assert_eq!(shown(&view, cx).map(|(path, _)| path), Some(image.clone()));
    assert_eq!(
        cx.debug_bounds("markdown-reader").unwrap(),
        slot,
        "the same slot"
    );
    open(reader(&file), cx);
    let document = view.read_with(cx, |view, _| view.panes[0].preview.document().unwrap());
    assert!(matches!(document.body, Body::Text { .. }));
    assert!(cx.debug_bounds("image-viewer").is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Delivers the display frame the last draw asked for, and says how many
/// animation-frame requests were waiting on it.
fn display_frames(cx: &mut gpui::VisualTestContext) -> usize {
    let frames = cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    frames
}

/// Lands every decode on the way and plays out the fades until the window
/// stops asking for frames.
fn settle(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    for _ in 0..30 {
        cx.executor().advance_clock(Duration::from_millis(16));
        if display_frames(cx) == 0 {
            break;
        }
    }
}

/// A click toggles fit and actual size: the scale eases over
/// `IMAGE_ZOOM`, the window asks for frames only while it moves, and the
/// Pane beside it is not rebuilt for any of them. A second click goes
/// back to fit.
#[gpui::test]
fn a_click_eases_the_image_to_actual_size_and_back(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    let dir = scratch("viewer-zoom-files");
    let image = dir.join("large.png");
    png(&image, 2000, 1500);
    let (view, _fake, _, cx) = board("viewer-zoom", cx);
    open_image(&view, &image, &[], cx);
    settle(cx);
    let (_, viewer) = shown(&view, cx).unwrap();
    let fitted = picture(&viewer, cx);
    assert!(fitted.size.width < px(2000.));
    let identity = view.read_with(cx, |view, _| view.panes[0].identity);
    let stage = cx.debug_bounds("image-viewer").unwrap();
    cx.simulate_click(stage.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    // (The press itself lands on the Pane that owns the reader.)
    let builds = crate::cockpit::drawn::pane_renders(identity);
    cx.executor().advance_clock(Duration::from_millis(40));
    assert!(display_frames(cx) > 0, "a moving zoom asks for frames");
    let mid = picture(&viewer, cx);
    assert!(
        mid.size.width > fitted.size.width && mid.size.width < px(2000.),
        "mid-flight: {mid:?}"
    );
    cx.executor()
        .advance_clock(Duration::from_millis(crate::theme::MOTION_IMAGE_ZOOM_MS));
    display_frames(cx);
    let actual = picture(&viewer, cx);
    assert_eq!(actual.size, gpui::size(px(2000.), px(1500.)), "actual size");
    assert_eq!(display_frames(cx), 0, "settled: no more frames");
    assert_eq!(
        crate::cockpit::drawn::pane_renders(identity),
        builds,
        "the zoom's frames never rebuilt the Pane"
    );
    cx.simulate_click(stage.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(
        crate::theme::MOTION_IMAGE_ZOOM_MS + 16,
    ));
    display_frames(cx);
    assert_eq!(picture(&viewer, cx), fitted, "back to fit");
    // ⌘= and ⌘0 zoom the image, not the reading size.
    let reading = view.read_with(cx, |view, _| view.prefs.settings.reading_size);
    cx.simulate_keystrokes("cmd-=");
    cx.executor().advance_clock(Duration::from_millis(
        crate::theme::MOTION_IMAGE_ZOOM_MS + 16,
    ));
    display_frames(cx);
    assert!(picture(&viewer, cx).size.width > fitted.size.width);
    cx.simulate_keystrokes("cmd-0");
    cx.executor().advance_clock(Duration::from_millis(
        crate::theme::MOTION_IMAGE_ZOOM_MS + 16,
    ));
    display_frames(cx);
    assert_eq!(picture(&viewer, cx), fitted);
    assert_eq!(
        view.read_with(cx, |view, _| view.prefs.settings.reading_size),
        reading
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Reduced motion: the click lands on actual size at once and asks for no
/// frame.
#[gpui::test]
fn reduced_motion_snaps_the_zoom(cx: &mut TestAppContext) {
    crate::motion::testing::drive();
    let dir = scratch("viewer-reduced-files");
    let image = dir.join("large.png");
    png(&image, 2000, 1500);
    let (view, _fake, _, cx) = board("viewer-reduced", cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    open_image(&view, &image, &[], cx);
    cx.run_until_parked();
    tick(cx);
    display_frames(cx);
    let (_, viewer) = shown(&view, cx).unwrap();
    let stage = cx.debug_bounds("image-viewer").unwrap();
    cx.simulate_click(stage.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(display_frames(cx), 0, "no frame asked for");
    assert_eq!(picture(&viewer, cx).size, gpui::size(px(2000.), px(1500.)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A file that cannot be read as an image says so quietly, with the
/// original a click away.
#[gpui::test]
fn an_unreadable_image_says_so(cx: &mut TestAppContext) {
    let dir = scratch("viewer-broken-files");
    let image = dir.join("broken.png");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(&image, b"not a png").unwrap();
    let (view, _fake, _, cx) = board("viewer-broken", cx);
    open_image(&view, &image, &[], cx);
    cx.run_until_parked();
    tick(cx);
    assert!(cx.debug_bounds("image-viewer-error").is_some());
    assert!(cx.debug_bounds("image-viewer-open-original").is_some());
    let (_, viewer) = shown(&view, cx).unwrap();
    assert!(viewer.read_with(cx, |viewer, _| viewer.picture()).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
