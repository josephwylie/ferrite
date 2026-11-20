//! The toast float (FL-18): the newest standing toast, top right of the
//! transcript's column, and the kit's toasts kept to the same column.

use super::*;
use crate::float::{self, FloatPlace};

impl CockpitView {
    /// The newest standing toast (FL-18), top right of the transcript's
    /// column where it covers the least (theme `TOAST_*`): under the Pane
    /// heads, clear of every Composer, and never over the right panel. It
    /// drops in when it comes, over the face of the one before it while
    /// that fades away (`notifications::ToastStage`). The kit's toasts keep
    /// to the same column (`toast_margins`).
    pub(super) fn toast_float(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let machines = self.machines.as_ref().map(|machines| machines.read(cx));
        let standing = self
            .needs_you
            .toasts()
            .last()
            .filter(|_| self.nav_folded())
            .and_then(|toast| {
                self.alert_row(&toast.alert, machines, ferrite_core::clock::system_time())
            });
        let leaving = self.toast_stage.borrow_mut().stage(
            standing.as_ref(),
            cx.background_executor().now(),
            ferrite_ui::motion::reduced_motion(cx),
        );
        use ferrite_ui::theme::{PANE_HEAD_H, TOAST_BOX_RIGHT, TOAST_W, WIN_CHROME_H};
        let viewport = window.viewport_size();
        self.toast_margins(cx);
        // What it hangs under, at its width and right edge: the window's
        // head and a Pane's head row, `TOAST_BOX_RIGHT` in from the board's
        // right edge (the panel's left while it shows).
        let right = self.panel_column_w() + self.task_panel_w() + TOAST_BOX_RIGHT;
        let room = f32::from(viewport.width) - self.nav_width() - right - TOAST_BOX_RIGHT;
        let width = TOAST_W.min(room.max(0.));
        let over = WIN_CHROME_H + PANE_HEAD_H;
        let anchor = gpui::Bounds::new(
            gpui::point(viewport.width - px(right + width), px(0.)),
            gpui::size(px(width), px(over)),
        );
        let hang = |toast: AnyElement| {
            float::hang_element(
                toast,
                anchor,
                FloatPlace::Below {
                    gap: ferrite_ui::theme::TOAST_TOP,
                },
            )
        };
        let leaving = leaving.map(|(row, left)| {
            hang(
                ferrite_ui::motion::toast_out(left, crate::notifications::toast_face(&row, width))
                    .into_any_element(),
            )
        });
        let standing = standing.map(|row| {
            let toast = crate::notifications::toast(&row, self.notice_handle(cx), width);
            hang(
                ferrite_ui::motion::toast_in(crate::notifications::toast_key(&row), toast)
                    .into_any_element(),
            )
        });
        leaving.into_iter().chain(standing).collect()
    }

    /// Keep the kit's toast stack (`theme.notification`) on the
    /// transcript's column: its right margin follows the panel, written
    /// only when it moves.
    fn toast_margins(&self, cx: &mut gpui::App) {
        let right =
            px(self.panel_column_w() + self.task_panel_w() + ferrite_ui::theme::TOAST_BOX_RIGHT);
        if gpui::component::Theme::global(cx)
            .notification
            .margins
            .right
            != right
        {
            gpui::component::Theme::global_mut(cx)
                .notification
                .margins
                .right = right;
        }
    }
}
