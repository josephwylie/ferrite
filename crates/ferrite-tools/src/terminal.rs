//! The Terminal (Claude's, `terminal-open.png`, `terminal-more-menu.png`,
//! `terminal-menu.png`): the user's shell in the Thread's folder —
//! `ferrite-terminal`'s grid — one shell a Panel Surface (`Terminal N`, ADR
//! 0039), with nothing more than Claude's: copy and paste, and `Attach as
//! context`.
//!
//! - **Opening:** ⌘J brings the Thread's last shell forward, or starts its
//!   first; the Panel's `+` menu (Terminal, T) starts another. Closing its
//!   tab ends its shell; a shell that exits on its own closes its tab.
//! - **Its row:** `⋯` — Cursor style ›, Blink cursor, Copy on select,
//!   Scrollback ›.
//! - **Right-click:** Attach selection as context (with a selection), Copy,
//!   Paste, Select all, Clear terminal. A selection also hangs the
//!   `Attach as context` pill over itself.
//! - **Attach as context** is the Reply card in the Composer's tray, the
//!   same one the message menu's `Attach message as context` makes.
//!
//! Every Thread's shells live in one view of their own (ADR 0041), which
//! the Panel draws as the body of whichever shell it shows. What it draws
//! of the window it reads through a `Source`, which the Cockpit makes over
//! itself (`terminal_inputs`) and a test over a fixture; what it asks of
//! the window (a menu, Attach, an address opened, a shell started again, a
//! tab gone) it says as a `TerminalsEvent`. Starting a shell needs the
//! Thread's folder and the machine, so the window starts it and hands it
//! to the view.

use std::collections::HashMap;
use std::path::Path;

use ferrite_core::machine::Bridge;
use ferrite_core::ThreadKey;
use ferrite_terminal::{Cursor, Settings, Terminal, TerminalEvent, TerminalView};
use ferrite_ui::components::MenuDoor;
use ferrite_ui::source::Source;
use ferrite_ui::{icons, menu, theme};
use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, ClickEvent, Context, Div, Entity, EventEmitter, Focusable, Pixels, Point, Window};

/// What a Terminal menu row does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Attach,
    Copy,
    Paste,
    SelectAll,
    Clear,
    ToggleBlink,
    ToggleCopyOnSelect,
    Cursor(Cursor),
    Scrollback(usize),
}

/// A parent row's submenu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sub {
    CursorStyle,
    Scrollback,
}

/// What a row of the `⋯` menu does: a verb, or a submenu beside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Verb(Verb),
    Sub(Sub),
}

/// The scrollbacks the menu offers.
const SCROLLBACKS: [usize; 3] = [1_000, theme::TERMINAL_SCROLLBACK, 100_000];

/// A shell's standing: running, or the reason it never started.
enum Shell {
    /// `remote`: it runs on another machine, so a failed end is the
    /// connection's and leaves its tab to say so.
    Running { view: Entity<TerminalView>, remote: bool },
    Failed(String),
}

struct Tab {
    /// `Terminal N`'s N, and its Surface's (`SurfaceId::Terminal`).
    number: u64,
    shell: Shell,
}

impl Tab {
    fn view(&self) -> Option<&Entity<TerminalView>> {
        match &self.shell {
            Shell::Running { view, .. } => Some(view),
            Shell::Failed(_) => None,
        }
    }
}

/// One Thread's shells.
#[derive(Default)]
struct Shells {
    tabs: Vec<Tab>,
    /// Shells started, which numbers the next.
    made: u64,
}

/// What the `⋯` menu sets, for every terminal of the window.
struct Prefs {
    settings: Settings,
    scrollback: usize,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            scrollback: theme::TERMINAL_SCROLLBACK,
        }
    }
}

fn scrollback_label(lines: usize) -> String {
    let digits = lines.to_string();
    let mut out = String::new();
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    format!("{out} lines")
}

