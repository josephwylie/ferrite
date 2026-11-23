//! Needs you: every request that waits on the operator and every Thread
//! that finished while they looked elsewhere (CONTEXT.md's Inbox,
//! its alerts and toasts).
//!
//! Drawing and toasts only. What counts as waiting or finished is decided
//! headless in `ferrite_core::notifications` — the rows this module draws
//! are read from there, worded by the cockpit (`Row`), and every click or
//! key comes back as one `Verb`. Ferrite draws all of it in the float
//! grammar (theme WP-E): the bar's need-you door and its badge (FL-17), the
//! Inbox's rows (FL-16), and the toasts (FL-18).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;
use std::time::{Duration, Instant};

use ferrite_core::notifications::{DecisionNoticeId, NoticeId, Notifications, RequestKind};
use ferrite_core::{MachineId, ThreadId, ThreadKey};
use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, App, Div, SharedString, Stateful, Window};

use ferrite_ui::components;
use ferrite_ui::pointer::{Pointer, PointerPressed};
use ferrite_ui::theme::*;

/// A toast's quick answer to its request (`1 allow`, `3 deny`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickAnswer {
    Allow,
    Deny,
}

/// What a click or a key on the Inbox's surfaces means. The cockpit answers
/// each against the core and repaints. A Notice's and a request's ids are
/// their machine's, so each verb names the machine it is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verb {
    /// Land on the Notice's Thread (a toast, a row, ⏎).
    Open(MachineId, NoticeId),
    OpenThread(ThreadKey),
    /// Forget one Notice (⌫ on its row).
    Dismiss(MachineId, NoticeId),
    /// Land on the exact live request the row represented.
    OpenDecision(MachineId, DecisionNoticeId),
    /// Hide one live request until its handle changes or ends.
    DismissDecision(MachineId, DecisionNoticeId),
    /// The head's `mark all read`: every row read on every machine, none
    /// removed.
    MarkAllRead,
    /// A toast's `1 allow` / `3 deny`.
    Answer(MachineId, DecisionNoticeId, QuickAnswer),
}

pub type Handle = Rc<dyn Fn(Verb, &mut Window, &mut App)>;

/// A NeedsYou row has one actionable target, independent of provider wire
/// data: an id on the row's machine.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RowTarget {
    Notice(NoticeId),
    Thread(ThreadId),
    Decision(DecisionNoticeId),
}

impl RowTarget {
    /// Whether its record still wants the operator: there, unread, and
    /// for a request still live and not dismissed.
    pub fn wants(&self, notifications: &Notifications) -> bool {
        match self {
            RowTarget::Thread(_) => false,
            RowTarget::Notice(id) => notifications.get(*id).is_some_and(|notice| !notice.read),
            RowTarget::Decision(id) => notifications
                .decision(id)
                .is_some_and(|notice| !notice.read),
        }
    }
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
    /// An open Thread awaiting handling with no completion Notice.
    Idle,
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
/// One about a machine other than this computer is titled with the
/// machine's name first.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub target: RowTarget,
    /// Its Thread, on the machine `target`'s id is of.
    pub thread: ThreadKey,
    pub title: SharedString,
    pub state: State,
    pub detail: Detail,
    /// Its age (`facts::since_label`: `2m`, `1h`; nothing in its first
    /// minute — no surface prints `now`, theme rule 11).
    pub when: SharedString,
    pub read: bool,
    /// Its machine is not this computer: its toast's ids and selectors
    /// say the machine (`nav::RowTag`).
    pub elsewhere: bool,
}

impl Row {
    /// The row's word in the lexicon.
    pub fn word(&self) -> &'static str {
        match self.state {
            State::NeedsYou(_) => words::NEEDS_YOU,
            State::Failed => words::FAILED,
            State::Done => words::DONE,
            State::Idle => "idle",
        }
    }

    /// Whether the row stands for a live request.
    pub fn request(&self) -> bool {
        matches!(self.state, State::NeedsYou(_))
    }

    /// What opening the row does.
    pub fn open_verb(&self) -> Verb {
        target_verb(self.thread, &self.target)
    }
}

