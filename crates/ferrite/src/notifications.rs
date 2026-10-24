//! Needs you: every request that waits on the operator and every Thread
//! that finished while they looked elsewhere (CONTEXT.md's Needs-you view,
//! its alerts and toasts).
//!
//! Drawing and toasts only. What counts as waiting or finished is decided
//! headless in `ferrite_core::notifications` — the rows this module draws
//! are read from there, worded by the cockpit (`Row`), and every click or
//! key comes back as one `Verb`. Ferrite draws all of it in the float
//! grammar (theme WP-E): the bar's need-you door and its badge (FL-17), the
//! Needs-you view's rows (FL-16), and the toasts (FL-18).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;
use std::time::{Duration, Instant};

use ferrite_core::notifications::{DecisionNoticeId, NoticeId, Notifications, RequestKind};
use ferrite_core::ThreadId;
use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, App, Div, SharedString, Stateful, Window};

use crate::components;
use crate::machines::MachineKey;
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

/// What a click or a key on the Needs-you view's surfaces means. The cockpit answers
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
    /// Land on an alert's Thread on a machine off screen: the window goes
    /// there first (ADR 0026).
    GoTo(Alert),
}

pub type Handle = Rc<dyn Fn(Verb, &mut Window, &mut App)>;

/// A NeedsYou row has one actionable target, independent of provider wire data.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RowTarget {
    Notice(NoticeId),
    Decision(DecisionNoticeId),
}