fn spawn_shell(
    machine: Option<&Bridge>,
    directory: &Path,
    scrollback: usize,
    size: (u16, u16),
) -> std::io::Result<Terminal> {
    match machine {
        Some(machine) => {
            let (program, args) = machine.terminal_command(directory)?;
            Terminal::spawn_command(&std::env::temp_dir(), &program, args, scrollback, size)
        }
        None => Terminal::spawn(directory, scrollback, size),
    }
}

/// Every Thread's shells, and what the `⋯` menu set. One view, not one
/// per Thread: the menu's choices are the window's.
pub struct TerminalsView {
    /// The shell the Panel shows, as the window has it.
    inputs: Source<TerminalInputs>,
    threads: HashMap<ThreadKey, Shells>,
    prefs: Prefs,
    /// The shown shell is to take the keyboard once it is drawn.
    focus_wanted: bool,
    /// Whether the shell's right-click found a selection.
    selected: bool,
}

/// What the Terminal draws from, as the window has it this frame.
pub struct TerminalInputs {
    /// The Thread whose Panel shows a shell, and the shell it shows.
    pub thread: ThreadKey,
    pub number: u64,
    /// Whether that Thread is the focused one: only then does the shell
    /// take the keyboard it was opened for.
    pub focused: bool,
    /// The `⋯` button's menu.
    pub more: MenuDoor,
}

/// What the Terminal asks of the window.
#[derive(Clone, Debug, PartialEq)]
pub enum TerminalsEvent {
    /// `Attach as context`: the text, a Reply in the Thread's Composer.
    Attach { thread: ThreadKey, text: String },
    /// A right-click in a shell: its menu, at the press.
    Menu { thread: ThreadKey, at: Point<Pixels> },
    /// A press on the `⋯` button: its menu, or shut it.
    More { thread: ThreadKey },
    /// A ⌘-press on an address the shell printed.
    Open { thread: ThreadKey, address: String },
    /// Try again, on a shell that could not start.
    Retry { thread: ThreadKey, number: u64 },
    /// A shell ended on its own: its tab goes.
    Ended { thread: ThreadKey, number: u64 },
}

impl EventEmitter<TerminalsEvent> for TerminalsView {}

impl TerminalsView {
    pub fn new(inputs: Source<TerminalInputs>) -> Self {
        Self {
            inputs,
            threads: HashMap::new(),
            prefs: Prefs::default(),
            focus_wanted: false,
            selected: false,
        }
    }

    fn tab(&self, thread: ThreadKey, number: u64) -> Option<&Tab> {
        self.threads.get(&thread)?.tabs.iter().find(|tab| tab.number == number)
    }

    fn tab_mut(&mut self, thread: ThreadKey, number: u64) -> Option<&mut Tab> {
        self.threads.get_mut(&thread)?.tabs.iter_mut().find(|tab| tab.number == number)
    }

    /// Shell `number` of `thread`, while it runs.
    pub fn shell(&self, thread: ThreadKey, number: u64) -> Option<Entity<TerminalView>> {
        self.tab(thread, number)?.view().cloned()
    }

    /// Whether shell `number` of `thread` has the keyboard.
    pub fn holds_focus(&self, thread: ThreadKey, number: u64, window: &Window, cx: &gpui::App) -> bool {
        self.shell(thread, number)
            .is_some_and(|view| view.focus_handle(cx).is_focused(window))
    }

    /// One more shell for `thread`, in `directory`: its number.
    pub fn open(&mut self, thread: ThreadKey, directory: &Path, machine: Option<&Bridge>, cx: &mut Context<Self>) -> u64 {
        let shell = self.start_shell(thread, directory, machine, cx);
        let shells = self.threads.entry(thread).or_default();
        shells.made += 1;
        let number = shells.made;
        shells.tabs.push(Tab { number, shell });
        self.focus_wanted = true;
        cx.notify();
        number
    }