/// A machine's name before a Thread's title: what a row, a toast or a
/// system notification about a machine other than this computer is
/// called.
pub fn machine_title(machine: &str, title: &str) -> SharedString {
    format!("{machine} \u{203a} {title}").into()
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

/// A live request's detail (R7): a question's text; an approval's
/// `<Tool> wants to run <command head>`, or `<Tool> wants to <verb>
/// <path>` — relative inside `cwd`, the Thread's checkout, and found in
/// the call it gates (`gated_path`) when the request names none.
pub fn request_detail(
    decision: &ferrite_core::Decision,
    kind: RequestKind,
    cwd: Option<&std::path::Path>,
    gated_path: impl FnOnce() -> Option<String>,
) -> Detail {
    if let Some(questions) = ferrite_pane::pane::questions_of(decision) {
        return questions
            .first()
            .map(|question| Detail::Words(question.question.clone().into()))
            .unwrap_or(Detail::None);
    }
    if kind == RequestKind::Question {
        return Detail::Words(decision.description.clone().into());
    }
    // The tool and its command as the Decision's own head and well say
    // them: Codex's `commandExecution` is `Bash`, its login-shell wrapper
    // taken off (`Bash wants to run cargo test`, never `commandExecution
    // wants to run /bin/zsh`).
    let tool: SharedString = ferrite_pane::decision::tool_word(&decision.tool_name)
        .to_string()
        .into();
    if let Some(command) = decision
        .input
        .get("command")
        .and_then(serde_json::Value::as_str)
    {
        return Detail::Run {
            tool,
            command: command_head(&ferrite_pane::decision::unwrap_shell(command)).into(),
        };
    }
    let path = ["file_path", "path", "notebook_path", "url"]
        .iter()
        .find_map(|key| decision.input.get(*key).and_then(serde_json::Value::as_str))
        .map(str::to_string)
        .or_else(gated_path)
        .unwrap_or_else(|| decision.description.clone());
    let path = cwd
        .and_then(|cwd| {
            ferrite_pane::file_links::inside_checkout(std::path::Path::new(&path), cwd)
                .map(|rest| rest.to_string_lossy().into_owned())
        })
        .unwrap_or(path);
    Detail::Touch {
        verb: tool_verb(&tool),
        tool,
        path: path.into(),
    }
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

// ----------------------------------------------------------------- NeedsYou

/// How long an event waits before it alerts: a request answered, or a
/// finish read, within it never interrupts anyone (ADR 0017).
pub const ALERT_DELAY: Duration = Duration::from_secs(1);

/// How long a toast for a request stands.
pub const NEEDS_YOU_TOAST: Duration = Duration::from_secs(8);

/// How long a toast for a finished turn stands.
pub const DONE_TOAST: Duration = Duration::from_secs(5);

/// One event that may want the operator: a Notice or a live request, and
/// its Thread. Ids are per Store: its Thread's machine is the one they are
/// of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    pub target: RowTarget,
    pub thread: ThreadKey,
}

impl Alert {
    /// What opening it does: land on it, in place, on whichever machine.
    pub fn open_verb(&self) -> Verb {
        target_verb(self.thread, &self.target)
    }

    /// Whether `other` is about the same record on the same machine.
    fn same(&self, machine: MachineId, target: &RowTarget) -> bool {
        self.thread.machine() == machine && self.target == *target
    }

    /// A live request (needs you), not a finished turn.
    pub fn needs_you(&self) -> bool {
        matches!(self.target, RowTarget::Decision(_))
    }

    fn stands(&self) -> Duration {
        if self.needs_you() {
            NEEDS_YOU_TOAST
        } else {
            DONE_TOAST
        }
    }
}

/// A toast standing, and when it goes on its own.
#[derive(Clone, Debug)]
pub struct Toast {
    pub alert: Alert,
    until: Instant,
}

/// What one `NeedsYou::ring` did: the events that rang (each owes its sound and
/// its system notification), and whether the toasts changed.
#[derive(Debug, Default)]
pub struct Ring {
    pub rang: Vec<Alert>,
    pub redraw: bool,
}

/// What the Inbox has heard of one machine: the Notice watermark (Notices
/// at or below it were heard) and the live requests.
#[derive(Default)]
struct Heard {
    presented: Option<NoticeId>,
    requests: BTreeSet<DecisionNoticeId>,
}

/// The window's side of the Inbox: whether its list is down and where its
/// cursor stands, which Notices and requests it has heard of each machine,
/// the events waiting out `ALERT_DELAY`, the toasts standing (oldest
/// first; the newest draws), and the system notifications posted.
///
/// **Toasts** (FL-18, ADR 0017). A new finish or request waits a second,
/// then is checked again: one read, answered or gone by then is dropped.
/// A survivor toasts unless the operator is already looking at it, and
/// replaces any toast its Thread had. A toast goes on its own (8s for a
/// request, 5s for a finish), or sooner once it is read, resolved, or in
/// view.
///
/// **Every machine** the window holds is heard (ADR 0045), each apart:
/// Thread 3 here and Thread 3 on box are two Threads. What a machine other
/// than this computer held when the window first heard it is not news (the
/// window met it already waiting); what it says after rings like the rest,
/// once, and opens in place. A machine not heard from yet cannot say
/// whether its alerts still stand: they wait for it, no longer than a
/// toast would stand.
pub struct NeedsYou {
    /// The list's cursor row in the Inbox (`nav::NavView`),
    /// first on open.
    pub cursor: usize,
    cursor_thread: RefCell<Option<ThreadKey>>,
    heard: HashMap<MachineId, Heard>,
    /// Heard and waiting out `ALERT_DELAY`, one per Thread: the newest.
    pending: Vec<(Alert, Instant)>,
    toasts: Vec<Toast>,
    /// The system notifications standing, by tag (one per Thread on each
    /// machine), and what each opens when clicked.
    pub posted: BTreeMap<String, Alert>,
}

impl NeedsYou {
    pub fn new() -> Self {
        Self {
            cursor: 0,
            cursor_thread: RefCell::new(None),
            heard: HashMap::new(),
            pending: Vec::new(),
            toasts: Vec::new(),
            posted: BTreeMap::new(),
        }
    }

    pub fn reset_cursor(&mut self) {
        self.cursor = 0;
        *self.cursor_thread.get_mut() = None;
    }

    /// Resolve the cursor by Thread identity when asynchronous updates move rows.
    pub fn cursor_at(&self, rows: &[Row]) -> usize {
        let mut selected = self.cursor_thread.borrow_mut();
        let at = selected
            .and_then(|thread| rows.iter().position(|row| row.thread == thread))
            .unwrap_or(self.cursor.min(rows.len().saturating_sub(1)));
        *selected = rows.get(at).map(|row| row.thread);
        at
    }

    pub fn select_cursor(&mut self, at: usize, rows: &[Row]) {
        self.cursor = at;
        *self.cursor_thread.get_mut() = rows.get(at).map(|row| row.thread);
    }

    /// A machine's watermark: its Notices at or below it have been heard.
    pub fn presented(&self, machine: MachineId) -> Option<NoticeId> {
        self.heard.get(&machine)?.presented
    }

    /// The toasts standing now, oldest first.
    pub fn toasts(&self) -> &[Toast] {
        &self.toasts
    }

    /// Whether anything is waiting or standing: what `ring` has to keep up.
    pub fn busy(&self) -> bool {
        !self.pending.is_empty() || !self.toasts.is_empty()
    }

    /// When `ring` next has work of the clock's own after `now`: a delay
    /// up, the end of a wait on a machine not yet heard, a toast's time up.
    pub fn next_due(&self, now: Instant) -> Option<Instant> {
        let pending = self.pending.iter().map(|(alert, due)| {
            if *due > now {
                *due
            } else {
                *due + alert.stands()
            }
        });
        pending.chain(self.toasts.iter().map(|toast| toast.until)).min()
    }

    /// Take every toast down and forget what was waiting, keeping what was
    /// heard as heard: a scene's requests that stood before the window
    /// opened.
    #[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
    pub fn clear_toasts(&mut self) {
        self.pending.clear();
        self.toasts.clear();
    }

    /// Take one toast down (its button acted on it): `target`, on
    /// `machine`.
    pub fn drop_toast(&mut self, machine: MachineId, target: &RowTarget) {
        self.toasts
            .retain(|toast| !toast.alert.same(machine, target));
    }

    /// Hear what `machine` said since the last call: every Notice born
    /// since its watermark (`notices`: id, Thread, read) moves it, and its
    /// live requests (`requests`: id, Thread, read) replace the set heard.
    /// Each unread newcomer waits out `ALERT_DELAY`, displacing whatever
    /// its Thread had waiting. A machine heard for the first time with
    /// `quiet_first` only sets what was heard.
    pub fn hear(
        &mut self,
        machine: MachineId,
        quiet_first: bool,
        notices: impl IntoIterator<Item = (NoticeId, ThreadKey, bool)>,
        requests: impl IntoIterator<Item = (DecisionNoticeId, ThreadKey, bool)>,
        now: Instant,
    ) {
        let quiet = quiet_first && !self.heard.contains_key(&machine);
        let state = self.heard.entry(machine).or_default();
        let mut heard = Vec::new();
        for (id, thread, read) in notices {
            state.presented = Some(state.presented.map_or(id, |seen| seen.max(id)));
            if !read {
                heard.push(Alert {
                    target: RowTarget::Notice(id),
                    thread,
                });
            }
        }
        let requests: Vec<(DecisionNoticeId, ThreadKey, bool)> = requests.into_iter().collect();
        let seen = std::mem::replace(
            &mut state.requests,
            requests.iter().map(|(id, _, _)| id.clone()).collect(),
        );
        for (id, thread, read) in requests {
            if !seen.contains(&id) && !read {
                heard.push(Alert {
                    thread,
                    target: RowTarget::Decision(id),
                });
            }
        }
        if quiet {
            return;
        }
        for alert in heard {
            self.pending
                .retain(|(waiting, _)| waiting.thread != alert.thread);
            self.pending.push((alert, now + ALERT_DELAY));
        }
    }

    /// Forget what was heard of `machine`: the window let it go.
    pub fn forget(&mut self, machine: MachineId) {
        self.heard.remove(&machine);
        self.pending
            .retain(|(alert, _)| alert.thread.machine() != machine);
        self.toasts
            .retain(|toast| toast.alert.thread.machine() != machine);
    }

    /// Bring the Inbox up to `now`. A standing toast goes once it expired,
    /// is no longer `live` (read, resolved, dismissed) or is `in_view`. A
    /// waiting event whose delay is up is checked again: still live and
    /// not in view, it rings — toasting in place of its Thread's toast.
    /// `live` is `None` while the alert's machine has not been heard from:
    /// its toast stands on, and a waiting event waits for it as long as its
    /// toast would have stood.
    pub fn ring(
        &mut self,
        now: Instant,
        live: &dyn Fn(&Alert) -> Option<bool>,
        in_view: &dyn Fn(&Alert) -> bool,
    ) -> Ring {
        let mut ring = Ring::default();
        let standing = self.toasts.len();
        self.toasts.retain(|toast| {
            toast.until > now && live(&toast.alert) != Some(false) && !in_view(&toast.alert)
        });
        ring.redraw = self.toasts.len() != standing;
        let (due, waiting) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|(_, due)| *due <= now);
        self.pending = waiting;
        for (alert, due) in due {
            match live(&alert) {
                Some(true) if !in_view(&alert) => {}
                None if now < due + alert.stands() => {
                    self.pending.push((alert, due));
                    continue;
                }
                _ => continue,
            }
            self.toasts.retain(|toast| toast.alert.thread != alert.thread);
            self.toasts.push(Toast {
                until: now + alert.stands(),
                alert: alert.clone(),
            });
            ring.redraw = true;
            ring.rang.push(alert);
        }
        ring
    }
}

