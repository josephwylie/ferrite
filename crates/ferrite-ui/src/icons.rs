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
//! them renders as a filled blob. Every line icon strokes 1.33px at 16px
//! (`theme::ICON_STROKE`), its `stroke-width` scaled to its viewBox.
//! `codex.svg` and `claude.svg` are fill
//! logomarks with no stroke at all, their path data copied verbatim from
//! the prototype.
//!
//! An `svg()` with no text color in scope paints **nothing, silently**:
//! `icon()` always sets one.

use std::borrow::Cow;

use gpui::prelude::*;
use gpui::{div, px, rgb, svg, AnyElement, AssetSource, SharedString, Svg};

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
    "gear",
    "subagents",
    "window-minimize",
    "window-maximize",
    "window-restore",
    "window-close",
    "copy",
    "resend",
    "ferrite-mono",
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
    // (end WP-D)

    // ---- WP-E icons (append names above the end line)
    "board",
    // (end WP-E)

    // ---- WP-F icons (append names above the end line)
    "laptop",
    "plug",
    // (end WP-F)

    // ---- WP-G icons (append names above the end line)
    "bell-fill",
    // (end WP-G)
    // ---- Title-bar icons (Wave 2 lane N)
    "back",
    "forward",
    "diff",
    // (end title bar)
    "update",
    // ---- UI refresh icons (SPEC §3.7): Lucide's shapes on a 24 grid,
    // stroked 2 — the family's 1.33px at 16px.
    "panel-right",
    "search",
    "inbox",
    "archive",
    "plus",
    "terminal",
    "split-right",
    "split-down",
    "check-circle",
    "x-circle",
    "shield",
    "arrow-up",
    "arrow-down",
    "arrow-right",
    "corner-down-left",
    "git-pull-request",
    "git-merge",
    "refresh",
    "external",
    "maximize",
    // A plan's pending step (the transcript's task list).
    "circle-dashed",
    // A row's more menu (the nav's `⋯`).
    "ellipsis",
    // The Composer's `+` menu (Claude's marks).
    "paperclip",
    "square-slash",
    "blocks",
    // Reply to a selection (Claude's bent-back arrow).
    "reply",
    // The lightbox's save, its zoom; a page's card and the preview's
    // expand turned back.
    "download",
    "zoom-in",
    "globe",
    "minimize",
    // (end UI refresh)
];

pub const SIDEBAR: &str = "icons/sidebar.svg";
pub const CHEVRON_DOWN: &str = "icons/chevron-down.svg";
pub const CHEVRON_RIGHT: &str = "icons/chevron-right.svg";
pub const CLOSE: &str = "icons/close.svg";
/// A pending or delivered file that is not an image: a 12px outline page.
pub const FILE: &str = "icons/file.svg";
pub const FOLDER: &str = "icons/folder.svg";
pub const WARNING: &str = "icons/warning.svg";
pub const PENCIL: &str = "icons/pencil.svg";
pub const CHECK: &str = "icons/check.svg";
pub const BRANCH: &str = "icons/branch.svg";
pub const CODEX: &str = "icons/codex.svg";
pub const CLAUDE: &str = "icons/claude.svg";
const FERRITE_UPPER: &str = "icons/ferrite-upper.svg";
const FERRITE_LOWER: &str = "icons/ferrite-lower.svg";
/// The settings gear.
pub const GEAR: &str = "icons/gear.svg";
/// A parent Agent branching to two children. (The nav no longer draws it:
/// a row's subagents are its tooltip.)
#[allow(dead_code)]
pub const SUBAGENTS: &str = "icons/subagents.svg";
/// The four caption marks the Windows titlebar draws (`titlebar.rs`). They
/// are 10px chrome, not 16px UI: on a 10-unit viewBox the line family's
/// stroke would render a blob, so these carry the 1px hairline Windows' own
/// caption glyphs use, with a square cap — every one of them is an unjoined
/// straight run.
pub const WINDOW_MINIMIZE: &str = "icons/window-minimize.svg";
pub const WINDOW_MAXIMIZE: &str = "icons/window-maximize.svg";
pub const WINDOW_RESTORE: &str = "icons/window-restore.svg";
pub const WINDOW_CLOSE: &str = "icons/window-close.svg";
pub const COPY: &str = "icons/copy.svg";
#[cfg_attr(not(test), allow(dead_code))]
pub const RESEND: &str = "icons/resend.svg";
/// Ferrite's mark as one monochrome path, for the answer gutter (the
/// gradient mark is reserved for the animated working line).
pub const FERRITE_MONO: &str = "icons/ferrite-mono.svg";

// ---- WP-A icon names (append consts above the end line)
/// `∴` (not in Geist Mono): the reasoning row's gutter mark, three dots.
pub const REASONING: &str = "icons/reasoning.svg";
// (end WP-A)

// ---- WP-B icon names (append consts above the end line)
// (end WP-B)

