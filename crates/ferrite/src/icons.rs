//! Every icon the prototype draws, compiled into the binary, plus the one
//! helper that places one. gpui hands the string in `.path(..)` to
//! `AssetSource::load` verbatim — there is no base directory and no search
//! path — so these keys are the whole naming scheme.
//!
//! The files are embedded with `include_bytes!` rather than read from disk:
//! a plain `cargo build` produces a bare binary with no app bundle beside
//! it, so any path an on-disk source could resolve would be the developer's
//! worktree, not the shipped program's.
//!
//! The seven line icons carry `fill`, `stroke`, `stroke-width` and the two
//! `stroke-linecap`/`linejoin` attributes on their root element. In the
//! prototype those come from a `.stroke` class in the page's stylesheet;
//! resvg parses a standalone file and never sees it, so an icon without
//! them renders as a filled blob. `branch.svg` carries the 1.65 stroke both
//! of its consumers override to. `codex.svg` and `claude.svg` are fill
//! logomarks with no stroke at all, their path data copied verbatim from
//! the prototype.
//!
//! An `svg()` with no text color in scope paints **nothing, silently**:
//! `icon()` always sets one.

use std::borrow::Cow;

use gpui::prelude::*;
use gpui::{px, rgb, svg, AssetSource, SharedString, Svg};

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        const ICONS: &[(&str, &[u8])] = &[
            $((
                concat!("icons/", $name, ".svg"),
                include_bytes!(concat!("../assets/icons/", $name, ".svg")),
            )),*
        ];
    };
}

icons![
    "sidebar",
    "chevron-down",
    "folder",
    "warning",
    "pencil",
    "check",
    "branch",
    "codex",
    "claude",
    "plus",
    "gear",
];

#[allow(dead_code)]
pub const SIDEBAR: &str = "icons/sidebar.svg";
#[allow(dead_code)]
pub const CHEVRON_DOWN: &str = "icons/chevron-down.svg";
#[allow(dead_code)]
pub const FOLDER: &str = "icons/folder.svg";
#[allow(dead_code)]
pub const WARNING: &str = "icons/warning.svg";
#[allow(dead_code)]
pub const PENCIL: &str = "icons/pencil.svg";
#[allow(dead_code)]
pub const CHECK: &str = "icons/check.svg";
#[allow(dead_code)]
pub const BRANCH: &str = "icons/branch.svg";
#[allow(dead_code)]
pub const CODEX: &str = "icons/codex.svg";
#[allow(dead_code)]
pub const CLAUDE: &str = "icons/claude.svg";
/// `+` — add a Project.
pub const PLUS: &str = "icons/plus.svg";
/// The settings gear.
pub const GEAR: &str = "icons/gear.svg";

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::new_static(name))
            .collect())
    }
}

/// One icon, square, tinted. gpui derives an SVG's scale from the element's
/// **width only** and centers the result, so the element must be square —
/// every icon here has a square viewBox.
#[allow(dead_code)]
pub fn icon(path: &'static str, size: f32, color: u32) -> Svg {
    svg()
        .path(path)
        .w(px(size))
        .h(px(size))
        .flex_shrink_0()
        .text_color(rgb(color))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every name a render site can write resolves to real bytes, and the
    /// bytes are an SVG. A typo here is a silently blank element at paint
    /// time, so it is caught at test time instead.
    #[test]
    fn every_icon_key_loads_an_svg() {
        for key in [
            SIDEBAR,
            CHEVRON_DOWN,
            FOLDER,
            WARNING,
            PENCIL,
            CHECK,
            BRANCH,
            CODEX,
            CLAUDE,
        ] {
            let bytes = Assets
                .load(key)
                .expect("the asset source never errors")
                .unwrap_or_else(|| panic!("{key} is embedded"));
            let svg = std::str::from_utf8(&bytes).expect("an SVG is text");
            assert!(svg.starts_with("<svg "), "{key} is an svg element");
            assert!(svg.contains("viewBox="), "{key} declares a viewBox");
        }
        assert_eq!(
            ICONS.len(),
            11,
            "the nine prototype icons, plus the add and settings marks"
        );
    }

    /// The line icons must carry the `.stroke` class's attributes on the
    /// root, or resvg renders them as filled blobs; the logomarks must not.
    #[test]
    fn line_icons_bake_the_stroke_class_and_logomarks_do_not() {
        for key in [SIDEBAR, CHEVRON_DOWN, FOLDER, WARNING, PENCIL, CHECK] {
            let bytes = Assets.load(key).unwrap().unwrap();
            let svg = std::str::from_utf8(&bytes).unwrap();
            assert!(svg.contains(r#"fill="none""#), "{key} does not fill");
            assert!(svg.contains(r#"stroke="currentColor""#), "{key} strokes");
            assert!(svg.contains(r#"stroke-width="1.5""#), "{key} is 1.5");
        }

        let branch = Assets.load(BRANCH).unwrap().unwrap();
        let branch = std::str::from_utf8(&branch).unwrap();
        assert!(
            branch.contains(r#"stroke-width="1.65""#),
            "both of branch.svg's consumers override the stroke to 1.65"
        );

        for key in [CODEX, CLAUDE] {
            let bytes = Assets.load(key).unwrap().unwrap();
            let svg = std::str::from_utf8(&bytes).unwrap();
            assert!(svg.contains(r#"fill="currentColor""#), "{key} fills");
            assert!(!svg.contains("stroke"), "{key} is a fill logomark");
        }
    }

    /// A name nothing embeds is `Ok(None)`, not an error: gpui asks for a
    /// path per frame and must not be handed a failure it cannot act on.
    #[test]
    fn an_unknown_key_is_absent_rather_than_an_error() {
        assert!(Assets.load("icons/nope.svg").unwrap().is_none());
        assert_eq!(Assets.list("icons/").unwrap().len(), 11);
    }
}