impl Default for NeedsYou {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------- marks, words

/// A row's or toast's mark, typed in its 2-cell column: `◆` for an
/// approval and `?` for a question, both `ATTENTION`; `✗` `BLOCKED` for a
/// failure; `✓` `INK_SUBTLE` for a turn that finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mark(&'static str, u32);

fn mark(state: State) -> Mark {
    match state {
        State::NeedsYou(RequestKind::Permission) => Mark("\u{25c6}", ATTENTION),
        State::NeedsYou(RequestKind::Question) => Mark("?", ATTENTION),
        State::Failed => Mark("\u{2717}", BLOCKED),
        State::Done => Mark("\u{2713}", INK_SUBTLE),
        State::Idle => Mark("", INK_SUBTLE),
    }
}

// ---------------------------------------------------------------- toasts

/// A quick answer's button: the word (sentence case, a button's) in `ink`,
/// then its key as a hint in `INK_SUBTLE` (`Allow 1`, `Open ⏎`), on a
/// `TOKEN_H` button of the float's hover fill (`paint::FLOAT_HOVER`), radius
/// `R_CONTROL`.
fn quick_pill(key: &'static str, word: &'static str, ink: gpui::Hsla) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap(px(SPACE_1_5))
        .h(px(TOKEN_H))
        .px(px(TOKEN_PAD_X))
        .rounded(px(R_CONTROL))
        .bg(paint::FLOAT_HOVER)
        .whitespace_nowrap()
        .child(div().text_color(ink).child(word))
        .child(components::glyph(key, INK_SUBTLE))
}

