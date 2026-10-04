//! The bell: every request that waits on the operator and every Thread that
//! finished while they looked elsewhere.
//!
//! Drawing and toasts only. What counts as waiting or finished is decided
//! headless in `ferrite_core::notifications` — the rows this module draws
//! are read from there, worded by the cockpit (`Row`), and every click or
//! key comes back as one `Verb`. Ferrite draws all of it in the float
//! grammar (theme WP-E): the bell's door and its badge (FL-17), the list
//! that hangs from it (FL-16), and the toasts (FL-18).

use std::collections::BTreeSet;
use std::rc::Rc;

use ferrite_core::notifications::{DecisionNoticeId, NoticeId, RequestKind};
use ferrite_core::ThreadId;
use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, App, Div, SharedString, Stateful, Window};

use crate::components;
use crate::icons;
use crate::pointer::{Pointer, PointerPressed};
use crate::theme::*;

gpui::actions!(
    notifications,
    [SelectNext, SelectPrevious, Open, Dismiss, Close]
);

/// A toast's quick answer to its request (`1 allow`, `3 deny`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickAnswer {
    Allow,
    Deny,
}

/// What a click or a key on the bell's surfaces means. The cockpit answers
/// each against the core and repaints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verb {
    /// Land on the Notice's Thread (a toast, a row, ⏎).
    Open(NoticeId),
    /// Forget one Notice (⌫ on its row).
    Dismiss(NoticeId),
    /// Land on the exact live request the row represented.
    OpenDecision(DecisionNoticeId),
    /// Hide one live request until its handle changes or ends.
    DismissDecision(DecisionNoticeId),
    /// The head's `mark all read`: every row read, none removed.
    MarkAllRead,
    /// A toast's `1 allow` / `3 deny`.
    Answer(DecisionNoticeId, QuickAnswer),
}

pub type Handle = Rc<dyn Fn(Verb, &mut Window, &mut App)>;

/// A Bell row has one actionable target, independent of provider wire data.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RowTarget {
    Notice(NoticeId),
    Decision(DecisionNoticeId),
}

/// A row's state in the lexicon: what its mark and its word say.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// A live request: `◆ needs you` for an approval, `? needs you` for a
    /// question.
    NeedsYou(RequestKind),
    /// A turn that failed, or whose last test run failed: `✗ failed`.
    Failed,
    /// A turn that finished: `✓ done`.
    Done,
}

/// What follows a row's title: what a request needs, what became of a
/// turn. One line, no project word, never a count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Detail {
    /// `Bash wants to run gh issue close 212`: the command's head (up to
    /// its first option), set as inline code in a toast.
    Run {
        tool: SharedString,
        command: SharedString,
    },
    /// `Write wants to edit src/nav.rs`.
    Touch {
        tool: SharedString,
        verb: &'static str,
        path: SharedString,
    },
    /// A question, a test summary, an error, a one-line answer, `worked
    /// for 3m 12s`.
    Words(SharedString),
    None,
}

impl Detail {
    /// The detail as one line of words.
    pub fn text(&self) -> String {
        match self {
            Detail::Run { tool, command } => format!("{tool} wants to run {command}"),
            Detail::Touch { tool, verb, path } => format!("{tool} wants to {verb} {path}"),
            Detail::Words(words) => words.to_string(),
            Detail::None => String::new(),
        }
    }
}

/// One row of the list (and the face of a toast), worded by the cockpit.
#[derive(Clone, Debug)]
pub struct Row {
    pub target: RowTarget,
    pub thread: ThreadId,
    pub title: SharedString,
    pub state: State,
    pub detail: Detail,
    /// Its age (`facts::age_label`: `now`, `2m`, `1h`).
    pub when: SharedString,
    pub read: bool,
    /// The older Notices folded under this row; dismissing the row
    /// dismisses them too.
    pub folded: Vec<NoticeId>,
}

impl Row {
    /// The row's word in the lexicon.
    pub fn word(&self) -> &'static str {
        match self.state {
            State::NeedsYou(_) => words::NEEDS_YOU,
            State::Failed => words::FAILED,
            State::Done => words::DONE,
        }
    }

    /// Whether the row stands for a live request.
    pub fn request(&self) -> bool {
        matches!(self.state, State::NeedsYou(_))
    }

    /// What opening the row does.
    pub fn open_verb(&self) -> Verb {
        target_verb(&self.target)
    }
}

