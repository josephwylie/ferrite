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
//! of the window it reads through `terminal_inputs`; what it asks of the
//! window (a menu, Attach, an address opened, a shell started again, a
//! tab gone) it says as a `TerminalsEvent`. Starting a shell needs the
//! Thread's folder and the machine, so the window starts it and hands it
//! to the view.

use std::collections::HashMap;
use std::path::Path;

use gpui::WeakEntity;

use ferrite_core::{machine::Bridge, replies::Reply};
use ferrite_pane::link_target::LinkTarget;
use ferrite_terminal::{Cursor, Settings, Terminal, TerminalEvent, TerminalView};
use crate::cockpit::menu_button::{Door, Doors, Edge, MenuHost};
use ferrite_ui::icons;
use ferrite_ui::theme;

use super::*;
use crate::cockpit::context_menu::Submenu;

/// What a Terminal menu row does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verb {
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
pub(crate) enum Sub {
    CursorStyle,
    Scrollback,
}

/// The scrollbacks the menu offers.
const SCROLLBACKS: [usize; 3] = [1_000, theme::TERMINAL_SCROLLBACK, 100_000];

/// A shell's standing: running, or the reason it never started.
enum Shell {
    /// `remote`: it runs on the window's machine, so a failed end is the
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
pub(crate) struct TerminalsView {
    cockpit: WeakEntity<CockpitView>,
    threads: HashMap<ThreadId, Shells>,
    prefs: Prefs,
    /// The shown shell is to take the keyboard once it is drawn.
    focus_wanted: bool,
    /// Whether the shell's right-click found a selection.
    selected: bool,
}

/// What the Terminal draws from, as the Cockpit has it this frame.
pub(crate) struct TerminalInputs {
    /// The Thread whose Panel shows a shell, and the shell it shows.
    thread: ThreadId,
    number: u64,
    /// Whether that Thread is the focused one: only then does the shell
    /// take the keyboard it was opened for.
    focused: bool,
    doors: Doors,
}

/// What the Terminal asks of the window.
pub(crate) enum TerminalsEvent {
    /// `Attach as context`: the text, a Reply in the Thread's Composer.
    Attach { thread: ThreadId, text: String },
    /// A right-click in a shell: its menu, at the press.
    Menu { thread: ThreadId, at: gpui::Point<Pixels> },
    /// A press on the `⋯` button.
    Door(Door),
    /// A ⌘-press on an address the shell printed.
    Open { thread: ThreadId, address: String },
    /// Try again, on a shell that could not start.
    Retry { thread: ThreadId, number: u64 },
    /// A shell ended on its own: its tab goes.
    Ended { thread: ThreadId, number: u64 },
}

impl EventEmitter<TerminalsEvent> for TerminalsView {}

impl MenuHost for TerminalsView {
    fn press_door(&mut self, door: Door, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(TerminalsEvent::Door(door));
    }

    fn tip<E: InteractiveElement>(element: E, text: SharedString) -> E {
        <CockpitView as MenuHost>::tip(element, text)
    }
}

impl CockpitView {
    /// One more shell in the Thread's folder, its Surface opened and
    /// shown (⌘J's first, the `+` menu's Terminal).
    pub(in crate::cockpit) fn new_terminal_surface(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(directory) = self.thread_path(thread, cx) else {
            return;
        };
        let machine = self.cockpit.read(cx).machine().cloned();
        let number = self.terminals.update(cx, |terminals, cx| {
            terminals.open(thread, &directory, machine.as_deref(), cx)
        });
        self.upsert_surface(thread, SurfaceId::Terminal(number), cx);
    }

    pub(in crate::cockpit) fn on_terminals_event(
        &mut self,
        _: Entity<TerminalsView>,
        event: &TerminalsEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TerminalsEvent::Attach { thread, text } => {
                if let Some(index) = self.pane_for(*thread) {
                    self.focus_pane(index, cx);
                    let reply = Reply::new(text.trim(), "");
                    self.panes[index]
                        .composer
                        .update(cx, |composer, cx| composer.reply(reply, cx));
                    cx.notify();
                }
            }
            TerminalsEvent::Menu { thread, at } => {
                self.open_context_menu(MenuTarget::Terminal(*thread), *at, cx)
            }
            TerminalsEvent::Door(door) => self.toggle_door(*door, cx),
            TerminalsEvent::Open { thread, address } => self.open_terminal_address(*thread, address, cx),
            TerminalsEvent::Retry { thread, number } => {
                let Some(directory) = self.thread_path(*thread, cx) else {
                    return;
                };
                let machine = self.cockpit.read(cx).machine().cloned();
                self.terminals.update(cx, |terminals, cx| {
                    terminals.retry(*thread, *number, &directory, machine.as_deref(), cx)
                });
            }
            TerminalsEvent::Ended { thread, number } => {
                self.forget_surface(*thread, &SurfaceId::Terminal(*number));
                cx.notify();
            }
        }
    }

