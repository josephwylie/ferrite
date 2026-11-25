//! The Settings sheet's views and the Project card, out of the Cockpit:
//! each owns its own state, reads what it draws of the window through a
//! `Source` (`ferrite_ui::source`) and says what the operator asks as an
//! event (ADR 0041). Nothing here names the Cockpit, so an edit to one
//! builds and tests without it.

pub mod connections;
pub mod folder_browser;
pub mod project_editor;

#[cfg(test)]
mod tests {
    /// Rule 10 over this crate's render code (`ferrite_ui::face`).
    #[test]
    fn render_code_draws_only_covered_glyphs() {
        ferrite_ui::face::assert_render_code_draws_only_covered_glyphs(&[
            ("connections.rs", include_str!("connections.rs")),
            ("folder_browser.rs", include_str!("folder_browser.rs")),
            ("project_editor.rs", include_str!("project_editor.rs")),
        ]);
    }
}
