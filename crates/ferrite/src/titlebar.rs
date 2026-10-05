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
//! The band itself is two cells (theme WP-C): over the sidebar, the
//! sidebar's own chrome row (the traffic lights, the sidebar toggle and the
//! bell, `cockpit::nav`); over the board, the right cell this module draws
//! on the reading plane — the location as a terminal prints a path (F-10):
//! `ferrite / Nav rows jitter on stream start · done · dev` in Solo,
//! `ferrite / Perf sweep · 4 · 1 needs you · dev` for a Group, `all projects
//! / Everything · 9 · 2 need you · 1 failing` for one that spans Projects,
//! `Ferrite` on the empty board. The title is the one strong word; every
//! other word, `/` and `·` is `TEXT_MUTED`. Its one trailing door is
//! `⌘K commands` (F-11), the chord read from the keymap. No hairline
//! closes the band and no build badge rides it (Settings › About keeps
//! that). The window's foot is the bottom bar (`bottom_bar`, F-9): the
//! session, `1 solo` and a tab per Group, usage and the clock — tmux's
//! grammar on the chrome.
//!
//! Drawing only, like `nav.rs`: the cockpit places these and owns the state
//! they read.

use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, Div, MouseButton, SharedString, Stateful, WindowControlArea};

use crate::theme::paint;

use crate::components;
use crate::icons::{self, icon};
use crate::pointer::{Pointer, PointerPressed};
use crate::theme::*;

/// Where the board is, as the titlebar names it (F-10).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Location {
    /// Nowhere yet — the empty board: the app's own name.
    #[default]
    Nowhere,
    /// One Thread fills the board (Solo, or one Pane fullscreen): `project /
    /// title · state · branch`.
    Thread {
        project: Option<SharedString>,
        title: SharedString,
        /// The state word (`done`, `working 12s`, `needs you · approval`);
        /// nothing while idle.
        state: Option<SharedString>,
        branch: Option<SharedString>,
    },
    /// A Group's board: `project / name · members · N need you`, then the
    /// branch when every member shares one Project, else `N failing`.
    Group {
        /// The one Project every member shares; `None` reads `all projects`.
        project: Option<SharedString>,
        name: SharedString,
        members: usize,
        /// Threads on this board waiting on the operator.
        need_you: usize,
        /// The shared Project's branch (always named, the default included).
        branch: Option<SharedString>,
        /// Members whose suite or Session failed — said only for a board
        /// that spans Projects, where no branch applies.
        failing: usize,
    },
}

/// The active location named in the window chrome.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Title {
    pub location: Location,
}

impl Title {
    /// The words the location reads, in order, each with whether it is the
    /// band's one strong word: what the cell draws and what a test reads.
    pub fn words(&self) -> Vec<(SharedString, bool)> {
        let plain = |text: SharedString| (text, false);
        match &self.location {
            Location::Nowhere => vec![plain("Ferrite".into())],
            Location::Thread {
                project,
                title,
                state,
                branch,
            } => {
                let mut words = Vec::new();
                if let Some(project) = project {
                    words.push(plain(project.clone()));
                    words.push(plain("/".into()));
                }
                words.push((title.clone(), true));
                for fact in [state, branch].into_iter().flatten() {
                    words.push(plain("\u{b7}".into()));
                    words.push(plain(fact.clone()));
                }
                words
            }
            Location::Group {
                project,
                name,
                members,
                need_you,
                branch,
                failing,
            } => {
                let mut words = vec![
                    plain(
                        project
                            .clone()
                            .unwrap_or_else(|| SharedString::from("all projects")),
                    ),
                    plain("/".into()),
                    (name.clone(), true),
                    plain("\u{b7}".into()),
                    plain(members.to_string().into()),
                ];
                if *need_you > 0 {
                    words.push(plain("\u{b7}".into()));
                    words.push(plain(need_you_label(*need_you)));
                }
                match (project, branch) {
                    (Some(_), Some(branch)) => {
                        words.push(plain("\u{b7}".into()));
                        words.push(plain(branch.clone()));
                    }
                    (None, _) if *failing > 0 => {
                        words.push(plain("\u{b7}".into()));
                        words.push(plain(format!("{failing} failing").into()));
                    }
                    _ => {}
                }
                words
            }
        }
    }
}