    /// A ⌘-press on an address in the shell: it opens where the Panel
    /// opens it (`Destination`), against the folder the shell started in.
    fn open_terminal_address(&mut self, thread: ThreadId, address: &str, cx: &mut Context<Self>) {
        let folder = self.thread_path(thread, cx);
        match Destination::of(address, folder.as_deref()) {
            Some(Destination::Web(url)) => {
                self.browser_action(thread, ferrite_core::browser::Action::Open { url }, cx)
            }
            Some(Destination::File(file)) => {
                if let Some(index) = self.pane_for(thread) {
                    let line = file.line();
                    self.open_preview_from(index, file.path, line, cx);
                }
            }
            Some(Destination::PullRequest { number, url }) => {
                self.open_named_pr(thread, number, url, cx)
            }
            None => {}
        }
    }

    /// The number of the shell `thread`'s Panel shows, if it shows one.
    fn shown_terminal(&self, thread: ThreadId) -> Option<u64> {
        match self.active_surface(thread)? {
            SurfaceId::Terminal(number) => Some(number),
            _ => None,
        }
    }

    /// Whether the shown shell has the keyboard. The focus pump
    /// (`render`'s) leaves it alone, else it would hand the keyboard back to
    /// the Composer every frame.
    pub(in crate::cockpit) fn terminal_holds_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.focused_thread(cx).is_some_and(|thread| {
            self.shown_terminal(thread)
                .is_some_and(|number| self.terminals.read(cx).holds_focus(thread, number, window, cx))
        })
    }

    /// A Terminal menu row, on the shell `thread`'s Panel shows.
    pub(in crate::cockpit) fn run_terminal_verb(&mut self, thread: ThreadId, verb: Verb, cx: &mut Context<Self>) {
        if let Some(number) = self.shown_terminal(thread) {
            self.terminals
                .update(cx, |terminals, cx| terminals.run_verb(thread, number, verb, cx));
        }
    }

    /// Everything the Terminal draws from: the shell the Panel shows.
    fn terminal_inputs(&self, cx: &gpui::App) -> Option<TerminalInputs> {
        let thread = self.panes[self.panel_drawn_pane(cx)?].thread()?;
        Some(TerminalInputs {
            thread,
            number: self.shown_terminal(thread)?,
            focused: self.focused_thread(cx) == Some(thread),
            doors: self.doors(),
        })
    }
}

#[cfg(feature = "visual-reference")]
impl CockpitView {
    /// A reference scene runs `command` in the shown shell and gives it a
    /// moment to answer.
    pub(in crate::cockpit) fn fixture_terminal_command(
        &mut self,
        thread: ThreadId,
        command: &'static [u8],
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = self
            .shown_terminal(thread)
            .and_then(|number| self.terminals.read(cx).shell(thread, number))
        {
            view.update(cx, |view, _| view.write(command));
            std::thread::sleep(std::time::Duration::from_millis(1500));
        }
    }
}

impl TerminalsView {
    pub(in crate::cockpit) fn new(cockpit: WeakEntity<CockpitView>) -> Self {
        Self {
            cockpit,
            threads: HashMap::new(),
            prefs: Prefs::default(),
            focus_wanted: false,
            selected: false,
        }
    }

    fn tab(&self, thread: ThreadId, number: u64) -> Option<&Tab> {
        self.threads.get(&thread)?.tabs.iter().find(|tab| tab.number == number)
    }

    fn tab_mut(&mut self, thread: ThreadId, number: u64) -> Option<&mut Tab> {
        self.threads.get_mut(&thread)?.tabs.iter_mut().find(|tab| tab.number == number)
    }

    /// Shell `number` of `thread`, while it runs.
    pub(in crate::cockpit) fn shell(&self, thread: ThreadId, number: u64) -> Option<Entity<TerminalView>> {
        self.tab(thread, number)?.view().cloned()
    }

