// Ferrite's Pane: the visible cell for one Thread. Its transcript, its
// Composer and the Draft band, and what they draw with.
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

// The shared UI, under the names the Pane has always used for it.
use ferrite_ui::{components, icons, keymap, loops_overlay, menu, motion, pointer, theme};
