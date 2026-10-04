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
    div, point, px, rgb, svg, AnyElement, App, AssetSource, ElementId, SharedString, Svg,
    Transformation, Window,
};
use std::time::Duration;

use crate::motion;
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
    "ferrite-steel",
    "gear",
    "subagents",
    "window-minimize",
    "window-maximize",
    "window-restore",
    "window-close",
    "copy",
    "resend",
    "prompt",
    "ferrite-mono",
    "option",
    "control",
    // ---- foundation: the spinner frames (theme rule 8; Geist Mono has
    // neither braille nor the dingbat stars, so every frame is drawn)
    "braille-0",
    "braille-1",
    "braille-2",
    "braille-3",
    "braille-4",
    "braille-5",
    "braille-6",
    "braille-7",
    "braille-8",
    "braille-9",
    "working-dot",
    "working-four",
    "working-eight",
    "working-star",
    "working-six",
    "working-heavy",
    // (end foundation)
    // ---- WP-A icons (append names above the end line)
    "reasoning",
    // (end WP-A)

    // ---- WP-B icons (append names above the end line)
    // (end WP-B)

    // ---- WP-C icons (append names above the end line)
    "bell",
    // (end WP-C)

    // ---- WP-D icons (append names above the end line)
    "stop",
    "file",
    "image",
    "mode-accept",
    "mode-on",
    "mode-plan",
    // (end WP-D)

    // ---- WP-E icons (append names above the end line)
    "command",
    "cross",
    // (end WP-E)

    // ---- WP-F icons (append names above the end line)
    "diamond",
    // (end WP-F)

    // ---- WP-G icons (append names above the end line)
    "disclosure-right",
    "disclosure-down",
    // (end WP-G)
    "update",
];

#[allow(dead_code)]
pub const SIDEBAR: &str = "icons/sidebar.svg";
#[allow(dead_code)]
pub const CHEVRON_DOWN: &str = "icons/chevron-down.svg";
pub const CHEVRON_RIGHT: &str = "icons/chevron-right.svg";
pub const CLOSE: &str = "icons/close.svg";
/// A pending or delivered file that is not an image: a 12px outline page.
pub const FILE: &str = "icons/file.svg";
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
/// The mark whole, in its steel gradient, cropped to the shards (the
/// prototype's banner `viewBox="280 30 700 1130"`): drawn as an image, so
/// the gradient paints (an `svg()` element is a one-ink mask).
pub const FERRITE_STEEL: &str = "icons/ferrite-steel.svg";
/// The settings gear.
pub const GEAR: &str = "icons/gear.svg";
/// A parent Agent branching to two children. (The nav no longer draws it:
/// a row's subagents are its tooltip.)
#[allow(dead_code)]
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
#[cfg_attr(not(test), allow(dead_code))]
pub const RESEND: &str = "icons/resend.svg";
/// The prompt mark `❯`, drawn: Geist Mono has no U+276F, so the mark the
/// transcript, the Composer, the nav cursor and every selection bar share
/// is the heavy wedge the prototype's fallback face draws at 13px — filled,
/// 5 × 9.5px with ~2px arms and angled ends, 0.9px into a 12-unit cell box
/// (`components::prompt_mark` lays it).
pub const PROMPT: &str = "icons/prompt.svg";
/// Ferrite's mark as one monochrome path, for the answer gutter (the
/// gradient mark is reserved for the animated working line).
#[allow(dead_code)]
pub const FERRITE_MONO: &str = "icons/ferrite-mono.svg";

