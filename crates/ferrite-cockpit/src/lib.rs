// The Cockpit: the grid, the nav, the Roster glue and the window's
// content. The app (`ferrite`) opens the window on it and owns the frame
// around it (`ferrite_ui::frame`, ADR 0035).
mod background_chips;
#[cfg(unix)]
mod board;
mod cli_updates;
pub mod cockpit;
pub mod cockpit_window;
pub mod demo;
mod facts;
pub mod ferrite_update;
mod float;
mod fuzzy;
pub mod machines;
mod nav;
mod notifications;
pub mod palette;
mod pr;
mod project_editor;
mod projects;
mod review;
pub mod shortcuts;
#[cfg(unix)]
mod sidebar;
mod status;
mod system_notifications;

/// Whether this build is an unreleased one. `--release` is not the
/// question — a locally built release binary is still a dev build, and
/// wants the `dev` mark. Only the release pipeline ships without it, which it
/// says by setting `FERRITE_RELEASE` for the compile (`build.rs` tracks
/// the variable so a cached build cannot keep a stale answer). Settings ›
/// About reports it.
pub const DEV: bool = option_env!("FERRITE_RELEASE").is_none();

/// The window's frame the Cockpit fills (ADR 0035): the app's. A test
/// build draws ferrite-frame's, as the app does.
pub(crate) fn frame() -> &'static dyn ferrite_ui::frame::Frame {
    #[cfg(test)]
    ferrite_ui::frame::install(ferrite_frame::Frame { menus: Vec::new });
    ferrite_ui::frame::active()
}

#[cfg(test)]
mod tests {
    /// Rule 10 over the Cockpit's render code (`ferrite_ui::face`).
    #[test]
    fn render_code_draws_only_covered_glyphs() {
        ferrite_ui::face::assert_render_code_draws_only_covered_glyphs(&[
            ("lib.rs", include_str!("lib.rs")),
            ("cockpit.rs", include_str!("cockpit.rs")),
            ("cockpit/subagents.rs", include_str!("cockpit/subagents.rs")),
            (
                "cockpit/panel/preview.rs",
                include_str!("cockpit/panel/preview.rs"),
            ),
            ("nav.rs", include_str!("nav.rs")),
            ("project_editor.rs", include_str!("project_editor.rs")),
            ("notifications.rs", include_str!("notifications.rs")),
            ("background_chips.rs", include_str!("background_chips.rs")),
        ]);
    }
}
