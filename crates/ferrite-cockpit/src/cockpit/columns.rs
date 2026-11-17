//! The two columns beside the board: the nav's and the right panel's.
//! Each slides open and shut over `COLUMN` (a `motion::Reveal`), the board
//! following the width on screen every frame. The decisions (the nav's
//! auto-fold, the panel's floor, a Level) read where each slide ends
//! (`nav_rest_w`, `panel_target_w`), so a width sweeping through a
//! threshold never flaps them; a flip mid-slide turns around from the
//! width on screen, and a column sliding out is drawn under an occluding
//! veil, taking no input. Pointer drags, maximize and floor changes land
//! at once; the first frame, a switch to another Pane's panel and reduced
//! motion arrive in place.

use super::*;
use ferrite_ui::motion::{Reveal, COLUMN};
use std::time::Instant;

/// One column's slide: how far in this frame draws it, 0 shut → 1 at rest.
struct ColumnSlide {
    reveal: Reveal,
    drawn: f32,
    moving: bool,
}

impl ColumnSlide {
    fn at_rest(open: bool, now: Instant) -> Self {
        let reveal = if open {
            Reveal::open(COLUMN, COLUMN, now)
        } else {
            Reveal::closed(COLUMN, COLUMN, now)
        };
        Self {
            reveal,
            drawn: f32::from(open),
            moving: false,
        }
    }

    fn tend(&mut self, open: bool, now: Instant, reduced: bool) {
        self.reveal.set(open, now, reduced);
        self.drawn = self.reveal.value(now, reduced);
        self.moving = self.reveal.running(now, reduced);
    }

    fn shut(&self, now: Instant, reduced: bool) -> bool {
        self.reveal.shut(now, reduced)
    }
}

struct NavResize {
    start_x: f32,
    start_width: f32,
    width: f32,
}

pub(crate) struct Columns {
    nav_viewport_width: f32,
    nav_brand_width: f32,
    nav_resize: Option<NavResize>,
    nav: ColumnSlide,
    panel: ColumnSlide,
    /// Whose panel the slide draws: the Pane whose panel is open, or was
    /// and is sliding shut.
    panel_owner: Option<PaneIdentity>,
    /// The first frame has been tended: what is open by then arrives in
    /// place, with no entrance.
    tended: bool,
}

impl Columns {
    pub(super) fn new() -> Self {
        let now = Instant::now();
        Self {
            nav_viewport_width: f32::INFINITY,
            nav_brand_width: 0.0,
            nav_resize: None,
            nav: ColumnSlide::at_rest(true, now),
            panel: ColumnSlide::at_rest(false, now),
            panel_owner: None,
            tended: false,
        }
    }

    /// A column slid this frame: the board moves, so frames keep coming.
    pub(super) fn moving(&self) -> bool {
        self.nav.moving || self.panel.moving
    }
}

fn clamp_nav_width(preferred: f32, viewport: f32, brand: f32) -> f32 {
    let minimum = ferrite_ui::theme::NAV_MIN_W.max(brand.ceil());
    let maximum = minimum.max(viewport.floor() - 640.0);
    let preferred = if preferred.is_finite() {
        preferred
    } else {
        nav::WIDTH
    };
    preferred.clamp(minimum, maximum)
}

impl CockpitView {
    pub(super) fn nav_expanded_width(&self) -> f32 {
        let preferred = self.columns.nav_resize.as_ref().map_or_else(
            || self.prefs.settings.nav_width_px.unwrap_or(nav::WIDTH),
            |drag| drag.width,
        );
        clamp_nav_width(
            preferred,
            self.columns.nav_viewport_width,
            self.columns.nav_brand_width,
        )
    }