// ---------------------------------------------------------------- words

/// A shell command's head (R7): its tokens up to the first one that is an
/// option (`gh issue close 212 --reason …` → `gh issue close 212`). A
/// command that opens with an option keeps that first token.
pub fn command_head(command: &str) -> String {
    let mut head: Vec<&str> = Vec::new();
    for token in command.split_whitespace() {
        if token.starts_with('-') && !head.is_empty() {
            break;
        }
        head.push(token);
    }
    head.join(" ")
}

/// The verb a non-shell tool's approval reads with (`Write wants to edit
/// …`): the files it touches in the words a person uses.
pub fn tool_verb(tool: &str) -> &'static str {
    match tool {
        "Write" | "Edit" | "MultiEdit" | "Update" | "NotebookEdit" | "apply_patch" => "edit",
        "Read" => "read",
        "WebFetch" | "Fetch" => "fetch",
        "WebSearch" => "search",
        _ => "use",
    }
}

/// A test run's summary from its output (`357 passed; 2 failed`): the last
/// line that counts both, cut to the two counts. `None` when the output
/// reports no failing run.
pub fn test_summary(output: &str) -> Option<String> {
    for line in output.lines().rev() {
        let Some(passed_at) = line.find(" passed;") else {
            continue;
        };
        let passed: String = line[..passed_at]
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let rest = &line[passed_at + " passed;".len()..];
        let failed: String = rest
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if passed.is_empty() || failed.is_empty() {
            continue;
        }
        if failed == "0" || !rest.trim_start()[failed.len()..].starts_with(" failed") {
            return None;
        }
        return Some(format!("{passed} passed; {failed} failed"));
    }
    None
}

/// A finished turn's detail (R7): its final answer when that is one line
/// that fits the row unclipped (`room` cells after the title), else `worked
/// for <settled duration>`, else nothing.
pub fn done_words(
    answer: Option<&str>,
    worked: Option<std::time::Duration>,
    room: usize,
) -> Detail {
    if let Some(answer) = answer.map(str::trim) {
        if !answer.is_empty() && !answer.contains('\n') && answer.chars().count() <= room {
            return Detail::Words(answer.to_string().into());
        }
    }
    match worked {
        Some(worked) => Detail::Words(
            format!(
                "worked for {}",
                ferrite_core::progress::settled_duration_label(worked)
            )
            .into(),
        ),
        None => Detail::None,
    }
}

/// How many cells a row's title and detail share: the list's 76 less its
/// cell of padding each side, the mark (2), the word (10) and the age (5).
pub const ROW_TEXT_CELLS: usize = 76 - 2 - 2 - 10 - 5;

// ----------------------------------------------------------------- Bell

/// The window's side of the bell: whether its list is down and where its
/// cursor stands, which Notices and requests it has seen, and which of them
/// stand as toasts right now (oldest first; the newest draws).
///
/// **Toasts** (FL-18). A new request from any Thread that is not the
/// focused Pane toasts, with the nav open or folded, and goes when it is
/// answered, opened or read. A finished turn keeps its rule: it toasts only
/// while the nav is folded and its Thread is off the board, and goes when
/// the Thread lands on the board or is read.
pub struct Bell {
    pub open: bool,
    /// The list's cursor row (`SELECTION`), first on open.
    pub cursor: usize,
    presented: Option<NoticeId>,
    presented_requests: BTreeSet<DecisionNoticeId>,
    toasts: Vec<(RowTarget, ThreadId)>,
    /// The bell's door as last laid out: what the list hangs from.
    pub door: Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
}

impl Bell {
    pub fn new() -> Self {
        Self {
            open: false,
            cursor: 0,
            presented: None,
            presented_requests: BTreeSet::new(),
            toasts: Vec::new(),
            door: Rc::new(std::cell::Cell::new(None)),
        }
    }

    /// The watermark: Notices at or below it have been seen.
    pub fn presented(&self) -> Option<NoticeId> {
        self.presented
    }

    /// The toasts standing now, oldest first.
    pub fn toasts(&self) -> &[(RowTarget, ThreadId)] {
        &self.toasts
    }

