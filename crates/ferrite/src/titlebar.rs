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
//! on the reading plane — the location as a terminal prints a path,
//! `ferrite / title · state · branch`, the title the one strong word and
//! the rest dim, and the trailing door (`⌘T new thread`) dim at the right.
//! No hairline closes the band. The window's foot is the bottom bar
//! (`bottom_bar`): the session, a tab per view, usage and the clock, tmux's
//! grammar on the chrome.
//!
//! Drawing only, like `nav.rs`: the cockpit places these and owns the state
//! they read.

use gpui::component::button::{Button, ButtonVariants};
use gpui::prelude::*;
use gpui::{
    div, px, rgb, rgba, AnyElement, App, Div, MouseButton, SharedString, Stateful,
    WindowControlArea,
};

use crate::theme::paint;

use crate::components;
use crate::icons::{self, icon};
use crate::pointer::{Pointer, PointerPressed};
use crate::theme::*;

/// The active location named in the window chrome. A Group may span
/// Projects, so the Project follows the focused Pane rather than trying to
/// summarize the whole Group.
#[derive(Default)]
pub struct Title {
    pub project: Option<SharedString>,
    pub group: Option<SharedString>,
    /// The Thread the board shows alone — Solo, or one Pane fullscreen:
    /// the Solo Pane has no head (C2), so its identity rides here.
    pub thread: Option<ThreadCrumb>,
}

/// One Thread's identity in the titlebar: `title · state · branch`, then the
/// plan's meter and the PR with its CI, fed from the one status truth
/// (`cockpit::thread_status`). The title is the band's one strong word.
pub struct ThreadCrumb {
    pub title: SharedString,
    /// The checkout, only when it says something: a single branch that is
    /// not the default, or every directory's branch of a multi-directory
    /// Project (`frontend:feat/header  api:main`).
    pub branches: Vec<(Option<SharedString>, SharedString)>,
    /// The plan's meter, while the Thread works to one.
    pub tasks: Option<AnyElement>,
    /// The PR and its CI, wired to open the checks card.
    pub ci: Option<AnyElement>,
    /// The state word (`needs you · approval`, `failed`, `working 12s`);
    /// nothing when idle.
    pub state: Option<crate::pane::HeadSlot>,
}

/// What the location adds about the board, beside its name.
#[derive(Clone, Copy, Default)]
pub struct Board {
    /// How many Panes the Group shows: cheap orientation beside its name.
    pub count: Option<usize>,
    /// One Pane fills the board: the only on-screen cue that its siblings
    /// are hidden, not gone.
    pub fullscreen: bool,
    /// How many Threads wait on the operator anywhere (the nav's Needs-you
    /// strip): `· N need you` after the location, a door to ⌘D.
    pub need_you: usize,
}

/// Whether this build is an unreleased one. `--release` is not the
/// question — a locally built release binary is still a dev build, and
/// wants the `dev` mark. Only the release pipeline ships without it, which it
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
/// The nav's width (and the 1px seam beside it) is skipped rather than
/// covered: the toggle and the bell live under it, and a drag region over
/// them would make both unclickable. What it draws is the right cell, on
/// the reading plane (`paint::PLANE`, the same glass as the board under it):
/// the location, the empty stretch the window drags by, and the trailing
/// door, then the Windows caption buttons.
///
/// `draggable` is false while a menu, popover or the settings panel is
/// open. Such an overlay can reach into the band, and Windows would route
/// the press to the frame instead of to the row under the pointer.
#[allow(clippy::too_many_arguments)]
pub fn strip(
    nav_width: f32,
    title: Title,
    board: Board,
    trailing: Option<AnyElement>,
    add_thread: AnyElement,
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
                // target; the trailing door sits at the right, before the
                // caption controls. The need-you count and `dev` are the
                // location's last segments, siblings of the drag region,
                // never inside it.
                .child(title_region(title, board, true))
                .child(trailing_drag)
                .children(trailing)
                .child(add_thread)
                .children(CUSTOM.then(|| caption_buttons(maximized))),
        )
}

/// The hover blend's key and the group name the add control's words ride.
const ADD_GROUP: &str = "titlebar-add-thread";