// ---- foundation: spinner frames (theme rule 8)
/// The braille spinner's ten frames in order, `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`: every frame
/// draws the whole 2×3 cell, its raised dots filled and the rest as thin
/// rings (as the prototype's braille face draws them), the cell's centre
/// ~1px above the line's in a 12-unit box: `components::braille_spinner`
/// draws them at `MOTION_BRAILLE_FRAME_MS`.
#[allow(dead_code)]
pub const BRAILLE_FRAMES: [&str; 10] = [
    "icons/braille-0.svg",
    "icons/braille-1.svg",
    "icons/braille-2.svg",
    "icons/braille-3.svg",
    "icons/braille-4.svg",
    "icons/braille-5.svg",
    "icons/braille-6.svg",
    "icons/braille-7.svg",
    "icons/braille-8.svg",
    "icons/braille-9.svg",
];
/// The working line's glyphs, drawn: `·`, `✢` (four teardrops), `✳` (eight
/// spokes), `✶` (six-pointed star), `✻` (six teardrops), `✽` (eight heavy
/// teardrops). Fill marks in a 12-unit box, `✳` a round-capped stroke.
#[allow(dead_code)]
pub const WORKING_DOT: &str = "icons/working-dot.svg";
#[allow(dead_code)]
pub const WORKING_FOUR: &str = "icons/working-four.svg";
#[allow(dead_code)]
pub const WORKING_EIGHT: &str = "icons/working-eight.svg";
#[allow(dead_code)]
pub const WORKING_STAR: &str = "icons/working-star.svg";
#[allow(dead_code)]
pub const WORKING_SIX: &str = "icons/working-six.svg";
#[allow(dead_code)]
pub const WORKING_HEAVY: &str = "icons/working-heavy.svg";
/// The working spinner's ten frames, Claude Code's cycle out and back:
/// `· ✢ ✳ ✶ ✻ ✽ ✻ ✶ ✳ ✢` at `MOTION_WORKING_FRAME_MS`
/// (`components::working_spinner`). Frame 4, `✻`, is its still state.
#[allow(dead_code)]
pub const WORKING_FRAMES: [&str; 10] = [
    WORKING_DOT,
    WORKING_FOUR,
    WORKING_EIGHT,
    WORKING_STAR,
    WORKING_SIX,
    WORKING_HEAVY,
    WORKING_SIX,
    WORKING_STAR,
    WORKING_EIGHT,
    WORKING_FOUR,
];
/// `✻`, still: a finished turn's mark (`✻ Worked for 41s`) and the working
/// spinner under reduced motion.
#[allow(dead_code)]
pub const WORKED: &str = WORKING_SIX;
// (end foundation)

// ---- WP-A icon names (append consts above the end line)
/// `∴` (not in Geist Mono): the reasoning row's gutter mark, three dots.
pub const REASONING: &str = "icons/reasoning.svg";
// (end WP-A)

// ---- WP-B icon names (append consts above the end line)
// (end WP-B)

// ---- WP-C icon names (append consts above the end line)
/// The notifications bell, in the line family (it replaces the kit's
/// Lucide bell, whose 24-box stroke read heavier than the gear beside it).
#[allow(dead_code)]
pub const BELL: &str = "icons/bell.svg";
// (end WP-C)

/// `⌘` (not in Geist Mono): the command key wherever a key combination is
/// drawn — menu shortcuts, keycaps. Only `components::key_combo` places it.
pub const COMMAND: &str = "icons/command.svg";
/// `⌥` and `⌃` (in neither bundled face): the option and control keys,
/// placed only by `components::key_combo` like `COMMAND`.
pub const OPTION: &str = "icons/option.svg";
pub const CONTROL: &str = "icons/control.svg";

// ---- WP-D icon names (append consts above the end line)
/// An image attachment's mark (the prototype's `#image` symbol): a frame,
/// a ridge and a sun, in the 16-unit line family.
pub const IMAGE: &str = "icons/image.svg";
/// The permission mode's marker in the status line, drawn (Geist Mono has
/// neither `⏵` nor `⏸`): `⏵⏵` for a mode that lets edits run, `⏸` for
/// plan, `⏵` for any other. Fill marks in a 12-unit box.
pub const MODE_ACCEPT: &str = "icons/mode-accept.svg";
pub const MODE_ON: &str = "icons/mode-on.svg";
pub const MODE_PLAN: &str = "icons/mode-plan.svg";
// (end WP-D)

// ---- WP-E icon names (append consts above the end line)
/// `✗`, drawn (Geist Mono lacks it): a failed turn's mark in the
/// notifications list, the line family's cross.
pub const CROSS: &str = "icons/cross.svg";
// (end WP-E)

// ---- WP-F icon names (append consts above the end line)
/// `◆`, drawn (Geist Mono lacks it): a Decision's mark — the card's head and
/// the L2 cell's. A fill glyph, like the logomarks.
pub const DIAMOND: &str = "icons/diamond.svg";
// (end WP-F)

