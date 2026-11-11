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
//! them renders as a filled blob. Every line icon strokes
//! `theme::ICON_STROKE` at whatever size it is drawn: `icon()` names the
//! size in the key (`drawn_at`), and `Assets` rewrites the baked
//! `stroke-width` against the viewBox for it (`stroke_at`). A baked width
//! alone scales with the box: Lucide's 2 on 24 is 1.33px at 16, 1.17 at 14.
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
    // stroked 2 as Lucide ships them (`icon()` restrokes them to 1px).
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
    "circle-minus",
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
    "chevrons-down-up",
    "list",
    "code",
    "pin",
    "clock",
    // (end UI refresh)
    // A side chat's Surface tab (Lucide's messages-square, T3's).
    "messages-square",
    // The nav's Thread card (T3 Code's): its status words' glyphs, the PR
    // chip's draft, the worktree on its checkout line, a pin's undo.
    "shield-question",
    "message-circle-question",
    "circle-alert",
    "git-pull-request-draft",
    "folder-git-2",
    "pin-off",
    // A transcript work row's gutter (T3 Code's `TimelineToolActivityIcon`).
    "square-pen",
    "eye",
    "brain",
    "hammer",
    "wrench",
    "bot",
    "zap",
    "message-circle",
    "list-todo",
    "app-window",
    // The Panel's Surfaces: a side chat's delete, the Changes toolbar's
    // view tools.
    "trash-2",
    "rows-3",
    "columns-2",
    "text-wrap",
    "pilcrow",
    "flask-conical-off",
    "folder-tree",
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
/// A plan's step the turn stopped on: a bar in a ring (the task list).
pub const CIRCLE_MINUS: &str = "icons/circle-minus.svg";
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
/// Files: collapse every folder.
pub const COLLAPSE_ALL: &str = "icons/chevrons-down-up.svg";
/// Files: the tree's door; Changes: its file list.
pub const LIST: &str = "icons/list.svg";
/// A source file in the Files tree (Claude's `<>`).
pub const CODE: &str = "icons/code.svg";
/// A pinned Thread: the bell's row action.
pub const PIN: &str = "icons/pin.svg";
pub const CLOCK: &str = "icons/clock.svg";
// (end UI refresh)
/// A side chat: its Surface tab and the + menu's row.
pub const MESSAGES_SQUARE: &str = "icons/messages-square.svg";
/// A Thread card's status (`nav::card`): an approval or plan waits, a
/// question waits, a run failed or its tests fail.
pub const SHIELD_QUESTION: &str = "icons/shield-question.svg";
pub const MESSAGE_CIRCLE_QUESTION: &str = "icons/message-circle-question.svg";
pub const CIRCLE_ALERT: &str = "icons/circle-alert.svg";
/// A draft PR on a Thread card's chip.
pub const GIT_PULL_REQUEST_DRAFT: &str = "icons/git-pull-request-draft.svg";
/// A Thread working in a worktree, on its card's checkout line.
pub const FOLDER_GIT: &str = "icons/folder-git-2.svg";
/// A pinned card's pin under the pointer: the press unpins.
pub const PIN_OFF: &str = "icons/pin-off.svg";
/// A transcript work row's glyphs (`tools::ToolIcon`), Lucide's.
pub const SQUARE_PEN: &str = "icons/square-pen.svg";
pub const EYE: &str = "icons/eye.svg";
pub const BRAIN: &str = "icons/brain.svg";
pub const HAMMER: &str = "icons/hammer.svg";
pub const WRENCH: &str = "icons/wrench.svg";
pub const BOT: &str = "icons/bot.svg";
pub const ZAP: &str = "icons/zap.svg";
pub const MESSAGE_CIRCLE: &str = "icons/message-circle.svg";
pub const LIST_TODO: &str = "icons/list-todo.svg";
pub const APP_WINDOW: &str = "icons/app-window.svg";
/// A side chat's delete, in its Surface's head.
pub const TRASH: &str = "icons/trash-2.svg";
/// The Changes toolbar: Stacked and Split, Wrap, Whitespace, Hide test
/// files, File tree.
pub const ROWS: &str = "icons/rows-3.svg";
pub const COLUMNS: &str = "icons/columns-2.svg";
pub const TEXT_WRAP: &str = "icons/text-wrap.svg";
pub const PILCROW: &str = "icons/pilcrow.svg";
pub const FLASK_OFF: &str = "icons/flask-conical-off.svg";
pub const FOLDER_TREE: &str = "icons/folder-tree.svg";

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let drawn = path.rsplit_once('@').and_then(|(path, drawn)| {
            let (size, stroke) = drawn.split_once('/').unwrap_or((drawn, ""));
            let stroke = if stroke.is_empty() {
                theme::ICON_STROKE
            } else {
                stroke.parse().ok()?
            };
            Some((path, size.parse::<f32>().ok()?, stroke))
        });
        let (path, drawn) = match drawn {
            Some((path, size, stroke)) => (path, Some((size, stroke))),
            None => (path, None),
        };
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| match drawn {
                Some((size, stroke)) => Cow::Owned(stroke_at(bytes, size, stroke)),
                None => Cow::Borrowed(*bytes),
            })
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
        .path(drawn_at(path, size))
        .w(px(size))
        .h(px(size))
        .flex_shrink_0()
        .text_color(rgb(color))
}

/// [`icon`] stroked `stroke` px at its `size` rather than
/// `theme::ICON_STROKE`, in `color` (its alpha kept): a work row's Lucide
/// glyph at Lucide's own weight (`theme::WORK_ICON_STROKE`).
pub fn icon_stroked(path: &'static str, size: f32, stroke: f32, color: gpui::Hsla) -> Svg {
    svg()
        .path(SharedString::from(format!("{path}@{size}/{stroke}")))
        .w(px(size))
        .h(px(size))
        .flex_shrink_0()
        .text_color(color)
}

