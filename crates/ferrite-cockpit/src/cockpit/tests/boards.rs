//! The image lightbox's bar.
#[allow(unused_imports)]
use super::*;

/// The lightbox's bar opens the original with the OS by its canonical
/// file URL, leaving the lightbox open and sending nothing; esc then
/// closes it.
#[gpui::test]
fn an_image_opens_its_canonical_original_without_sending_or_closing(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("open-original-image", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    let directory = scratch("original-image-path");
    std::fs::create_dir_all(directory.join("nested")).unwrap();
    let image = directory.join("résumé #50%.png");
    image::RgbaImage::from_pixel(40, 30, image::Rgba([90, 140, 200, 255]))
        .save(&image)
        .unwrap();
    // Include a parent component so the route must canonicalize, not
    // concatenate a file:// prefix or hand the OS the unresolved path.
    let unresolved = directory.join("nested/../résumé #50%.png");
    view.update_in(cx, |view, window, cx| {
        view.panes[0]
            .preview
            .open(unresolved, None, &[], window, cx);
    });
    tick(cx);
    let open = cx
        .debug_bounds("lightbox-original")
        .expect("the lightbox's bar offers the original");
    cx.simulate_click(open.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let opened = url::Url::parse(&cx.opened_url().expect("the OS received a file URL")).unwrap();
    assert_eq!(opened.scheme(), "file");
    assert_eq!(
        opened.to_file_path().unwrap().canonicalize().unwrap(),
        image.canonicalize().unwrap()
    );
    assert!(opened.fragment().is_none() && opened.query().is_none());
    assert!(
        fake.sent.borrow().is_empty(),
        "inspection never submits a prompt"
    );
    assert!(cx.debug_bounds("image-viewer").is_some());
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(cx.debug_bounds("image-viewer").is_none());
    assert!(view.read_with(cx, |view, _| view.panes[0].preview.lightbox().is_none()));
    assert_eq!(
        *fake.interrupts.borrow(),
        0,
        "esc closed the image, not the turn"
    );
}