    /// Take every toast down, keeping what was presented as presented: a
    /// scene's requests that stood before the window opened.
    #[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
    pub fn clear_toasts(&mut self) {
        self.toasts.clear();
    }

    /// Take one toast down (its button acted on it).
    pub fn drop_toast(&mut self, target: &RowTarget) {
        self.toasts.retain(|(standing, _)| standing != target);
    }

    /// Move the watermark past every Notice born since the last frame
    /// (`fresh`: id, Thread, read), and toast each unread one whose Thread
    /// is `toastable`.
    pub fn present(
        &mut self,
        fresh: impl IntoIterator<Item = (NoticeId, ThreadId, bool)>,
        toastable: &dyn Fn(ThreadId) -> bool,
    ) {
        for (id, thread, read) in fresh {
            self.presented = Some(self.presented.map_or(id, |seen| seen.max(id)));
            if read || !toastable(thread) {
                continue;
            }
            // One finished toast per Thread: a second finish replaces it.
            self.toasts.retain(|(target, standing)| {
                !(matches!(target, RowTarget::Notice(_)) && *standing == thread)
            });
            self.toasts.push((RowTarget::Notice(id), thread));
        }
    }

    /// Take down every standing completion toast `keep` no longer admits:
    /// its Thread landed on the board, or its Notice was read.
    pub fn retract(&mut self, keep: &dyn Fn(NoticeId, ThreadId) -> bool) {
        self.toasts.retain(|(target, thread)| match target {
            RowTarget::Notice(id) => keep(*id, *thread),
            RowTarget::Decision(_) => true,
        });
    }

    /// Keep request toasts in step with the live requests (`live`: id, read):
    /// a request arriving unread from a `toastable` Thread toasts once; its
    /// toast goes the moment it resolves, is dismissed or is read.
    pub fn present_requests(
        &mut self,
        live: impl IntoIterator<Item = (DecisionNoticeId, bool)>,
        toastable: &dyn Fn(ThreadId) -> bool,
    ) {
        let live: Vec<(DecisionNoticeId, bool)> = live.into_iter().collect();
        self.toasts.retain(|(target, _)| match target {
            RowTarget::Decision(id) => live.iter().any(|(live, read)| live == id && !read),
            RowTarget::Notice(_) => true,
        });
        let seen = std::mem::replace(
            &mut self.presented_requests,
            live.iter().map(|(id, _)| id.clone()).collect(),
        );
        for (id, read) in live {
            if seen.contains(&id) || read || !toastable(id.thread) {
                continue;
            }
            let thread = id.thread;
            self.toasts.push((RowTarget::Decision(id), thread));
        }
    }
}

impl Default for Bell {
    fn default() -> Self {
        Self::new()
    }
}

// --------------------------------------------------------------- the door

/// The bell's door in the titlebar band (the prototype's `.ib`, FL-17):
/// `ICON_BUTTON` × `ICON_BUTTON_H`, the bell at `ICON_BUTTON_GLYPH` in
/// `TEXT_MUTED`, `paint::HOVER` and `TEXT` under the pointer (and while the
/// list is down), and the badge: the unread waiting requests as a bare
/// `BADGE_FS` digit in `ATTENTION` at its top-right corner — hidden at
/// zero, never tinted by a failure.
pub fn door(requests: usize, open: bool, cx: &App) -> gpui::component::button::Button {
    let id = gpui::ElementId::from("notifications-bell");
    let key = crate::pointer::hover_key(&id);
    let rest: gpui::Hsla = if open {
        paint::HOVER.into()
    } else {
        gpui::rgba(TRANSPARENT).into()
    };
    let glyph = if open {
        rgb(TEXT).into()
    } else {
        crate::motion::hover_blend(&key, rgb(TEXT_MUTED).into(), rgb(TEXT).into())
    };
    components::faded_button(
        id,
        rest,
        paint::HOVER.into(),
        paint::PRESS.into(),
        rgb(TEXT_MUTED).into(),
        cx,
    )
    .debug_selector(|| "notifications-bell".into())
    .relative()
    .w(px(ICON_BUTTON))
    .h(px(ICON_BUTTON_H))
    .p_0()
    .accessibility_label("Notifications")
    .child(components::css_box(
        icons::icon(icons::BELL, ICON_BUTTON_GLYPH, TEXT_MUTED).text_color(glyph),
    ))
    .when(requests > 0, |door| door.child(badge(requests)))
}