// ---- WP-G icon names (append consts above the end line)
/// `▸` and `▾`, drawn (Geist Mono has neither small triangle): the nav's
/// disclosure marks — open and folded Projects and Groups, and the Parked
/// fold's `▸` (turned a quarter when open). Fill marks in a 12-unit box
/// centred on their character cell, each fitted to the prototype's glyph at
/// 2× (about 4.9 × 4.95 px, a hair below the line's centre).
pub const DISCLOSURE_RIGHT: &str = "icons/disclosure-right.svg";
pub const DISCLOSURE_DOWN: &str = "icons/disclosure-down.svg";
// (end WP-G)

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
/// the two paths are embedded separately and translated by hand. The phase
/// comes from the shared pulse clock (`motion::pulse_phase`), not a
/// per-frame `with_animation` loop: the painting view re-renders at ~30fps
/// while the mark is mounted and nothing is scheduled once it is gone. Every
/// mark reads the same clock, so no per-element state keys it and `_id` is
/// kept for callers only. Reduced motion holds the assembled mark.
pub fn animated_ferrite_icon(size: f32, _id: impl Into<ElementId>) -> AnyElement {
    FerriteSnap { size }.into_any_element()
}

#[derive(IntoElement)]
struct FerriteSnap {
    size: f32,
}