// ---- WP-C icon names (append consts above the end line)
/// The notifications bell, in the line family (it replaces the kit's
/// Lucide bell, whose 24-box stroke read heavier than the gear beside it).
pub const BELL: &str = "icons/bell.svg";
// (end WP-C)

// ---- WP-D icon names (append consts above the end line)
/// An image attachment's mark (the prototype's `#image` symbol): a frame,
/// a ridge and a sun, in the 16-unit line family.
pub const IMAGE: &str = "icons/image.svg";
// (end WP-D)

// ---- WP-E icon names (append consts above the end line)
/// A board of Panes: three columns of cards, in the line family.
pub const BOARD: &str = "icons/board.svg";
// (end WP-E)

// ---- WP-F icon names (append consts above the end line)
/// The machine a Thread runs on (the Panel's Details: Environment), Lucide's
/// laptop on the 24 grid.
pub const LAPTOP: &str = "icons/laptop.svg";
/// An MCP server (the Panel's Details: Connectors), Lucide's plug.
pub const PLUG: &str = "icons/plug.svg";
// (end WP-F)

// ---- WP-G icon names (append consts above the end line)
/// The bell filled: the nav's Activity view while it shows.
pub const BELL_FILL: &str = "icons/bell-fill.svg";
// (end WP-G)

// ---- Title-bar icon names (Wave 2 lane N)
/// The head's history arrows: Claude's chevrons, taller than
/// `CHEVRON_RIGHT` (16 of the 24 grid, 11pt at the doors' 16).
pub const BACK: &str = "icons/back.svg";
pub const FORWARD: &str = "icons/forward.svg";
/// Review the branch's changes: Claude's `±` in a rounded square.
pub const DIFF: &str = "icons/diff.svg";
// (end title bar)

// ---- UI refresh icon names (SPEC §3.7; the phases after 0 draw them)
/// The right panel's toggle.
pub const PANEL_RIGHT: &str = "icons/panel-right.svg";
/// The nav's `Search` view and the palette's input.
pub const SEARCH: &str = "icons/search.svg";
/// The nav's `Needs you` view.
pub const INBOX: &str = "icons/inbox.svg";
/// The nav's `Parked` view.
pub const ARCHIVE: &str = "icons/archive.svg";
/// Add: a new Project, a new Thread, attach.
pub const PLUS: &str = "icons/plus.svg";
/// An idle Thread's mark, and `Main`'s tab: a square holding `>_`.
pub const TERMINAL: &str = "icons/terminal.svg";
/// Open the Beside slot to the right.
pub const SPLIT_RIGHT: &str = "icons/split-right.svg";
/// Open the Beside slot below.
#[allow(dead_code)]
pub const SPLIT_DOWN: &str = "icons/split-down.svg";
/// Done: a check in a ring (a shape, never a hue).
pub const CHECK_CIRCLE: &str = "icons/check-circle.svg";
/// Failed: a cross in a ring.
pub const X_CIRCLE: &str = "icons/x-circle.svg";
/// A plan's step still to do: a dashed ring (the task list).
pub const CIRCLE_DASHED: &str = "icons/circle-dashed.svg";
/// A row's more menu: `⋯`, under the pointer.
pub const ELLIPSIS: &str = "icons/ellipsis.svg";
/// The permission mode.
pub const SHIELD: &str = "icons/shield.svg";
/// The send circle while a turn runs and nothing is typed.
pub const STOP: &str = "icons/stop.svg";
/// Send; a commit not pushed.
pub const ARROW_UP: &str = "icons/arrow-up.svg";
/// A commit to pull.
pub const ARROW_DOWN: &str = "icons/arrow-down.svg";
/// A pull request's head branch into its base.
pub const ARROW_RIGHT: &str = "icons/arrow-right.svg";
/// The palette cursor row's enter hint.
pub const CORNER_DOWN_LEFT: &str = "icons/corner-down-left.svg";
/// Reply to a selection: quote it into the Composer.
pub const REPLY: &str = "icons/reply.svg";
/// An open, draft or closed pull request.
pub const GIT_PULL_REQUEST: &str = "icons/git-pull-request.svg";
/// A merged pull request.
pub const GIT_MERGE: &str = "icons/git-merge.svg";
/// Read the pull request again.
pub const REFRESH: &str = "icons/refresh.svg";
/// Open on GitHub.
pub const EXTERNAL: &str = "icons/external.svg";
/// Fill the board with a Pane.
pub const MAXIMIZE: &str = "icons/maximize.svg";
/// The `+` menu's `Add files or photos`: Lucide's paperclip.
pub const PAPERCLIP: &str = "icons/paperclip.svg";
/// The `+` menu's `Slash commands`: a slash in a square.
pub const SQUARE_SLASH: &str = "icons/square-slash.svg";
/// The `+` menu's `Connectors ›`: Lucide's blocks.
pub const BLOCKS: &str = "icons/blocks.svg";
/// Save a copy (the lightbox's download).
pub const DOWNLOAD: &str = "icons/download.svg";
/// Zoom an image between fit and its own size.
pub const ZOOM_IN: &str = "icons/zoom-in.svg";
/// A page: its card's mark until a picture of it is drawn.
pub const GLOBE: &str = "icons/globe.svg";
/// Turn an expanded preview back to its width (`MAXIMIZE`'s undo).
pub const MINIMIZE: &str = "icons/minimize.svg";
// (end UI refresh)

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