    /// A shell in `directory`, or the reason there is none: its Surface
    /// says why instead of the door looking dead.
    fn start_shell(
        &mut self,
        thread: ThreadKey,
        directory: &Path,
        machine: Option<&Bridge>,
        cx: &mut Context<Self>,
    ) -> Shell {
        let (settings, scrollback) = (self.prefs.settings, self.prefs.scrollback);
        // The column's width in cells, reckoned from the face's usual advance.
        let columns = ((theme::TERMINAL_W - 2. * theme::TERMINAL_PAD_X)
            / (theme::FS_TERMINAL * theme::CODE_ADVANCE)) as u16;
        let lines = (theme::TERMINAL_FIRST_ROWS) as u16;
        match spawn_shell(machine, directory, scrollback, (columns, lines)) {
            Ok(shell) => {
                let view = cx.new(|cx| TerminalView::new(shell, settings, cx));
                cx.subscribe(&view, move |terminals, view, event: &TerminalEvent, cx| {
                    terminals.on_shell_event(thread, view, event, cx)
                })
                .detach();
                Shell::Running { view, remote: machine.is_some() }
            }
            Err(error) => Shell::Failed(error.to_string()),
        }
    }

    /// Start shell `number` again, where it failed to start.
    pub fn retry(
        &mut self,
        thread: ThreadKey,
        number: u64,
        directory: &Path,
        machine: Option<&Bridge>,
        cx: &mut Context<Self>,
    ) {
        if !self
            .tab(thread, number)
            .is_some_and(|tab| matches!(tab.shell, Shell::Failed(_)))
        {
            return;
        }
        let shell = self.start_shell(thread, directory, machine, cx);
        if let Some(tab) = self.tab_mut(thread, number) {
            tab.shell = shell;
            self.focus_wanted = true;
            cx.notify();
        }
    }

    fn on_shell_event(
        &mut self,
        thread: ThreadKey,
        view: Entity<TerminalView>,
        event: &TerminalEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self
            .threads
            .get_mut(&thread)
            .and_then(|shells| shells.tabs.iter_mut().find(|tab| tab.view() == Some(&view)))
        else {
            return;
        };
        let number = tab.number;
        match event {
            TerminalEvent::Attach(text) => cx.emit(TerminalsEvent::Attach { thread, text: text.clone() }),
            TerminalEvent::ContextMenu(at) => {
                // The rows read what is selected as the menu opens.
                self.selected = view.read(cx).selection().is_some();
                cx.emit(TerminalsEvent::Menu { thread, at: *at })
            }
            TerminalEvent::Failed(status) if matches!(tab.shell, Shell::Running { remote: true, .. }) => {
                tab.shell = Shell::Failed(format!(
                    "The remote terminal ended ({status}). Check the connection in Settings, then try again."
                ));
                cx.notify();
            }
            TerminalEvent::Exited | TerminalEvent::Failed(_) => {
                self.end(thread, number, cx);
                cx.emit(TerminalsEvent::Ended { thread, number });
            }
            TerminalEvent::Open(address) => cx.emit(TerminalsEvent::Open { thread, address: address.clone() }),
            TerminalEvent::Title(_) => {}
        }
    }

    /// The window let go of `machine`: each of its shells ends with it,
    /// and its tab says so, to start again once the machine is back.
    pub fn let_go(&mut self, machine: ferrite_core::MachineId, cx: &mut Context<Self>) {
        let tabs = self
            .threads
            .iter_mut()
            .filter(|(thread, _)| thread.machine() == machine)
            .flat_map(|(_, shells)| &mut shells.tabs);
        for tab in tabs.filter(|tab| tab.view().is_some()) {
            tab.shell = Shell::Failed("The remote terminal ended when its machine was turned off.".into());
        }
        cx.notify();
    }

    /// A machine let go is held again: its Threads' shells are theirs.
    pub fn held_again(&mut self, rehold: ferrite_core::Rehold, cx: &mut Context<Self>) {
        let moved: Vec<_> = self.threads.extract_if(|thread, _| rehold.key(*thread).is_some()).collect();
        self.threads.extend(
            moved.into_iter().filter_map(|(thread, shells)| Some((rehold.key(thread)?, shells))),
        );
        cx.notify();
    }

