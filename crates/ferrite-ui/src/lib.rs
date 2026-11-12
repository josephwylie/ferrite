// Ferrite's shared UI: what the Cockpit and the Pane both draw with.
pub mod components;
pub mod controls;
#[cfg(any(test, feature = "test-support"))]
pub mod face;
pub mod frame;
pub mod icons;
pub mod keymap;
pub mod loops_overlay;
pub mod menu;
pub mod motion;
pub mod odometer;
pub mod platform_text;
pub mod pointer;
pub mod project_badge;
pub mod provider;
mod scrollbar;
pub mod sheet;
pub mod theme;
mod working_mark;

use gpui::App;

/// The bundled face, compiled in: Geist Mono for code (theme rule 1); the
/// rest of the window is the system's UI font. gpui has no variation-axis
/// support, so each weight is its own static file. The weights share the
/// typographic family name (`theme::FONT_CODE`), and CoreText /
/// DirectWrite resolve the right one from `.font_weight(..)` — never reach
/// a weight by family name. The baseline correction is measured on the
/// first.
pub static FONTS: [&[u8]; 5] = [
    include_bytes!("../assets/fonts/GeistMono.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Italic.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Medium.ttf"),
    include_bytes!("../assets/fonts/GeistMono-SemiBold.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Bold.ttf"),
];

/// Registers the bundled faces. Call it before anything lays out text:
/// `add_fonts` returns a Result, and a discarded one fails silently — you get
/// the system font and no explanation.
pub fn register_fonts(cx: &App) {
    cx.text_system()
        .add_fonts(
            FONTS
                .iter()
                .map(|face| std::borrow::Cow::Owned(platform_text::browser_baseline(face)))
                .collect(),
        )
        .expect("the bundled faces load");
}

/// The vendored gpui's port of upstream zed#65952 (vendor/README.md): an
/// entity read only by a deferred draw or a paint callback under a reused
/// cached view keeps the window subscribed to it. gpui-pre is not a
/// workspace member, so its own copy of this test does not run; this is it.
#[cfg(test)]
mod vendored_gpui {
    use std::{cell::Cell, rc::Rc};

    use gpui::{
        canvas, deferred, prelude::*, px, AnyElement, Context, Entity, StyleRefinement,
        TestAppContext, Window,
    };

    struct Leaf {
        value: usize,
        output: Rc<Cell<usize>>,
    }

    struct Panel {
        leaf: Entity<Leaf>,
        deferred_child: bool,
        renders: Rc<Cell<usize>>,
    }

    impl Render for Panel {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            let leaf = self.leaf.clone();
            let child = canvas(
                |_, _, _| {},
                move |_, _, _, cx| {
                    let leaf = leaf.read(cx);
                    leaf.output.set(leaf.value);
                },
            )
            .size(px(50.));
            if self.deferred_child {
                deferred(child).into_any_element()
            } else {
                child.into_any_element()
            }
        }
    }

    struct Root {
        panel: Entity<Panel>,
        deferred_panel: bool,
        renders: Rc<Cell<usize>>,
    }

    impl Render for Root {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            let panel: AnyElement = self
                .panel
                .clone()
                .cached(StyleRefinement::default().size(px(100.)))
                .into_any_element();
            if self.deferred_panel {
                deferred(panel).into_any_element()
            } else {
                panel
            }
        }
    }

    #[gpui::test]
    fn a_reused_cached_view_keeps_what_it_read_subscribed(cx: &mut TestAppContext) {
        for deferred_child in [true, false] {
            for deferred_panel in [false, true] {
                let context = format!("deferred child {deferred_child}, panel {deferred_panel}");
                let output = Rc::new(Cell::new(0));
                let renders = Rc::new(Cell::new(0));
                let root_renders = Rc::new(Cell::new(0));
                let leaf = cx.new(|_| Leaf {
                    value: 1,
                    output: output.clone(),
                });
                let panel = cx.new(|_| Panel {
                    leaf: leaf.clone(),
                    deferred_child,
                    renders: renders.clone(),
                });
                let window = cx.add_window(|_, _| Root {
                    panel,
                    deferred_panel,
                    renders: root_renders.clone(),
                });
                cx.run_until_parked();
                assert_eq!(output.get(), 1, "{context}");
                let initial = renders.get();
                for _ in 0..2 {
                    window.update(cx, |_, _, cx| cx.notify()).unwrap();
                    cx.run_until_parked();
                    assert_eq!(
                        renders.get(),
                        initial,
                        "the panel reuses its cache: {context}"
                    );
                }
                let before = root_renders.get();
                leaf.update(cx, |leaf, cx| {
                    leaf.value = 2;
                    cx.notify();
                });
                cx.run_until_parked();
                assert!(
                    root_renders.get() > before,
                    "the leaf redraws the window: {context}"
                );
                // A plain entity read has no dispatch-tree path to the
                // cached panel: refresh to run its paint callback again.
                window.update(cx, |_, window, _| window.refresh()).unwrap();
                cx.run_until_parked();
                assert_eq!(output.get(), 2, "{context}");
            }
        }
    }
}