    pub(super) fn measure_nav_width(&mut self, window: &Window, cx: &mut Context<Self>) {
        use ferrite_ui::theme::{
            FONT_UI, FS_NAV_TITLE, HISTORY_X, ICON_BUTTON, INK_STRONG, SPACE_1, W_LABEL,
        };
        let previous = self.nav_expanded_width();
        let width = f32::from(window.viewport_size().width);
        if self.columns.nav_viewport_width != width {
            self.cancel_nav_resize();
        }
        self.columns.nav_viewport_width = width;
        let brand = window.text_system().shape_line(
            "Ferrite".into(),
            px(FS_NAV_TITLE),
            &[gpui::TextRun {
                len: "Ferrite".len(),
                font: gpui::Font {
                    weight: W_LABEL,
                    ..gpui::font(FONT_UI)
                },
                color: rgb(INK_STRONG).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        self.columns.nav_brand_width =
            HISTORY_X + f32::from(brand.width) + SPACE_1 + 2.0 * ICON_BUTTON + 12.0 + 1.0;
        if previous != self.nav_expanded_width() {
            self.notify_part(parts::Part::Nav, cx);
        }
    }

    pub(super) fn cancel_nav_resize(&mut self) {
        self.columns.nav_resize = None;
    }

    pub(super) fn finish_nav_resize(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.columns.nav_resize.take() else {
            return;
        };
        self.change_settings(|settings| settings.nav_width_px = Some(drag.width), cx);
        cx.notify();
    }

    pub(super) fn nav_pointer_moved(
        &mut self,
        position: Point<Pixels>,
        dragging: bool,
        cx: &mut Context<Self>,
    ) {
        if !dragging {
            if self.columns.nav_resize.take().is_some() {
                self.notify_part(parts::Part::Nav, cx);
                cx.notify();
            }
            return;
        }
        let Some(drag) = &mut self.columns.nav_resize else {
            return;
        };
        let width = clamp_nav_width(
            drag.start_width + f32::from(position.x) - drag.start_x,
            self.columns.nav_viewport_width,
            self.columns.nav_brand_width,
        );
        if drag.width != width {
            drag.width = width;
            self.nav_snap.set(true);
            self.notify_part(parts::Part::Nav, cx);
            cx.notify();
        }
    }

    pub(super) fn nav_resize_rail(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("nav-resize-rail")
            .debug_selector(|| "nav-resize-rail".into())
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(self.nav_width() - 8.0))
            .w(px(16.0))
            .occlude()
            .cursor(gpui::CursorStyle::ResizeLeftRight)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if event.click_count == 2 {
                        view.cancel_nav_resize();
                        view.change_settings(|settings| settings.nav_width_px = None, cx);
                        view.notify_part(parts::Part::Nav, cx);
                    } else {
                        let width = view.nav_expanded_width();
                        view.columns.nav =
                            ColumnSlide::at_rest(true, cx.background_executor().now());
                        view.columns.nav_resize = Some(NavResize {
                            start_x: f32::from(event.position.x),
                            start_width: width,
                            width,
                        });
                    }
                    cx.notify();
                }),
            )
            // The rail follows the edge, so a drag ends on it, and its
            // click stops the release before the root's handler hears it.
            .on_click(cx.listener(|view, _, _, cx| {
                cx.stop_propagation();
                view.finish_nav_resize(cx);
            }))
    }

    /// Each frame, once the nav's auto-fold and the panel's Pane are
    /// settled and before the board is measured: a column that opened or
    /// shut starts sliding from where it is on screen; a panel sliding shut
    /// is let go when it has.
    pub(super) fn tend_columns(&mut self, cx: &gpui::App) {
        let now = cx.background_executor().now();
        let reduced = ferrite_ui::motion::reduced_motion(cx);
        let nav_open = !self.nav_folded();
        if !nav_open {
            self.cancel_nav_resize();
        }
        let wanted = self.panel_pane().map(|index| self.panes[index].identity);
        let owner_present = self
            .columns
            .panel_owner
            .is_some_and(|owner| self.index_of(owner).is_some());

        let columns = &mut self.columns;
        let first = !std::mem::replace(&mut columns.tended, true);
        if first {
            columns.nav = ColumnSlide::at_rest(nav_open, now);
        }
        columns.nav.tend(nav_open, now, reduced);

        match wanted {
            Some(identity) => {
                let switched = columns.panel_owner.is_some_and(|owner| owner != identity);
                if first || switched {
                    columns.panel = ColumnSlide::at_rest(true, now);
                }
                columns.panel.tend(true, now, reduced);
                columns.panel_owner = Some(identity);
            }
            None if columns.panel_owner.is_some() => {
                columns.panel.tend(false, now, reduced);
                if !owner_present || columns.panel.shut(now, reduced) {
                    columns.panel = ColumnSlide::at_rest(false, now);
                    columns.panel_owner = None;
                }
            }
            None => {}
        }
    }

    /// A seam grabbed mid-slide: the panel lands where it was going, so the
    /// drag starts from the board at rest.
    pub(super) fn land_panel(&mut self, cx: &gpui::App) {
        let now = cx.background_executor().now();
        let open = self.panel_pane().is_some();
        self.columns.panel = ColumnSlide::at_rest(open, now);
        if !open {
            self.columns.panel_owner = None;
        }
    }

    /// Whether the board is being measured where the columns' slides end.
    pub(super) fn measuring_rest(&self) -> bool {
        self.nav_measure_rest.get() || self.nav_measure_full.get()
    }

    /// The nav's width where its slide ends.
    pub(super) fn nav_rest_w(&self) -> f32 {
        if !self.nav_measure_full.get() && self.nav_folded() {
            0.0
        } else {
            self.nav_expanded_width()
        }
    }

    /// The nav's width on screen this frame.
    pub(super) fn nav_width(&self) -> f32 {
        if self.measuring_rest() {
            self.nav_rest_w()
        } else {
            self.nav_expanded_width() * self.columns.nav.drawn
        }
    }

    /// The Pane whose panel is drawn this frame: the one open, or the one
    /// sliding shut.
    pub(super) fn panel_drawn_pane(&self) -> Option<usize> {
        self.panel_pane().or_else(|| {
            self.columns
                .panel_owner
                .and_then(|owner| self.index_of(owner))
        })
    }

    /// The panel's width on screen this frame.
    pub(super) fn panel_column_w(&self) -> f32 {
        if self.measuring_rest() {
            return self.panel_target_w();
        }
        self.panel_drawn_pane().map_or(0.0, |index| {
            self.panel_rest_w(index) * self.columns.panel.drawn
        })
    }
}

