// Ferrite's Pane: the visible cell for one Thread. Its transcript, its
// Composer and the Draft, and what they draw with.
mod attachments;
pub mod composer;
pub mod decision;
pub mod file_links;
pub mod find;
pub mod hover_card;
pub mod image_viewer;
mod line;
pub mod mark;
pub mod pane;
pub mod preview;
pub mod prompt_drop;
pub mod rich;
pub mod select;
pub mod transcript;
pub mod visual;

#[cfg(test)]
mod tests {
    /// Rule 10 over the Pane's render code (`ferrite_ui::face`).
    #[test]
    fn render_code_draws_only_covered_glyphs() {
        ferrite_ui::face::assert_render_code_draws_only_covered_glyphs(&[
            ("lib.rs", include_str!("lib.rs")),
            ("pane.rs", include_str!("pane.rs")),
            ("pane/text.rs", include_str!("pane/text.rs")),
            ("transcript.rs", include_str!("transcript.rs")),
            ("transcript/rows.rs", include_str!("transcript/rows.rs")),
            ("transcript/scroll.rs", include_str!("transcript/scroll.rs")),
            ("rich.rs", include_str!("rich.rs")),
            ("mark.rs", include_str!("mark.rs")),
            ("composer.rs", include_str!("composer.rs")),
            ("attachments.rs", include_str!("attachments.rs")),
            ("image_viewer.rs", include_str!("image_viewer.rs")),
            ("decision.rs", include_str!("decision.rs")),
            ("preview.rs", include_str!("preview.rs")),
            ("prompt_drop.rs", include_str!("prompt_drop.rs")),
            ("file_links.rs", include_str!("file_links.rs")),
        ]);
    }
}