/// A quick answer in a toast: its pill, its word `INK_STRONG`, under the
/// pointer a step up over the one blend (`hover_quick`).
fn quick_button(
    id: impl Into<gpui::ElementId>,
    key: &'static str,
    word: &'static str,
) -> Stateful<Div> {
    let id = id.into();
    let blend = ferrite_ui::pointer::hover_key(&id);
    quick_pill(key, word, rgb(INK_STRONG).into())
        .id(id)
        .hover_quick(blend)
        .press_float()
}

/// A toast (the prototype's `.toast`, FL-18), in the float grammar with no
/// close: the head (`◆ needs you · <title>`; `✓ done · <title>` for a
/// finished turn), one body line,
/// and the quick answers — `Allow 1` `Deny 3` `Open ⏎` for an approval,
/// `Open ⏎` alone for a question or a finished turn — on whichever
/// machine.
pub fn toast(row: &Row, handle: Handle, width: f32) -> Div {
    toast_card(row, Some(handle), width)
}

/// A toast on its way out (`ToastStage`): the same card, answering no
/// pointer — its answers drawn at rest and pressing nothing, nothing under
/// it occluded, none of the live toast's selectors (`toast-leaving`).
pub fn toast_face(row: &Row, width: f32) -> Div {
    toast_card(row, None, width)
}

