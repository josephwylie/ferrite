//! Group boards: seams, drops and the cards on them.
#[allow(unused_imports)]
use super::*;

/// The image viewer's head opens the original with the OS by its
/// canonical file URL, leaving the viewer open and sending nothing;
/// esc then closes the slot.
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
        .debug_bounds("open-original-image")
        .expect("the viewer's head offers the original");
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
    assert!(view.read_with(cx, |view, _| view.panes[0].preview.document().is_none()));
    assert_eq!(
        *fake.interrupts.borrow(),
        0,
        "esc closed the image, not the turn"
    );
}

#[gpui::test]
fn native_file_drops_follow_the_pointer_across_panes_and_groups(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("native-file-drop", 4);
    let threads = core.threads();
    let mut groups = Vec::new();
    for pair in threads.chunks(2) {
        groups.push(
            core.apply_group(GroupChange::Create {
                first: pair[0],
                second: pair[1],
            })
            .unwrap()
            .group
            .unwrap(),
        );
    }
    bind_production_keys(cx);
    let (root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| CockpitView::new(core, cx));
        gpui::component::Root::new(
            cx.new(|cx| crate::cockpit_window::CockpitWindow::new(view, window, cx)),
            window,
            cx,
        )
        .bordered(false)
    });
    let view = root.read_with(cx, |root, cx| {
        crate::cockpit_window::testing::cockpit(root.view(), cx)
    });
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1200.), px(700.)));
    let files = vec![
        here().join("screen shot.png"),
        here().join("data.weird-extension"),
    ];
    let payload = gpui::ExternalPaths(files.clone().into());
    for (n, group) in groups.into_iter().enumerate() {
        view.update(cx, |view, cx| view.enter_group(group, cx));
        tick(cx);
        let points: Vec<_> = cx.update(|window, cx| {
            view.read(cx)
                .pane_rects(window)
                .iter()
                .map(|(_, r)| gpui::point(px(r.x + r.w / 2.), px(r.y + r.h / 2.)))
                .collect()
        });
        cx.simulate_event(gpui::FileDropEvent::Entered {
            position: points[0],
            paths: payload.clone(),
        });
        tick(cx);
        cx.simulate_event(gpui::FileDropEvent::Pending {
            position: points[1],
        });
        tick(cx);
        cx.simulate_event(gpui::FileDropEvent::Submit {
            position: points[1],
        });
        tick(cx);
        view.read_with(cx, |view, cx| {
            assert_eq!(view.focused_thread(), Some(threads[n * 2 + 1]));
            assert!(view.panes[view.pane_for(threads[n * 2]).unwrap()]
                .composer
                .read(cx)
                .is_empty());
            let composer = view.panes[view.focused()].composer.read(cx);
            assert!(
                composer.text().is_empty(),
                "files do not clutter editable prose"
            );
            assert_eq!(
                ferrite_core::prompt_files::paths(&composer.prompt(), None),
                files
            );
        });
        let tray = cx.debug_bounds("pending-attachment-tray").unwrap();
        let editor = cx.debug_bounds("focused-prompt-editor").unwrap();
        assert!(
            tray.bottom() <= editor.top(),
            "the attachment island stays above the editable prompt"
        );
        // The thumbnail overlay and card share a hit area: one click must
        // open one image, so one Escape must close it completely.
        let island = cx.debug_bounds("attachment-island-content").unwrap();
        cx.simulate_click(
            island.origin + gpui::point(px(24.), island.size.height / 2.),
            gpui::Modifiers::none(),
        );
        tick(cx);
        let viewer = cx.debug_bounds("image-viewer").unwrap();
        let owner = cx.update(|window, cx| {
            let view = view.read(cx);
            view.pane_rects(window)
                .into_iter()
                .find(|(index, _)| *index == view.focused())
                .unwrap()
                .1
        });
        assert!(
            viewer.left() >= px(owner.x + owner.w),
            "the image opens in the Panel beside its Pane: {viewer:?} / {owner:?}"
        );
        cx.simulate_keystrokes("escape");
        tick(cx);
        assert!(
            cx.debug_bounds("image-viewer").is_none(),
            "one Escape closes the only image"
        );
        cx.simulate_click(editor.center(), gpui::Modifiers::none());
        tick(cx);
        // Leaving the window cancels the drag; a later release adds nothing.
        cx.simulate_event(gpui::FileDropEvent::Entered {
            position: points[0],
            paths: payload.clone(),
        });
        cx.simulate_event(gpui::FileDropEvent::Exited);
        cx.simulate_event(gpui::FileDropEvent::Submit {
            position: points[0],
        });
        tick(cx);
        view.read_with(cx, |view, cx| {
            assert!(view.panes[view.pane_for(threads[n * 2]).unwrap()]
                .composer
                .read(cx)
                .is_empty())
        });
        // The kit's controls receive Tab and Enter/Space without sending.
        cx.simulate_keystrokes("tab");
        tick(cx);
        cx.simulate_keystrokes("enter");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
        });
        tick(cx);
        let viewer = cx
            .debug_bounds("image-viewer")
            .expect("an image opened from the keyboard");
        assert!(
            fake.sent.borrow().len() == n,
            "opening an image never submits"
        );
        cx.simulate_resize(gpui::size(px(1000.), px(480.)));
        tick(cx);
        tick(cx);
        let smaller = cx.debug_bounds("image-viewer").unwrap();
        assert!(
            smaller.size.height < viewer.size.height,
            "the image's slot resizes with the board: {viewer:?} -> {smaller:?}"
        );
        cx.simulate_keystrokes("escape");
        tick(cx);
        assert!(cx.debug_bounds("image-viewer").is_none());
        cx.simulate_resize(gpui::size(px(1200.), px(700.)));
        tick(cx);
        cx.executor().advance_clock(Duration::from_millis(300));
        cx.run_until_parked();
        // Esc handed the keyboard back to the Composer: past the
        // image's two controls to the other file's ×.
        cx.simulate_keystrokes("tab tab tab");
        tick(cx);
        cx.simulate_keystrokes("space");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("space").unwrap(),
        });
        tick(cx);
        view.read_with(cx, |view, cx| {
            assert_eq!(
                ferrite_core::prompt_files::paths(
                    &view.panes[view.focused()].composer.read(cx).prompt(),
                    None
                ),
                files[..1]
            );
        });
        assert!(
            cx.debug_bounds("image-viewer").is_none(),
            "a closed image leaves no hitbox"
        );
        // A repeated drop restores the removed file and deduplicates the image.
        cx.simulate_event(gpui::FileDropEvent::Entered {
            position: points[1],
            paths: payload.clone(),
        });
        tick(cx);
        cx.simulate_event(gpui::FileDropEvent::Pending {
            position: points[1],
        });
        tick(cx);
        cx.simulate_event(gpui::FileDropEvent::Submit {
            position: points[1],
        });
        tick(cx);
        // Attachments-only prompts still submit through the ordinary send path.
        cx.dispatch_action(Submit);
        tick(cx);
        let chip = cx
            .debug_bounds("prompt-chip-data.weird-extension")
            .expect("a sent file is its band's chip");
        let band = cx.debug_bounds("transcript-prompt").unwrap();
        // The band's padding above and below the chips (a Group Pane's),
        // which wrap in a half-width Pane; the last chip line stands
        // taller, as the inline chip makes it.
        let lines = (band.size.height
            - px(
                crate::theme::PROMPT_CHIP_LINE_EXTRA + 2. * crate::theme::prompt_band_pad_y(false)
            ))
            / chip.size.height;
        assert!(
            lines >= 1. && lines.fract() == 0.,
            "whole lines plus the chip line's extra: {band:?} / {chip:?}"
        );
        assert!(
            cx.debug_bounds("sent-image").is_some(),
            "a sent image is framed under its band"
        );
        view.read_with(cx, |view, cx| {
            assert!(view.panes[view.focused()].composer.read(cx).is_empty());
        });
        let sent = fake.sent.borrow();
        assert_eq!(
            ferrite_core::prompt_files::paths(sent.last().unwrap(), None),
            files
        );
    }
}

