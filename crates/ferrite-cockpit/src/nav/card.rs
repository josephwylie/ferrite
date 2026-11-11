//! The Thread card (T3 Code's `SidebarThreadRow`, measured in
//! `docs/design/t3-reference/sidebar.md` §3): every Thread's row in the
//! nav, three lines in a fixed box.
//!
//! - **Line 1**: the Project's badge (`project_badge`) and name, the pin
//!   when pinned, and at the right the status slot: a 16px glyph and a
//!   word in the state's ink (`Working 12m`, `Approval`, `Done`), or for a
//!   quiet Thread its age (`now`, `12m`, `3h`). Under the pointer, or the
//!   nav keyboard's cursor, the slot holds the hover strip instead
//!   (`Snooze`, `Park`), at once.
//! - **Line 2**: the title, fading at its edge (`fitted`).
//! - **Line 3**: the checkout — the worktree glyph, the branch cut in its
//!   middle (`middle_truncate`), the PR chip, and the Provider's logo at
//!   the right. With none of them the card hugs lines 1 and 2 and the
//!   logo ends line 2.
//!
//! At rest a card is bare: a fill only under the pointer, for the Thread
//! the board shows, and for a card picked (⌘/⇧-click, a Park sweep). Busy cards recede
//! (`ThreadRow::receded`) so what needs the operator reads brightest.
//!
//! Drawing only, as the rest of the nav: the cockpit wires every press
//! (`CardDoors`).

use std::time::{Duration, Instant, SystemTime};

use ferrite_core::store::Provider;
use ferrite_core::workspace::{PrState, PullRequest};
use ferrite_core::ThreadId;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, rgba, AnyElement, App, Div, FontWeight, Hsla, SharedString, Stateful, Window,
};

use ferrite_ui::icons::{self, icon};
use ferrite_ui::theme::*;
use ferrite_ui::{components, motion};

use super::{NavTooltip, cut_ground, fitted_in, hovered, Measure, NavTail, RowStatus, ThreadRow, TEXT_W};

/// A Thread's pull request as its card's chip reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavPr {
    pub number: u64,
    pub state: NavPrState,
    pub title: Option<SharedString>,
    pub url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavPrState {
    Open,
    Draft,
    Merged,
    Closed,
}

impl NavPr {
    pub fn of(pr: &PullRequest) -> Self {
        NavPr {
            number: pr.number,
            state: match pr.state {
                PrState::Open if pr.draft => NavPrState::Draft,
                PrState::Open => NavPrState::Open,
                PrState::Merged => NavPrState::Merged,
                PrState::Closed => NavPrState::Closed,
            },
            title: pr.title.clone().map(SharedString::from),
            url: pr.url.clone(),
        }
    }
}

impl NavPrState {
    fn glyph(self) -> &'static str {
        match self {
            NavPrState::Open | NavPrState::Closed => icons::GIT_PULL_REQUEST,
            NavPrState::Draft => icons::GIT_PULL_REQUEST_DRAFT,
            NavPrState::Merged => icons::GIT_MERGE,
        }
    }

    /// The glyph's tint, with its alpha.
    pub(crate) fn tint(self) -> u32 {
        match self {
            NavPrState::Open => NAV_PR_OPEN,
            NavPrState::Draft => NAV_PR_DRAFT,
            NavPrState::Merged => NAV_PR_MERGED,
            NavPrState::Closed => NAV_PR_CLOSED,
        }
    }
}

// ------------------------------------------------------------- the words

/// How long a run has worked, as the card says it: `42s`, `12m`, then
/// `1h 5m` (T3's `WorkingDuration`).
pub fn working_for(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s => format!("{}h {}m", s / 3600, s % 3600 / 60),
    }
}

/// A quiet card's age: `now` in its first minute (T3's), then the nav's
/// own shorthand (`facts::since_label`: `12m`, `3h`, `2d`, `1w`…).
pub fn card_age(at: SystemTime, now: SystemTime) -> SharedString {
    let age = crate::facts::since_label(at, now);
    if age.is_empty() {
        "now".into()
    } else {
        age
    }
}

