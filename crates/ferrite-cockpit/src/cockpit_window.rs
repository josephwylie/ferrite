//! The window's content: the Cockpit, cached, with the loops it shows
//! drawn by an overlay of their own above it (`ferrite_ui::loops_overlay`).

use gpui::{div, prelude::*, AnyView, Context, Entity, StyleRefinement, Window};

use crate::cockpit::CockpitView;
use ferrite_ui::loops_overlay::{self, LoopsOverlay};

/// Whether the overlay runs: on unless `FERRITE_LOOPS_OVERLAY=0`.
fn enabled() -> bool {
    #[cfg(test)]
    if let Some(forced) = testing::FORCED.with(std::cell::Cell::get) {
        return forced;
    }
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("FERRITE_LOOPS_OVERLAY").map_or(true, |value| value.trim() != "0")
    })
}

/// The kit Root's view: the Cockpit, cached, and the loops over it. It
/// also brings the window forward for a clicked system notification (ADR
/// 0017).
pub struct CockpitWindow {
    cockpit: Entity<CockpitView>,
    overlay: Option<Entity<LoopsOverlay>>,
}

impl CockpitWindow {
    pub fn new(cockpit: Entity<CockpitView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_overlay(cockpit, enabled(), window, cx)
    }

    /// With the overlay, or without it (the Cockpit uncached, every loop
    /// drawn in it): what `FERRITE_LOOPS_OVERLAY` picks, and what a parity
    /// capture sets side by side.
    pub fn with_overlay(
        cockpit: Entity<CockpitView>,
        overlay: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Over a cached Cockpit, its nav and Panes are cached parts too.
        cockpit.update(cx, |view, cx| view.set_parts(overlay, cx));
        let handle = window.window_handle();
        let clicked = cx.entity().downgrade();
        crate::system_notifications::on_click(
            move |tag, cx| {
                // alerter leaves bringing the app forward to its caller.
                cx.activate(true);
                let _ = handle.update(cx, |_, window, _| window.activate_window());
                let _ = clicked.update(cx, |this, cx| {
                    this.cockpit
                        .update(cx, |view, cx| view.open_system_notification(tag, cx))
                });
            },
            cx,
        );
        let overlay = overlay.then(|| loops_overlay::overlay(window, cx));
        Self { cockpit, overlay }
    }

    /// The window's view: one for its whole run, holding every machine.
    pub fn cockpit(&self) -> &Entity<CockpitView> {
        &self.cockpit
    }
}

impl Render for CockpitWindow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let shell = div().relative().size_full();
        match &self.overlay {
            // Rebuilt only when it, a view in it, or anything it read was
            // notified; the overlay's own redraws replay it.
            Some(overlay) => shell
                .child(
                    AnyView::from(self.cockpit.clone())
                        .cached(StyleRefinement::default().size_full())
                        .tracking_reads(),
                )
                .child(overlay.clone()),
            None => shell.child(self.cockpit.clone()),
        }
    }
}

#[cfg(test)]
pub mod testing {
    use std::cell::Cell;

    thread_local! {
        pub(super) static FORCED: Cell<Option<bool>> = const { Cell::new(None) };
    }

    /// This test's windows (its thread's) draw every loop in the Cockpit,
    /// uncached: the behaviour before the overlay.
    pub fn disable() {
        FORCED.with(|forced| forced.set(Some(false)));
    }

    /// The Cockpit under a kit Root's view (a test window's root).
    pub fn cockpit(
        view: &gpui::AnyView,
        cx: &gpui::App,
    ) -> gpui::Entity<crate::cockpit::CockpitView> {
        view.clone()
            .downcast::<super::CockpitWindow>()
            .expect("the window's content")
            .read(cx)
            .cockpit()
            .clone()
    }
}