/// A Group's board is its split tree: two members sit side by side at
/// the tree's rects; dragging the seam between them re-shares the
/// width and persists on release; a Pane dropped on the other's centre
/// swaps them, on an edge splits — both persisted.
#[gpui::test]
fn the_group_board_resizes_by_its_seam_and_reorders_by_drop(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("tree-board", 2);
    let threads = core.threads();
    let group = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(700.)));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    tick(cx);

    let rects: Vec<ferrite_core::layout::Rect> = cx.update(|window, cx| {
        view.read(cx)
            .pane_rects(window)
            .into_iter()
            .map(|(_, rect)| rect)
            .collect()
    });
    assert_eq!(rects.len(), 2, "both members have a rect");
    // Two equal cards, the gutter between.
    assert!(
        (rects[0].w - rects[1].w).abs() < 0.01,
        "a halving: {rects:?}"
    );
    assert!(
        rects[1].x > rects[0].x + rects[0].w,
        "side by side, a gap between"
    );

    // Grab the seam (the gap between the two rects) and drag it right.
    let seam_x = rects[0].x + rects[0].w + crate::theme::BOARD_SEAM / 2.0;
    let seam_y = rects[0].y + rects[0].h / 2.0;
    cx.simulate_mouse_down(
        gpui::point(px(seam_x), px(seam_y)),
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.seam_drag.is_some(), "the seam is held")
    });
    cx.simulate_mouse_move(
        gpui::point(px(seam_x + 200.), px(seam_y)),
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
    cx.run_until_parked();
    cx.simulate_mouse_up(
        gpui::point(px(seam_x + 200.), px(seam_y)),
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.seam_drag.is_none(), "released");
        let tree = view.group_layout(group).unwrap();
        let ratio = match tree.root.as_ref().unwrap() {
            ferrite_core::layout::Node::Split { ratio, .. } => *ratio,
            _ => panic!("two members make a split"),
        };
        assert!(ratio > 0.6 && ratio < 0.8, "the first side grew: {ratio}");
    });

    // A drop on the centre swaps; on an edge splits the target's slot.
    view.update(cx, |view, cx| {
        view.apply_pane_drop(threads[0], threads[1], Zone::Swap, cx);
        let tree = view.group_layout(group).unwrap();
        assert_eq!(tree.leaves(), vec![threads[1], threads[0]], "swapped");
        view.apply_pane_drop(threads[0], threads[1], Zone::Split(Edge::Top), cx);
        let tree = view.group_layout(group).unwrap();
        match tree.root.as_ref().unwrap() {
            ferrite_core::layout::Node::Split { axis, .. } => {
                assert_eq!(*axis, ferrite_core::layout::Axis::Column, "stacked now")
            }
            _ => panic!("still a split"),
        }
        assert_eq!(
            tree.leaves(),
            vec![threads[0], threads[1]],
            "the source sits above"
        );
    });
}