/// A word as the card's status label says it: capitalised (`Done`).
fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// What the status slot says: its glyph, ink, weight and words.
#[derive(Clone, Debug, PartialEq)]
pub struct StatusLabel {
    pub glyph: Option<&'static str>,
    pub ink: u32,
    pub weight: FontWeight,
    pub text: SharedString,
}

impl StatusLabel {
    fn word(glyph: &'static str, ink: u32, text: impl Into<SharedString>) -> Self {
        StatusLabel {
            glyph: Some(glyph),
            ink,
            weight: W_LABEL,
            text: text.into(),
        }
    }
}

/// The status slot's label (sidebar.md §4's table), from the row's word
/// and its clocks: `now` and `wall` are the frame's. `None` for a quiet
/// row with no age to tell.
pub fn status_label(row: &ThreadRow, now: Instant, wall: SystemTime) -> Option<StatusLabel> {
    let working = matches!(row.status, RowStatus::Working | RowStatus::Failing);
    Some(match &row.tail {
        NavTail::Waiting(kind) if *kind == words::QUESTION => StatusLabel::word(
            icons::MESSAGE_CIRCLE_QUESTION,
            NAV_INPUT_INK,
            capitalised(words::INPUT),
        ),
        NavTail::Waiting(_) => StatusLabel::word(
            icons::SHIELD_QUESTION,
            ATTENTION,
            capitalised(words::APPROVAL),
        ),
        NavTail::Failing(count) => StatusLabel::word(
            icons::CIRCLE_ALERT,
            BLOCKED,
            match count {
                Some(count) => format!("{} {count}", capitalised(words::FAILING)),
                None => capitalised(words::FAILING),
            },
        ),
        NavTail::Failed => {
            StatusLabel::word(icons::CIRCLE_ALERT, BLOCKED, capitalised(words::FAILED))
        }
        NavTail::Done => {
            StatusLabel::word(icons::CHECK_CIRCLE, NAV_DONE_INK, capitalised(words::DONE))
        }
        NavTail::Background => StatusLabel {
            weight: W_BODY,
            ..StatusLabel::word(
                icons::CIRCLE_DASHED,
                NAV_WORKING_INK,
                capitalised(words::BACKGROUND),
            )
        },
        _ if working => StatusLabel::word(
            icons::CIRCLE_DASHED,
            NAV_WORKING_INK,
            match row.working_since {
                Some(since) => format!(
                    "{} {}",
                    capitalised(words::WORKING),
                    working_for(now.saturating_duration_since(since))
                ),
                None => capitalised(words::SETTING_UP),
            },
        ),
        _ => StatusLabel {
            glyph: None,
            ink: INK_SECONDARY_LABEL,
            weight: W_BODY,
            text: card_age(row.activity_at?, wall),
        },
    })
}

// ------------------------------------------------------------- the branch

/// T3's `splitForMiddleTruncate`: the branch's head and the tail kept
/// whole after the cut — the last `/` segment when it is short, else the
/// last `NAV_BRANCH_TAIL` characters started on a word (after `-`, `_`,
/// `.` or `/`), a shorter tail preferred. `None` when the branch is too
/// short to cut.
fn split_branch(branch: &str) -> Option<(String, String)> {
    let chars: Vec<char> = branch.chars().collect();
    let len = chars.len();
    let mut keep = NAV_BRANCH_TAIL;
    if let Some(slash) = chars.iter().rposition(|ch| *ch == '/') {
        if slash > 0 && slash < len - 1 {
            let segment = len - slash - 1;
            if segment <= NAV_BRANCH_SEGMENT_MAX {
                keep = segment;
            }
        }
    }
    if len <= keep + NAV_BRANCH_MIN_CUT {
        return None;
    }
    let breaks = |ch: char| matches!(ch, '-' | '_' | '.' | '/');
    let starts_word = |index: usize| breaks(chars[index - 1]);
    let mut cut = len - keep;
    if !starts_word(cut) {
        let shorter = (cut + 1..=len - NAV_BRANCH_TAIL_MIN).find(|index| starts_word(*index));
        let longer = || {
            let floor = len
                .saturating_sub(NAV_BRANCH_SEGMENT_MAX)
                .max(NAV_BRANCH_MIN_CUT + 1);
            (floor..cut).rev().find(|index| starts_word(*index))
        };
        if let Some(snapped) = shorter.or_else(longer) {
            cut = snapped;
        }
    }
    Some((chars[..cut].iter().collect(), chars[cut..].iter().collect()))
}

