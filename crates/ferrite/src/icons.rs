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
use gpui::{
    div, point, px, rgb, svg, Animation, AnimationExt, AnyElement, AssetSource, ElementId,
    SharedString, Svg, Transformation,
};
use std::time::Duration;

use crate::theme;

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
    "chevron-right",
    "close",
    "folder",
    "warning",
    "pencil",
    "check",
    "branch",
    "codex",
    "claude",
    "ferrite-upper",
    "ferrite-lower",
    "plus",
    "gear",
    "list-filter",
    "group",
    "subagents",
    "window-minimize",
    "window-maximize",
    "window-restore",
    "window-close",
    "copy",
    "resend",
];

#[allow(dead_code)]
pub const SIDEBAR: &str = "icons/sidebar.svg";
#[allow(dead_code)]
pub const CHEVRON_DOWN: &str = "icons/chevron-down.svg";
pub const CHEVRON_RIGHT: &str = "icons/chevron-right.svg";
pub const CLOSE: &str = "icons/close.svg";
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
const FERRITE_UPPER: &str = "icons/ferrite-upper.svg";
const FERRITE_LOWER: &str = "icons/ferrite-lower.svg";
/// `+` — add a Project.
pub const PLUS: &str = "icons/plus.svg";
/// The settings gear.
pub const GEAR: &str = "icons/gear.svg";
/// Sort and grouping choices for the Thread list.
pub const LIST_FILTER: &str = "icons/list-filter.svg";
/// Four Panes held together as one durable Group.
pub const GROUP: &str = "icons/group.svg";
/// A parent Agent branching to two children.
pub const SUBAGENTS: &str = "icons/subagents.svg";
/// The four caption marks the Windows titlebar draws (`titlebar.rs`). They
/// are 10px chrome, not 16px UI: on a 10-unit viewBox the line family's 1.5
/// stroke would render a blob, so these carry the 1px hairline Windows' own
/// caption glyphs use, with a square cap — every one of them is an unjoined
/// straight run.
#[allow(dead_code)]
pub const WINDOW_MINIMIZE: &str = "icons/window-minimize.svg";
#[allow(dead_code)]
pub const WINDOW_MAXIMIZE: &str = "icons/window-maximize.svg";
#[allow(dead_code)]
pub const WINDOW_RESTORE: &str = "icons/window-restore.svg";
#[allow(dead_code)]
pub const WINDOW_CLOSE: &str = "icons/window-close.svg";
pub const COPY: &str = "icons/copy.svg";
pub const RESEND: &str = "icons/resend.svg";

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes))
            .or_else(|| gpui::assets::Assets::get(path).map(|asset| asset.data)))
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

/// Ferrite's mark at rest: the same two shards the animated icon uses, drawn
/// assembled. A finished answer is not live, so its mark does not move.
pub fn ferrite_icon(size: f32) -> AnyElement {
    let shard = |path| {
        svg()
            .absolute()
            .top_0()
            .left_0()
            .w(px(size))
            .h(px(size))
            .path(path)
            // GPUI skips `paint_svg` without a concrete text color, even
            // where the SVG paints only its own gradient.
            .text_color(rgb(theme::TEXT))
    };
    div()
        .relative()
        .flex_shrink_0()
        .w(px(size))
        .h(px(size))
        .child(shard(FERRITE_UPPER))
        .child(shard(FERRITE_LOWER))
        .into_any_element()
}

/// Ferrite's two shards pull apart and snap home on the supplied logo's
/// three-second timeline. GPUI rasterizes SVG rather than running its CSS, so
/// the two paths are embedded separately and their transforms run on GPUI's
/// animation clock. That also gives reduced-motion users the assembled mark.
pub fn animated_ferrite_icon(size: f32, id: impl Into<ElementId>) -> AnyElement {
    let id = id.into();
    let animation = || {
        Animation::new(Duration::from_millis(theme::FERRITE_SNAP_MS))
            .repeat_synced()
            .with_easing(ferrite_snap)
    };
    let shard = |path, id: ElementId, x: f32, y: f32| {
        svg()
            .absolute()
            .top_0()
            .left_0()
            .w(px(size))
            .h(px(size))
            .path(path)
            // GPUI skips `paint_svg` entirely without a concrete text color,
            // even when the SVG paints only its own gradient. Set it on the
            // shard itself because AnimationElement does not carry the
            // surrounding text style into the animated child.
            .text_color(rgb(theme::TEXT))
            .with_animation(id, animation(), move |shard, displacement| {
                shard.with_transformation(Transformation::translate(point(
                    px(x * size * displacement),
                    px(y * size * displacement),
                )))
            })
    };
    div()
        .relative()
        .flex_shrink_0()
        .w(px(size))
        .h(px(size))
        .child(shard(
            FERRITE_UPPER,
            (id.clone(), "upper").into(),
            theme::FERRITE_SHARD_X,
            -theme::FERRITE_SHARD_Y,
        ))
        .child(shard(
            FERRITE_LOWER,
            (id, "lower").into(),
            -theme::FERRITE_SHARD_X,
            theme::FERRITE_SHARD_Y,
        ))
        .into_any_element()
}