    /// Shell `number` of `thread` ends: its tab closed, or it exited.
    pub fn end(&mut self, thread: ThreadKey, number: u64, cx: &mut Context<Self>) {
        let Some(shells) = self.threads.get_mut(&thread) else {
            return;
        };
        shells.tabs.retain(|tab| tab.number != number);
        if shells.tabs.is_empty() {
            self.threads.remove(&thread);
        }
        cx.notify();
    }

    /// The shown shell is to take the keyboard (⌘J back to it).
    pub fn want_focus(&mut self) {
        self.focus_wanted = true;
    }

    /// The shown shell takes the keyboard it was opened for, after the
    /// frame that draws it. Not sooner: a handle not yet in the window's
    /// tree cannot be focused. And not inside the frame: it is built on
    /// where the keyboard was when the root drew, so the next key would
    /// still be typed into the old owner.
    fn tend_focus(&mut self, inputs: &TerminalInputs, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.focus_wanted) {
            return;
        }
        let Some(view) = self.shell(inputs.thread, inputs.number).filter(|_| inputs.focused) else {
            return;
        };
        window.defer(cx, move |window, cx| {
            if view.read(cx).is_drawn() {
                let focus = view.focus_handle(cx);
                window.focus(&focus, cx);
            }
        });
    }

    /// A Terminal menu row, on shell `number` of `thread`.
    pub fn run_verb(&mut self, thread: ThreadKey, number: u64, verb: Verb, cx: &mut Context<Self>) {
        let Some(view) = self.shell(thread, number) else {
            return;
        };
        match verb {
            Verb::Attach => view.update(cx, |view, cx| view.attach_selection(cx)),
            Verb::Copy => {
                view.update(cx, |view, cx| view.copy(cx));
            }
            Verb::Paste => view.update(cx, |view, cx| view.paste(cx)),
            Verb::SelectAll => view.update(cx, |view, cx| view.select_all(cx)),
            Verb::Clear => view.update(cx, |view, cx| view.clear(cx)),
            Verb::ToggleBlink => self.prefs.settings.blink ^= true,
            Verb::ToggleCopyOnSelect => self.prefs.settings.copy_on_select ^= true,
            Verb::Cursor(cursor) => self.prefs.settings.cursor = cursor,
            Verb::Scrollback(lines) => self.prefs.scrollback = lines,
        }
        let (settings, scrollback) = (self.prefs.settings, self.prefs.scrollback);
        for view in self.threads.get(&thread).into_iter().flat_map(|t| &t.tabs).filter_map(Tab::view) {
            view.update(cx, |view, cx| view.configure(settings, scrollback, cx));
        }
        cx.notify();
    }

    /// The right-click menu's rows.
    pub fn context_rows(&self) -> Vec<Option<(menu::Item, Verb)>> {
        let selected = self.selected;
        let row = |label: &'static str, action: &str, verb: Verb, enabled: bool| {
            let chord = ferrite_ui::components::bound_chord_in(
                action,
                Some(ferrite_ui::keymap::TERMINAL),
            );
            let mut item = menu::Item::new(label).shortcut(chord.unwrap_or_default());
            item.disabled = !enabled;
            Some((item, verb))
        };
        let mut rows = Vec::new();
        if selected {
            rows.push(row("Attach selection as context", "terminal::Attach", Verb::Attach, true));
            rows.push(None);
        }
        rows.extend([
            row("Copy", "terminal::Copy", Verb::Copy, selected),
            row("Paste", "terminal::Paste", Verb::Paste, true),
            row("Select all", "terminal::SelectAll", Verb::SelectAll, true),
            None,
            row("Clear terminal", "", Verb::Clear, true),
        ]);
        rows
    }

    /// The Terminal `⋯` menu's rows.
    pub fn more_rows(&self) -> Vec<Option<(menu::Item, Row)>> {
        let prefs = &self.prefs;
        let cursor = match prefs.settings.cursor {
            Cursor::Block => "Block",
            Cursor::Bar => "Bar",
            Cursor::Underline => "Underline",
        };
        let check = |label: &'static str, on: bool, verb: Verb| {
            let mut item = menu::Item::new(label);
            item.checked = on;
            Some((item, Row::Verb(verb)))
        };
        vec![
            Some((menu::Item::new("Cursor style").detail(cursor), Row::Sub(Sub::CursorStyle))),
            check("Blink cursor", prefs.settings.blink, Verb::ToggleBlink),
            check("Copy on select", prefs.settings.copy_on_select, Verb::ToggleCopyOnSelect),
            Some((
                menu::Item::new("Scrollback").detail(scrollback_label(prefs.scrollback)),
                Row::Sub(Sub::Scrollback),
            )),
        ]
    }

    pub fn sub_rows(&self, sub: Sub) -> Vec<(menu::Item, Verb)> {
        let prefs = &self.prefs;
        match sub {
            Sub::CursorStyle => [
                ("Block", Cursor::Block),
                ("Bar", Cursor::Bar),
                ("Underline", Cursor::Underline),
            ]
            .into_iter()
            .map(|(label, cursor)| {
                let mut item = menu::Item::new(label);
                item.checked = prefs.settings.cursor == cursor;
                (item, Verb::Cursor(cursor))
            })
            .collect(),
            Sub::Scrollback => SCROLLBACKS
                .into_iter()
                .map(|lines| {
                    let mut item = menu::Item::new(scrollback_label(lines));
                    item.checked = prefs.scrollback == lines;
                    (item, Verb::Scrollback(lines))
                })
                .collect(),
        }
    }

    // ------------------------------------------------------------ drawing

    /// The shown shell's Surface: its row (`⋯`), then the grid.
    fn body(&self, inputs: TerminalInputs, cx: &mut Context<Self>) -> Div {
        let (thread, number) = (inputs.thread, inputs.number);
        let body = self.tab(thread, number).map(|tab| match &tab.shell {
            Shell::Failed(why) => Self::failure(why.clone(), thread, number, cx),
            Shell::Running { view, .. } => view.clone().into_any_element(),
        });
        let press = cx.listener(move |_, _: &(), _, cx| cx.emit(TerminalsEvent::More { thread }));
        let more = inputs.more.button(
            ferrite_ui::components::icon_button(
                "terminal-more",
                icons::ELLIPSIS,
                ferrite_ui::components::IconSize::Standard,
                "More",
                cx,
            ),
            "More",
            move |window, cx| press(&(), window, cx),
        );
        div()
            .debug_selector(|| "terminal-pane".into())
            .flex()
            .flex_col()
            .size_full()
            .child(ferrite_ui::components::surface_bar().child(div().flex_1()).child(more))
            .child(div().flex_1().min_h_0().children(body))
    }

    /// Why the shell never started, and a way to try again.
    fn failure(why: String, thread: ThreadKey, number: u64, cx: &mut Context<Self>) -> AnyElement {
        use theme::*;
        div()
            .debug_selector(|| "terminal-failed".into())
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_4))
            .text_size(px(FS_LABEL))
            .text_color(rgb(INK_SUBTLE))
            .child(div().w_full().text_center().whitespace_normal().child("The terminal could not start"))
            .child(div().w_full().text_center().whitespace_normal().child(why))
            .child(
                ferrite_ui::components::form_button("terminal-retry", cx)
                    .debug_selector(|| "terminal-retry".into())
                    .h(px(ROW))
                    .px(px(CH))
                    .child(ferrite_ui::components::form_label("Try again", INK_SUBTLE))
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.emit(TerminalsEvent::Retry { thread, number })
                    })),
            )
            .into_any_element()
    }

    /// The numbers of the Thread's shells — what the tests read.
    #[cfg(any(test, feature = "test-support"))]
    pub fn numbers(&self, thread: ThreadKey) -> Vec<u64> {
        self.threads
            .get(&thread)
            .map(|shells| shells.tabs.iter().map(|tab| tab.number).collect())
            .unwrap_or_default()
    }
}

impl Render for TerminalsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(inputs) = self.inputs.read(&(), cx) else {
            return div().into_any_element();
        };
        self.tend_focus(&inputs, window, cx);
        self.body(inputs, cx).into_any_element()
    }
}

#[cfg(test)]
mod tests;