    fn holds_focus(&self, thread: ThreadId, number: u64, window: &Window, cx: &gpui::App) -> bool {
        self.shell(thread, number)
            .is_some_and(|view| view.focus_handle(cx).is_focused(window))
    }

    /// One more shell for `thread`, in `directory`: its number.
    fn open(&mut self, thread: ThreadId, directory: &Path, machine: Option<&Bridge>, cx: &mut Context<Self>) -> u64 {
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
        thread: ThreadId,
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
    fn retry(
        &mut self,
        thread: ThreadId,
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
        thread: ThreadId,
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

    /// Shell `number` of `thread` ends: its tab closed, or it exited.
    pub(in crate::cockpit) fn end(&mut self, thread: ThreadId, number: u64, cx: &mut Context<Self>) {
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
    pub(in crate::cockpit) fn want_focus(&mut self) {
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

    fn run_verb(&mut self, thread: ThreadId, number: u64, verb: Verb, cx: &mut Context<Self>) {
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
    pub(in crate::cockpit) fn context_rows(&self) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let selected = self.selected;
        let row = |label: &'static str, action: &str, verb: Verb, enabled: bool| {
            let chord = ferrite_ui::components::bound_chord_in(
                action,
                Some(ferrite_ui::keymap::TERMINAL),
            );
            let mut item = menu::Item::new(label).shortcut(chord.unwrap_or_default());
            item.disabled = !enabled;
            Some((item, MenuVerb::Terminal(verb)))
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
    pub(in crate::cockpit) fn more_rows(&self) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let prefs = &self.prefs;
        let cursor = match prefs.settings.cursor {
            Cursor::Block => "Block",
            Cursor::Bar => "Bar",
            Cursor::Underline => "Underline",
        };
        let check = |label: &'static str, on: bool, verb: Verb| {
            let mut item = menu::Item::new(label);
            item.checked = on;
            Some((item, MenuVerb::Terminal(verb)))
        };
        vec![
            Some((
                menu::Item::new("Cursor style").detail(cursor),
                MenuVerb::Submenu(Submenu::Terminal(Sub::CursorStyle)),
            )),
            check("Blink cursor", prefs.settings.blink, Verb::ToggleBlink),
            check("Copy on select", prefs.settings.copy_on_select, Verb::ToggleCopyOnSelect),
            Some((
                menu::Item::new("Scrollback").detail(scrollback_label(prefs.scrollback)),
                MenuVerb::Submenu(Submenu::Terminal(Sub::Scrollback)),
            )),
        ]
    }

    pub(in crate::cockpit) fn sub_rows(&self, sub: Sub) -> Vec<(menu::Item, MenuVerb)> {
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
                (item, MenuVerb::Terminal(Verb::Cursor(cursor)))
            })
            .collect(),
            Sub::Scrollback => SCROLLBACKS
                .into_iter()
                .map(|lines| {
                    let mut item = menu::Item::new(scrollback_label(lines));
                    item.checked = prefs.scrollback == lines;
                    (item, MenuVerb::Terminal(Verb::Scrollback(lines)))
                })
                .collect(),
        }
    }

    // ------------------------------------------------------------ drawing

    /// The shown shell's Surface: its row (`⋯`), then the grid.
    fn body(&self, inputs: &TerminalInputs, cx: &mut Context<Self>) -> Div {
        let (thread, number) = (inputs.thread, inputs.number);
        let body = self.tab(thread, number).map(|tab| match &tab.shell {
            Shell::Failed(why) => Self::failure(why.clone(), thread, number, cx),
            Shell::Running { view, .. } => view.clone().into_any_element(),
        });
        let more = inputs.doors.button(
            ferrite_ui::components::icon_button(
                "terminal-more",
                icons::ELLIPSIS,
                ferrite_ui::components::IconSize::Standard,
                "More",
                cx,
            ),
            Door::new(MenuTarget::TerminalMore(thread), Edge::Right),
            "More",
            cx,
        );
        div()
            .debug_selector(|| "terminal-pane".into())
            .flex()
            .flex_col()
            .size_full()
            .child(super::surface_bar().child(div().flex_1()).child(more))
            .child(div().flex_1().min_h_0().children(body))
    }