/// The titlebar's trailing door (UI-13), the right cell's last words: the
/// chord and what it makes, `⌘T new thread`, dim on the plane with no box
/// (the prototype's `⌘K commands` slot, holding the most useful real key).
/// The words rest at `TEXT_MUTED` and blend to `TEXT` under the pointer
/// over the one 150ms hover blend; a press turns them `TEXT_STRONG` at
/// once. It is a sibling of the Windows drag region, never a child, so its
/// click reaches the app instead of the non-client frame. The cockpit wires
/// the click and hangs the tooltip (`add_thread`).
pub fn add_thread_button(label: &'static str, chord: Option<&str>, cx: &App) -> Button {
    let ink = crate::motion::hover_blend(ADD_GROUP, rgb(TEXT_MUTED).into(), rgb(TEXT).into());
    let ink_u32 = TEXT_MUTED;
    components::button("titlebar-add-thread")
        .custom(crate::pointer::button_variant(
            rgba(TRANSPARENT).into(),
            rgb(TEXT_MUTED).into(),
            rgba(TRANSPARENT).into(),
            cx,
        ))
        .debug_selector(|| "titlebar-add-thread".into())
        .group(ADD_GROUP)
        .flex_shrink_0()
        .h(px(ROW + SPACE_1))
        .px(px(TITLE_ADD_PAD_X))
        // Windows follows this control with its caption buttons. macOS has
        // no trailing sibling, so its words end two cells in from the
        // window's edge, the right cell's own inset.
        .when(cfg!(target_os = "macos"), |button| {
            button.mr(px(TITLE_PAD_X - TITLE_ADD_PAD_X))
        })
        .on_hover(crate::motion::hover_listener(ADD_GROUP.into()))
        .accessibility_label(label)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(TITLE_ADD_GAP))
                .font_family(FONT_UI)
                .text_size(px(FS_UI))
                .line_height(px(LH_UI))
                .text_color(ink)
                .children(chord.map(|chord| components::key_combo(chord, ink_u32).text_color(ink)))
                .child(
                    div()
                        .id("titlebar-add-thread-label")
                        .group_active(ADD_GROUP, |style| style.text_color(rgb(TEXT_STRONG)))
                        .child(SharedString::from(label.to_lowercase())),
                ),
        )
}

/// The add control with its tooltip: what the click makes, and the chord
/// after it only where the click is exactly that key's action
/// (`menu::tooltip_with_key`).
pub fn add_thread(button: Button, tooltip: &'static str, chord: Option<String>) -> AnyElement {
    div()
        .id("titlebar-add-thread-tip")
        .flex_shrink_0()
        .tooltip(crate::menu::tooltip_with_key(tooltip, chord))
        .child(button)
        .into_any_element()
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
                .child(title_region(title, Board::default(), false))
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

/// The dev-build mark, the location's last segment: plain `dev` in `FS_SM`
/// `W_BODY` `TEXT_MUTED` after a faint `·` (`seam`, false when nothing
/// precedes it), with no box. Not a state — `ATTENTION` would say
/// "something needs you" to every operator of a local build. It rides the
/// location region, a sibling of the drag region rather than a child:
/// anything inside one is non-client to Windows.
fn dev_badge(seam: bool) -> Div {
    div()
        .debug_selector(|| "titlebar-dev-badge".into())
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(TITLE_GAP))
        .children(seam.then(|| div().text_color(rgb(TEXT_FAINT)).child("·")))
        .child(
            div()
                .font_weight(W_BODY)
                .text_color(rgb(TEXT_MUTED))
                .child("dev"),
        )
}

/// `· N need you` (rule 2.7.6): how many Threads wait on the operator,
/// dim and tabular like every fact on the band (no coloured counts, theme
/// WP-C), never shrinking. A press runs the ⌘D jump from wherever the
/// keyboard is; the tooltip names the key.
fn need_you(count: usize) -> Div {
    let label = div()
        .id("titlebar-need-you")
        .debug_selector(|| "titlebar-need-you".into())
        .flex_shrink_0()
        .cursor_pointer()
        .font_weight(W_BODY)
        .text_color(rgb(TEXT_MUTED))
        .child(SharedString::from(format!("{count} need you")))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(|_, window, cx| {
            window.dispatch_action(Box::new(crate::cockpit::NextDecision), cx)
        });
    let label = label.tooltip(crate::menu::action_tooltip(
        "Next needs you",
        "cockpit::NextDecision",
    ));
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(TITLE_GAP))
        .child(div().text_color(rgb(TEXT_FAINT)).child("·"))
        .child(components::tabular(label))
}