fn toast_card(row: &Row, handle: Option<Handle>, width: f32) -> Div {
    let tag = crate::nav::RowTag::of(row.thread, row.elsewhere);
    let machine = row.thread.machine();
    let word = row.word();
    let body: AnyElement = match &row.detail {
        Detail::Run { tool, command } => div()
            .flex()
            .min_w_0()
            .overflow_hidden()
            .child(
                div()
                    .flex_shrink_0()
                    .whitespace_nowrap()
                    .text_color(rgb(INK_STRONG))
                    .child(format!("{tool} wants to run\u{a0}")),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(INLINE_CODE_INK))
                    .child(command.clone()),
            )
            .into_any_element(),
        detail => div()
            .min_w_0()
            .truncate()
            .text_color(rgb(INK_STRONG))
            .child(SharedString::from(detail.text()))
            .into_any_element(),
    };
    let mut answers: Vec<(&'static str, &'static str, &'static str, Verb)> = Vec::new();
    if let (RowTarget::Decision(id), State::NeedsYou(RequestKind::Permission)) =
        (&row.target, row.state)
    {
        answers.push((
            "toast-allow",
            "1",
            "Allow",
            Verb::Answer(machine, id.clone(), QuickAnswer::Allow),
        ));
        answers.push((
            "toast-deny",
            "3",
            "Deny",
            Verb::Answer(machine, id.clone(), QuickAnswer::Deny),
        ));
    }
    answers.push(("toast-open", "\u{23ce}", "Open", row.open_verb()));
    let buttons = answers.into_iter().map(|(name, key, word, verb)| {
        let Some(handle) = handle.clone() else {
            return quick_pill(key, word, rgb(INK_STRONG).into()).into_any_element();
        };
        quick_button(tag.id(name), key, word)
            .debug_selector(move || tag.key(name).to_string())
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                handle(verb.clone(), window, cx)
            })
            .into_any_element()
    });
    let card = match handle {
        Some(_) => ferrite_ui::menu::float()
            .debug_selector(move || tag.key("toast").to_string())
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation()),
        None => components::float_ground()
            .p(px(0.))
            .debug_selector(|| "toast-leaving".into()),
    };
    card.w(px(width))
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
                        // `◆ needs you` is one run, as the prototype types
                        // it: the mark, a space, the word, in the word's ink.
                        .child(components::glyph(
                            format!("{} {word}", mark(row.state).0),
                            word_ink(word),
                        ))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(rgb(INK_SUBTLE))
                                .child(SharedString::from(format!(" \u{b7} {}", row.title))),
                        ),
                ),
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
                .gap(px(SPACE_1_5))
                .px(px(FLOAT_PAD_X))
                .pb(px(TOAST_QUICK_GAP))
                .children(buttons),
        )
}