/// A branch cut in its middle to what `fits` (T3's `MiddleTruncate`): the
/// longest head that fits before `…` and the kept tail. A branch that
/// fits, or is too short to cut, is whole.
pub fn middle_truncate(branch: &str, fits: impl Fn(&str) -> bool) -> String {
    if fits(branch) {
        return branch.to_string();
    }
    let Some((head, tail)) = split_branch(branch) else {
        return branch.to_string();
    };
    let head: Vec<char> = head.chars().collect();
    let cut = |kept: usize| format!("{}\u{2026}{tail}", head[..kept].iter().collect::<String>());
    // The longest head that fits: a binary search, as T3's.
    let (mut fitting, mut over) = (0, head.len());
    while over - fitting > 1 {
        let mid = (fitting + over) / 2;
        if fits(&cut(mid)) {
            fitting = mid;
        } else {
            over = mid;
        }
    }
    cut(fitting)
}

// ------------------------------------------------------------- the face

impl ThreadRow {
    /// Busy cards recede so what needs the operator reads brightest (T3's
    /// `shouldRecedeSidebarThread`): a working or background Thread, and a
    /// quiet one already read. Never the Thread the board shows, one that
    /// waits on the operator, an unseen finish or a failure.
    pub fn receded(&self) -> bool {
        !self.selected
            && matches!(
                self.status,
                RowStatus::Working
                    | RowStatus::Failing
                    | RowStatus::Background
                    | RowStatus::Idle
                    | RowStatus::Parked
            )
    }

    /// A receded working card is the one that dims whole (70%).
    fn dims_whole(&self) -> bool {
        self.receded() && matches!(self.status, RowStatus::Working | RowStatus::Failing)
    }

    /// Whether line 3 has anything to say.
    fn has_checkout(&self) -> bool {
        self.branch.is_some() || self.pr.is_some() || self.worktree.is_some()
    }
}

/// A card's title ink and weight: a receded card's 400 at 75% ink, back
/// to full under the pointer (`hover`); a failure 500 at 95%; every
/// other 500 at full ink.
pub(crate) fn title_face(row: &ThreadRow, hover: f32) -> (Hsla, FontWeight) {
    if row.receded() {
        let ink = motion::mix(
            rgba(NAV_CARD_TITLE_RECEDED).into(),
            rgb(INK_STRONG).into(),
            hover,
        );
        (ink, W_BODY)
    } else if row.status == RowStatus::Failed {
        (rgba(NAV_CARD_TITLE_SEEN_FAILED).into(), W_LABEL)
    } else {
        (rgb(INK_STRONG).into(), W_LABEL)
    }
}

/// A white fill (`NAV_CARD_*`, alpha in its low byte) as it lands over
/// the column: the ground a title's fade melts into.
fn over_column(fill: u32) -> Hsla {
    motion::mix(
        paint::SIDEBAR.hsla(),
        rgb(fill >> 8).into(),
        (fill & 0xff) as f32 / 255.0,
    )
}

/// The ground under a card's text: the active fill, or the column blended
/// toward the hover fill as far as the pointer has brought it.
pub(super) fn card_ground(key: &SharedString, active: bool, picked: bool) -> Hsla {
    if active {
        return over_column(NAV_CARD_ACTIVE);
    }
    let rest = if picked {
        over_column(NAV_CARD_SELECTED)
    } else {
        paint::SIDEBAR.hsla()
    };
    motion::mix(rest, over_column(NAV_CARD_HOVER), motion::hover_t(key))
}

/// The blend a receded card's opacity rides, apart from its fill's.
fn recede_key(key: &str) -> SharedString {
    format!("{key}:recede").into()
}

/// A card's hover, for the pointer and the nav keyboard's cursor alike:
/// the fill and the strip snap; a card that dims whole (`dims`) also
/// fades up (`motion::RECEDE`). Every other card asks for no frames.
pub(crate) fn card_hover(
    key: SharedString,
    dims: bool,
) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
    let snap = motion::hover_snap(key.clone());
    let fade =
        dims.then(|| motion::hover_listener_with(recede_key(&key), motion::RECEDE, motion::RECEDE));
    move |hovered, window, cx| {
        snap(hovered, window, cx);
        if let Some(fade) = &fade {
            fade(hovered, window, cx);
        }
    }
}

