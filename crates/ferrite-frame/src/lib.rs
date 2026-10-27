// The window's frame around the Cockpit (ADR 0035): the titlebar, the
// bottom bar, the Settings sheet and the key table's binding. The app
// installs `Frame` with its menu bar; the Cockpit fills it through
// `ferrite_ui::frame` and depends on none of this.
pub mod keys;
mod settings;
mod titlebar;

use gpui::{AnyElement, App, IntoElement, Menu, Window};

use ferrite_ui::frame::{Bar, SettingsSheet};
use ferrite_ui::keymap::Binding;

/// The frame, with the menu bar the app sets again whenever the keys are
/// bound, so its key equivalents follow.
pub struct Frame {
    pub menus: fn() -> Vec<Menu>,
}

impl ferrite_ui::frame::Frame for Frame {
    fn titlebar(&self, board_left: f32, draggable: bool, maximized: bool) -> AnyElement {
        titlebar::strip(board_left, draggable, maximized).into_any_element()
    }

    fn head_drag(&self, draggable: bool, maximized: bool) -> AnyElement {
        titlebar::head_drag(draggable, maximized).into_any_element()
    }

    fn bottom_bar(&self, bar: Bar) -> AnyElement {
        titlebar::bar(bar).into_any_element()
    }

    fn settings(&self, sheet: SettingsSheet, window: &mut Window, cx: &mut App) -> AnyElement {
        settings::draw(sheet, window, cx)
    }

    fn bind_keys(&self, table: Vec<Binding>, cx: &mut App) {
        keys::bind(table, cx);
        cx.set_menus((self.menus)());
    }
}

#[cfg(test)]
mod tests {
    /// Rule 10 over the frame's render code (`ferrite_ui::face`).
    #[test]
    fn render_code_draws_only_covered_glyphs() {
        ferrite_ui::face::assert_render_code_draws_only_covered_glyphs(&[
            ("lib.rs", include_str!("lib.rs")),
            ("titlebar.rs", include_str!("titlebar.rs")),
            ("settings.rs", include_str!("settings.rs")),
        ]);
    }
}