/// The badge: the prototype's superscript — `BADGE_FS` `W_STRONG` tabular
/// digits in `ATTENTION` on no ground at the door's top-right, `99+` past
/// two digits. It hangs in the kit button's content box, whose transparent
/// 1px edge already stands it the prototype's `right:1px` in from the
/// door's edge.
fn badge(requests: usize) -> Div {
    let count: SharedString = if requests > 99 {
        "99+".into()
    } else {
        requests.to_string().into()
    };
    components::tabular(
        div()
            .debug_selector(|| "notifications-badge".into())
            .absolute()
            .top(px(0.))
            .right(px(0.))
            .font_family(FONT_UI)
            .text_size(px(BADGE_FS))
            .line_height(px(BADGE_LH))
            .font_weight(W_STRONG)
            .text_color(rgb(BADGE_INK))
            .child(count),
    )
}

/// The badge's one ink: what waits on the operator.
pub const BADGE_INK: u32 = ATTENTION;

// ---------------------------------------------------------- marks, words

/// A row's or toast's mark, in its 2-cell column: `◆` (drawn) for an
/// approval and `?` for a question, both `ATTENTION`; `✗` (drawn) `BLOCKED`
/// for a failure; `✓` (drawn) `TEXT_MUTED` for a turn that finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Drawn(&'static str, u32),
    Typed(&'static str, u32),
}

fn mark(state: State) -> Mark {
    match state {
        State::NeedsYou(RequestKind::Permission) => Mark::Drawn(icons::DIAMOND, ATTENTION),
        State::NeedsYou(RequestKind::Question) => Mark::Typed("?", ATTENTION),
        State::Failed => Mark::Drawn(icons::CROSS, BLOCKED),
        State::Done => Mark::Drawn(icons::CHECK, TEXT_MUTED),
    }
}

/// The mark drawn in its 2-cell column, one row high.
fn mark_cell(state: State) -> Div {
    let cell = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .w(px(NOTICE_MARK_W))
        .h(px(LH_UI));
    match mark(state) {
        // The `◆` as the fallback face sets it: 8px wide, its ink starting
        // where the cell's text would (the drawn shape is 60% of its box).
        Mark::Drawn(path, ink) if path == icons::DIAMOND => {
            cell.child(icons::icon(path, NOTICE_DIAMOND, ink).ml(px(-NOTICE_DIAMOND * 0.2 + 0.25)))
        }
        Mark::Drawn(path, ink) => cell.child(icons::icon(path, GLYPH_BOX, ink)),
        Mark::Typed(glyph, ink) => cell.text_color(rgb(ink)).child(glyph),
    }
}

/// Folds each Thread's completions into its newest (`rows` are newest
/// first): the survivor carries the older ids in `folded`, unread if any of
/// them is. Requests are never folded — each is its own question.
pub fn fold(rows: Vec<Row>) -> Vec<Row> {
    let mut kept: Vec<Row> = Vec::with_capacity(rows.len());
    let mut by_thread: std::collections::HashMap<ThreadId, usize> =
        std::collections::HashMap::new();
    for row in rows {
        let RowTarget::Notice(id) = &row.target else {
            kept.push(row);
            continue;
        };
        let id = *id;
        match by_thread.get(&row.thread) {
            Some(&at) => {
                kept[at].folded.push(id);
                kept[at].read &= row.read;
            }
            None => {
                by_thread.insert(row.thread, kept.len());
                kept.push(row);
            }
        }
    }
    kept
}

// ------------------------------------------------------------------ list

/// The list's footer, word for word (the prototype's `#notes .ffoot`).
pub const LIST_FOOTER: &str =
    "\u{2191}\u{2193} select \u{b7} \u{23ce} open \u{b7} \u{232b} dismiss \u{b7} esc";

