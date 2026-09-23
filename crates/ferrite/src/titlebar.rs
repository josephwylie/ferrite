//! The window's own titlebar, on the platforms that make an app draw one.
//!
//! macOS is given its titlebar: `appears_transparent` hides the face, the
//! traffic lights stay AppKit's, and the band above the board is already a
//! drag strip the system owns. Windows gives nothing — a window either
//! wears the whole system titlebar or draws every part of one itself. It
//! wore the system's, which put a second bar above the app's own band and
//! left the nav's chrome row reserving 77px for lights that are not there
//! (`theme::TRAFFIC_RESERVE`, macOS only now). Ferrite draws the titlebar
//! instead: one band, the sidebar and settings buttons at its left where
//! the nav's own rows line up, and the caption buttons at the window's
//! top-right corner where Windows puts them.
//!
//! gpui does the platform half. A div tagged `window_control_area(..)`
//! answers `WM_NCHITTEST` with the matching non-client code, so **Windows**
//! drags, minimises, maximises and closes — the same path its own caption
//! buttons take, snap-layout flyout included. Nothing here calls a window
//! verb: these are faces, and the OS acts on the hit test.
//!
//! Two consequences shape everything below. A drag region is non-client to
//! Windows, so **nothing interactive may sit under one** — the press never
//! reaches the client and the control cannot be clicked. And the control
//! area is consulted *before* gpui's own resize fallback, so a region flush
//! to y = 0 would eat the top resize edge; `CAPTION_RESIZE_EDGE` is the
//! inset that gives it back.
//!
//! Drawing only, like `nav.rs`: the cockpit places these and owns the state
//! they read.

use gpui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, App, Div, MouseButton, SharedString, Stateful, WindowControlArea};

use crate::icons::{self, icon};
use crate::pointer::{Pointer, PointerPressed};
use crate::theme::*;

/// The active location named in the window chrome. A Group may span
/// Projects, so the Project follows the focused Pane rather than trying to
/// summarize the whole Group.
#[derive(Clone)]
pub struct Title {
    pub project: Option<SharedString>,
    pub group: Option<SharedString>,
}

/// What the location adds about the board, beside its name.
#[derive(Clone, Copy, Default)]
pub struct Board {
    /// How many Panes the Group shows: cheap orientation beside its name.
    pub count: Option<usize>,
    /// One Pane fills the board: the only on-screen cue that its siblings
    /// are hidden, not gone.
    pub fullscreen: bool,
}

/// Whether this build is an unreleased one. `--release` is not the
/// question — a locally built release binary is still a dev build, and
/// wants the badge. Only the release pipeline ships without it, which it
/// says by setting `FERRITE_RELEASE` for the compile (`build.rs` tracks
/// the variable so a cached build cannot keep a stale answer). Settings
/// reports the same fact under About.
pub const DEV: bool = option_env!("FERRITE_RELEASE").is_none();

/// Whether this build draws its own titlebar. macOS keeps the host's, and
/// hiding it there would take the traffic lights with it.
pub const CUSTOM: bool = cfg!(target_os = "windows");

/// The band above the Pane board, as an overlay: the board's geometry
/// already reserves `WIN_CHROME_H` at the top (`board_bounds`) and the nav
/// draws its own chrome row inside the column, so this adds no layout — it
/// claims what the window already left empty.
///
/// The nav's width is skipped rather than covered: the collapse and gear
/// buttons live under it, and a drag region over them would make both
/// unclickable. The nav band's own empty stretch is draggable through
/// `drag_region`, which the cockpit puts between those two buttons.
///
/// `draggable` is false while a menu, popover or the settings panel is
/// open. Such an overlay can reach into the band, and Windows would route
/// the press to the frame instead of to the row under the pointer.
pub fn strip(
    nav_width: f32,
    title: Title,
    board: Board,
    add_thread: Button,
    draggable: bool,
    maximized: bool,
) -> Div {
    let trailing_drag = if CUSTOM && draggable {
        drag_region(
            "titlebar-drag",
            Title {
                project: None,
                group: None,
            },
            maximized,
        )
    } else {
        div().flex_1().h_full()
    };
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(WIN_CHROME_H))
        .flex()
        .flex_row()
        .items_center()
        .child(div().flex_shrink_0().w(px(nav_width)))
        // The location stays anchored to the content edge. The empty stretch
        // absorbs spare width and remains the Windows drag target, while the
        // contextual creation door sits at the trailing edge immediately
        // before the caption controls.
        // An empty location (the empty board) takes no slot, so the `dev`
        // tag keeps the location's own inset instead of trailing an empty
        // region and its gap.
        .map(|strip| {
            if title.project.is_none() && title.group.is_none() {
                strip.children(DEV.then(|| dev_badge().ml(px(GRID_PAD))))
            } else {
                strip
                    .child(title_region(title, board))
                    .children(DEV.then(dev_badge))
            }
        })
        .child(trailing_drag)
        .child(add_thread)
        .children(CUSTOM.then(|| caption_buttons(maximized)))
}