impl CockpitView {
    /// Whether the keyboard is inside a column that is sliding shut. It
    /// draws for a few frames more, but a column that is going holds no
    /// keys: the focus pump takes the keyboard back to where it goes when
    /// a column snaps shut, so ⌥⌘B still reaches the window and a shell or
    /// an editor in the panel hears nothing typed during the slide.
    pub(super) fn closing_holds_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        let panel_closing =
            self.panel_pane().is_none() && self.panel_closing_focus.contains_focused(window, cx);
        let nav_closing = self.nav_folded() && self.nav_focus.contains_focused(window, cx);
        panel_closing || nav_closing
    }
}

/// Over a column sliding out: it draws, and nothing in it hears the pointer.
pub(super) fn veil() -> Div {
    div()
        .debug_selector(|| "column-veil".into())
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .occlude()
}

#[cfg(test)]
mod tests {
    use super::clamp_nav_width;

    #[test]
    fn sidebar_width_preserves_the_brand_and_main_content() {
        assert_eq!(clamp_nav_width(256.0, 1440.0, 180.0), 256.0);
        assert_eq!(clamp_nav_width(100.0, 1440.0, 180.0), 208.0);
        assert_eq!(clamp_nav_width(100.0, 1440.0, 226.2), 227.0);
        assert_eq!(clamp_nav_width(1000.0, 1440.9, 180.0), 800.0);
        assert_eq!(clamp_nav_width(256.0, 700.0, 226.2), 227.0);
        assert_eq!(clamp_nav_width(f32::NAN, 1440.0, 180.0), 256.0);
        assert_eq!(clamp_nav_width(f32::INFINITY, 1440.0, 180.0), 256.0);
    }
}