/// The blend key a Thread's row's hover reads, its card's or its slim
/// row's (`NavCursor::thread` mirrors it).
pub(crate) fn card_key(thread: ThreadId) -> SharedString {
    format!("nav-thread-{}", thread.get()).into()
}

/// What the cockpit wires into a card.
#[derive(Default)]
pub struct CardDoors {
    /// The hover strip (`snooze_button`, `park_button`), shown in the
    /// status slot under the pointer.
    pub strip: Option<AnyElement>,
    /// The strip stays up whatever the pointer does: its Snooze menu is
    /// open.
    pub strip_held: bool,
    /// The PR chip, wired (`pr_chip`); unwired from the row when `None`.
    pub pr: Option<AnyElement>,
    /// The pin glyph, wired (`pin_glyph`).
    pub pin: Option<AnyElement>,
}

/// A Thread's card. `title` is the title's leaf (the cockpit hands in a
/// rename target, or the live field while renaming: `editing`).
pub fn thread_card(
    row: &ThreadRow,
    title: impl IntoElement,
    doors: CardDoors,
    editing: bool,
    measure: &Measure,
) -> Stateful<Div> {
    card(row, title, doors, editing, measure, true)
}

/// A card that has left the list, as it was last drawn, for its ghost to
/// fade (`flip::ListMotion`): no doors, no hover, and none of a live
/// card's ids or selectors, so nothing addresses it.
pub fn ghost_card(row: &ThreadRow, measure: &Measure) -> Stateful<Div> {
    card(row, row.name.clone(), CardDoors::default(), false, measure, false)
}