/// The titlebar's contextual creation door (UI-13): the `+` and its UI
/// label in the chrome icon-button face — `TEXT_MUTED` glyph, the `HOVER`
/// face under the pointer, `PRESSED` held, `R_CONTROL`. It is a sibling of
/// the Windows drag region, never a child, so its click reaches the app
/// instead of the non-client frame. macOS receives the same control in its
/// transparent band.
pub fn add_thread_button(label: &'static str, tooltip: &'static str, cx: &App) -> Button {
    crate::components::button("titlebar-add-thread")
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(rgb(TEXT_2).into())
                .hover(rgb(HOVER).into())
                .active(rgb(PRESSED).into()),
        )
        .debug_selector(|| "titlebar-add-thread".into())
        .flex_shrink_0()
        .h(px(ICON_BUTTON))
        .px(px(TITLE_ADD_PAD_X))
        // Windows follows this control with its caption buttons. macOS has
        // no trailing sibling, so keep the creation door inside the same
        // shell inset as the Pane board instead of flush with the window.
        .when(cfg!(target_os = "macos"), |button| button.mr(px(GRID_PAD)))
        .tooltip(tooltip)
        .accessibility_label(tooltip)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(TITLE_ADD_GAP))
                .font_family(FONT_UI)
                .text_size(px(FS_UI))
                .line_height(px(LH_UI))
                .text_color(rgb(TEXT_2))
                .child(icon(icons::PLUS, ICON_BUTTON_GLYPH, TEXT_MUTED))
                .child(label),
        )
}

/// An empty stretch Windows drags the window by. The tagged part starts
/// below the resize edge on a restored window, so the top border still
/// resizes; maximized, there is no border to preserve and it runs flush.
pub fn drag_region(id: &'static str, title: Title, maximized: bool) -> Div {
    let inset = if maximized { 0.0 } else { CAPTION_RESIZE_EDGE };
    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .child(div().flex_shrink_0().h(px(inset)))
        .child(
            div()
                .id(id)
                .flex_1()
                .w_full()
                .child(title_region(title, Board::default()))
                // See `button`: the root's focus hitbox must not count as
                // hovered under a caption region, or the press is marked
                // handled and Windows never starts the move.
                .occlude()
                // Window-level text selection otherwise treats this
                // out-of-transcript press as a drag from the nearest run.
                // Suppression does not consume the non-client event, so
                // Windows still receives it and starts the window move.
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    gpui::base::GlobalState::suppress_text_selection(cx);
                })
                .window_control_area(WindowControlArea::Drag),
        )
}

/// The dev-build mark, beside the location it qualifies: a quiet UI
/// `dev` in a hairline box. Not a state — `ATTENTION` would say "something
/// needs you" to every operator of a local build. It is a sibling of the
/// drag region rather than a child: anything inside one is non-client to
/// Windows, and the band's text should not travel with the two drag
/// stretches that also render a `Title`.
fn dev_badge() -> Div {
    div()
        .debug_selector(|| "titlebar-dev-badge".into())
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .h(px(DEV_TAG_H))
        .px(px(DEV_TAG_PAD_X))
        .rounded(px(R_CHIP))
        .border_1()
        .border_color(rgba(HAIRLINE_STRONG))
        .font_family(FONT_UI)
        .text_size(px(FS_SM))
        .line_height(px(LH_META))
        .font_weight(W_BODY)
        .text_color(rgb(TEXT_MUTED))
        .child("dev")
}

/// The location, on one UI baseline: in Solo the Project alone; in a
/// Group the Project, a faint `/`, the Group's name as the band's one title
/// and how many Panes it shows; ` · fullscreen` while one Pane fills the
/// board. The Group's name gives way last.
fn title_region(title: Title, board: Board) -> Div {
    let Title { project, group } = title;
    let Board { count, fullscreen } = board;
    let solo = group.is_none();
    let separator = |glyph: &'static str| {
        div()
            .flex_shrink_0()
            .text_color(rgb(TEXT_FAINT))
            .child(glyph)
    };
    let fact = |text: SharedString| {
        div()
            .flex_shrink_0()
            .text_color(rgb(TEXT_MUTED))
            .child(text)
    };
    let has_group = group.is_some();
    div()
        .h_full()
        .flex()
        .items_center()
        .justify_start()
        .min_w_0()
        .px(px(GRID_PAD))
        .gap(px(TITLE_GAP))
        .font_family(FONT_UI)
        .text_size(px(FS_UI))
        .line_height(px(LH_UI))
        .children(project.map(|project| {
            div()
                .debug_selector(|| "project-titlebar-name".into())
                .min_w_0()
                .flex_shrink(2.)
                .truncate()
                .when(solo, |name| name.font_weight(W_LABEL))
                .text_color(rgb(if solo { TEXT_2 } else { TEXT_MUTED }))
                .child(project)
        }))
        .when(has_group, |title| title.child(separator("/")))
        .children(group.map(|group| {
            div()
                .debug_selector(|| "group-titlebar-name".into())
                .min_w_0()
                .flex_shrink(1.)
                .truncate()
                .font_weight(W_LABEL)
                .text_color(rgb(TEXT_STRONG))
                .child(group)
        }))
        .children(count.map(|count| {
            div()
                .flex()
                .flex_shrink_0()
                .gap(px(TITLE_GAP))
                .child(separator("·"))
                .child(fact(SharedString::from(count.to_string())))
        }))
        .when(fullscreen, |title| {
            title
                .child(separator("·"))
                .child(fact(SharedString::from("fullscreen")))
        })
}