/// The list under the bell (the prototype's `#notes`, FL-16): the float,
/// its head `notifications` with `mark all read` at its right (always),
/// one row per notice — the live requests first, in the order the answer
/// keys take them, then finished turns, each Thread folded to its newest —
/// the cursor's row on `FLOAT_SEL`, and the footer of its keys. Its keys
/// ride `focus` (`Notifications` context).
pub fn list(rows: &[Row], cursor: usize, focus: &gpui::FocusHandle, handle: Handle) -> Div {
    let mark_read = handle.clone();
    let head = crate::menu::head("notifications").child(
        div()
            .id("notifications-mark-read")
            .debug_selector(|| "notifications-mark-read".into())
            .flex_shrink_0()
            .px(px(CH))
            .mr(px(-CH))
            .text_color(rgb(TEXT_MUTED))
            .hover_float("notifications-mark-read")
            .press_float()
            .child("mark all read")
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                mark_read(Verb::MarkAllRead, window, cx)
            }),
    );
    let panel = crate::menu::float()
        .debug_selector(|| "notifications-panel".into())
        .w(px(NOTICE_PANEL_W))
        .max_h(px(MENU_MAX_H))
        .track_focus(focus)
        .key_context("Notifications")
        .child(head);
    if rows.is_empty() {
        return panel
            .child(crate::menu::note("nothing waits, nothing finished"))
            .child(crate::menu::footer_line(LIST_FOOTER));
    }
    let list = div()
        .id("notifications-list")
        .flex()
        .flex_col()
        .min_h_0()
        .overflow_y_scroll()
        .children(
            rows.iter()
                .enumerate()
                .map(|(index, row)| row_element(index, row, index == cursor, handle.clone())),
        );
    panel
        .child(list)
        .child(crate::menu::footer_line(LIST_FOOTER))
}

/// One notice (the prototype's `.nt`): one row in four columns — the mark,
/// the state word in its colour, the title (`TEXT_STRONG` while unread)
/// with ` · <detail>` muted after it, and the age right-aligned. Under the
/// pointer only `FLOAT_HOVER`; its whole text stays reachable to assistive
/// technology.
fn row_element(index: usize, row: &Row, cursor: bool, handle: Handle) -> Stateful<Div> {
    let verb = row.open_verb();
    let key: SharedString = format!("notice-row-{index}").into();
    let detail = row.detail.text();
    let word_color = word_ink(row.word());
    let element = components::text_ui()
        .id(("notice-row", index))
        .debug_selector(move || format!("notice-row-{index}"))
        .flex()
        .items_center()
        .w_full()
        .flex_shrink_0()
        .h(px(FLOAT_ROW_H))
        .px(px(FLOAT_PAD_X))
        .whitespace_nowrap()
        .aria_label(SharedString::from(format!(
            "{} \u{b7} {} \u{b7} {} \u{b7} {}",
            row.word(),
            row.title,
            detail,
            row.when
        )))
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            handle(verb.clone(), window, cx)
        })
        .child(mark_cell(row.state))
        .child(
            div()
                .flex_shrink_0()
                .w(px(NOTICE_STATE_W))
                .text_color(rgb(word_color))
                .child(row.word()),
        )
        .child({
            // The title and its detail are one run, cut as the prototype's
            // cell cuts it: its `…` in the cell's own `TEXT`, whatever it
            // hides (the detail's dim, mostly).
            let title_ink = rgb(if row.read { TEXT } else { TEXT_STRONG });
            let line = if detail.is_empty() {
                row.title.to_string()
            } else {
                format!("{} \u{b7} {detail}", row.title)
            };
            let highlights = vec![
                (
                    0..row.title.len(),
                    gpui::HighlightStyle {
                        color: Some(title_ink.into()),
                        ..Default::default()
                    },
                ),
                (
                    row.title.len()..line.len(),
                    gpui::HighlightStyle {
                        color: Some(rgb(TEXT_MUTED).into()),
                        ..Default::default()
                    },
                ),
            ];
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_color(rgb(TEXT))
                .child(
                    crate::pane::CellCut::plain(line.clone(), rgb(TEXT).into(), CH).child(
                        gpui::StyledText::new(line)
                            .with_highlights(highlights)
                            .into_any_element(),
                    ),
                )
        })
        .child(components::tabular(
            div()
                .flex_shrink_0()
                .w(px(NOTICE_AGE_W))
                .flex()
                .justify_end()
                .text_color(rgb(TEXT_MUTED))
                .child(row.when.clone()),
        ));
    if cursor {
        element.float_cursor().press_float()
    } else {
        element.hover_float(key).press_float()
    }
}