/// `debug_selector(name-thread)` on a live card's part; nothing on a
/// ghost's.
fn tag<E: Styled>(element: E, live: bool, name: &'static str, thread: ThreadId) -> E
where
    E: gpui::InteractiveElement,
{
    if live {
        element.debug_selector(move || format!("{name}-{}", thread.get()))
    } else {
        element
    }
}

fn card(
    row: &ThreadRow,
    title: impl IntoElement,
    doors: CardDoors,
    editing: bool,
    measure: &Measure,
    live: bool,
) -> Stateful<Div> {
    let thread = row.thread;
    let key = card_key(thread);
    let active = row.selected;
    let hover = if live { motion::hover_t(&key) } else { 0.0 };
    #[cfg(test)]
    if live {
        super::testing::DRAWN_HOVER.with(|drawn| drawn.borrow_mut().insert(key.clone(), hover));
    }
    let lit = !editing && (hover > 0.0 || doors.strip_held);
    let text_w = TEXT_W;
    let checkout = row.has_checkout();
    let logo_w = row
        .provider
        .map_or(0.0, |_| NAV_PROVIDER_LOGO + NAV_CARD_GAP);
    let title_w = text_w - if checkout { 0.0 } else { logo_w };
    let (ink, weight) = title_face(row, hover);
    let ground = (!editing)
        .then(|| {
            let ground = card_ground(&key, active, row.picked);
            cut_ground(measure, &row.name, FS_NAV_TITLE, weight, title_w, ground)
        })
        .flatten();
    let opacity = if row.dims_whole() && live {
        motion::lerp(
            NAV_CARD_RECEDED_OPACITY,
            1.0,
            motion::hover_t(&recede_key(&key)),
        )
    } else {
        1.0
    };
    #[cfg(test)]
    if live {
        super::testing::DRAWN_CARDS.with(|drawn| {
        drawn.borrow_mut().insert(
            thread,
            super::testing::DrawnCard {
                receded: row.receded(),
                opacity,
                title: (ink, weight),
                    strip: lit && doors.strip.is_some(),
                },
            )
        });
    }
    let rest = if row.picked {
        NAV_CARD_SELECTED
    } else {
        TRANSPARENT
    };
    let fill = if active {
        Some(rgba(NAV_CARD_ACTIVE).into())
    } else if hover > 0.0 {
        Some(motion::mix(
            rgba(rest).into(),
            rgba(NAV_CARD_HOVER).into(),
            hover,
        ))
    } else {
        row.picked.then(|| rgba(NAV_CARD_SELECTED).into())
    };
    let slot: AnyElement = match doors.strip.filter(|_| lit) {
        Some(strip) => strip,
        None => status_label(
            row,
            ferrite_core::clock::instant(),
            ferrite_core::clock::system_time(),
        )
        .map(|label| status_face(thread, &label, live).into_any_element())
        .unwrap_or_else(|| div().into_any_element()),
    };
    let line1 = tag(div(), live, "nav-card-line1", thread)
        .flex()
        .items_center()
        .flex_shrink_0()
        .h(px(NAV_CARD_LINE1_H))
        .gap(px(NAV_CARD_GAP))
        .text_size(px(FS_NAV_META))
        .children(
            row.project
                .as_ref()
                .map(|project| ferrite_ui::project_badge::project_badge(project)),
        )
        .child(
            tag(div(), live, "nav-card-project", thread)
                .flex_1()
                .min_w_0()
                .truncate()
                .font_weight(W_BODY)
                .text_color(rgb(INK_SECONDARY_LABEL))
                .children(row.project.clone()),
        )
        .children((row.pinned && live).then(|| {
            doors
                .pin
                .unwrap_or_else(|| pin_glyph(thread, false).into_any_element())
        }))
        .child(
            tag(div(), live, "nav-card-status", thread)
                .flex()
                .flex_shrink_0()
                .justify_end()
                .items_center()
                .h_full()
                .min_w(px(NAV_STATUS_MIN_W))
                .child(slot),
        );
    let logo = || row.provider.map(provider_logo);
    let line2 = tag(div(), live, "nav-card-line2", thread)
        .flex()
        .items_center()
        .flex_shrink_0()
        .mt(px(NAV_CARD_LINE2_GAP))
        .h(px(NAV_CARD_LINE2_H))
        .gap(px(NAV_CARD_GAP))
        .child(
            tag(
                fitted_in(title_w, FS_NAV_TITLE, ink, weight, title, ground),
                live,
                "nav-title",
                thread,
            )
            .when(row.retitling, |title| {
                    title.opacity(NAV_CARD_RETITLING_OPACITY)
                }),
        )
        .children(if checkout { None } else { logo() });
    let line3 = checkout.then(|| {
        let pr_w = row.pr.as_ref().map_or(0.0, |pr| {
            NAV_PR_MARK
                + NAV_PR_GAP
                + measure.ui(&pr.number.to_string(), FS_NAV_META, W_BODY)
                + NAV_CARD_GAP
        });
        let worktree_w = row
            .worktree
            .as_ref()
            .map_or(0.0, |_| NAV_CHECKOUT_GLYPH + NAV_CARD_GAP);
        let branch_w = text_w - worktree_w - pr_w - logo_w;
        let branch = row.branch.as_ref().map(|branch| {
            middle_truncate(branch, |text| {
                measure.ui(text, FS_NAV_META, W_BODY) <= branch_w
            })
        });
        tag(div(), live, "nav-card-line3", thread)
            .flex()
            .items_center()
            .flex_shrink_0()
            .mt(px(NAV_CARD_LINE3_GAP))
            .h(px(NAV_CARD_LINE3_H))
            .gap(px(NAV_CARD_GAP))
            .text_size(px(FS_NAV_META))
            .line_height(px(NAV_CARD_LINE3_H))
            .text_color(rgb(INK_SECONDARY_LABEL))
            .children(row.worktree.clone().map(|path| {
                let glyph = icon(icons::FOLDER_GIT, NAV_CHECKOUT_GLYPH, INK_SUBTLE);
                if !live {
                    return div().flex_shrink_0().child(glyph).into_any_element();
                }
                let tip = match &row.branch {
                    Some(branch) => format!("Worktree: {path} ({branch})"),
                    None => format!("Worktree: {path}"),
                };
                div()
                    .id(("nav-card-worktree", thread.get() as usize))
                    .debug_selector(move || format!("nav-card-worktree-{}", thread.get()))
                    .flex_shrink_0()
                    .nav_tooltip(tip)
                    .child(glyph)
                    .into_any_element()
            }))
            .children(branch.map(|branch| {
                tag(div(), live, "nav-card-branch", thread)
                    .min_w_0()
                    .flex_shrink(1.)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(branch)
            }))
            .children(row.pr.as_ref().filter(|_| live).map(|pr| {
                doors
                    .pr
                    .unwrap_or_else(|| pr_chip(thread, pr).into_any_element())
            }))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .justify_end()
                    .gap(px(SPACE_1))
                    .children(logo()),
            )
    });
    let frame = if live {
        div()
            .id(("nav-thread", thread.get() as usize))
            .debug_selector(move || format!("nav-thread-{}", thread.get()))
            .on_hover(card_hover(key, row.dims_whole()))
    } else {
        div().id(("nav-ghost", thread.get() as usize))
    };
    frame
        .relative()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .my(px(NAV_CARD_ITEM_PAD_Y))
        .h(px(if checkout {
            NAV_CARD_H
        } else {
            NAV_CARD_H_SHORT
        }))
        .pt(px(NAV_CARD_PAD_Y))
        .pb(px(NAV_CARD_PAD_Y))
        .px(px(NAV_ROW_PAD_X))
        .rounded(px(R_CONTROL))
        .overflow_hidden()
        .whitespace_nowrap()
        .line_height(px(NAV_CARD_LINE1_H))
        .when_some(fill, |card, fill: Hsla| card.bg(fill))
        .when(opacity < 1.0, |card| card.opacity(opacity))
        .child(line1)
        .child(line2)
        .children(line3)
}