/// What the toast float draws as toasts come and go (FL-18). The toast
/// standing now rises in (`motion::toast_in`, keyed by `toast_key`, so a
/// new one enters and the same one stays put); one that goes on its own —
/// read elsewhere, resolved, timed out or replaced — leaves as its face
/// (`toast_face`), fading over `motion::TOAST_OUT` (`motion::toast_out`),
/// under whatever stands next. One the operator answers from the toast goes
/// at once (`dismiss`). Under reduced motion nothing lingers.
#[derive(Default)]
pub struct ToastStage(ferrite_ui::motion::Exits<Row>);

impl ToastStage {
    /// This frame's standing toast, at `now`: the toast leaving, and when it
    /// went, while it still fades.
    pub fn stage(
        &mut self,
        standing: Option<&Row>,
        now: Instant,
        reduced: bool,
    ) -> Option<(Row, Instant)> {
        self.0.stage(
            standing,
            |a, b| toast_key(a) == toast_key(b),
            ferrite_ui::motion::TOAST_OUT,
            now,
            reduced,
        )
    }

    /// The operator answered or opened the standing toast: it goes at once,
    /// as everything they dismiss does — only a toast that leaves on its own
    /// fades.
    pub fn dismiss(&mut self) {
        self.0.dismiss();
    }
}

/// A toast's identity on screen: its record, on its machine.
pub fn toast_key(row: &Row) -> SharedString {
    format!("toast-in-{}-{:?}", row.thread.machine().get(), row.target).into()
}