// ---------------------------------------------------------------- toasts

/// A quick answer in a toast (the prototype's `.qa button`): the key in
/// `TEXT_MUTED` and the word in `TEXT`, in a 1px `FLOAT_EDGE` box a cell
/// of padding wide, `TOAST_BUTTON_H` tall; under the pointer the float's
/// band (`FLOAT_BAND2`) and the word `TEXT_STRONG`, over the one blend.
fn quick_button(
    id: impl Into<gpui::ElementId>,
    key: &'static str,
    word: &'static str,
) -> Stateful<Div> {
    let id = id.into();
    let blend = crate::pointer::hover_key(&id);
    let ink = crate::motion::hover_blend(&blend, rgb(TEXT).into(), rgb(TEXT_STRONG).into());
    // Its cells, a cell of padding each side and its edge — on the whole
    // pixel, as the browser rounds each button's box (`3 deny`'s 64.4px
    // lays out at 64, where the device grid would make it 64.5): padded
    // runs would snap each pad from 7.8 to 8 and drift the row.
    let width =
        (components::cells_width(key) + components::cells_width(word) + 2.0 * CH + 2.0).round();
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .h(px(TOAST_BUTTON_H))
        .w(px(width))
        .border_1()
        .border_color(FLOAT_EDGE)
        .whitespace_nowrap()
        .hover_quick(blend)
        .press_float()
        .child(components::cells(key).text_color(rgb(TEXT_MUTED)))
        .child(components::cells(word).text_color(ink))
}