impl RowTarget {
    /// Whether its record still wants the operator: there, unread, and
    /// for a request still live and not dismissed.
    pub fn wants(&self, notifications: &Notifications) -> bool {
        match self {
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
    /// The machine off screen it is on; `None` for the one on screen. Its
    /// title then begins with the machine's name.
    pub away: Option<MachineKey>,
    pub target: RowTarget,
    pub thread: ThreadId,
    pub title: SharedString,
    pub state: State,
    pub detail: Detail,
    /// Its age (`facts::since_label`: `2m`, `1h`; nothing in its first
    /// minute — no surface prints `now`, theme rule 11).
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
        Alert {
            away: self.away.clone(),
            target: self.target.clone(),
            thread: self.thread,
        }
        .open_verb()
    }

    /// What dismissing the row does: nothing for a machine off screen,
    /// whose Notices only its own windows change.
    pub fn dismiss_verb(&self) -> Option<Verb> {
        self.away.is_none().then(|| dismiss_verb(&self.target))
    }
}

/// A machine's name before a Thread's title: what a row, a toast or a
/// system notification about a machine off screen is called.
pub fn away_title(machine: &str, title: &str) -> SharedString {
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
    if let Some(questions) = crate::pane::questions_of(decision) {
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
    let tool: SharedString = crate::decision::tool_word(&decision.tool_name)
        .to_string()
        .into();
    if let Some(command) = decision
        .input
        .get("command")
        .and_then(serde_json::Value::as_str)
    {
        return Detail::Run {
            tool,
            command: command_head(&crate::decision::unwrap_shell(command)).into(),
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
            std::path::Path::new(&path)
                .strip_prefix(cwd)
                .ok()
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
/// its Thread. Ids are per Store, so a machine off screen keys its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    /// The machine off screen it is on; `None` for the one on screen.
    pub away: Option<MachineKey>,
    pub target: RowTarget,
    pub thread: ThreadId,
}

impl Alert {
    /// What opening it does: land on it, or on a machine off screen go
    /// there first.
    pub fn open_verb(&self) -> Verb {
        match &self.away {
            None => target_verb(&self.target),
            Some(_) => Verb::GoTo(self.clone()),
        }
    }

    /// Whether `other` is about the same Thread on the same machine.
    fn same_thread(&self, other: &Alert) -> bool {
        self.away == other.away && self.thread == other.thread
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

/// What the Needs-you view has heard of one machine: the Notice watermark (Notices
/// at or below it were heard) and the live requests.
#[derive(Default)]
struct Heard {
    presented: Option<NoticeId>,
    requests: BTreeSet<DecisionNoticeId>,
}

/// The window's side of the Needs-you view: whether its list is down and where its
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
/// **Machines off screen** (ADR 0026) are heard too, each keyed by its
/// `MachineKey`. What one held when the window first heard it is not news
/// (the window just left it, or met it already waiting); what it says
/// after rings like the rest. Nobody looks at a machine off screen, so its
/// alerts are never in view. When the window goes to another machine, the
/// Needs-you view moves with it (`moved`): an alert rings once, off screen or on, and
/// what waits or stands goes on doing so. A machine not heard from yet (the
/// one just left, or the one arrived at before it answers) cannot say
/// whether its alerts still stand: they wait for it, no longer than a
/// toast would stand.
pub struct NeedsYou {
    /// The list's cursor row in the Needs-you view (`nav::NavView`),
    /// first on open.
    pub cursor: usize,
    /// Per machine; `None` is the one on screen.
    heard: HashMap<Option<MachineKey>, Heard>,
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
            heard: HashMap::new(),
            pending: Vec::new(),
            toasts: Vec::new(),
            posted: BTreeMap::new(),
        }
    }

    /// The Needs-you view of a window that went from machine `left` to `arrived`:
    /// all it holds of each re-keyed (the one left is off screen now, the
    /// one arrived at on screen), so neither rings again for what it rang
    /// already, and what waits, stands or is posted goes on. A system
    /// notification keeps its tag: the notification centre holds it so.
    pub fn moved(self, left: &MachineKey, arrived: &MachineKey) -> Self {
        let rekey = |away: Option<MachineKey>| match away {
            None => Some(left.clone()),
            Some(key) if key == *arrived => None,
            other => other,
        };
        let realert = |alert: Alert| Alert {
            away: rekey(alert.away.clone()),
            ..alert
        };
        Self {
            heard: self
                .heard
                .into_iter()
                .map(|(away, heard)| (rekey(away), heard))
                .collect(),
            pending: self
                .pending
                .into_iter()
                .map(|(alert, due)| (realert(alert), due))
                .collect(),
            toasts: self
                .toasts
                .into_iter()
                .map(|toast| Toast {
                    alert: realert(toast.alert),
                    ..toast
                })
                .collect(),
            posted: self
                .posted
                .into_iter()
                .map(|(tag, alert)| (tag, realert(alert)))
                .collect(),
            ..Self::new()
        }
    }

    /// A machine's watermark: its Notices at or below it have been heard.
    pub fn presented(&self, away: Option<&MachineKey>) -> Option<NoticeId> {
        self.heard.get(&away.cloned())?.presented
    }

    /// The toasts standing now, oldest first.
    pub fn toasts(&self) -> &[Toast] {
        &self.toasts
    }

    /// Whether anything is waiting or standing: what `ring` has to keep up.
    pub fn busy(&self) -> bool {
        !self.pending.is_empty() || !self.toasts.is_empty()
    }

    /// Take every toast down and forget what was waiting, keeping what was
    /// heard as heard: a scene's requests that stood before the window
    /// opened.
    #[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
    pub fn clear_toasts(&mut self) {
        self.pending.clear();
        self.toasts.clear();
    }

    /// Take one toast down (its button acted on it): one about the
    /// machine on screen.
    pub fn drop_toast(&mut self, target: &RowTarget) {
        self.toasts
            .retain(|toast| toast.alert.away.is_some() || &toast.alert.target != target);
    }

    /// Hear what a machine (`away`; `None` for the one on screen) said
    /// since the last call: every Notice born since its watermark
    /// (`notices`: id, Thread, read) moves it, and its live requests
    /// (`requests`: id, read) replace the set heard. Each unread newcomer
    /// waits out `ALERT_DELAY`, displacing whatever its Thread had
    /// waiting. A machine off screen heard for the first time only sets
    /// what was heard.
    pub fn hear(
        &mut self,
        away: Option<MachineKey>,
        notices: impl IntoIterator<Item = (NoticeId, ThreadId, bool)>,
        requests: impl IntoIterator<Item = (DecisionNoticeId, bool)>,
        now: Instant,
    ) {
        let quiet = away.is_some() && !self.heard.contains_key(&away);
        let state = self.heard.entry(away.clone()).or_default();
        let mut heard = Vec::new();
        for (id, thread, read) in notices {
            state.presented = Some(state.presented.map_or(id, |seen| seen.max(id)));
            if !read {
                heard.push(Alert {
                    away: away.clone(),
                    target: RowTarget::Notice(id),
                    thread,
                });
            }
        }
        let requests: Vec<(DecisionNoticeId, bool)> = requests.into_iter().collect();
        let seen = std::mem::replace(
            &mut state.requests,
            requests.iter().map(|(id, _)| id.clone()).collect(),
        );
        for (id, read) in requests {
            if !seen.contains(&id) && !read {
                heard.push(Alert {
                    away: away.clone(),
                    thread: id.thread,
                    target: RowTarget::Decision(id),
                });
            }
        }
        if quiet {
            return;
        }
        for alert in heard {
            self.pending
                .retain(|(waiting, _)| !waiting.same_thread(&alert));
            self.pending.push((alert, now + ALERT_DELAY));
        }
    }

    /// Bring the Needs-you view up to `now`. A standing toast goes once it expired,
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
            self.toasts.retain(|toast| !toast.alert.same_thread(&alert));
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
/// failure; `✓` `TEXT_DIM` for a turn that finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mark(&'static str, u32);

fn mark(state: State) -> Mark {
    match state {
        State::NeedsYou(RequestKind::Permission) => Mark("\u{25c6}", ATTENTION),
        State::NeedsYou(RequestKind::Question) => Mark("?", ATTENTION),
        State::Failed => Mark("\u{2717}", BLOCKED),
        State::Done => Mark("\u{2713}", TEXT_DIM),
    }
}

/// Folds each Thread's completions into its newest (`rows` are newest
/// first; a Thread is per machine): the survivor carries the older ids in
/// `folded`, unread if any of them is. Requests are never folded — each is
/// its own question.
pub fn fold(rows: Vec<Row>) -> Vec<Row> {
    let mut kept: Vec<Row> = Vec::with_capacity(rows.len());
    let mut by_thread: HashMap<(Option<MachineKey>, ThreadId), usize> = HashMap::new();
    for row in rows {
        let RowTarget::Notice(id) = &row.target else {
            kept.push(row);
            continue;
        };
        let id = *id;
        match by_thread.get(&(row.away.clone(), row.thread)) {
            Some(&at) => {
                kept[at].folded.push(id);
                kept[at].read &= row.read;
            }
            None => {
                by_thread.insert((row.away.clone(), row.thread), kept.len());
                kept.push(row);
            }
        }
    }
    kept
}

// ---------------------------------------------------------------- toasts

/// A quick answer's pill: the key in `TEXT_DIM` and the word in `ink`, on
/// a `TOKEN_H` pill of the float's raised fill (`FLOAT_BAND2`), radius
/// `R_SMALL`.
fn quick_pill(key: &'static str, word: &'static str, ink: gpui::Hsla) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .h(px(TOKEN_H))
        .px(px(TOKEN_PAD_X))
        .rounded(px(R_SMALL))
        .bg(FLOAT_BAND2)
        .whitespace_nowrap()
        .child(div().text_color(rgb(TEXT_DIM)).child(key))
        .child(div().text_color(ink).child(word))
}

/// A quick answer in a toast: its pill, under the pointer a step up and
/// the word `TEXT_STRONG`, over the one blend (`hover_quick`).
fn quick_button(
    id: impl Into<gpui::ElementId>,
    key: &'static str,
    word: &'static str,
) -> Stateful<Div> {
    let id = id.into();
    let blend = crate::pointer::hover_key(&id);
    let ink = crate::motion::hover_blend(&blend, rgb(TEXT).into(), rgb(TEXT_STRONG).into());
    quick_pill(key, word, ink)
        .id(id)
        .hover_quick(blend)
        .press_float()
}

/// A toast (the prototype's `.toast`, FL-18), in the float grammar with no
/// close: the head (`◆ needs you · <title>`, `⌘D` at its right while a
/// request waits; `✓ done · <title>` for a finished turn), one body line,
/// and the quick answers — `1 allow` `3 deny` `⏎ open` for an approval,
/// `⏎ open` alone for a question, a finished turn, or anything on a
/// machine off screen (its status link answers nothing).
pub fn toast(row: &Row, handle: Handle) -> Div {
    toast_card(row, Some(handle))
}

/// A toast on its way out (`ToastStage`): the same card, answering no
/// pointer — its answers drawn at rest and pressing nothing, nothing under
/// it occluded, none of the live toast's selectors (`toast-leaving`).
pub fn toast_face(row: &Row) -> Div {
    toast_card(row, None)
}

fn toast_card(row: &Row, handle: Option<Handle>) -> Div {
    let thread = row.thread.get() as usize;
    let request = row.request();
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
                    .text_color(rgb(TEXT))
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
            .text_color(rgb(TEXT))
            .child(SharedString::from(detail.text()))
            .into_any_element(),
    };
    let mut answers: Vec<(&'static str, &'static str, &'static str, Verb)> = Vec::new();
    if let (None, RowTarget::Decision(id), State::NeedsYou(RequestKind::Permission)) =
        (&row.away, &row.target, row.state)
    {
        answers.push((
            "toast-allow",
            "1 ",
            "allow",
            Verb::Answer(id.clone(), QuickAnswer::Allow),
        ));
        answers.push((
            "toast-deny",
            "3 ",
            "deny",
            Verb::Answer(id.clone(), QuickAnswer::Deny),
        ));
    }
    answers.push(("toast-open", "\u{23ce} ", "open", row.open_verb()));
    let buttons = answers.into_iter().map(|(name, key, word, verb)| {
        let Some(handle) = handle.clone() else {
            return quick_pill(key, word, rgb(TEXT).into()).into_any_element();
        };
        quick_button((name, thread), key, word)
            .debug_selector(move || format!("{name}-{thread}"))
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                handle(verb.clone(), window, cx)
            })
            .into_any_element()
    });
    let card = match handle {
        Some(_) => crate::menu::float()
            .debug_selector(move || format!("toast-{thread}"))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation()),
        None => components::float_ground()
            .p(px(0.))
            .debug_selector(|| "toast-leaving".into()),
    };
    card.w(px(TOAST_W))
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
                                .text_color(rgb(TEXT_DIM))
                                .child(SharedString::from(format!(" \u{b7} {}", row.title))),
                        ),
                )
                .when(request, |head| {
                    head.children(
                        components::bound_chord("cockpit::NextDecision").map(|keys| {
                            components::key_combo(&keys, TEXT_DIM)
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
                .gap(px(SPACE_1_5))
                .px(px(FLOAT_PAD_X))
                .pb(px(TOAST_QUICK_GAP))
                .children(buttons),
        )
}

/// What the toast float draws as toasts come and go (FL-18). The toast
/// standing now rises in (`motion::toast_in`, keyed by `toast_key`, so a
/// new one enters and the same one stays put); one that goes — read,
/// resolved, timed out or replaced — leaves as its face (`toast_face`),
/// fading over `motion::TOAST_OUT` (`motion::toast_out`), under whatever
/// stands next. Under reduced motion nothing lingers.
#[derive(Default)]
pub struct ToastStage {
    standing: Option<Row>,
    leaving: Option<(Row, Instant)>,
}

impl ToastStage {
    /// This frame's standing toast, at `now`: the toast leaving, and when it
    /// went, while it still fades.
    pub fn stage(
        &mut self,
        standing: Option<&Row>,
        now: Instant,
        reduced: bool,
    ) -> Option<(Row, Instant)> {
        let same = |a: &Row, b: &Row| toast_key(a) == toast_key(b);
        let went = match (self.standing.take(), standing) {
            (Some(shown), Some(next)) if same(&shown, next) => None,
            (shown, _) => shown,
        };
        if let Some(went) = went.filter(|_| !reduced) {
            self.leaving = Some((went, now));
        }
        self.standing = standing.cloned();
        let fading = |left: Instant| {
            !reduced && now.saturating_duration_since(left) < crate::motion::TOAST_OUT.duration()
        };
        if !self.leaving.as_ref().is_some_and(|(_, left)| fading(*left)) {
            self.leaving = None;
        }
        self.leaving.clone()
    }
}

/// A toast's identity on screen: its record, on its machine.
pub fn toast_key(row: &Row) -> SharedString {
    format!("toast-in-{:?}-{:?}", row.away, row.target).into()
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
            away: None,
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

    /// A toast that goes leaves for `TOAST_OUT`, while the same one standing
    /// on (its words updated) stays put; a new one replacing it sends the
    /// old one out under it. Reduced motion keeps nothing leaving.
    #[test]
    fn the_toast_stage_lets_a_toast_go_softly_and_keeps_one_that_stays() {
        let t0 = Instant::now();
        let ms = |m: u64| t0 + Duration::from_millis(m);
        let out = crate::motion::TOAST_OUT.duration();
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
            (words::DONE, TEXT_DIM)
        );
        assert_eq!(mark(State::Done), Mark("\u{2713}", TEXT_DIM));
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

    /// ADR 0017: an event waits a second and is checked again before it
    /// rings; a survivor toasts once, unless in view, and goes on its own
    /// or as soon as it is read.
    #[test]
    fn an_event_rings_after_its_delay_if_it_still_stands() {
        let mut needs_you = NeedsYou::new();
        let start = Instant::now();
        let waiting = request(2, 1);
        let here = request(1, 2);
        let in_view = |alert: &Alert| alert.thread == ThreadId::new(1);
        needs_you.hear(
            None,
            [],
            [(waiting.clone(), false), (here.clone(), false)],
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
            away: None,
            target: RowTarget::Decision(waiting.clone()),
            thread: ThreadId::new(2),
        };
        assert_eq!(ring.rang, [toast], "the one in view stays quiet");
        assert!(ring.redraw);
        assert_eq!(needs_you.toasts().len(), 1);
        // Heard once: hearing the same requests again adds nothing.
        needs_you.hear(
            None,
            [],
            [(waiting.clone(), false), (here.clone(), false)],
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
            None,
            [
                (read, ThreadId::new(5), true),
                (unread, ThreadId::new(6), false),
            ],
            [],
            later,
        );
        assert_eq!(needs_you.presented(None), Some(unread));
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
            None,
            [(NoticeId::from_u64(1), ThreadId::new(1), false)],
            [(asking, false)],
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
        let thread = ThreadId::new(3);
        let start = Instant::now();
        needs_you.hear(None, [(NoticeId::from_u64(1), thread, false)], [], start);
        needs_you.ring(start + ALERT_DELAY, &|_| Some(true), &|_| false);
        // Two requests in the same second: only the newer rings.
        let at = start + DONE_TOAST / 2;
        let (first, second) = (request(3, 1), request(3, 2));
        needs_you.hear(None, [], [(first.clone(), false)], at);
        needs_you.hear(None, [], [(first, false), (second.clone(), false)], at);
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
        needs_you.drop_toast(&target);
        assert!(needs_you.toasts().is_empty());
    }

    /// ADR 0026: a machine off screen is heard quietly the first time —
    /// what it held then is not news — and rings for what it says after,
    /// its Threads apart from the same ids on screen.
    #[test]
    fn a_machine_off_screen_rings_only_for_what_is_new() {
        let mut needs_you = NeedsYou::new();
        let start = Instant::now();
        let away = Some(MachineKey(Some("me@box".into())));
        let held = request(3, 1);
        needs_you.hear(away.clone(), [], [(held.clone(), false)], start);
        assert!(
            !needs_you.busy(),
            "what it held when first heard is not news"
        );

        let asked = request(3, 2);
        needs_you.hear(
            away.clone(),
            [],
            [(held, false), (asked.clone(), false)],
            start,
        );
        needs_you.hear(None, [], [(request(3, 1), false)], start);
        let ring = needs_you.ring(start + ALERT_DELAY, &|_| Some(true), &|_| false);
        assert_eq!(
            ring.rang,
            [
                Alert {
                    away: away.clone(),
                    target: RowTarget::Decision(asked),
                    thread: ThreadId::new(3),
                },
                Alert {
                    away: None,
                    target: RowTarget::Decision(request(3, 1)),
                    thread: ThreadId::new(3),
                },
            ],
            "Thread 3 here and Thread 3 on box are two Threads"
        );
        assert_eq!(needs_you.toasts().len(), 2);
        needs_you.drop_toast(&RowTarget::Decision(request(3, 1)));
        assert_eq!(needs_you.toasts().len(), 1, "only the one on screen went");
        assert_eq!(needs_you.toasts()[0].alert.away, away);
    }

    /// What was heard moves with the window: arriving at box, what rang
    /// off screen does not ring again; leaving this computer, what it held
    /// is heard already. What either says after still rings.
    #[test]
    fn what_rang_off_screen_does_not_ring_again_on_arrival() {
        let mut needs_you = NeedsYou::new();
        let start = Instant::now();
        let home = MachineKey(None);
        let on_box = MachineKey(Some("me@box".into()));
        let finished = NoticeId::from_u64(7);
        needs_you.hear(None, [(finished, ThreadId::new(1), false)], [], start);
        needs_you.hear(Some(on_box.clone()), [], [], start);
        let (one, two) = (request(1, 1), request(2, 1));
        needs_you.hear(
            Some(on_box.clone()),
            [],
            [(one, false), (two.clone(), false)],
            start,
        );
        let ring = needs_you.ring(start + ALERT_DELAY, &|_| Some(true), &|_| false);
        assert_eq!(ring.rang.len(), 3, "box's two and the finish here");

        let mut needs_you = needs_you.moved(&home, &on_box);
        let later = start + 2 * ALERT_DELAY;
        assert_eq!(needs_you.presented(Some(&home)), Some(finished));
        assert_eq!(needs_you.presented(None), None, "box had no Notice");
        needs_you.hear(None, [], [(two, false)], later);
        needs_you.hear(Some(home.clone()), [], [], later);
        let ring = needs_you.ring(later + ALERT_DELAY, &|_| Some(true), &|_| false);
        assert!(ring.rang.is_empty(), "nothing heard rings twice");

        let (later, newer) = (later + ALERT_DELAY, NoticeId::from_u64(8));
        needs_you.hear(
            Some(home.clone()),
            [(newer, ThreadId::new(1), false)],
            [],
            later,
        );
        let ring = needs_you.ring(later + ALERT_DELAY, &|_| Some(true), &|_| false);
        assert_eq!(
            ring.rang,
            [Alert {
                away: Some(home),
                target: RowTarget::Notice(newer),
                thread: ThreadId::new(1),
            }]
        );
    }

    /// The window goes from this computer to box within a second of box
    /// asking: what waits goes on waiting, now on screen, and rings once
    /// box answers; the toast standing for box3 stands on, and what was
    /// posted opens on the machine it was about, under the same tag. Each
    /// is about the same Thread, re-keyed.
    #[test]
    fn what_waits_or_stands_moves_with_the_window() {
        let mut needs_you = NeedsYou::new();
        let start = Instant::now();
        let home = MachineKey(None);
        let (on_box, box3) = (
            MachineKey(Some("me@box".into())),
            MachineKey(Some("me@box3".into())),
        );
        needs_you.hear(Some(box3.clone()), [], [], start);
        needs_you.hear(Some(box3.clone()), [], [(request(4, 1), false)], start);
        needs_you.ring(start + ALERT_DELAY, &|_| Some(true), &|_| false);
        let finished = NoticeId::from_u64(2);
        needs_you.posted.insert(
            "here".into(),
            Alert {
                away: None,
                target: RowTarget::Notice(finished),
                thread: ThreadId::new(1),
            },
        );
        let asking = request(3, 1);
        let at = start + ALERT_DELAY;
        needs_you.hear(Some(on_box.clone()), [], [], at);
        needs_you.hear(Some(on_box.clone()), [], [(asking.clone(), false)], at);

        let mut needs_you = needs_you.moved(&home, &on_box);
        assert_eq!(needs_you.toasts().len(), 1, "box3's toast stands on");
        assert_eq!(needs_you.toasts()[0].alert.away, Some(box3));
        assert_eq!(
            needs_you.posted["here"].away,
            Some(home),
            "same tag, re-keyed"
        );
        let due = at + ALERT_DELAY;
        let ring = needs_you.ring(due, &|alert| alert.away.is_some().then_some(true), &|_| {
            false
        });
        assert!(ring.rang.is_empty(), "box has not answered yet");
        let ring = needs_you.ring(due + ALERT_DELAY, &|_| Some(true), &|_| false);
        assert_eq!(
            ring.rang,
            [Alert {
                away: None,
                target: RowTarget::Decision(asking),
                thread: ThreadId::new(3),
            }]
        );

        // A machine that never answers is waited for no longer than the
        // toast would have stood.
        let late = request(3, 2);
        needs_you.hear(None, [], [(late, false)], due);
        needs_you.ring(due + ALERT_DELAY, &|_| None, &|_| false);
        needs_you.ring(due + ALERT_DELAY + NEEDS_YOU_TOAST, &|_| None, &|_| false);
        let ring = needs_you.ring(
            due + ALERT_DELAY + NEEDS_YOU_TOAST,
            &|_| Some(true),
            &|_| false,
        );
        assert!(ring.rang.is_empty(), "given up on");
    }
}