/// Minimise, maximise/restore and close, in the platform's order, flush to
/// the corner. The maximise mark becomes the restore mark while the window
/// is maximized — the button says what the click will do.
fn caption_buttons(maximized: bool) -> Div {
    let (zoom_glyph, zoom_id) = if maximized {
        (icons::WINDOW_RESTORE, "caption-restore")
    } else {
        (icons::WINDOW_MAXIMIZE, "caption-maximize")
    };
    div()
        .flex()
        .flex_shrink_0()
        .h_full()
        .child(button(
            "caption-minimize",
            WindowControlArea::Min,
            icons::WINDOW_MINIMIZE,
            false,
        ))
        .child(button(zoom_id, WindowControlArea::Max, zoom_glyph, false))
        // Close takes Windows' own red field and white mark under the
        // pointer: platform muscle memory, not a Ferrite state colour.
        .child(button(
            "caption-close",
            WindowControlArea::Close,
            icons::WINDOW_CLOSE,
            true,
        ))
}

/// One caption button: square-cornered and edge-to-edge, unlike every other
/// control, because the pointer stops at the window's corner and the hover
/// face has to be there when it does. Full height — the buttons win the top
/// edge from the resize border, as Windows' own do. The mark rests at
/// `TEXT_2` (a 1px stroke at `TEXT_MUTED` is too faint on the ground) and
/// brightens under the pointer; `close` lays the platform red under it.
fn button(
    id: &'static str,
    area: WindowControlArea,
    glyph: &'static str,
    close: bool,
) -> Stateful<Div> {
    let hover_ink = if close {
        CAPTION_CLOSE_INK
    } else {
        TEXT_STRONG
    };
    div()
        .id(id)
        .debug_selector(move || id.into())
        .group(id)
        .relative()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .w(px(CAPTION_W))
        .h_full()
        .hover_control()
        .press_control()
        // A caption press arrives as a *non-client* press, which gpui
        // dispatches into the tree first and Windows acts on only if the
        // tree left it alone. The root tracks focus, and gpui's focus
        // transfer calls `prevent_default()` for every press over a
        // focus-tracked hitbox it counts as hovered — which would mark
        // every caption press handled and swallow it. Occluding stops the
        // hover count at this hitbox, so the root's never runs and the
        // press reaches the frame.
        .occlude()
        // Caption presses share the window's pointer stream with native
        // selectable text. Keep them from anchoring a selection at the
        // nearest transcript run without handling the frame event itself.
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            gpui::base::GlobalState::suppress_text_selection(cx);
        })
        .window_control_area(area)
        .when(close, |button| {
            button.child(
                div()
                    .id("caption-close-field")
                    .absolute()
                    .inset_0()
                    .group_hover(id, |style| style.bg(rgb(CAPTION_CLOSE)))
                    .group_active(id, |style| style.bg(rgb(CAPTION_CLOSE_PRESSED))),
            )
        })
        .child(
            icon(glyph, CAPTION_GLYPH, TEXT_2)
                .group_hover(id, move |style| style.text_color(rgb(hover_ink))),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The buttons are the platform's, so their width is the platform's
    /// too: a narrower one would hang Windows' own snap-layout flyout off
    /// the maximise mark it opens under.
    #[test]
    fn a_caption_button_is_the_platform_width_and_square() {
        let mut close = button(
            "caption-close",
            WindowControlArea::Close,
            icons::WINDOW_CLOSE,
            true,
        );
        assert_eq!(close.style().size.width, Some(px(CAPTION_W).into()));
        assert!(
            close.style().corner_radii.top_right.is_none(),
            "a caption button reaches the window's corner, so it has none"
        );
    }

    /// The strip claims the band the board already leaves empty — it must
    /// take no layout of its own, or every Pane would move down by
    /// `WIN_CHROME_H`.
    #[gpui::test]
    fn the_strip_is_an_overlay_of_the_band_the_board_reserves(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            crate::theme::init_components(cx);
            let mut strip = strip(
                crate::nav::WIDTH,
                Title {
                    project: Some("Ferrite".into()),
                    group: Some("Group Alpha".into()),
                },
                Board {
                    count: Some(4),
                    fullscreen: false,
                },
                add_thread_button("Add Thread", "New Thread in Group", cx),
                true,
                false,
            );
            assert_eq!(strip.style().size.height, Some(px(WIN_CHROME_H).into()));
            assert_eq!(strip.style().position, Some(gpui::Position::Absolute));
        });
    }

    /// A restored window keeps its top resize edge, which is an inset the
    /// drag region gives up; maximized there is no edge to preserve.
    #[test]
    fn a_restored_window_keeps_its_top_resize_edge() {
        assert!(
            CAPTION_RESIZE_EDGE > 0.0,
            "a drag region flush to y = 0 eats the top border"
        );
    }
}