/// The status slot's label: its glyph, then its words in tabular
/// figures, in the state's ink.
fn status_face(thread: ThreadId, label: &StatusLabel, live: bool) -> Div {
    tag(div(), live, "nav-state", thread)
        .flex()
        .items_center()
        .gap(px(NAV_STATUS_GAP))
        .font_weight(label.weight)
        .text_color(rgb(label.ink))
        .children(
            label
                .glyph
                .map(|glyph| icon(glyph, NAV_STATUS_ICON, label.ink)),
        )
        .child(components::tabular(div().child(label.text.clone())))
}

/// The Provider's logo at the end of the card, in its brand colour at
/// 60%. (Earlier Providers after a Handover are not drawn: the history is
/// not cheaply readable.)
fn provider_logo(provider: Provider) -> Div {
    let (glyph, ink) = match provider {
        Provider::Claude => (icons::CLAUDE, PROVIDER_CLAUDE),
        Provider::Codex => (icons::CODEX, PROVIDER_CODEX),
    };
    div()
        .debug_selector(move || {
            format!("nav-card-provider-{}", ferrite_ui::provider::word(provider))
        })
        .flex()
        .flex_shrink_0()
        .opacity(NAV_PROVIDER_OPACITY)
        .child(icon(glyph, NAV_PROVIDER_LOGO, ink))
}

/// The pin on line 1 of a pinned card; under the pointer it turns to
/// `PinOff`, whose press (the cockpit's) unpins.
pub fn pin_glyph(thread: ThreadId, hover_unpin: bool) -> Stateful<Div> {
    let key = SharedString::from(format!("nav-card-pin-{}", thread.get()));
    let over = hover_unpin && hovered(&key);
    div()
        .id(key.clone())
        .debug_selector(move || format!("nav-card-pin-{}", thread.get()))
        .flex()
        .flex_shrink_0()
        .on_hover(motion::hover_snap(key))
        .child(
            div()
                .opacity((NAV_PIN_INK & 0xff) as f32 / 255.0)
                .child(icon(
                    if over { icons::PIN_OFF } else { icons::PIN },
                    NAV_SNOOZE_GLYPH,
                    NAV_PIN_INK >> 8,
                )),
        )
}

/// One of the strip's buttons: muted, strong under the pointer.
fn strip_button(key: SharedString, selector: String) -> Stateful<Div> {
    let ink = motion::mix(
        rgb(INK_SUBTLE).into(),
        rgb(INK_STRONG).into(),
        motion::hover_t(&key),
    );
    div()
        .id(key.clone())
        .debug_selector(move || selector.clone())
        .flex()
        .flex_shrink_0()
        .items_center()
        .h_full()
        .px(px(NAV_STRIP_PAD_X))
        .rounded(px(R_CONTROL))
        .text_size(px(FS_NAV_META))
        .text_color(ink)
        .on_hover(motion::hover_snap(key))
}