/// What opening `target` does: land on it, on `thread`'s machine.
pub fn target_verb(thread: ThreadKey, target: &RowTarget) -> Verb {
    let machine = thread.machine();
    match target {
        RowTarget::Thread(_) => Verb::OpenThread(thread),
        RowTarget::Notice(id) => Verb::Open(machine, *id),
        RowTarget::Decision(id) => Verb::OpenDecision(machine, id.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::activity::DecisionHandle;
    use std::time::Duration;

    /// Thread `thread` on this computer (machine 0) or another.
    fn on(machine: u32, thread: u64) -> ThreadKey {
        ThreadKey::fixture(machine, thread)
    }

    fn here(thread: u64) -> ThreadKey {
        on(0, thread)
    }

    fn machine(machine: u32) -> MachineId {
        on(machine, 1).machine()
    }

    fn row(state: State, detail: Detail) -> Row {
        Row {
            target: RowTarget::Notice(NoticeId::from_u64(1)),
            thread: here(3),
            title: "Fold regression".into(),
            state,
            detail,
            when: "9m".into(),
            read: false,
            elsewhere: false,
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

    /// A toast that goes leaves for `TOAST_OUT`, while the same one standing
    /// on (its words updated) stays put; a new one replacing it sends the
    /// old one out under it; one the operator answers goes at once. Reduced
    /// motion keeps nothing leaving.
    #[test]
    fn the_toast_stage_lets_a_toast_go_softly_and_keeps_one_that_stays() {
        let t0 = Instant::now();
        let ms = |m: u64| t0 + Duration::from_millis(m);
        let out = ferrite_ui::motion::TOAST_OUT.duration();
        let done = row(State::Done, Detail::None);
        let mut stage = ToastStage::default();
        assert!(stage.stage(Some(&done), t0, false).is_none(), "it enters");
        let aged = Row {
            when: "10m".into(),
            ..done.clone()
        };
        assert!(
            stage.stage(Some(&aged), ms(500), false).is_none(),
            "the same toast stands on"
        );

        let (left, at) = stage.stage(None, ms(1_000), false).expect("it leaves");
        assert_eq!((toast_key(&left), at), (toast_key(&done), ms(1_000)));
        assert!(stage.stage(None, ms(1_050), false).is_some(), "fading");
        assert!(
            stage.stage(None, ms(1_000) + out, false).is_none(),
            "gone after TOAST_OUT"
        );

        let asking = Row {
            target: RowTarget::Decision(request(3, 1)),
            ..done.clone()
        };
        stage.stage(Some(&done), ms(2_000), false);
        let (left, _) = stage
            .stage(Some(&asking), ms(2_100), false)
            .expect("replaced, the old one leaves");
        assert_eq!(toast_key(&left), toast_key(&done));
        assert_ne!(toast_key(&left), toast_key(&asking), "the new one enters");

        stage.stage(Some(&done), ms(3_000), false);
        stage.dismiss();
        assert!(
            stage.stage(None, ms(3_010), false).is_none(),
            "answered: it goes at once"
        );

        let mut reduced = ToastStage::default();
        reduced.stage(Some(&done), t0, true);
        assert!(
            reduced.stage(None, ms(10), true).is_none(),
            "reduced: it just goes"
        );
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
        assert_eq!(mark(waiting.state), Mark("\u{25c6}", ATTENTION));
        assert_eq!(
            mark(State::NeedsYou(RequestKind::Question)),
            Mark("?", ATTENTION)
        );
        let failed = row(State::Failed, Detail::Words("357 passed; 2 failed".into()));
        assert_eq!(
            (failed.word(), word_ink(failed.word())),
            (words::FAILED, BLOCKED)
        );
        assert_eq!(mark(State::Failed), Mark("\u{2717}", BLOCKED));
        let done = row(State::Done, Detail::None);
        assert_eq!(
            (done.word(), word_ink(done.word())),
            (words::DONE, INK_SUBTLE)
        );
        assert_eq!(mark(State::Done), Mark("\u{2713}", INK_SUBTLE));
        const { assert!(ROW_TEXT_CELLS >= 40) };
    }

    /// ADR 0017: an event waits a second and is checked again before it
    /// rings; a survivor toasts once, unless in view, and goes on its own
    /// or as soon as it is read.
    #[test]
    fn an_event_rings_after_its_delay_if_it_still_stands() {
        let mut needs_you = NeedsYou::new();
        let start = Instant::now();
        let waiting = request(2, 1);
        let shown = request(1, 2);
        let in_view = |alert: &Alert| alert.thread == here(1);
        needs_you.hear(
            machine(0),
            false,
            [],
            [(waiting.clone(), here(2), false), (shown.clone(), here(1), false)],
            start,
        );
        let ring = needs_you.ring(start, &|_| Some(true), &in_view);
        assert!(
            ring.rang.is_empty() && needs_you.toasts().is_empty(),
            "not yet"
        );

        let due = start + ALERT_DELAY;
        let ring = needs_you.ring(due, &|_| Some(true), &in_view);
        let toast = Alert {
            target: RowTarget::Decision(waiting.clone()),
            thread: here(2),
        };
        assert_eq!(ring.rang, [toast], "the one in view stays quiet");
        assert!(ring.redraw);
        assert_eq!(needs_you.toasts().len(), 1);
        // Heard once: hearing the same requests again adds nothing.
        needs_you.hear(
            machine(0),
            false,
            [],
            [(waiting.clone(), here(2), false), (shown.clone(), here(1), false)],
            due,
        );
        let later = due + ALERT_DELAY;
        assert!(needs_you
            .ring(later, &|_| Some(true), &in_view)
            .rang
            .is_empty());
        assert_eq!(needs_you.toasts().len(), 1);
        // Read (opened, or its Thread landed on): it goes.
        let ring = needs_you.ring(later, &|_| Some(false), &in_view);
        assert!(ring.redraw && needs_you.toasts().is_empty());

        // A read Notice never waits; an unread one does, and one read
        // during its delay never rings.
        let read = NoticeId::from_u64(4);
        let unread = NoticeId::from_u64(5);
        needs_you.hear(
            machine(0),
            false,
            [(read, here(5), true), (unread, here(6), false)],
            [],
            later,
        );
        assert_eq!(needs_you.presented(machine(0)), Some(unread));
        let ring = needs_you.ring(later + ALERT_DELAY, &|_| Some(false), &|_| false);
        assert!(ring.rang.is_empty() && needs_you.toasts().is_empty());
    }

    /// A toast goes on its own: a request's after 8s, a finish's after 5s.
    #[test]
    fn toasts_expire_by_kind() {
        let mut needs_you = NeedsYou::new();
        let start = Instant::now();
        let asking = request(2, 1);
        needs_you.hear(
            machine(0),
            false,
            [(NoticeId::from_u64(1), here(1), false)],
            [(asking, here(2), false)],
            start,
        );
        let shown = start + ALERT_DELAY;
        assert_eq!(
            needs_you
                .ring(shown, &|_| Some(true), &|_| false)
                .rang
                .len(),
            2
        );
        let ring = needs_you.ring(shown + DONE_TOAST, &|_| Some(true), &|_| false);
        assert!(ring.redraw);
        assert_eq!(needs_you.toasts().len(), 1);
        assert!(
            needs_you.toasts()[0].alert.needs_you(),
            "the request stands longer"
        );
        needs_you.ring(shown + NEEDS_YOU_TOAST, &|_| Some(true), &|_| false);
        assert!(needs_you.toasts().is_empty() && !needs_you.busy());
    }

    /// A Thread has one voice: a new event for it displaces what it had
    /// waiting, and its toast replaces the one it had standing.
    #[test]
    fn a_new_event_for_a_thread_replaces_its_toast() {
        let mut needs_you = NeedsYou::new();
        let start = Instant::now();
        needs_you.hear(
            machine(0),
            false,
            [(NoticeId::from_u64(1), here(3), false)],
            [],
            start,
        );
        needs_you.ring(start + ALERT_DELAY, &|_| Some(true), &|_| false);
        // Two requests in the same second: only the newer rings.
        let at = start + DONE_TOAST / 2;
        let (first, second) = (request(3, 1), request(3, 2));
        needs_you.hear(machine(0), false, [], [(first.clone(), here(3), false)], at);
        needs_you.hear(
            machine(0),
            false,
            [],
            [(first, here(3), false), (second.clone(), here(3), false)],
            at,
        );
        let ring = needs_you.ring(at + ALERT_DELAY, &|_| Some(true), &|_| false);
        assert_eq!(ring.rang.len(), 1);
        assert_eq!(
            needs_you
                .toasts()
                .iter()
                .map(|t| &t.alert.target)
                .collect::<Vec<_>>(),
            [&RowTarget::Decision(second)]
        );
        // A button that acted takes its toast down.
        let target = needs_you.toasts()[0].alert.target.clone();
        needs_you.drop_toast(machine(0), &target);
        assert!(needs_you.toasts().is_empty());
    }

    /// ADR 0045: a machine other than this computer is heard quietly the
    /// first time — what it held then is not news — and rings for what it
    /// says after, its Threads apart from the same ids on this computer.
    #[test]
    fn another_machine_rings_only_for_what_is_new() {
        let mut needs_you = NeedsYou::new();
        let start = Instant::now();
        let on_box = machine(1);
        let held = request(3, 1);
        needs_you.hear(on_box, true, [], [(held.clone(), on(1, 3), false)], start);
        assert!(
            !needs_you.busy(),
            "what it held when first heard is not news"
        );

        let asked = request(3, 2);
        needs_you.hear(
            on_box,
            true,
            [],
            [(held, on(1, 3), false), (asked.clone(), on(1, 3), false)],
            start,
        );
        needs_you.hear(machine(0), false, [], [(request(3, 1), here(3), false)], start);
        let ring = needs_you.ring(start + ALERT_DELAY, &|_| Some(true), &|_| false);
        assert_eq!(
            ring.rang,
            [
                Alert {
                    target: RowTarget::Decision(asked),
                    thread: on(1, 3),
                },
                Alert {
                    target: RowTarget::Decision(request(3, 1)),
                    thread: here(3),
                },
            ],
            "Thread 3 here and Thread 3 on box are two Threads"
        );
        assert_eq!(needs_you.toasts().len(), 2);
        needs_you.drop_toast(machine(0), &RowTarget::Decision(request(3, 1)));
        assert_eq!(needs_you.toasts().len(), 1, "only this computer's went");
        assert_eq!(needs_you.toasts()[0].alert.thread, on(1, 3));
    }

    /// A machine not heard from yet is waited for no longer than the
    /// toast would have stood; one the window lets go is forgotten.
    #[test]
    fn a_machine_that_never_answers_is_given_up_on() {
        let mut needs_you = NeedsYou::new();
        let start = Instant::now();
        let on_box = machine(1);
        needs_you.hear(on_box, true, [], [], start);
        needs_you.hear(on_box, true, [], [(request(3, 2), on(1, 3), false)], start);
        let due = start + ALERT_DELAY;
        needs_you.ring(due, &|_| None, &|_| false);
        assert!(needs_you.busy(), "waiting for box");
        needs_you.ring(due + NEEDS_YOU_TOAST, &|_| None, &|_| false);
        let ring = needs_you.ring(due + NEEDS_YOU_TOAST, &|_| Some(true), &|_| false);
        assert!(ring.rang.is_empty(), "given up on");

        needs_you.hear(on_box, true, [], [(request(3, 3), on(1, 3), false)], due);
        needs_you.forget(on_box);
        assert!(!needs_you.busy(), "let go");
        assert_eq!(needs_you.presented(on_box), None);
    }
}