/// A toast (the prototype's `.toast`, FL-18), in the float grammar with no
/// close: the head (`◆ needs you · <title>`, `⌘D` at its right while a
/// request waits; `✓ done · <title>` for a finished turn), one body line,
/// and the quick answers — `1 allow` `3 deny` `⏎ open` for an approval,
/// `⏎ open` alone for a question or a finished turn.
pub fn toast(row: &Row, handle: Handle) -> Div {
    let thread = row.thread.get() as usize;
    let request = row.request();
    let word = row.word();
    let body: AnyElement = match &row.detail {
        Detail::Run { tool, command } => div()
            .flex()
            .min_w_0()
            .overflow_hidden()
            .child(components::cells(format!("{tool} wants to run ")).text_color(rgb(TEXT)))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(INLINE_CODE))
                    .child(command.clone()),
            )
            .into_any_element(),
        detail => div()
            .min_w_0()
            .truncate()
            .text_color(rgb(TEXT))
            .child(SharedString::from(detail.text()))
            .into_any_element(),
    };
    let mut buttons: Vec<AnyElement> = Vec::new();
    if let (RowTarget::Decision(id), State::NeedsYou(RequestKind::Permission)) =
        (&row.target, row.state)
    {
        let (allow, deny) = (handle.clone(), handle.clone());
        let (allow_id, deny_id) = (id.clone(), id.clone());
        buttons.push(
            quick_button(("toast-allow", thread), "1 ", "allow")
                .debug_selector(move || format!("toast-allow-{thread}"))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    allow(
                        Verb::Answer(allow_id.clone(), QuickAnswer::Allow),
                        window,
                        cx,
                    )
                })
                .into_any_element(),
        );
        buttons.push(
            quick_button(("toast-deny", thread), "3 ", "deny")
                .debug_selector(move || format!("toast-deny-{thread}"))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    deny(Verb::Answer(deny_id.clone(), QuickAnswer::Deny), window, cx)
                })
                .into_any_element(),
        );
    }
    let open = row.open_verb();
    let opener = handle.clone();
    buttons.push(
        quick_button(("toast-open", thread), "\u{23ce} ", "open")
            .debug_selector(move || format!("toast-open-{thread}"))
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                opener(open.clone(), window, cx)
            })
            .into_any_element(),
    );
    crate::menu::float()
        .debug_selector(move || format!("toast-{thread}"))
        .w(px(TOAST_BOX_W))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .h(px(FLOAT_ROW_H))
                .px(px(FLOAT_PAD_X))
                .whitespace_nowrap()
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .child(mark_cell(row.state))
                        .child(components::cells(word).text_color(rgb(word_ink(word))))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(rgb(TEXT_MUTED))
                                .child(SharedString::from(format!(" \u{b7} {}", row.title))),
                        ),
                )
                .when(request, |head| {
                    head.children(
                        components::bound_chord("cockpit::NextDecision").map(|keys| {
                            components::key_combo(&keys, TEXT_MUTED)
                                .flex_shrink_0()
                                .ml(px(FLOAT_DETAIL_GAP))
                        }),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .h(px(FLOAT_ROW_H))
                .items_center()
                .min_w_0()
                .px(px(FLOAT_PAD_X))
                .child(body),
        )
        .child(
            div()
                .flex()
                .gap(px(CH))
                .px(px(FLOAT_PAD_X))
                .pt(px(TOAST_QUICK_GAP))
                .pb(px(TOAST_QUICK_GAP))
                .children(buttons),
        )
}

pub fn target_verb(target: &RowTarget) -> Verb {
    match target {
        RowTarget::Notice(id) => Verb::Open(*id),
        RowTarget::Decision(id) => Verb::OpenDecision(id.clone()),
    }
}

pub fn dismiss_verb(target: &RowTarget) -> Verb {
    match target {
        RowTarget::Notice(id) => Verb::Dismiss(*id),
        RowTarget::Decision(id) => Verb::DismissDecision(id.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::activity::DecisionHandle;
    use std::time::Duration;

    fn row(state: State, detail: Detail) -> Row {
        Row {
            target: RowTarget::Notice(NoticeId::from_u64(1)),
            thread: ThreadId::new(3),
            title: "Fold regression".into(),
            state,
            detail,
            when: "9m".into(),
            read: false,
            folded: Vec::new(),
        }
    }

    fn request(thread: u64, serial: u64) -> DecisionNoticeId {
        DecisionNoticeId {
            thread: ThreadId::new(thread),
            handle: DecisionHandle {
                generation: 1,
                serial,
                request_id: format!("r{serial}"),
            },
        }
    }

    /// Each detail shape the list prints (R7): an approval's command head,
    /// a non-shell tool's verb and path, a question, a failing run's
    /// summary, a one-line answer that fits, else `worked for`.
    #[test]
    fn each_detail_reads_as_the_prototype() {
        assert_eq!(
            command_head("gh issue close 212 --reason \"not planned\""),
            "gh issue close 212"
        );
        assert_eq!(command_head("cargo test -p ferrite nav::"), "cargo test");
        assert_eq!(command_head("--version"), "--version");
        let run = Detail::Run {
            tool: "Bash".into(),
            command: command_head("gh issue close 212 --reason x").into(),
        };
        assert_eq!(run.text(), "Bash wants to run gh issue close 212");
        let touch = Detail::Touch {
            tool: "Write".into(),
            verb: tool_verb("Write"),
            path: "src/nav.rs".into(),
        };
        assert_eq!(touch.text(), "Write wants to edit src/nav.rs");
        assert_eq!(tool_verb("Read"), "read");
        assert_eq!(
            test_summary("running 359 tests\ntest result: FAILED. 357 passed; 2 failed; 0 ignored"),
            Some("357 passed; 2 failed".into())
        );
        assert_eq!(
            test_summary("test result: ok. 38 passed; 0 failed; 0 ignored"),
            None,
            "a green run is no failure"
        );
        assert_eq!(test_summary("nothing ran"), None);
        assert_eq!(
            done_words(
                Some("14 crates bumped, deny clean"),
                Some(Duration::from_secs(160)),
                40
            ),
            Detail::Words("14 crates bumped, deny clean".into())
        );
        let worked = done_words(Some("a\nlong answer"), Some(Duration::from_secs(192)), 40);
        assert!(worked.text().starts_with("worked for "), "{worked:?}");
        assert_eq!(
            done_words(Some("x".repeat(41).as_str()), None, 40),
            Detail::None,
            "too long for the row, no time known"
        );
    }

    /// The marks and words (WP-E): `◆`/`?` needs you in attention, `✗`
    /// failed in red, `✓` done muted — and the badge never borrows red.
    #[test]
    fn the_marks_and_words_follow_the_lexicon() {
        let waiting = row(State::NeedsYou(RequestKind::Permission), Detail::None);
        assert_eq!(waiting.word(), words::NEEDS_YOU);
        assert_eq!(word_ink(waiting.word()), ATTENTION);
        assert_eq!(mark(waiting.state), Mark::Drawn(icons::DIAMOND, ATTENTION));
        assert_eq!(
            mark(State::NeedsYou(RequestKind::Question)),
            Mark::Typed("?", ATTENTION)
        );
        let failed = row(State::Failed, Detail::Words("357 passed; 2 failed".into()));
        assert_eq!(
            (failed.word(), word_ink(failed.word())),
            (words::FAILED, BLOCKED)
        );
        assert_eq!(mark(State::Failed), Mark::Drawn(icons::CROSS, BLOCKED));
        let done = row(State::Done, Detail::None);
        assert_eq!(
            (done.word(), word_ink(done.word())),
            (words::DONE, TEXT_MUTED)
        );
        assert_eq!(mark(State::Done), Mark::Drawn(icons::CHECK, TEXT_MUTED));
        assert_eq!(BADGE_INK, ATTENTION);
        const { assert!(ROW_TEXT_CELLS >= 40) };
    }

    /// A Thread's completions fold into its newest; requests never fold;
    /// the survivor is unread if any folded one is.
    #[test]
    fn completions_fold_per_thread_and_requests_stand_alone() {
        let at = |id: u64, thread: u64, read: bool| Row {
            target: RowTarget::Notice(NoticeId::from_u64(id)),
            thread: ThreadId::new(thread),
            read,
            ..row(State::Done, Detail::None)
        };
        let asking = Row {
            target: RowTarget::Decision(request(3, 9)),
            state: State::NeedsYou(RequestKind::Question),
            ..at(9, 3, false)
        };
        let folded = fold(vec![
            asking,
            at(5, 3, true),
            at(4, 7, true),
            at(3, 3, false),
            at(2, 3, true),
        ]);
        assert_eq!(folded.len(), 3);
        assert!(folded[0].request());
        assert_eq!(
            folded[1].folded,
            vec![NoticeId::from_u64(3), NoticeId::from_u64(2)]
        );
        assert!(!folded[1].read, "unread if any folded Notice is");
        assert!(folded[2].folded.is_empty());
    }

    /// FL-18: a new request from a Thread off the focused Pane toasts once
    /// and goes when it is read or resolves; one on the focused Pane never
    /// toasts; a finish toasts by its own rule and goes when kept no more.
    #[test]
    fn toasts_stand_by_the_rules_and_retract() {
        let mut bell = Bell::new();
        let focused = ThreadId::new(1);
        let off_focus = |thread: ThreadId| thread != focused;
        let waiting = request(2, 1);
        let here = request(1, 2);
        bell.present_requests(
            vec![(waiting.clone(), false), (here.clone(), false)],
            &off_focus,
        );
        assert_eq!(
            bell.toasts(),
            &[(RowTarget::Decision(waiting.clone()), ThreadId::new(2))]
        );
        // Seen once: the next frame adds nothing.
        bell.present_requests(
            vec![(waiting.clone(), false), (here.clone(), false)],
            &off_focus,
        );
        assert_eq!(bell.toasts().len(), 1);
        // Read (opened, or its Thread landed on): it goes.
        bell.present_requests(vec![(waiting.clone(), true)], &off_focus);
        assert!(bell.toasts().is_empty());

        let notice = NoticeId::from_u64(4);
        bell.present(vec![(notice, ThreadId::new(5), false)], &|_| true);
        assert_eq!(bell.presented(), Some(notice));
        assert_eq!(bell.toasts().len(), 1);
        bell.retract(&|_, _| false);
        assert!(bell.toasts().is_empty());
        // A read Notice never toasts.
        bell.present(
            vec![(NoticeId::from_u64(5), ThreadId::new(5), true)],
            &|_| true,
        );
        assert!(bell.toasts().is_empty());
        // A button that acted takes its toast down.
        let other = request(6, 3);
        bell.present_requests(vec![(other.clone(), false)], &off_focus);
        bell.drop_toast(&RowTarget::Decision(other));
        assert!(bell.toasts().is_empty());
    }
}