/// The strip's Snooze: a 12px clock. The cockpit opens its menu.
pub fn snooze_button(thread: ThreadId) -> Stateful<Div> {
    let key = SharedString::from(format!("nav-card-snooze-{}", thread.get()));
    let ink = if hovered(&key) {
        INK_STRONG
    } else {
        INK_SUBTLE
    };
    strip_button(key, format!("nav-card-snooze-{}", thread.get()))
        .nav_tooltip("Snooze thread")
        .child(icon(icons::CLOCK, NAV_SNOOZE_GLYPH, ink))
}

/// The strip's Park: a 14px check and the word, which meets the card's
/// right padding. The cockpit parks.
pub fn park_button(thread: ThreadId) -> Stateful<Div> {
    let key = SharedString::from(format!("nav-card-park-{}", thread.get()));
    let ink = if hovered(&key) {
        INK_STRONG
    } else {
        INK_SUBTLE
    };
    strip_button(key, format!("nav-card-park-{}", thread.get()))
        .gap(px(SPACE_1))
        .mr(px(-NAV_PARK_PULL))
        .nav_tooltip("Park thread")
        .child(icon(icons::CHECK, NAV_PARK_GLYPH, ink))
        .child("Park")
}

/// The strip: its buttons, left to right, the slot's height, `width`
/// wide. The width is said rather than left to layout, which sizes the
/// slot to nothing and lets the buttons spill past the card's clip, out
/// of the pointer's reach.
pub fn strip(buttons: Vec<AnyElement>, width: f32) -> AnyElement {
    div()
        .flex()
        .flex_shrink_0()
        .w(px(width))
        .items_center()
        .h_full()
        .children(buttons)
        .into_any_element()
}

/// How wide the strip of Snooze (when it is offered) and Park is.
pub fn strip_w(snooze: bool, measure: &Measure) -> f32 {
    let button = |content: f32| content + 2.0 * NAV_STRIP_PAD_X;
    let park =
        button(NAV_PARK_GLYPH + SPACE_1 + measure.ui("Park", FS_NAV_META, W_BODY)) - NAV_PARK_PULL;
    park + if snooze {
        button(NAV_SNOOZE_GLYPH)
    } else {
        0.0
    }
}

/// The PR chip on line 3: the state's glyph in its tint, then the number
/// (no `#`), underlined under the pointer. Its hover popover names the PR
/// and opens it in the browser. The cockpit wires the chip's press.
pub fn pr_chip(thread: ThreadId, pr: &NavPr) -> Stateful<Div> {
    let key = SharedString::from(format!("nav-card-pr-{}", thread.get()));
    let over = hovered(&key);
    let popover = pr.clone();
    div()
        .id(key.clone())
        .debug_selector(move || format!("nav-card-pr-{}", thread.get()))
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(NAV_PR_GAP))
        .on_hover(motion::hover_snap(key))
        .hoverable_tooltip(move |_, cx| pr_popover(&popover, cx))
        .child(pr_glyph(pr.state))
        .child(components::tabular(
            div()
                .text_color(rgb(INK_SECONDARY_LABEL))
                .when(over, |number| number.text_decoration_1())
                .child(pr.number.to_string()),
        ))
}

fn pr_glyph(state: NavPrState) -> Div {
    let tint = state.tint();
    div()
        .flex()
        .opacity((tint & 0xff) as f32 / 255.0)
        .child(icon(state.glyph(), NAV_PR_MARK, tint >> 8))
}

/// The chip's popover, `TOOLTIP_MAX_W` wide: `#415` in mono and the PR's
/// title at 75%; a press on the row opens the PR in the browser.
fn pr_popover(pr: &NavPr, cx: &mut App) -> gpui::AnyView {
    let pr = pr.clone();
    cx.new(|_| PrPopover(pr)).into()
}

struct PrPopover(NavPr);