impl RenderOnce for FerriteSnap {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let period = Duration::from_millis(theme::FERRITE_SNAP_MS);
        let displacement = ferrite_snap(motion::pulse_phase(period, window.current_view(), cx));
        let size = self.size;
        let shard = |path, x: f32, y: f32| {
            svg()
                .absolute()
                .top_0()
                .left_0()
                .w(px(size))
                .h(px(size))
                .path(path)
                // GPUI skips `paint_svg` entirely without a concrete text
                // color, even when the SVG paints only its own gradient.
                .text_color(rgb(theme::TEXT))
                .with_transformation(Transformation::translate(point(
                    px(x * size * displacement),
                    px(y * size * displacement),
                )))
        };
        div()
            .relative()
            .flex_shrink_0()
            .w(px(size))
            .h(px(size))
            .child(shard(
                FERRITE_UPPER,
                theme::FERRITE_SHARD_X,
                -theme::FERRITE_SHARD_Y,
            ))
            .child(shard(
                FERRITE_LOWER,
                -theme::FERRITE_SHARD_X,
                theme::FERRITE_SHARD_Y,
            ))
    }
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
            BELL,
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
            SUBAGENTS,
            WINDOW_MINIMIZE,
            WINDOW_MAXIMIZE,
            WINDOW_RESTORE,
            WINDOW_CLOSE,
            COPY,
            RESEND,
            PROMPT,
            FERRITE_MONO,
            DIAMOND,
            REASONING,
            OPTION,
            CONTROL,
            IMAGE,
            MODE_ACCEPT,
            MODE_ON,
            MODE_PLAN,
            CROSS,
            WORKING_DOT,
            WORKING_FOUR,
            WORKING_EIGHT,
            WORKING_STAR,
            WORKING_SIX,
            WORKING_HEAVY,
        ]
        .into_iter()
        .chain(BRAILLE_FRAMES)
        {
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
            56,
            "the prototype and app controls, the steel banner mark, including close, the four Windows caption glyphs, \
             the option and control key glyphs, the image mark, the three mode markers, the \
             failed cross, the CLI update mark, the sixteen spinner frames (ten braille, six \
             working glyphs) and the nav's two disclosure triangles"
        );
    }

    /// F-16: the prompt mark is U+276F's heavy wedge — filled, no stroke,
    /// from x 0.9 to 5.9 and y 1.55 to 11.05 of its 12-unit box (a 5 × 9.5px
    /// mark at 13px), its arms 2.15 across with parallel, angled ends.
    #[test]
    fn the_prompt_mark_is_the_heavy_wedge() {
        let bytes = Assets.load(PROMPT).unwrap().unwrap();
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(svg.contains(r#"viewBox="0 0 12 12""#));
        assert!(svg.contains(r#"fill="currentColor""#));
        assert!(
            !svg.contains("stroke"),
            "a filled wedge, not a stroked chevron"
        );
        for corner in [
            "M0.9 1.55",
            "H3.05",
            "L5.9 6.3",
            "L3.05 11.05",
            "H0.9",
            "L3.75 6.3",
        ] {
            assert!(svg.contains(corner), "{corner}");
        }
    }

    /// The line icons must carry the `.stroke` class's attributes on the
    /// root, or resvg renders them as filled blobs; the logomarks must not.
    #[test]
    fn line_icons_bake_the_stroke_class_and_logomarks_do_not() {
        // The sidebar toggle is the prototype's own drawing: a 20-unit box
        // stroked 1.6, at the titlebar doors' 15px.
        let sidebar = Assets.load(SIDEBAR).unwrap().unwrap();
        let sidebar = std::str::from_utf8(&sidebar).unwrap();
        assert!(sidebar.contains(r#"fill="none""#));
        assert!(sidebar.contains(r#"stroke="currentColor""#));
        assert!(sidebar.contains(r#"viewBox="0 0 20 20""#));
        assert!(sidebar.contains(r#"stroke-width="1.6""#));
        assert!(sidebar.contains(r#"<rect x="2.5" y="3" width="15" height="14" rx="2.5"/>"#));
        assert!(sidebar.contains(r#"<path d="M7.5 3v14"/>"#));

        // The bell is the prototype's own too: a 16-unit box stroked 1.4.
        let bell = Assets.load(BELL).unwrap().unwrap();
        let bell = std::str::from_utf8(&bell).unwrap();
        assert!(bell.contains(r#"stroke-width="1.4""#));
        assert!(bell.contains(r#"viewBox="0 0 16 16""#));
        // The check is the picker's `✓` as the prototype's face draws it: a
        // 12-unit box stroked 1.15.
        let check = Assets.load(CHECK).unwrap().unwrap();
        let check = std::str::from_utf8(&check).unwrap();
        assert!(check.contains(r#"viewBox="0 0 12 12""#));
        assert!(check.contains(r#"stroke-width="1.15""#));
        // The failed cross is the prototype's `✗` glyph, drawn the same way:
        // a 12-unit box stroked 1.2.
        let cross = Assets.load(CROSS).unwrap().unwrap();
        let cross = std::str::from_utf8(&cross).unwrap();
        assert!(cross.contains(r#"viewBox="0 0 12 12""#));
        assert!(cross.contains(r#"stroke-width="1.2""#));

        for key in [
            CHEVRON_DOWN,
            CHEVRON_RIGHT,
            CLOSE,
            FOLDER,
            WARNING,
            PENCIL,
            GEAR,
            SUBAGENTS,
            COPY,
            RESEND,
            IMAGE,
        ] {
            let bytes = Assets.load(key).unwrap().unwrap();
            let svg = std::str::from_utf8(&bytes).unwrap();
            assert!(svg.contains(r#"fill="none""#), "{key} does not fill");
            assert!(svg.contains(r#"stroke="currentColor""#), "{key} strokes");
            // One drawn weight whatever the viewBox: 1.5px at 16px.
            assert!(
                (effective_stroke(svg) - 1.5).abs() < 0.01,
                "{key} draws {} at 16px, not 1.5",
                effective_stroke(svg)
            );
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

    /// `stroke-width × 16 / viewBox` — the stroke an icon draws at 16px.
    fn effective_stroke(svg: &str) -> f32 {
        let attr = |name: &str| {
            let at = svg.find(&format!("{name}=\"")).unwrap() + name.len() + 2;
            let end = at + svg[at..].find('"').unwrap();
            svg[at..end].to_string()
        };
        let stroke: f32 = attr("stroke-width").parse().unwrap();
        let view: f32 = attr("viewBox")
            .split_whitespace()
            .nth(2)
            .unwrap()
            .parse()
            .unwrap();
        stroke * 16.0 / view
    }

    /// A name nothing embeds is `Ok(None)`, not an error: gpui asks for a
    /// path per frame and must not be handed a failure it cannot act on.
    #[test]
    fn an_unknown_key_is_absent_rather_than_an_error() {
        assert!(Assets.load("icons/nope.svg").unwrap().is_none());
        assert_eq!(Assets.list("icons/").unwrap().len(), ICONS.len());
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