/// `1 needs you`, `2 need you`.
pub fn need_you_label(count: usize) -> SharedString {
    if count == 1 {
        "1 needs you".into()
    } else {
        format!("{count} need you").into()
    }
}

/// Whether this build is an unreleased one. `--release` is not the
/// question — a locally built release binary is still a dev build, and
/// wants the `dev` mark. Only the release pipeline ships without it, which it
/// says by setting `FERRITE_RELEASE` for the compile (`build.rs` tracks
/// the variable so a cached build cannot keep a stale answer). Settings ›
/// About reports it; the titlebar carries no badge.
pub const DEV: bool = option_env!("FERRITE_RELEASE").is_none();

/// Whether this build draws its own titlebar. macOS keeps the host's, and
/// hiding it there would take the traffic lights with it.
pub const CUSTOM: bool = cfg!(target_os = "windows");

/// The band above the Pane board, as an overlay: the board's geometry
/// already reserves `WIN_CHROME_H` at the top (`board_bounds`) and the nav
/// draws its own chrome row inside the column, so this adds no layout — it
/// claims what the window already left empty.
///
/// The nav's titlebar cell (`nav_width`) is skipped rather than covered:
/// the toggle and the bell live under it, and a drag region over them would
/// make both unclickable. What it draws is the right cell, on the reading
/// plane (`paint::PLANE`, the same glass as the board under it): the
/// location, the empty stretch the window drags by, and the `⌘K commands`
/// door, then the Windows caption buttons.
///
/// `draggable` is false while a menu, popover or the settings panel is
/// open. Such an overlay can reach into the band, and Windows would route
/// the press to the frame instead of to the row under the pointer.
pub fn strip(
    nav_width: f32,
    title: Title,
    door: AnyElement,
    draggable: bool,
    maximized: bool,
) -> Div {
    let trailing_drag = if CUSTOM && draggable {
        drag_region("titlebar-drag", Title::default(), maximized)
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
        .child(div().flex_shrink_0().w(px(nav_width + CHROME_SEAM_W)))
        .child(
            div()
                .debug_selector(|| "titlebar-cell".into())
                .flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .items_center()
                .bg(paint::PLANE)
                // The location stays anchored to the content edge. The empty
                // stretch absorbs spare width and remains the Windows drag
                // target; the door sits at the right, before the caption
                // controls, a sibling of the drag region, never inside it.
                .child(title_region(&title))
                .child(trailing_drag)
                .child(door)
                .children(CUSTOM.then(|| caption_buttons(maximized))),
        )
}

/// The hover blend's key and the group name the door's words ride.
const DOOR_GROUP: &str = "titlebar-commands";

/// The titlebar's one trailing door (F-11): `⌘K commands`, dim on the plane
/// with no box, `TITLE_PAD_X` in from the window's edge on macOS (Windows
/// follows it with its caption buttons). The chord is the keymap's binding
/// of `palette::Toggle` (`components::bound_chord`); the words blend to
/// `TEXT` under the pointer and a press opens the palette. The cockpit wires
/// the click.
pub fn commands_door(chord: Option<&str>) -> Stateful<Div> {
    let ink = crate::motion::hover_blend(DOOR_GROUP, rgb(TEXT_MUTED).into(), rgb(TEXT).into());
    div()
        .id(DOOR_GROUP)
        .debug_selector(|| "titlebar-commands".into())
        .group(DOOR_GROUP)
        .flex()
        .flex_shrink_0()
        .items_center()
        .h_full()
        .gap(px(TITLE_DOOR_GAP))
        .when(!CUSTOM, |door| door.mr(px(TITLE_PAD_X)))
        .cursor_pointer()
        .font_family(FONT_UI)
        .text_size(px(FS_UI))
        .line_height(px(LH_UI))
        .text_color(ink)
        .on_hover(crate::motion::hover_listener(DOOR_GROUP.into()))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .children(chord.map(|chord| components::key_combo(chord, TEXT_MUTED).text_color(ink)))
        // Exactly its cells: the door is right-aligned, and a measured run
        // rounds up a pixel and would stand it that much left.
        .child(components::cells("commands"))
}

/// An empty stretch Windows drags the window by. The tagged part starts
/// below the resize edge on a restored window, so the top border still
/// resizes; maximized, there is no border to preserve and it runs flush.
pub fn drag_region(id: &'static str, title: Title, maximized: bool) -> Div {
    let inset = if maximized { 0.0 } else { CAPTION_RESIZE_EDGE };
    let located = title.location != Location::Nowhere;
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
                .children(located.then(|| title_region(&title)))
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

/// The location (F-10), on one UI baseline, its words one space apart as
/// the prototype's one inline run sets them: the title `TEXT_STRONG` at
/// `W_LABEL`, the one strong word, and everything else — the Project, `/`,
/// `·`, the counts, the state, the branch — `TEXT_MUTED`. Only the title
/// truncates (with the whole of it one hover away); every other word keeps
/// its width.
fn title_region(title: &Title) -> Div {
    // The plain words either side of the title join into one run each,
    // the separating spaces inside them: every run holds exactly its cells
    // (measured runs round up a pixel each, and a flex gap snaps 7.8 to
    // 8.0), so nothing drifts from the prototype's single line.
    let words = title.words();
    let strong_at = words.iter().position(|(_, strong)| *strong);
    let mut runs: Vec<(SharedString, bool)> = Vec::new();
    match strong_at {
        Some(at) => {
            let before: Vec<&str> = words[..at].iter().map(|(word, _)| word.as_ref()).collect();
            if !before.is_empty() {
                runs.push((format!("{} ", before.join(" ")).into(), false));
            }
            runs.push((words[at].0.clone(), true));
            let after: Vec<&str> = words[at + 1..]
                .iter()
                .map(|(word, _)| word.as_ref())
                .collect();
            if !after.is_empty() {
                runs.push((format!(" {}", after.join(" ")).into(), false));
            }
        }
        None => {
            let all: Vec<&str> = words.iter().map(|(word, _)| word.as_ref()).collect();
            runs.push((all.join(" ").into(), false));
        }
    }
    div()
        .debug_selector(|| "titlebar-location".into())
        .h_full()
        .flex()
        .items_center()
        .justify_start()
        .min_w_0()
        .px(px(TITLE_PAD_X))
        .whitespace_nowrap()
        .font_family(FONT_UI)
        .text_size(px(FS_UI))
        .line_height(px(LH_UI))
        .text_color(rgb(TEXT_MUTED))
        .children(runs.into_iter().enumerate().map(|(at, (word, strong))| {
            if strong {
                div()
                    .id(("titlebar-title", at))
                    .debug_selector(|| "titlebar-title".into())
                    .min_w_0()
                    .w(px(components::run_width(&word)))
                    .flex_shrink(1.)
                    .truncate()
                    .tooltip(crate::menu::tooltip(word.clone()))
                    .font_weight(W_LABEL)
                    .text_color(rgb(TEXT_STRONG))
                    .child(word)
                    .into_any_element()
            } else {
                components::tabular(
                    components::cells(word).debug_selector(move || format!("titlebar-word-{at}")),
                )
                .into_any_element()
            }
        }))
}

// ------------------------------------------------------------- bottom bar

/// The bottom bar (theme WP-C, F-9): one row on the chrome (`paint::CHROME`),
/// closed above by a `paint::LINE` rule, tmux's grammar in the one face —
/// `ferrite` in `ACCENT` at the strong weight, `1 solo` and a tab per Group
/// (the current view's bright on a band), the empty stretch, a usage
/// segment per provider and the clock in `TEXT`. No state counts: the
/// sidebar and the bell hold those.
pub fn bottom_bar(tabs: Vec<AnyElement>, usage: Vec<AnyElement>, clock: SharedString) -> Div {
    div()
        .debug_selector(|| "bottom-bar".into())
        .flex()
        .flex_row()
        .flex_shrink_0()
        .w_full()
        .h(px(STATUS_BAR_H))
        .bg(paint::CHROME)
        .border_t_1()
        .border_color(paint::LINE)
        .font_family(FONT_UI)
        .text_size(px(FS_UI))
        .line_height(px(LH_UI))
        .text_color(rgb(CHROME_MUTED))
        .whitespace_nowrap()
        .overflow_hidden()
        .child(
            div()
                .debug_selector(|| "bottom-bar-session".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .px(px(BAR_SEG_PAD_X))
                .font_weight(W_STRONG)
                .text_color(rgb(ACCENT))
                .child(components::cells("ferrite")),
        )
        .children(tabs)
        .child(div().flex_1().min_w_0())
        .children(usage)
        .child(
            components::tabular(
                div()
                    .debug_selector(|| "bottom-bar-clock".into())
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .pl(px(BAR_SEG_PAD_X))
                    .pr(px(BAR_SEG_PAD_TAIL))
                    .text_color(rgb(TEXT)),
            )
            .child(components::cells(clock)),
        )
}

/// One view's tab: ` 1 solo `, ` 2 perf sweep ` — the words a cell in from
/// the tab's own cell of padding. The current view's is `TEXT_STRONG` on
/// `paint::BAND2`; the rest are dim and lift to `TEXT` on `paint::HOVER`
/// under the pointer. The cockpit wires the press.
pub fn bar_tab(
    id: SharedString,
    ordinal: usize,
    label: SharedString,
    current: bool,
) -> Stateful<Div> {
    let selector = id.clone();
    let key = id.clone();
    let words = format!("{ordinal} {label}");
    // The prototype's ` 2 perf sweep `, one run, in a cell of padding
    // each side.
    let width = components::run_width(&format!(" {words} ")) + 2.0 * BAR_SEG_PAD_X;
    div()
        .id(gpui::ElementId::Name(id))
        .debug_selector(move || selector.to_string())
        .flex()
        .flex_shrink_0()
        .items_center()
        .w(px(width))
        .pl(px(2.0 * BAR_SEG_PAD_X))
        .map(|tab| {
            if current {
                tab.cursor_pointer()
                    .bg(paint::BAND2)
                    .text_color(rgb(TEXT_STRONG))
            } else {
                tab.hover_row(key.clone()).group(key.clone()).press_row()
            }
        })
        .child(
            div()
                .when(!current, |label| {
                    label.group_hover(key, |style| style.text_color(rgb(TEXT)))
                })
                .child(components::cells(words)),
        )
}

/// The window a provider's usage segment reads: its more used one, the 5h
/// window on a tie (`5h 41%`, `wk 12%`). `None` when it has reported
/// neither.
pub fn usage_window(limits: &ferrite_core::transcript::RateLimits) -> Option<(&'static str, f32)> {
    let five = limits
        .five_hour
        .map(|window| window.used_fraction.clamp(0.0, 1.0));
    let week = limits
        .weekly
        .map(|window| window.used_fraction.clamp(0.0, 1.0));
    match (five, week) {
        (Some(five), Some(week)) if week > five => Some(("wk", week)),
        (Some(five), _) => Some(("5h", five)),
        (None, Some(week)) => Some(("wk", week)),
        (None, None) => None,
    }
}

/// One provider's usage segment (F-9): its 11px mark, a cell, and its more
/// used window (`5h 41%`), a cell of padding each side, `CHROME_MUTED` —
/// `TEXT` on `paint::HOVER` under the pointer. `None` when the provider has
/// reported no window.
pub fn bar_usage(
    provider: ferrite_core::store::Provider,
    limits: &ferrite_core::transcript::RateLimits,
) -> Option<Stateful<Div>> {
    let (window, used) = usage_window(limits)?;
    let (glyph, ink, name) = match provider {
        ferrite_core::store::Provider::Claude => (icons::CLAUDE, PROVIDER_CLAUDE, "claude"),
        ferrite_core::store::Provider::Codex => (icons::CODEX, PROVIDER_CODEX, "codex"),
    };
    let key = SharedString::from(format!("bottom-bar-usage-{name}"));
    let selector = key.clone();
    Some(
        div()
            .id(gpui::ElementId::Name(key.clone()))
            .debug_selector(move || selector.to_string())
            .group(key.clone())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(BAR_SEG_PAD_X))
            .pl(px(BAR_SEG_PAD_X))
            .pr(px(BAR_SEG_PAD_TAIL))
            .hover_row(key.clone())
            .child(icon(glyph, BAR_MARK, ink))
            .child(components::tabular(
                components::cells(format!("{window} {}%", (used * 100.).round() as u32))
                    .text_color(rgb(CHROME_MUTED))
                    .group_hover(key, |style| style.text_color(rgb(TEXT))),
            )),
    )
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
/// `TEXT_MUTED` and brightens under the pointer; `close` lays the platform
/// red under it.
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
        .hover_control(id)
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
            icon(glyph, CAPTION_GLYPH, TEXT_MUTED)
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
                    location: Location::Group {
                        project: Some("ferrite".into()),
                        name: "Perf sweep".into(),
                        members: 4,
                        need_you: 1,
                        branch: Some("dev".into()),
                        failing: 0,
                    },
                },
                commands_door(Some("cmd-K")).into_any_element(),
                true,
                false,
            );
            assert_eq!(strip.style().size.height, Some(px(WIN_CHROME_H).into()));
            assert_eq!(strip.style().position, Some(gpui::Position::Absolute));
        });
    }

    /// F-10: the location reads as the prototype prints it, the title the
    /// one strong word; `1 needs you` is singular, `2 need you` plural; a
    /// board spanning Projects says how many fail instead of a branch.
    #[test]
    fn the_location_reads_as_a_terminal_prints_a_path() {
        let text = |title: Title| {
            title
                .words()
                .into_iter()
                .map(|(word, _)| word.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        };
        let strong = |title: Title| {
            title
                .words()
                .into_iter()
                .filter(|(_, strong)| *strong)
                .map(|(word, _)| word.to_string())
                .collect::<Vec<_>>()
        };
        let solo = Title {
            location: Location::Thread {
                project: Some("ferrite".into()),
                title: "Nav rows jitter on stream start".into(),
                state: Some("done".into()),
                branch: Some("dev".into()),
            },
        };
        assert_eq!(
            text(solo.clone()),
            "ferrite / Nav rows jitter on stream start \u{b7} done \u{b7} dev"
        );
        assert_eq!(strong(solo), ["Nav rows jitter on stream start"]);
        let group = Title {
            location: Location::Group {
                project: Some("ferrite".into()),
                name: "Perf sweep".into(),
                members: 4,
                need_you: 1,
                branch: Some("dev".into()),
                failing: 1,
            },
        };
        assert_eq!(
            text(group),
            "ferrite / Perf sweep \u{b7} 4 \u{b7} 1 needs you \u{b7} dev"
        );
        let wall = Title {
            location: Location::Group {
                project: None,
                name: "Everything".into(),
                members: 9,
                need_you: 2,
                branch: Some("dev".into()),
                failing: 1,
            },
        };
        assert_eq!(
            text(wall),
            "all projects / Everything \u{b7} 9 \u{b7} 2 need you \u{b7} 1 failing"
        );
        assert_eq!(text(Title::default()), "Ferrite");
        assert_eq!(need_you_label(1), "1 needs you");
        assert_eq!(need_you_label(3), "3 need you");
    }

    /// F-9: a usage segment reads the provider's more used window, the 5h
    /// one on a tie.
    #[test]
    fn a_usage_segment_reads_the_more_used_window() {
        use ferrite_core::transcript::RateLimits;
        use ferrite_core::RateLimitWindow;
        let window = |used: f32| {
            Some(RateLimitWindow {
                used_fraction: used,
                resets_at: None,
            })
        };
        let limits = |five_hour, weekly| RateLimits { five_hour, weekly };
        assert_eq!(
            usage_window(&limits(window(0.41), window(0.12))),
            Some(("5h", 0.41))
        );
        assert_eq!(
            usage_window(&limits(window(0.10), window(0.12))),
            Some(("wk", 0.12))
        );
        assert_eq!(
            usage_window(&limits(window(0.3), window(0.3))),
            Some(("5h", 0.3))
        );
        assert_eq!(usage_window(&limits(None, None)), None);
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