impl gpui::Render for PrPopover {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        let pr = &self.0;
        let url = pr.url.clone();
        let key = SharedString::from(format!("nav-pr-popover-{}", pr.number));
        let ground = motion::mix(
            rgba(TRANSPARENT).into(),
            rgb(NAV_POPOVER_ROW_HOVER).into(),
            motion::hover_t(&key),
        );
        ferrite_ui::menu::glass()
            .debug_selector(|| "nav-pr-popover".into())
            .w(px(TOOLTIP_MAX_W))
            .p(px(SPACE_1))
            .font_family(FONT_UI)
            .text_size(px(FS_NAV_META))
            .child(
                div()
                    .id(key.clone())
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .p(px(SPACE_1))
                    .rounded(px(NAV_POPOVER_ROW_R))
                    .bg(ground)
                    .on_hover(motion::hover_snap(key))
                    .when_some(url, |row, url| {
                        row.on_click(move |_, _, cx| cx.open_url(&url))
                    })
                    .child(pr_line(pr)),
            )
    }
}

/// A pull request's line in the PR popover and the hover card's foot: its
/// glyph, its number and its title.
pub(crate) fn pr_line(pr: &NavPr) -> Div {
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap(px(SPACE_2))
        .child(pr_glyph(pr.state))
        .child(components::tabular(
            div()
                .flex_shrink_0()
                .font_family(FONT_CODE)
                .text_color(rgb(INK_STRONG))
                .child(format!("#{}", pr.number)),
        ))
        .children(pr.title.clone().map(|title| {
            div()
                .min_w_0()
                .truncate()
                .text_color(rgba(NAV_CARD_TITLE_RECEDED))
                .child(title)
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T3's working clock: seconds, then minutes, then hours and minutes.
    #[test]
    fn a_working_card_counts_seconds_then_minutes_then_hours() {
        let secs = Duration::from_secs;
        assert_eq!(working_for(secs(0)), "0s");
        assert_eq!(working_for(secs(42)), "42s");
        assert_eq!(working_for(secs(60)), "1m");
        assert_eq!(working_for(secs(12 * 60 + 59)), "12m");
        assert_eq!(working_for(secs(3600 + 5 * 60 + 30)), "1h 5m");
    }

    /// A quiet card says `now` in its first minute, then its age.
    #[test]
    fn a_quiet_card_says_now_then_its_age() {
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let after = |secs| card_age(at, at + Duration::from_secs(secs));
        assert_eq!(after(0), "now");
        assert_eq!(after(59), "now");
        assert_eq!(after(12 * 60), "12m");
        assert_eq!(after(3 * 3600), "3h");
        assert_eq!(after(2 * 86_400), "2d");
    }

    /// The tail rules: a short last segment, else ten characters snapped
    /// to a word; nothing cut while the branch is within four of its tail.
    #[test]
    fn a_branch_keeps_its_tail_on_a_word() {
        let split = |branch| split_branch(branch);
        assert_eq!(
            split("feature/login"),
            Some(("feature/".into(), "login".into()))
        );
        assert_eq!(
            split("t3/improve-sidebar-architecture"),
            Some(("t3/improve-sidebar-".into(), "architecture".into())),
            "a long segment keeps ten, snapped back to its word"
        );
        assert_eq!(
            split("joseph/nav-cards"),
            Some(("joseph/".into(), "nav-cards".into())),
            "a short last segment is the tail"
        );
        assert_eq!(split("main"), None);
        assert_eq!(split("fix-it-now"), None, "within four of a ten tail");
        assert_eq!(
            split("worktree-pay-api-migration"),
            Some(("worktree-pay-api-".into(), "migration".into())),
            "no slash: ten, snapped forward to the shorter word"
        );
    }

    /// T3's example: the head is cut to what fits, the tail whole.
    #[test]
    fn a_long_branch_is_cut_in_its_middle() {
        let branch = "t3/improve-sidebar-architecture";
        let fits = |width: usize| move |text: &str| text.chars().count() <= width;
        assert_eq!(
            middle_truncate(branch, fits(22)),
            "t3/improv\u{2026}architecture"
        );
        assert_eq!(middle_truncate(branch, fits(40)), branch, "it fits whole");
        assert_eq!(middle_truncate("main", fits(2)), "main", "too short to cut");
        assert_eq!(
            middle_truncate(branch, fits(1)),
            "\u{2026}architecture",
            "the tail stays whole however narrow"
        );
    }
}
