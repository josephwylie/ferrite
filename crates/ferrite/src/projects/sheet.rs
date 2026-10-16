//! The Projects sheet: herdr-projects' popup panes drawn in the window's
//! float grammar — the six-section popup (`popup::Popup`, its screen line
//! for line), the text overview, and the small asks (`new`, `adopt`, `pick`)
//! upstream ran on a terminal. Its keys arrive in the shape the popup
//! answers (`keys`), and what only the window can do — land on a Thread,
//! open a file, copy, notify — goes out as a [`SheetEvent`].
//!
//! The popup itself lives on a thread of its own (`Live`): every key may run
//! a CLI command and every refresh reads the projects' files, and neither
//! may hold the window. Each key and each two-second refresh sends back a
//! frame: the screen, and whatever the popup asked of the window.

use std::path::PathBuf;
use std::sync::mpsc;

use ferrite_core::ThreadId;
use ferrite_projects::keys::{KeyCode, KeyEvent, KeyModifiers};
use ferrite_projects::popup::{self, Color, Line, Request, Style};
use futures::StreamExt as _;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, App, Context, EventEmitter, FocusHandle, Focusable, KeyDownEvent, Keystroke,
    SharedString, Window,
};

use super::data::{self, Jump, Place, Verb};
use crate::components;
use crate::theme::*;

/// What the sheet asks of the cockpit.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SheetEvent {
    Close,
    /// The popup closed on a Thread to land on (`↵` on a row), maybe on
    /// another machine.
    Jump(Jump),
    /// The popup closed on a jump that lands nowhere: why.
    Refused(String),
    /// A file to open: a text one in the reader, else the system opener.
    Open(PathBuf),
    Copy(String),
    Notify {
        title: String,
        body: String,
    },
}

pub(crate) struct ProjectsSheet {
    pub focus: FocusHandle,
    place: Place,
    body: Body,
    /// The sheet's size in cells (columns, lines), as the cockpit hangs it:
    /// the most it may grow to, and the screen the popup draws.
    size: (usize, usize),
    /// The most lines it has drawn since it opened: it never shrinks back,
    /// so switching to a shorter section holds the sheet still.
    tallest: usize,
}

enum Body {
    Popup(Live),
    /// The text overview: its lines, scrolled.
    Text {
        title: String,
        lines: Vec<String>,
        scroll: usize,
    },
    Ask(Ask),
    Pick(Pick),
}

impl EventEmitter<SheetEvent> for ProjectsSheet {}