/// Many members on one board, at two zoom levels: every seam grabs and
/// follows the pointer while every other seam holds its line — a chain
/// of N is nested splits, and a naive ratio drag squeezed the whole
/// rest of the chain — and every Pane's title drags onto another Pane,
/// not only the first two.
#[gpui::test]
fn a_crowded_board_drags_every_seam_and_every_title(cx: &mut TestAppContext) {
    for (count, width, height) in [(6, 1400., 900.), (9, 900., 600.)] {
        exercise_board(cx, count, width, height);
    }
}

/// Every slot of a Group's board paints a card (the CLI-park bug left
/// one empty, the glass showing through). A member parked in its Group
/// keeps its Pane, drawn parked; a member this window holds no Pane for
/// yet takes no slot, and the board closes up around the rest.
#[gpui::test]
fn every_slot_of_a_group_board_paints_a_card(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("board-no-hole", 3);
    let threads = core.threads();
    let group = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    core.apply_group(GroupChange::Join {
        thread: threads[2],
        group,
        index: None,
    })
    .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    tick(cx);
    let filled = |cx: &mut gpui::VisualTestContext, cards: usize| {
        let (bounds, rects) = cx.update(|window, cx| {
            let view = view.read(cx);
            (view.board_bounds(window), view.pane_rects(window))
        });
        assert_eq!(rects.len(), cards, "one card per slot");
        let area: f32 = rects.iter().map(|(_, rect)| rect.w * rect.h).sum();
        let seams = (cards - 1) as f32 * crate::theme::BOARD_SEAM;
        assert!(
            area >= (bounds.w.min(bounds.h) - seams) * bounds.w.max(bounds.h) - 1.0,
            "the cards fill the board, nothing left unpainted: {area} of {bounds:?}"
        );
    };

    view.update(cx, |view, cx| {
        view.cockpit.park(threads[1]).unwrap();
        view.sync_panes(cx);
        cx.notify();
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.cockpit.thread(threads[1]).is_none(), "parked");
        assert!(view.pane_for(threads[1]).is_some(), "its Pane stays");
    });
    filled(cx, 3);

    // The window's Panes trail the Roster for a moment after an act.
    view.update(cx, |view, _| {
        view.panes.retain(|pane| pane.thread() != Some(threads[1]));
        let tree = view.board_tree(Board::Group(group)).unwrap();
        assert_eq!(tree.leaves(), [threads[0], threads[2]]);
    });
    filled(cx, 2);
}