/// The location, on one UI baseline, segments `TITLE_GAP` apart. In Solo:
/// the Project in `TEXT_MUTED`, a faint `/`, then the Thread
/// (`thread_crumb`). In a Group: the Project, `/`, the Group's name as the
/// band's one title and how many Panes it shows; while one Pane fills the
/// board the Thread follows the Group in place of the count, and how many
/// Threads that hides is the Group name's tooltip. Truncation order: the
/// branch first, then the Project, then the title. The band's own copy
/// (`chrome`, never inside a drag region) closes it: `· N need you` —
/// unless the Solo Thread's state word already says `needs you`, which is
/// said once — then `· dev` in a development build.
fn title_region(title: Title, board: Board, chrome: bool) -> Div {
    let Title {
        project,
        group,
        thread,
    } = title;
    let Board {
        count,
        fullscreen,
        need_you: waiting,
    } = board;
    let located = project.is_some() || group.is_some() || thread.is_some();
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
    let has_project = project.is_some();
    let has_group = group.is_some();
    let has_thread = thread.is_some();
    // `needs you` is said once on the band: while the Solo Thread's own
    // state word says it (and is the same ⌘D door), the count is not
    // repeated after it — the nav's Needs-you strip holds the queue.
    let said = thread
        .as_ref()
        .is_some_and(|thread| matches!(thread.state, Some(crate::pane::HeadSlot::NeedsYou(_))));
    let waiting = if said { 0 } else { waiting };
    let hidden = count
        .filter(|_| fullscreen)
        .map(|count| count.saturating_sub(1));
    div()
        .h_full()
        .flex()
        .items_center()
        .justify_start()
        .min_w_0()
        .px(px(TITLE_PAD_X))
        .gap(px(TITLE_GAP))
        .font_family(FONT_UI)
        .text_size(px(FS_UI))
        .line_height(px(LH_UI))
        .text_color(rgb(TEXT_MUTED))
        // Nowhere yet (the empty board): the app's own name, dim.
        .when(chrome && !located, |title| {
            title.child(
                div()
                    .debug_selector(|| "titlebar-ferrite".into())
                    .flex_shrink_0()
                    .child("Ferrite"),
            )
        })
        // Either name may truncate when the band is narrow; the whole of it
        // is one hover away.
        .children(project.map(|project| {
            div()
                .id("project-titlebar-name")
                .debug_selector(|| "project-titlebar-name".into())
                .min_w(px(TITLE_PROJECT_MIN_W))
                .flex_shrink(2.)
                .truncate()
                .tooltip(crate::menu::tooltip(project.clone()))
                .font_weight(W_BODY)
                .text_color(rgb(TEXT_MUTED))
                .child(project)
        }))
        .when(has_group || (has_thread && has_project), |title| {
            title.child(separator("/"))
        })
        .children(group.map(|group| {
            let tip = match hidden {
                Some(hidden) => SharedString::from(format!(
                    "{group} \u{b7} {hidden} thread{} hidden",
                    if hidden == 1 { "" } else { "s" }
                )),
                None => group.clone(),
            };
            div()
                .id("group-titlebar-name")
                .debug_selector(|| "group-titlebar-name".into())
                .min_w_0()
                .flex_shrink(1.)
                .truncate()
                .tooltip(crate::menu::tooltip(tip))
                .font_weight(W_LABEL)
                .text_color(rgb(TEXT_STRONG))
                .child(group)
        }))
        .children(count.filter(|_| !fullscreen).map(|count| {
            div()
                .flex()
                .flex_shrink_0()
                .gap(px(TITLE_GAP))
                .child(separator("·"))
                .child(crate::components::tabular(fact(SharedString::from(
                    count.to_string(),
                ))))
        }))
        .when(has_group && has_thread, |title| title.child(separator("/")))
        .children(thread.map(thread_crumb))
        .when(chrome && waiting > 0, |title| {
            title.child(need_you(waiting))
        })
        .when(chrome && DEV, |title| title.child(dev_badge(true)))
}

/// The Thread in the titlebar (theme WP-C): the title, the band's one
/// strong word (`W_LABEL` `TEXT_STRONG`, truncating, the whole of it one
/// hover away), then `· state` and `· branch` dim, the plan's meter, and the
/// PR with its CI. The state word never shrinks; `needs you` is a door to
/// ⌘D. A draft is `New thread` alone.
fn thread_crumb(thread: ThreadCrumb) -> Div {
    let ThreadCrumb {
        title,
        branches,
        tasks,
        ci,
        state,
    } = thread;
    let separator = || {
        div()
            .flex_shrink_0()
            .text_color(rgb(TEXT_FAINT))
            .child("\u{b7}")
    };
    let branch = (!branches.is_empty()).then(|| {
        div()
            .debug_selector(|| "titlebar-thread-branch".into())
            .flex()
            .flex_shrink(4.)
            .min_w_0()
            .overflow_hidden()
            .items_center()
            .gap(px(TITLE_GAP))
            .text_color(rgb(TEXT_MUTED))
            .child(separator())
            .children(
                branches
                    .into_iter()
                    .enumerate()
                    .map(|(index, (directory, branch))| {
                        div()
                            .debug_selector(move || format!("project-branch-{index}"))
                            .flex()
                            .min_w_0()
                            .items_center()
                            .when(index > 0, |item| item.ml(px(TITLE_GAP)))
                            .when_some(directory, |item, directory| {
                                item.child(div().flex_shrink_0().child(directory)).child(
                                    div().flex_shrink_0().text_color(rgb(TEXT_FAINT)).child(":"),
                                )
                            })
                            .child(div().min_w_0().truncate().child(branch))
                    }),
            )
    });
    div()
        .debug_selector(|| "titlebar-thread".into())
        .flex()
        .flex_shrink(1.)
        .min_w_0()
        .items_center()
        .gap(px(TITLE_GAP))
        .child(
            div()
                .id("thread-titlebar-name")
                .debug_selector(|| "thread-titlebar-name".into())
                .min_w_0()
                .flex_shrink(1.)
                .truncate()
                .tooltip(crate::menu::tooltip(title.clone()))
                .font_weight(W_LABEL)
                .text_color(rgb(TEXT_STRONG))
                .child(title),
        )
        .children(state.map(|state| {
            div()
                .debug_selector(|| "titlebar-thread-state".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(TITLE_GAP))
                .child(separator())
                .child(match state {
                    crate::pane::HeadSlot::NeedsYou(_) => crate::pane::needs_you_door(
                        "titlebar-needs-you".into(),
                        "titlebar-needs-you".into(),
                        crate::pane::head_slot_face(&state),
                    ),
                    _ => crate::pane::head_slot_face(&state).into_any_element(),
                })
        }))
        .children(branch)
        .children(tasks.map(|tasks| div().flex_shrink_0().child(tasks)))
        .children(ci.map(|ci| {
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(TITLE_GAP))
                .text_color(rgb(TEXT_MUTED))
                .child(separator())
                .child(ci)
        }))
}