fn ferrite_snap(phase: f32) -> f32 {
    let [pull_x1, pull_y1, pull_x2, pull_y2] = theme::FERRITE_PULL_EASING;
    let [snap_x1, snap_y1, snap_x2, snap_y2] = theme::FERRITE_SNAP_EASING;
    match phase {
        phase if phase < theme::FERRITE_PULL_START => 0.0,
        phase if phase < theme::FERRITE_PULL_END => cubic_bezier(
            (phase - theme::FERRITE_PULL_START)
                / (theme::FERRITE_PULL_END - theme::FERRITE_PULL_START),
            pull_x1,
            pull_y1,
            pull_x2,
            pull_y2,
        ),
        phase if phase < theme::FERRITE_HOLD_END => 1.0,
        phase if phase < theme::FERRITE_SNAP_END => {
            1.0 - cubic_bezier(
                (phase - theme::FERRITE_HOLD_END)
                    / (theme::FERRITE_SNAP_END - theme::FERRITE_HOLD_END),
                snap_x1,
                snap_y1,
                snap_x2,
                snap_y2,
            )
        }
        _ => 0.0,
    }
}

/// Evaluate a CSS cubic-bezier easing at an x-position. The curve is
/// monotonic for both easings in the supplied artwork, so a small binary
/// search is stable and more than precise enough for a rendered frame.
fn cubic_bezier(x: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let component = |t: f32, first: f32, second: f32| {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * t * first + 3.0 * inverse * t * t * second + t * t * t
    };
    let mut low = 0.0;
    let mut high = 1.0;
    const SEARCH_STEPS: usize = 12;
    for _ in 0..SEARCH_STEPS {
        let t = (low + high) / 2.0;
        if component(t, x1, x2) < x {
            low = t;
        } else {
            high = t;
        }
    }
    component((low + high) / 2.0, y1, y2)
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
            CHEVRON_RIGHT,
            CLOSE,
            FOLDER,
            WARNING,
            PENCIL,
            CHECK,
            BRANCH,
            CODEX,
            CLAUDE,
            FERRITE_UPPER,
            FERRITE_LOWER,
            GEAR,
            LIST_FILTER,
            GROUP,
            SUBAGENTS,
            WINDOW_MINIMIZE,
            WINDOW_MAXIMIZE,
            WINDOW_RESTORE,
            WINDOW_CLOSE,
            COPY,
            RESEND,
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
            24,
            "the prototype and app controls, including disclosure and close,              and the four Windows caption glyphs"
        );
    }

    /// The line icons must carry the `.stroke` class's attributes on the
    /// root, or resvg renders them as filled blobs; the logomarks must not.
    #[test]
    fn line_icons_bake_the_stroke_class_and_logomarks_do_not() {
        for key in [
            SIDEBAR,
            CHEVRON_DOWN,
            CHEVRON_RIGHT,
            CLOSE,
            FOLDER,
            WARNING,
            PENCIL,
            CHECK,
            GEAR,
            LIST_FILTER,
            GROUP,
            SUBAGENTS,
            COPY,
            RESEND,
        ] {
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
        assert_eq!(Assets.list("icons/").unwrap().len(), 24);
    }

    #[test]
    fn ferrite_snap_matches_the_supplied_animation_timeline() {
        assert_eq!(ferrite_snap(0.0), 0.0);
        assert_eq!(ferrite_snap(theme::FERRITE_PULL_START), 0.0);
        assert_eq!(ferrite_snap(theme::FERRITE_PULL_END), 1.0);
        assert_eq!(ferrite_snap(theme::FERRITE_HOLD_END), 1.0);
        assert_eq!(ferrite_snap(theme::FERRITE_SNAP_END), 0.0);
        assert_eq!(ferrite_snap(1.0), 0.0);
    }
}