    /// Why the shell never started, and a way to try again.
    fn failure(why: String, thread: ThreadId, number: u64, cx: &mut Context<Self>) -> AnyElement {
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
    #[cfg(test)]
    pub(in crate::cockpit) fn numbers(&self, thread: ThreadId) -> Vec<u64> {
        self.threads
            .get(&thread)
            .map(|shells| shells.tabs.iter().map(|tab| tab.number).collect())
            .unwrap_or_default()
    }
}

impl Render for TerminalsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(inputs) = self
            .cockpit
            .upgrade()
            .and_then(|cockpit| cockpit.read(cx).terminal_inputs(cx))
        else {
            return div().into_any_element();
        };
        self.tend_focus(&inputs, window, cx);
        self.body(&inputs, cx).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{scrollback_label, spawn_shell, Bridge};

    #[test]
    fn scrollback_reads_with_thousands_separators() {
        assert_eq!(scrollback_label(1_000), "1,000 lines");
        assert_eq!(scrollback_label(10_000), "10,000 lines");
        assert_eq!(scrollback_label(100_000), "100,000 lines");
    }

    #[test]
    fn a_remote_terminal_never_falls_back_to_a_local_shell() {
        let directory = std::env::temp_dir();
        let bridge = Bridge::new(
            "remote fixture",
            directory.join("unused-terminal-bridge.sock"),
            Box::new(|| unreachable!("terminal startup does not dial the Server")),
        );
        let error = spawn_shell(Some(&bridge), &directory, 1000, (80, 24))
            .err()
            .expect("a non-SSH remote must fail even when the folder exists locally");
        assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
    }
}

/// Where a printed address goes: a web address to the Thread's browser, a
/// path to the Panel's editor, a PR of the checkout's repository to its
/// native detail (`open_named_pr`). Other schemes a program names stay shut:
/// terminal output is not trusted with the OS's openers.
#[derive(Debug, PartialEq)]
enum Destination {
    Web(String),
    File(ferrite_pane::file_links::FileLink),
    PullRequest { number: u64, url: String },
}

impl Destination {
    fn of(address: &str, folder: Option<&std::path::Path>) -> Option<Self> {
        match LinkTarget::resolve(address, folder) {
            LinkTarget::External(url) if url.starts_with("http://") || url.starts_with("https://") => {
                Some(Self::Web(url))
            }
            LinkTarget::File(file) => Some(Self::File(file)),
            LinkTarget::RepoPullRequest { number, url } => Some(Self::PullRequest { number, url }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod destination_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn web_addresses_go_to_the_browser_and_paths_to_the_editor_at_their_line() {
        let folder = Path::new("/work/app");
        assert_eq!(
            Destination::of("http://localhost:5173/app", Some(folder)),
            Some(Destination::Web("http://localhost:5173/app".into()))
        );
        assert_eq!(
            Destination::of("https://example.com/docs", Some(folder)),
            Some(Destination::Web("https://example.com/docs".into()))
        );
        let Some(Destination::File(file)) = Destination::of("src/main.rs:12", Some(folder)) else {
            panic!("a path opens in the editor");
        };
        assert_eq!(file.path, folder.join("src").join("main.rs"));
        assert_eq!(file.line(), Some(12));
    }

    /// `gh pr create`'s printed address of the checkout's own repository
    /// reads natively, keeping the address as printed.
    #[test]
    fn a_pr_address_of_the_checkouts_repository_reads_natively() {
        let folder = std::env::temp_dir().join(format!("ferrite-terminal-pr-{}", std::process::id()));
        std::fs::create_dir_all(folder.join(".git")).unwrap();
        std::fs::write(
            folder.join(".git/config"),
            "[remote \"origin\"]\n\turl = git@github.com:owner/repo.git\n",
        )
        .unwrap();
        let printed = "https://github.com/owner/repo/pull/123/files";
        assert_eq!(
            Destination::of(printed, Some(&folder)),
            Some(Destination::PullRequest { number: 123, url: printed.into() })
        );
        assert_eq!(
            Destination::of("https://github.com/other/repo/pull/9", Some(&folder)),
            Some(Destination::Web("https://github.com/other/repo/pull/9".into()))
        );
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn other_schemes_stay_shut() {
        assert_eq!(Destination::of("mailto:a@b.c", None), None);
        assert_eq!(Destination::of("slack://open", None), None);
    }
}