// ------------------------------------------------------------- bottom bar

/// The bottom bar (theme WP-C): one row on the chrome (`paint::CHROME`),
/// closed above by a `paint::LINE` rule, tmux's grammar in the one face —
/// the session name, then a tab per view (the current one bright on a
/// band), the empty stretch, the provider usage segments and the clock. No
/// state counts: the sidebar and the bell hold those.
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
        .text_color(rgb(TEXT_MUTED))
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
                .child("ferrite"),
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
                    .px(px(BAR_SEG_PAD_X))
                    .text_color(rgb(TEXT)),
            )
            .child(clock),
        )
}

/// One view's tab: `1 solo`, `2 perf sweep`. The current view's is
/// `TEXT_STRONG` on `paint::BAND2`; the rest are dim and lift to `TEXT` on
/// `paint::HOVER` under the pointer. The cockpit wires the press.
pub fn bar_tab(
    id: SharedString,
    ordinal: usize,
    label: SharedString,
    current: bool,
) -> Stateful<Div> {
    let selector = id.clone();
    let key = id.clone();
    div()
        .id(gpui::ElementId::Name(id))
        .debug_selector(move || selector.to_string())
        .flex()
        .flex_shrink_0()
        .items_center()
        .px(px(2.0 * BAR_SEG_PAD_X))
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
                .child(SharedString::from(format!("{ordinal} {label}"))),
        )
}

/// One provider's usage segment: its mark, then `5h 41%` and `wk 12%` for
/// the windows it has reported, each dim until it runs tight
/// (`pane::readout_ink`). `None` when the provider has reported neither.
pub fn bar_usage(
    provider: ferrite_core::store::Provider,
    limits: &ferrite_core::transcript::RateLimits,
) -> Option<Div> {
    let windows: Vec<(&str, f32)> = [
        ("5h", limits.five_hour.map(|window| window.used_fraction)),
        ("wk", limits.weekly.map(|window| window.used_fraction)),
    ]
    .into_iter()
    .filter_map(|(name, used)| used.map(|used| (name, used.clamp(0.0, 1.0))))
    .collect();
    if windows.is_empty() {
        return None;
    }
    let (glyph, ink, name) = match provider {
        ferrite_core::store::Provider::Claude => (icons::CLAUDE, PROVIDER_CLAUDE, "claude"),
        ferrite_core::store::Provider::Codex => (icons::CODEX, PROVIDER_CODEX, "codex"),
    };
    Some(
        div()
            .debug_selector(move || format!("bottom-bar-usage-{name}"))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(BAR_SEG_PAD_X))
            .px(px(BAR_SEG_PAD_X))
            .child(icon(glyph, BAR_MARK, ink))
            .children(windows.into_iter().map(|(name, used)| {
                components::tabular(div().text_color(rgb(crate::pane::readout_ink(used)))).child(
                    SharedString::from(format!("{name} {}%", (used * 100.).round() as u32)),
                )
            })),
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
                    thread: None,
                },
                Board {
                    count: Some(4),
                    fullscreen: false,
                    need_you: 2,
                },
                None,
                add_thread(
                    add_thread_button("Add thread", Some("cmd-T"), cx),
                    "New thread in this group",
                    Some("cmd-T".into()),
                ),
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