/// `count` Threads with a few lines each, grouped, at `width`×`height`.
fn exercise_board(cx: &mut TestAppContext, count: usize, width: f32, height: f32) {
    let name: &'static str =
        Box::leak(format!("tree-board-{count}-{width}x{height}").into_boxed_str());
    let label = format!("{count} panes at {width}x{height}");
    let (mut core, fake) = cockpit(name, count);
    let threads = core.threads();
    let group = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    for thread in &threads[2..] {
        core.apply_group(GroupChange::Join {
            thread: *thread,
            group,
            index: None,
        })
        .unwrap();
    }
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    // The column stays open: this is the board's seams, and folding the
    // nav away (rule 2.7.7) as a drag narrows a flush cell under the
    // instruments floor would move every band under the pointer mid-test.
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(width), px(height)));
    for (n, stream) in fake.streams.borrow().iter().enumerate() {
        stream
            .send(SessionEvent::TextDelta {
                text: format!("pane {n} says a few lines\nand a few more\n").repeat(12),
            })
            .unwrap();
    }
    view.update(cx, |view, cx| view.enter_group(group, cx));
    tick(cx);

    let bounds = cx.update(|window, cx| view.read(cx).board_bounds(window));
    let tree = view.read_with(cx, |view, _| view.group_layout(group).unwrap());
    let seams = tree.seams(bounds, crate::theme::BOARD_SEAM, SEAM_GRAB);
    assert_eq!(seams.len(), count - 1, "{label}: n leaves make n-1 seams");
    for seam in &seams {
        let at = gpui::point(
            px(seam.band.x + seam.band.w / 2.0),
            px(seam.band.y + seam.band.h / 2.0),
        );
        cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            let held = view.seam_drag.as_ref().map(|drag| drag.seam.clone());
            assert_eq!(
                held,
                Some(seam.id.clone()),
                "{label}: seam {:?} at {at:?} is held",
                seam.id
            );
        });
        let nudge = match seam.axis {
            layout::Axis::Row => gpui::point(px(30.), px(0.)),
            layout::Axis::Column => gpui::point(px(0.), px(30.)),
        };
        cx.simulate_mouse_move(at + nudge, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        cx.simulate_mouse_up(at + nudge, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        view.read_with(cx, |view, _| {
            assert!(view.seam_drag.is_none(), "{label}: released");
            let now = view.group_layout(group).unwrap();
            let along = |s: &layout::Seam| match s.axis {
                layout::Axis::Row => s.band.x,
                layout::Axis::Column => s.band.y,
            };
            // F-18: a seam drags the whole line it is part of — its
            // linked seams move with it, every other holds.
            let linked = tree.linked(&seam.id, bounds, crate::theme::BOARD_SEAM);
            for after in now.seams(bounds, crate::theme::BOARD_SEAM, SEAM_GRAB) {
                let before = seams.iter().find(|s| s.id == after.id).unwrap();
                let (was, is) = (along(before), along(&after));
                if after.id == seam.id || linked.contains(&after.id) {
                    assert!(
                        (is - was - 30.0).abs() < 2.0,
                        "{label}: seam {:?} followed the pointer: {was} -> {is}",
                        seam.id
                    );
                } else {
                    assert!(
                        (is - was).abs() < 0.5,
                        "{label}: seam {:?} held its line while {:?} moved: {was} -> {is}",
                        after.id,
                        seam.id
                    );
                }
            }
        });
        // Put it back so later seams sit where the tree says.
        view.update(cx, |view, _| {
            view.cockpit.set_group_layout(group, tree.clone()).unwrap();
        });
        tick(cx);
    }

    let rect_of = |cx: &mut gpui::VisualTestContext, thread: ThreadId| {
        cx.update(|window, cx| {
            let view = view.read(cx);
            let index = view.pane_for(thread).unwrap();
            view.pane_rects(window)
                .into_iter()
                .find(|(i, _)| *i == index)
                .map(|(_, r)| r)
                .unwrap()
        })
    };
    for source in 0..count {
        let target = (source + count / 2) % count;
        {
            let (a, b) = (threads[source], threads[target]);
            let before = view.read_with(cx, |view, _| view.group_layout(group).unwrap().leaves());
            let selector: &'static str =
                Box::leak(format!("pane-title-{}", a.get()).into_boxed_str());
            let title = cx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("{label}: {selector} is drawn"));
            let grab = title.center();
            let target_rect = rect_of(cx, b);
            let drop = gpui::point(
                px(target_rect.x + target_rect.w / 2.0),
                px(target_rect.y + target_rect.h / 2.0),
            );
            cx.simulate_mouse_down(grab, gpui::MouseButton::Left, gpui::Modifiers::none());
            cx.run_until_parked();
            cx.simulate_mouse_move(
                grab + gpui::point(px(12.), px(12.)),
                gpui::MouseButton::Left,
                gpui::Modifiers::none(),
            );
            cx.run_until_parked();
            cx.simulate_mouse_move(drop, gpui::MouseButton::Left, gpui::Modifiers::none());
            cx.run_until_parked();
            view.read_with(cx, |view, _| {
                assert_eq!(
                    view.drop_preview,
                    Some((b, Zone::Swap)),
                    "{label}: {source}->{target}: the preview says swap"
                );
            });
            cx.simulate_mouse_up(drop, gpui::MouseButton::Left, gpui::Modifiers::none());
            cx.run_until_parked();
            tick(cx);
            let after = view.read_with(cx, |view, _| view.group_layout(group).unwrap().leaves());
            let mut expected = before.clone();
            let ia = expected.iter().position(|t| *t == a).unwrap();
            let ib = expected.iter().position(|t| *t == b).unwrap();
            expected.swap(ia, ib);
            assert_eq!(after, expected, "{label}: {source}->{target}: swapped");
        }
    }
}