/// Ferrite's mark at rest: its two shards, drawn assembled. A finished
/// answer is not live, so its mark does not move.
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
            .text_color(rgb(theme::INK_STRONG))
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
            BELL_FILL,
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
            FERRITE_MONO,
            REASONING,
            IMAGE,
            BOARD,
        ]
        .into_iter()
        .chain(REFRESH_ICONS)
        .chain(TITLE_BAR_ICONS)
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
            30 + REFRESH_ICONS.len() + TITLE_BAR_ICONS.len(),
            "the prototype and app controls, including close, the four \
             Windows caption glyphs, the image mark, the CLI update mark and the board door; every glyph \
             Geist Mono lacks is typed in the face the platform falls back to (theme rule 10)"
        );
    }

    /// The line icons must carry the `.stroke` class's attributes on the
    /// root, or resvg renders them as filled blobs; the logomarks must not.
    #[test]
    fn line_icons_bake_the_stroke_class_and_logomarks_do_not() {
        // The sidebar toggle is the prototype's own drawing: a 20-unit box,
        // at the titlebar doors' 15px.
        let sidebar = Assets.load(SIDEBAR).unwrap().unwrap();
        let sidebar = std::str::from_utf8(&sidebar).unwrap();
        assert!(sidebar.contains(r#"fill="none""#));
        assert!(sidebar.contains(r#"stroke="currentColor""#));
        assert!(sidebar.contains(r#"viewBox="0 0 20 20""#));
        assert!(sidebar.contains(r#"<rect x="2.5" y="3" width="15" height="14" rx="2.5"/>"#));
        assert!(sidebar.contains(r#"<path d="M7.5 3v14"/>"#));

        for key in [
            SIDEBAR,
            BELL,
            CHECK,
            IMAGE,
            BRANCH,
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
        ]
        .into_iter()
        .chain(REFRESH_ICONS)
        .chain(TITLE_BAR_ICONS)
        {
            let bytes = Assets.load(key).unwrap().unwrap();
            let svg = std::str::from_utf8(&bytes).unwrap();
            assert!(svg.contains(r#"fill="none""#), "{key} does not fill");
            assert!(svg.contains(r#"stroke="currentColor""#), "{key} strokes");
            // One drawn weight whatever the viewBox: 1.33px at 16px.
            assert!(
                (effective_stroke(svg) - theme::ICON_STROKE).abs() < 0.01,
                "{key} draws {} at 16px, not {}",
                effective_stroke(svg),
                theme::ICON_STROKE
            );
        }

        // The image mark is the prototype's `#image` symbol as written:
        // square-ended, its sun filled.
        let image = Assets.load(IMAGE).unwrap().unwrap();
        let image = std::str::from_utf8(&image).unwrap();
        assert!(!image.contains("stroke-linecap"));

        for key in [CODEX, CLAUDE] {
            let bytes = Assets.load(key).unwrap().unwrap();
            let svg = std::str::from_utf8(&bytes).unwrap();
            assert!(svg.contains(r#"fill="currentColor""#), "{key} fills");
            assert!(!svg.contains("stroke"), "{key} is a fill logomark");
        }
    }

    /// The title bar's line icons (Wave 2), in the family.
    const TITLE_BAR_ICONS: [&str; 3] = [BACK, FORWARD, DIFF];

    /// The UI refresh's line icons (SPEC §3.7), all in the family.
    const REFRESH_ICONS: [&str; 32] = [
        PANEL_RIGHT,
        SEARCH,
        INBOX,
        ARCHIVE,
        PLUS,
        TERMINAL,
        SPLIT_RIGHT,
        SPLIT_DOWN,
        CHECK_CIRCLE,
        X_CIRCLE,
        SHIELD,
        ARROW_UP,
        ARROW_DOWN,
        ARROW_RIGHT,
        CORNER_DOWN_LEFT,
        GIT_PULL_REQUEST,
        GIT_MERGE,
        REFRESH,
        EXTERNAL,
        MAXIMIZE,
        CIRCLE_DASHED,
        ELLIPSIS,
        LAPTOP,
        PLUG,
        PAPERCLIP,
        SQUARE_SLASH,
        BLOCKS,
        REPLY,
        DOWNLOAD,
        ZOOM_IN,
        GLOBE,
        MINIMIZE,
    ];

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
}