/// The key of `path` drawn `size` px square: `Assets` strokes it at
/// `theme::ICON_STROKE` for that size. gpui rasterises an SVG per key and
/// device size, and hands `AssetSource::load` only the key, so the size
/// rides in it.
fn drawn_at(path: &str, size: f32) -> SharedString {
    format!("{path}@{size}").into()
}

/// `svg` with every `stroke-width` set so that, scaled from its viewBox to
/// `size` px, it strokes `theme::ICON_STROKE`. An SVG with no stroke (a
/// fill logomark) is returned as it is.
fn stroke_at(svg: &[u8], size: f32, stroke: f32) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(svg) else {
        return svg.to_vec();
    };
    let view = text
        .split_once(r#"viewBox=""#)
        .and_then(|(_, rest)| rest.split('"').next())
        .and_then(|view| view.split_whitespace().nth(2)?.parse::<f32>().ok());
    let Some(view) = view else {
        return svg.to_vec();
    };
    let width = format!("{}", stroke * view / size);
    let mut pieces = text.split(r#"stroke-width=""#);
    let mut out = pieces.next().unwrap_or_default().to_string();
    for piece in pieces {
        let rest = piece.split_once('"').map_or("", |(_, rest)| rest);
        out.push_str(r#"stroke-width=""#);
        out.push_str(&width);
        out.push('"');
        out.push_str(rest);
    }
    out.into_bytes()
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
            IMAGE,
            BOARD,
        ]
        .into_iter()
        .chain(REFRESH_ICONS)
        .chain(TITLE_BAR_ICONS)
        .chain(WORK_ROW_ICONS)
        {
            let bytes = Assets
                .load(key)
                .expect("the asset source never errors")
                .unwrap_or_else(|| panic!("{key} is embedded"));
            let svg = std::str::from_utf8(&bytes).expect("an SVG is text");
            assert!(svg.starts_with("<svg "), "{key} is an svg element");
            assert!(svg.contains("viewBox="), "{key} declares a viewBox");
        }
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
        .chain(WORK_ROW_ICONS)
        {
            let bytes = Assets.load(key).unwrap().unwrap();
            let svg = std::str::from_utf8(&bytes).unwrap();
            assert!(svg.contains(r#"fill="none""#), "{key} does not fill");
            assert!(svg.contains(r#"stroke="currentColor""#), "{key} strokes");
            // One drawn weight whatever the viewBox or the size drawn.
            for size in [10.0, 13.0, 14.0, 15.0, 16.0, 18.0] {
                let bytes = Assets.load(&drawn_at(key, size)).unwrap().unwrap();
                let drawn = std::str::from_utf8(&bytes).unwrap();
                assert!(
                    (drawn_stroke(drawn, size) - theme::ICON_STROKE).abs() < 0.001,
                    "{key} draws {} at {size}px, not {}",
                    drawn_stroke(drawn, size),
                    theme::ICON_STROKE
                );
                let unstroked = |svg: &str| {
                    svg.replace(
                        &format!(r#"stroke-width="{}""#, attr(svg, "stroke-width")),
                        "",
                    )
                };
                assert_eq!(
                    unstroked(drawn),
                    unstroked(svg),
                    "{key} changes only its stroke"
                );
            }
        }
        // A logomark has no stroke to change.
        let claude = Assets.load(&drawn_at(CLAUDE, 14.0)).unwrap().unwrap();
        assert_eq!(&*claude, &*Assets.load(CLAUDE).unwrap().unwrap());

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

    /// A work row's Lucide glyphs.
    const WORK_ROW_ICONS: [&str; 10] = [
        SQUARE_PEN,
        EYE,
        BRAIN,
        HAMMER,
        WRENCH,
        BOT,
        ZAP,
        MESSAGE_CIRCLE,
        LIST_TODO,
        APP_WINDOW,
    ];

    /// A work row's glyph strokes Lucide's own weight, 1.33px at 16.
    #[test]
    fn a_work_row_icon_strokes_lucides_weight() {
        let key = format!("{BRAIN}@16/{}", theme::WORK_ICON_STROKE);
        let bytes = Assets.load(&key).unwrap().unwrap();
        let drawn = std::str::from_utf8(&bytes).unwrap();
        assert!((drawn_stroke(drawn, 16.0) - 4.0 / 3.0).abs() < 0.001);
    }

    /// The UI refresh's line icons (SPEC §3.7), all in the family.
    const REFRESH_ICONS: [&str; 37] = [
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
        CIRCLE_MINUS,
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
        COLLAPSE_ALL,
        LIST,
        CODE,
        PIN,
    ];

    fn attr(svg: &str, name: &str) -> String {
        let at = svg.find(&format!("{name}=\"")).unwrap() + name.len() + 2;
        let end = at + svg[at..].find('"').unwrap();
        svg[at..end].to_string()
    }

    /// `stroke-width × size / viewBox`: the stroke an icon draws at `size`.
    fn drawn_stroke(svg: &str, size: f32) -> f32 {
        let stroke: f32 = attr(svg, "stroke-width").parse().unwrap();
        let view: f32 = attr(svg, "viewBox")
            .split_whitespace()
            .nth(2)
            .unwrap()
            .parse()
            .unwrap();
        stroke * size / view
    }

    /// A name nothing embeds is `Ok(None)`, not an error: gpui asks for a
    /// path per frame and must not be handed a failure it cannot act on.
    #[test]
    fn an_unknown_key_is_absent_rather_than_an_error() {
        assert!(Assets.load("icons/nope.svg").unwrap().is_none());
        assert_eq!(Assets.list("icons/").unwrap().len(), ICONS.len());
    }
}
