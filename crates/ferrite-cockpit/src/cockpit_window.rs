//! The window's content: the Cockpit, cached, with the loops it shows
//! drawn by an overlay of their own above it (`ferrite_ui::loops_overlay`).

use gpui::{div, prelude::*, AnyView, Context, Entity, StyleRefinement, Window};

use crate::cockpit::{CockpitView, GoToMachine};
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

/// How long an alert the window went to another machine for waits for that
/// machine to answer: more than ssh takes to reach one that is up. Later,
/// the operator has moved on, and focus is not taken from them.
const LANDING_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// The kit Root's view: the Cockpit, cached, and the loops over it. It
/// also brings the window forward for a clicked system notification (ADR
/// 0017), and puts the window on another machine (ADR 0026) by building
/// the Cockpit afresh on it.
pub struct CockpitWindow {
    cockpit: Entity<CockpitView>,
    overlay: Option<Entity<LoopsOverlay>>,
    /// The Cockpit asking to be on another machine.
    _going: gpui::Subscription,
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
        let going = Self::follow(&cockpit, window, cx);
        let overlay = overlay.then(|| loops_overlay::overlay(window, cx));
        Self {
            cockpit,
            overlay,
            _going: going,
        }
    }

    fn follow(
        cockpit: &Entity<CockpitView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Subscription {
        cx.subscribe_in(
            cockpit,
            window,
            |this, _, going: &GoToMachine, window, cx| {
                this.go_to(going.index, going.open.clone(), window, cx)
            },
        )
    }

    /// Put the window on machine `index`: a Cockpit attached there, with
    /// the settings the window has now, which opens `open` once attached.
    /// The one it leaves lets its machine's Threads go; that machine
    /// streams status from here on.
    fn go_to(
        &mut self,
        index: usize,
        open: Option<crate::cockpit::Landing>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (machines, prefs, provider) = {
            let view = self.cockpit.read(cx);
            let Some(machines) = view.machines() else {
                return;
            };
            (
                machines,
                view.successor_preferences(),
                view.launch_provider(),
            )
        };
        let left = machines.read(cx).selected_key();
        let core = match machines.update(cx, |machines, _| machines.go_to(index)) {
            Some(Ok(core)) => core,
            Some(Err(error)) => {
                ferrite_core::log!("the window could not go to that machine: {error}");
                return;
            }
            None => return,
        };
        let arrived = machines.read(cx).selected_key();
        let needs_you = self
            .cockpit
            .update(cx, |view, _| std::mem::take(&mut view.needs_you))
            .moved(&left, &arrived);
        let until = cx.background_executor().now() + LANDING_WAIT;
        let view = cx.new(|cx| {
            CockpitView::new_with_settings(core, provider, prefs, cx)
                .with_machines(machines)
                .with_needs_you(needs_you)
                .with_landing(open, until)
        });
        view.update(cx, |view, cx| view.set_parts(self.overlay.is_some(), cx));
        self.cockpit
            .update(cx, |left, cx| left.take_server_bands(window, cx));
        self._going = Self::follow(&view, window, cx);
        self.cockpit = view;
        cx.notify();
    }

    /// The Cockpit this window shows now.
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