impl Focusable for ProjectsSheet {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl ProjectsSheet {
    /// The popup, scoped to `scope` (all projects when `None`).
    pub(crate) fn popup(place: Place, scope: Option<String>, cx: &mut Context<Self>) -> Self {
        let size = DEFAULT_SIZE;
        let (live, woken) = Live::spawn(place.clone(), scope, size);
        cx.spawn(async move |this, cx| {
            let mut woken = woken;
            while woken.next().await.is_some() {
                if this.update(cx, |sheet, cx| sheet.take_frames(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        Self::with(place, Body::Popup(live), cx)
    }

    /// The text overview of `slugs`, read off the UI thread.
    pub(crate) fn overview(place: Place, slugs: Vec<String>, cx: &mut Context<Self>) -> Self {
        let reading = place.clone();
        cx.spawn(async move |this, cx| {
            let text = cx
                .background_spawn(async move { overview_text(&reading, &slugs) })
                .await;
            this.update(cx, |sheet, cx| {
                if let Body::Text { lines, .. } = &mut sheet.body {
                    *lines = text.lines().map(str::to_string).collect();
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        let lines = vec!["reading\u{2026}".to_string()];
        Self::with(
            place,
            Body::Text {
                title: "Overview".into(),
                lines,
                scroll: 0,
            },
            cx,
        )
    }

    pub(crate) fn ask(place: Place, ask: Ask, cx: &mut Context<Self>) -> Self {
        Self::with(place, Body::Ask(ask), cx)
    }

    pub(crate) fn pick(
        place: Place,
        verb: Verb,
        slugs: Vec<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with(place, Body::Pick(Pick::new(verb, slugs)), cx)
    }

    fn with(place: Place, body: Body, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            place,
            body,
            size: DEFAULT_SIZE,
            tallest: 0,
        }
    }

    /// Fit the sheet to a box `width` by `height` pixels: the popup draws
    /// its screen at that many cells.
    pub(crate) fn fit(&mut self, width: f32, height: f32) {
        let size = cells(width, height);
        if size != self.size {
            self.size = size;
            if let Body::Popup(live) = &mut self.body {
                live.send(Op::Size(size));
            }
        }
    }

    /// The popup's newest frames: the last one's screen, and every request
    /// on the way, in order.
    fn take_frames(&mut self, cx: &mut Context<Self>) {
        let Body::Popup(live) = &mut self.body else {
            return;
        };
        let frames: Vec<Frame> = live.frames.try_iter().collect();
        self.apply(frames, cx);
    }

    fn apply(&mut self, frames: Vec<Frame>, cx: &mut Context<Self>) {
        let Body::Popup(live) = &mut self.body else {
            return;
        };
        let mut events = Vec::new();
        for frame in frames {
            live.seen = frame.seq;
            live.lines = frame.lines;
            for request in frame.requests {
                events.push(match request {
                    Request::OpenFile(path) => SheetEvent::Open(path),
                    Request::Copy(text) => SheetEvent::Copy(text),
                });
            }
            if frame.quit {
                match frame.jump {
                    Some(Ok(jump)) => events.push(SheetEvent::Jump(jump)),
                    Some(Err(why)) => events.push(SheetEvent::Refused(why)),
                    None => {}
                }
                events.push(SheetEvent::Close);
            }
        }
        for event in events {
            cx.emit(event);
        }
        cx.notify();
    }

    /// Wait for the popup to answer every key sent so far — what the tests
    /// and reference scenes read, where no frame may arrive between two keys.
    #[cfg(any(test, feature = "visual-reference"))]
    pub(crate) fn settle(&mut self, cx: &mut Context<Self>) {
        let Body::Popup(live) = &mut self.body else {
            return;
        };
        let mut frames = Vec::new();
        while live.seen < live.sent {
            match live.frames.recv_timeout(std::time::Duration::from_secs(10)) {
                Ok(frame) => {
                    live.seen = frame.seq;
                    frames.push(frame);
                }
                Err(_) => break,
            }
        }
        self.apply(frames, cx);
    }

    /// Stop the popup's thread once every key is answered: the reference
    /// harness's scheduler must not be woken from another thread mid-capture,
    /// so the sheet holds the screen it has.
    #[cfg(feature = "visual-reference")]
    pub(crate) fn stop(&mut self) {
        if let Body::Popup(live) = &mut self.body {
            live.ops = mpsc::channel().0;
            // The thread answers the dropped sender by returning, which
            // closes its frames: wait for that.
            while live.frames.recv().is_ok() {}
        }
    }

    /// The lines drawn now — what the tests read.
    #[cfg(test)]
    pub(crate) fn screen(&self) -> Vec<String> {
        self.lines()
            .into_iter()
            .map(|line| match line.right {
                Some((right, _)) => format!("{}  {right}", line.text.trim_end()),
                None => line.text,
            })
            .collect()
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let pasted = is_paste(keystroke)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let keys: Vec<KeyEvent> = match pasted {
            Some(text) => text
                .chars()
                .filter(|c| !c.is_control())
                .map(|c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
                .collect(),
            None => match key_of(keystroke) {
                Some(key) => vec![key],
                None => return,
            },
        };
        cx.stop_propagation();
        window.prevent_default();
        for key in keys {
            self.key(key, cx);
        }
    }

    /// One key, to whichever body holds the sheet.
    pub(crate) fn key(&mut self, key: KeyEvent, cx: &mut Context<Self>) {
        let place = self.place.clone();
        match &mut self.body {
            Body::Popup(live) => live.send(Op::Key(key)),
            Body::Text { lines, scroll, .. } => match key.code {
                KeyCode::Esc | KeyCode::Char('q') => cx.emit(SheetEvent::Close),
                KeyCode::Down | KeyCode::Char('j') => {
                    *scroll = (*scroll + 1).min(lines.len().saturating_sub(1))
                }
                KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
                KeyCode::PageDown => *scroll = (*scroll + 20).min(lines.len().saturating_sub(1)),
                KeyCode::PageUp => *scroll = scroll.saturating_sub(20),
                _ => {}
            },
            Body::Ask(ask) => match ask.key(key) {
                Answer::Stay => {}
                Answer::Cancel => cx.emit(SheetEvent::Close),
                Answer::Done(values) => {
                    let job = ask.job.clone();
                    ask.status = Some("working\u{2026}".into());
                    self.run(move || job.run(&place, &values), cx);
                }
            },
            Body::Pick(pick) => match pick.key(key) {
                Answer::Stay => {}
                Answer::Cancel => cx.emit(SheetEvent::Close),
                Answer::Done(values) => {
                    let verb = pick.verb;
                    let slug = values.into_iter().next().unwrap_or_default();
                    pick.status = Some("working\u{2026}".into());
                    self.run(move || run_verb(&place, verb, &slug), cx);
                }
            },
        }
        cx.notify();
    }

    /// Run an ask's work off the UI thread: done, the sheet closes with a
    /// notification; refused, it stays open with the error, as upstream
    /// held its pane open on one.
    fn run(&mut self, work: impl FnOnce() -> Outcome + Send + 'static, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let outcome = cx.background_spawn(async move { work() }).await;
            this.update(cx, |sheet, cx| match outcome {
                Outcome::Done { title, body } => {
                    cx.emit(SheetEvent::Notify { title, body });
                    cx.emit(SheetEvent::Close);
                }
                Outcome::Refused(error) => {
                    let status = Some(format!("error: {error}"));
                    match &mut sheet.body {
                        Body::Ask(ask) => ask.status = status,
                        Body::Pick(pick) => pick.status = status,
                        Body::Popup(_) | Body::Text { .. } => {}
                    }
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Every line of the sheet, top to bottom: its screen fitted to what it
    /// holds (`fit_content`), never shorter than it has been.
    fn lines(&self) -> Vec<Line> {
        fit_content(self.screen_lines(), self.tallest.max(MIN_LINES))
    }

    /// The full screen, `size` lines tall, its footer at the foot.
    fn screen_lines(&self) -> Vec<Line> {
        let (_, height) = self.size;
        match &self.body {
            Body::Popup(live) => live.lines.clone(),
            Body::Text {
                title,
                lines,
                scroll,
            } => {
                let mut out = vec![bold(&format!(" {title}")), rule()];
                out.extend(
                    lines
                        .iter()
                        .skip(*scroll)
                        .take(height.saturating_sub(4))
                        .map(|line| overview_line(line)),
                );
                out.push(rule());
                out.push(dim(" \u{2191}\u{2193} scroll  esc close"));
                pad_to(out, height)
            }
            Body::Ask(ask) => pad_to(ask.lines(), height),
            Body::Pick(pick) => pad_to(pick.lines(), height),
        }
    }
}

/// The popup's size before the cockpit first fits it.
const DEFAULT_SIZE: (usize, usize) = (100, 30);

/// The fewest lines the sheet draws, header and footer included: most
/// sections fit, so moving between them rarely changes its height.
const MIN_LINES: usize = 16;

/// A screen cut to its content: the header, the body down to its last
/// written line and a line of air, then the footer's two lines — at least
/// `floor` lines, at most the screen. The popup still draws (and scrolls)
/// at the full size, so a long section fills the sheet and says `↓ more`.
fn fit_content(mut lines: Vec<Line>, floor: usize) -> Vec<Line> {
    let footer = lines.split_off(lines.len().saturating_sub(2));
    let written = lines
        .iter()
        .rposition(|line| {
            !line.text.trim().is_empty() || line.rule || line.right.is_some() || line.style.reverse
        })
        .map_or(0, |last| last + 1);
    let body = (written + 1)
        .max(floor.saturating_sub(footer.len()))
        .min(lines.len());
    lines.truncate(body);
    lines.extend(footer);
    lines
}

/// How many cells a box `width` by `height` pixels holds, inside the
/// float's side padding.
fn cells(width: f32, height: f32) -> (usize, usize) {
    let columns = ((width - 2.0 * FLOAT_PAD_X) / CH).floor().max(20.0) as usize;
    let lines = (height / ROW).floor().max(8.0) as usize;
    (columns, lines)
}

/// ⌘V (ctrl-V on Windows): the clipboard's text, typed.
fn is_paste(keystroke: &Keystroke) -> bool {
    let primary = match crate::keymap::PLATFORM {
        crate::keymap::Platform::Mac => keystroke.modifiers.platform,
        crate::keymap::Platform::Windows => keystroke.modifiers.control,
    };
    primary && keystroke.key == "v"
}

/// A keystroke as the popup reads it, or `None` for one the window keeps
/// (anything with ⌘ or alt: the cockpit's own keys).
pub(crate) fn key_of(keystroke: &Keystroke) -> Option<KeyEvent> {
    let modifiers = keystroke.modifiers;
    if modifiers.platform || modifiers.alt {
        return None;
    }
    let plain = |code| Some(KeyEvent::new(code, KeyModifiers::NONE));
    match keystroke.key.as_str() {
        "up" => plain(KeyCode::Up),
        "down" => plain(KeyCode::Down),
        "left" => plain(KeyCode::Left),
        "right" => plain(KeyCode::Right),
        "enter" => plain(KeyCode::Enter),
        "escape" => plain(KeyCode::Esc),
        "backspace" => plain(KeyCode::Backspace),
        "pageup" => plain(KeyCode::PageUp),
        "pagedown" => plain(KeyCode::PageDown),
        "tab" if modifiers.shift => plain(KeyCode::BackTab),
        "tab" => plain(KeyCode::Tab),
        "space" => plain(KeyCode::Char(' ')),
        key if modifiers.control => {
            let mut chars = key.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Some(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)),
                _ => None,
            }
        }
        _ => {
            let text = keystroke.key_char.as_deref()?;
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if !c.is_control() => plain(KeyCode::Char(c)),
                _ => None,
            }
        }
    }
}

// ------------------------------------------------------------ the popup

/// What the window sends the popup's thread.
enum Op {
    Key(KeyEvent),
    Size((usize, usize)),
}

/// What the popup's thread sends back after each op and each refresh.
struct Frame {
    /// The ops answered so far.
    seq: u64,
    lines: Vec<Line>,
    quit: bool,
    jump: Option<Result<Jump, String>>,
    requests: Vec<Request>,
}

/// The popup on its own thread, and the window's end of it.
struct Live {
    ops: mpsc::Sender<Op>,
    frames: mpsc::Receiver<Frame>,
    sent: u64,
    seen: u64,
    lines: Vec<Line>,
}

impl Live {
    /// Start the popup's thread; the stream wakes the window for each frame.
    fn spawn(
        place: Place,
        scope: Option<String>,
        size: (usize, usize),
    ) -> (Live, futures::channel::mpsc::UnboundedReceiver<()>) {
        let (ops, inbox) = mpsc::channel::<Op>();
        let (outbox, frames) = mpsc::channel::<Frame>();
        let (wake, woken) = futures::channel::mpsc::unbounded();
        let spawned = std::thread::Builder::new()
            .name("projects-popup".into())
            .spawn(move || {
                let _ = place.with_ctx(|ctx| {
                    let mut popup = popup::Popup::new(ctx, scope, String::new());
                    let mut size = size;
                    let mut seq = 0;
                    loop {
                        let frame = Frame {
                            seq,
                            lines: popup.screen(size.0, size.1),
                            quit: popup.quit,
                            jump: popup
                                .jump
                                .take()
                                .map(|(_, machine, pane)| Jump::of(&machine, &pane)),
                            requests: std::mem::take(&mut popup.requests),
                        };
                        if outbox.send(frame).is_err() || wake.unbounded_send(()).is_err() {
                            return;
                        }
                        if popup.quit {
                            return;
                        }
                        match inbox.recv_timeout(popup::REFRESH) {
                            Ok(op) => {
                                seq += 1;
                                match op {
                                    Op::Key(key) => popup.key(key),
                                    Op::Size(new) => size = new,
                                }
                            }
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                if matches!(popup.mode, popup::Mode::List) {
                                    popup.reload();
                                }
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        }
                    }
                });
            });
        let lines = match spawned {
            Ok(_) => Vec::new(),
            Err(error) => vec![plain(&format!(" could not start the popup: {error}"))],
        };
        let live = Live {
            ops,
            frames,
            sent: 0,
            seen: 0,
            lines,
        };
        (live, woken)
    }

    fn send(&mut self, op: Op) {
        if self.ops.send(op).is_ok() {
            self.sent += 1;
        }
    }
}

// ------------------------------------------------------------- the asks

/// What an ask or a pick did with its answer.
pub(crate) enum Outcome {
    Done { title: String, body: String },
    Refused(String),
}

#[derive(Debug, PartialEq)]
pub(crate) enum Answer {
    Stay,
    Cancel,
    Done(Vec<String>),
}

/// The work an ask's answers go to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Job {
    /// `new`: Name, then Goal.
    NewProject,
    /// `adopt`: the Thread, and the directory it works in.
    Adopt { thread: ThreadId, cwd: String },
}

impl Job {
    fn run(&self, place: &Place, values: &[String]) -> Outcome {
        let value = |at: usize| {
            values
                .get(at)
                .map(|v| v.trim().to_string())
                .unwrap_or_default()
        };
        let done = match self {
            Job::NewProject => data::new_project(place, &value(0), &value(1)),
            Job::Adopt { thread, cwd } => data::adopt(place, *thread, &value(0), cwd),
        };
        match done {
            Ok(body) => Outcome::Done {
                title: "Projects".into(),
                body,
            },
            Err(error) => Outcome::Refused(error),
        }
    }
}

/// One question of an ask: its prompt, the typed answer, the answer
/// Enter takes when nothing is typed.
#[derive(Clone, Debug, PartialEq)]
struct Question {
    prompt: &'static str,
    typed: String,
    default: String,
}

/// Upstream's terminal questions (`actions::ask`), one at a time.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Ask {
    title: &'static str,
    questions: Vec<Question>,
    at: usize,
    job: Job,
    /// `working…`, or the error that held the sheet open.
    status: Option<String>,
}

impl Ask {
    /// `new`: "New project", a name, then an optional goal.
    pub(crate) fn new_project() -> Ask {
        Ask {
            title: "New project",
            questions: vec![
                question("Name", ""),
                question("Goal (one line, optional)", ""),
            ],
            at: 0,
            job: Job::NewProject,
            status: None,
        }
    }

    /// `adopt`: "Continue this Thread as a project", its name offered.
    pub(crate) fn adopt(thread: ThreadId, label: String, cwd: String) -> Ask {
        Ask {
            title: "Continue this Thread as a project",
            questions: vec![question("Project name", &label)],
            at: 0,
            job: Job::Adopt { thread, cwd },
            status: None,
        }
    }

    fn key(&mut self, key: KeyEvent) -> Answer {
        if self.status.as_deref() == Some("working\u{2026}") {
            return Answer::Stay;
        }
        let at = self.at;
        match key.code {
            KeyCode::Esc => return Answer::Cancel,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Answer::Cancel
            }
            KeyCode::Backspace => {
                self.questions[at].typed.pop();
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.questions[at].typed.push(c)
            }
            KeyCode::Enter => {
                let answer = self.questions[at].answer();
                if self.at == 0 && answer.is_empty() {
                    self.status = Some("error: no name given".into());
                    return Answer::Stay;
                }
                if self.at + 1 < self.questions.len() {
                    self.at += 1;
                } else {
                    return Answer::Done(self.questions.iter().map(Question::answer).collect());
                }
            }
            _ => {}
        }
        self.status = None;
        Answer::Stay
    }

    fn lines(&self) -> Vec<Line> {
        let mut out = vec![bold(&format!(" {}", self.title)), rule(), Line::default()];
        for (at, question) in self.questions.iter().enumerate().take(self.at + 1) {
            let shown = if question.default.is_empty() {
                format!("  {}: {}", question.prompt, question.typed)
            } else {
                format!(
                    "  {} [{}]: {}",
                    question.prompt, question.default, question.typed
                )
            };
            out.push(plain(&if at == self.at {
                format!("{shown}\u{258f}")
            } else {
                shown
            }));
        }
        if let Some(status) = &self.status {
            out.push(Line::default());
            out.push(plain(&format!("  {status}")));
        }
        out.push(rule());
        out.push(dim(" \u{21b5} next  esc cancel"));
        out
    }
}

fn question(prompt: &'static str, default: &str) -> Question {
    Question {
        prompt,
        typed: String::new(),
        default: default.to_string(),
    }
}

impl Question {
    fn answer(&self) -> String {
        let typed = self.typed.trim();
        if typed.is_empty() {
            self.default.clone()
        } else {
            typed.to_string()
        }
    }
}

/// `pick`'s numbered list: "Which project should `<cmd>` act on?", a
/// number typed (or ↑↓ to it) and ↵.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Pick {
    verb: Verb,
    slugs: Vec<String>,
    typed: String,
    selected: usize,
    status: Option<String>,
}

impl Pick {
    fn new(verb: Verb, slugs: Vec<String>) -> Pick {
        Pick {
            verb,
            slugs,
            typed: String::new(),
            selected: 0,
            status: None,
        }
    }

    fn key(&mut self, key: KeyEvent) -> Answer {
        let n = self.slugs.len();
        match key.code {
            KeyCode::Esc => return Answer::Cancel,
            KeyCode::Up if n > 0 => self.selected = (self.selected + n - 1) % n,
            KeyCode::Down if n > 0 => self.selected = (self.selected + 1) % n,
            KeyCode::Backspace => {
                self.typed.pop();
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                self.typed.push(c);
                if let Some(at) = self.number() {
                    self.selected = at;
                }
            }
            KeyCode::Enter => {
                let at = if self.typed.is_empty() {
                    Some(self.selected)
                } else {
                    self.number()
                };
                match at.and_then(|at| self.slugs.get(at)) {
                    Some(slug) => return Answer::Done(vec![slug.clone()]),
                    None => self.status = Some(format!("error: no project number {}", self.typed)),
                }
                return Answer::Stay;
            }
            _ => {}
        }
        Answer::Stay
    }

    /// The typed number's index, when it names a listed project.
    fn number(&self) -> Option<usize> {
        let choice: usize = self.typed.parse().ok()?;
        (1..=self.slugs.len()).contains(&choice).then(|| choice - 1)
    }

    fn lines(&self) -> Vec<Line> {
        let mut out = vec![
            bold(&format!(
                " Which project should `{}` act on?",
                self.verb.word()
            )),
            rule(),
        ];
        for (index, slug) in self.slugs.iter().enumerate() {
            let line = plain(&format!("  {}. {slug}", index + 1));
            out.push(if index == self.selected {
                Line {
                    style: Style {
                        reverse: true,
                        ..line.style
                    },
                    ..line
                }
            } else {
                line
            });
        }
        out.push(Line::default());
        out.push(plain(&format!("Project number: {}\u{258f}", self.typed)));
        if let Some(status) = &self.status {
            out.push(plain(&format!("  {status}")));
        }
        out.push(rule());
        out.push(dim(
            " type a number or \u{2191}\u{2193}  \u{21b5} ok  esc cancel",
        ));
        out
    }
}

/// `open`, `pause` or `resume` on one project: the CLI's own subcommand.
pub(crate) fn run_verb(place: &Place, verb: Verb, slug: &str) -> Outcome {
    match place.run(&[verb.word(), slug]) {
        (true, line) => Outcome::Done {
            title: "Projects".into(),
            body: line,
        },
        (false, error) => Outcome::Refused(error),
    }
}

/// `overview`'s text for `slugs`: each project's `overview::render`.
fn overview_text(place: &Place, slugs: &[String]) -> String {
    place
        .with_ctx(|ctx| {
            if slugs.is_empty() {
                return format!("there are no projects in {}", ctx.root.display());
            }
            let mut out = String::new();
            for (index, slug) in slugs.iter().enumerate() {
                let Ok(project) = ferrite_projects::project::Project::load(&ctx.root, slug) else {
                    continue;
                };
                if index > 0 {
                    out.push('\n');
                }
                let rows = ferrite_projects::threads::rows(ctx, &project);
                out.push_str(&ferrite_projects::overview::render(&project, &rows));
            }
            out
        })
        .unwrap_or_else(|error| format!("error: {error}"))
}

// ------------------------------------------------------------ drawing

fn plain(text: &str) -> Line {
    Line {
        text: text.to_string(),
        ..Line::default()
    }
}

/// A line of the text overview: its headings (a project, a group) start
/// at the margin, and are bold.
fn overview_line(text: &str) -> Line {
    let heading = !text.is_empty() && !text.starts_with(' ');
    Line {
        text: format!(" {text}"),
        style: Style {
            bold: heading,
            ..Style::default()
        },
        ..Line::default()
    }
}

fn bold(text: &str) -> Line {
    Line {
        text: text.to_string(),
        style: Style {
            bold: true,
            ..Style::default()
        },
        ..Line::default()
    }
}

fn dim(text: &str) -> Line {
    Line {
        text: text.to_string(),
        style: Style {
            dim: true,
            ..Style::default()
        },
        ..Line::default()
    }
}

fn rule() -> Line {
    Line {
        rule: true,
        ..Line::default()
    }
}

/// The footer's two lines held at the bottom of a sheet `height` lines tall.
fn pad_to(mut lines: Vec<Line>, height: usize) -> Vec<Line> {
    if lines.len() < height {
        let footer = lines.split_off(lines.len().saturating_sub(2));
        lines.resize(height - footer.len(), Line::default());
        lines.extend(footer);
    }
    lines
}

/// A row colour's ink.
fn ink(color: Color) -> u32 {
    match color {
        Color::Red => RED,
        Color::Yellow => YELLOW,
        Color::Green => GREEN,
        Color::DarkGrey => TEXT_MUTED,
    }
}

fn line_element(index: usize, line: Line) -> gpui::AnyElement {
    let row = components::text_ui()
        .id(("projects-line", index))
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(ROW))
        .px(px(FLOAT_PAD_X))
        .whitespace_nowrap()
        .overflow_hidden();
    if line.rule {
        return row
            .child(
                div()
                    .w_full()
                    .h(px(0.))
                    .border_t_1()
                    .border_color(FLOAT_RULE),
            )
            .into_any_element();
    }
    let style = line.style;
    let color = if style.reverse || style.bold {
        TEXT_STRONG
    } else if let Some(color) = style.color {
        ink(color)
    } else if style.dim {
        TEXT_MUTED
    } else {
        TEXT
    };
    let color = match (style.reverse, style.color) {
        (true, Some(tint)) => ink(tint),
        _ => color,
    };
    row.debug_selector(move || format!("projects-line-{index}"))
        .when(style.reverse, |row| row.bg(FLOAT_SEL))
        .when(style.bold, |row| row.font_weight(W_STRONG))
        .text_color(rgb(color))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(SharedString::from(line.text)),
        )
        .children(line.right.map(|(text, _)| {
            div()
                .flex_shrink_0()
                .ml(px(FLOAT_DETAIL_GAP))
                .text_color(rgb(TEXT_MUTED))
                .font_weight(W_BODY)
                .child(SharedString::from(text))
        }))
        .into_any_element()
}

impl Render for ProjectsSheet {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lines = self.lines();
        self.tallest = lines.len();
        crate::menu::float()
            .debug_selector(|| "projects-sheet".into())
            .track_focus(&self.focus)
            .key_context("Projects")
            .h(px(lines.len() as f32 * ROW))
            .overflow_hidden()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_key_down(cx.listener(Self::on_key))
            .children(
                lines
                    .into_iter()
                    .enumerate()
                    .map(|(index, line)| line_element(index, line)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(key: &str, typed: Option<&str>, modifiers: gpui::Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.into(),
            key_char: typed.map(str::to_string),
        }
    }

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn typed(ask: &mut Ask, text: &str) {
        for c in text.chars() {
            assert_eq!(ask.key(press(KeyCode::Char(c))), Answer::Stay);
        }
    }

    #[test]
    fn keystrokes_read_as_the_popup_reads_keys() {
        let none = gpui::Modifiers::default();
        let shift = gpui::Modifiers {
            shift: true,
            ..none
        };
        let control = gpui::Modifiers {
            control: true,
            ..none
        };
        let command = gpui::Modifiers {
            platform: true,
            ..none
        };
        assert_eq!(
            key_of(&stroke("tab", None, none)),
            Some(press(KeyCode::Tab))
        );
        assert_eq!(
            key_of(&stroke("tab", None, shift)),
            Some(press(KeyCode::BackTab))
        );
        assert_eq!(
            key_of(&stroke("escape", None, none)),
            Some(press(KeyCode::Esc))
        );
        assert_eq!(
            key_of(&stroke("p", Some("P"), shift)),
            Some(press(KeyCode::Char('P')))
        );
        assert_eq!(
            key_of(&stroke("space", Some(" "), none)),
            Some(press(KeyCode::Char(' ')))
        );
        assert_eq!(
            key_of(&stroke("c", None, control)),
            Some(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
        );
        assert_eq!(
            key_of(&stroke("k", None, command)),
            None,
            "the cockpit keeps ⌘ keys"
        );
    }

    #[test]
    fn the_new_project_ask_takes_a_name_then_an_optional_goal() {
        let mut ask = Ask::new_project();
        assert_eq!(ask.key(press(KeyCode::Enter)), Answer::Stay);
        assert_eq!(ask.status.as_deref(), Some("error: no name given"));
        typed(&mut ask, "Demo");
        assert_eq!(ask.status, None, "typing clears the error");
        assert_eq!(ask.key(press(KeyCode::Enter)), Answer::Stay);
        assert_eq!(ask.at, 1);
        assert_eq!(
            ask.key(press(KeyCode::Enter)),
            Answer::Done(vec!["Demo".into(), String::new()]),
            "the goal is optional"
        );
        let texts: Vec<String> = ask.lines().into_iter().map(|line| line.text).collect();
        assert_eq!(texts[0], " New project");
        assert!(texts.contains(&"  Name: Demo".to_string()), "{texts:?}");
        assert_eq!(Ask::new_project().key(press(KeyCode::Esc)), Answer::Cancel);
    }

    #[test]
    fn the_adopt_ask_offers_the_threads_name() {
        let mut ask = Ask::adopt(ThreadId::new(7), "My Repo".into(), "/work".into());
        let texts: Vec<String> = ask.lines().into_iter().map(|line| line.text).collect();
        assert_eq!(texts[0], " Continue this Thread as a project");
        assert!(
            texts.contains(&"  Project name [My Repo]: \u{258f}".to_string()),
            "{texts:?}"
        );
        assert_eq!(
            ask.key(press(KeyCode::Enter)),
            Answer::Done(vec!["My Repo".into()])
        );
    }

    #[test]
    fn the_pick_takes_a_number_or_the_arrows() {
        let slugs = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];
        let mut pick = Pick::new(Verb::Pause, slugs.clone());
        let texts: Vec<String> = pick.lines().into_iter().map(|line| line.text).collect();
        assert_eq!(texts[0], " Which project should `pause` act on?");
        assert_eq!(texts[2], "  1. alpha");
        assert_eq!(pick.key(press(KeyCode::Char('2'))), Answer::Stay);
        assert_eq!(
            pick.key(press(KeyCode::Enter)),
            Answer::Done(vec!["beta".into()])
        );
        let mut pick = Pick::new(Verb::Open, slugs.clone());
        pick.key(press(KeyCode::Up));
        assert_eq!(
            pick.key(press(KeyCode::Enter)),
            Answer::Done(vec!["gamma".into()]),
            "wraps"
        );
        let mut pick = Pick::new(Verb::Open, slugs);
        pick.key(press(KeyCode::Char('9')));
        assert_eq!(pick.key(press(KeyCode::Enter)), Answer::Stay);
        assert_eq!(pick.status.as_deref(), Some("error: no project number 9"));
    }

    #[test]
    fn a_short_sheet_keeps_its_footer_at_the_bottom() {
        let lines = pad_to(vec![bold("t"), rule(), plain("a"), rule(), dim("keys")], 8);
        assert_eq!(lines.len(), 8);
        assert!(lines[6].rule);
        assert_eq!(lines[7].text, "keys");
    }

    /// The sheet fits what it holds: the body to its last written line and
    /// a line of air, the footer under it; never under the floor, never
    /// over the screen.
    #[test]
    fn a_sheet_fits_its_content_between_its_floor_and_its_screen() {
        let screen = |body: usize| {
            let written = (0..body).map(|i| plain(&format!("row {i}")));
            let lines = [bold("t"), rule()].into_iter().chain(written);
            pad_to(lines.chain([rule(), dim("keys")]).collect(), 30)
        };
        let short = fit_content(screen(3), 4);
        assert_eq!(short.len(), 2 + 3 + 1 + 2, "body, a line of air, footer");
        assert_eq!(short[4].text, "row 2");
        assert!(short[6].rule);
        assert_eq!(short[7].text, "keys");
        assert_eq!(fit_content(screen(3), 16).len(), 16, "the floor holds");
        assert_eq!(
            fit_content(screen(26), 16).len(),
            30,
            "a full screen stays whole"
        );
    }
}
