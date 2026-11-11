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

use ferrite_pane::quote::Reply;
use ferrite_terminal::{Cursor, Settings, Terminal, TerminalEvent, TerminalView};
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

struct Tab {
    /// `Terminal N`'s N, and its Surface's (`SurfaceId::Terminal`).
    number: u64,
    view: Entity<TerminalView>,
}

#[derive(Default)]
pub(crate) struct Terminals {
    tabs: Vec<Tab>,
    /// Shells started, which numbers the next.
    made: u64,
}

/// What the `⋯` menu sets, for every terminal of the window.
pub(crate) struct Prefs {
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

impl CockpitView {
    /// One more shell in the Thread's folder, its Surface opened and
    /// shown (⌘J's first, the `+` menu's Terminal).
    pub(in crate::cockpit) fn new_terminal_surface(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(directory) = self.thread_path(thread) else {
            return;
        };
        let prefs = &self.tools.terminal_prefs;
        let (settings, scrollback) = (prefs.settings, prefs.scrollback);
        // The column's width in cells, reckoned from the face's usual advance.
        let columns = ((theme::TERMINAL_W - 2. * theme::TERMINAL_PAD_X)
            / (theme::FS_TERMINAL * theme::CODE_ADVANCE)) as u16;
        let lines = (theme::TERMINAL_FIRST_ROWS) as u16;
        let Ok(shell) = Terminal::spawn(&directory, scrollback, (columns, lines)) else {
            return;
        };
        let view = cx.new(|cx| TerminalView::new(shell, settings, cx));
        cx.subscribe(&view, move |cockpit, view, event: &TerminalEvent, cx| {
            cockpit.terminal_event(thread, view, event, cx)
        })
        .detach();
        let terminals = self.tools.terminals.entry(thread).or_default();
        terminals.made += 1;
        let number = terminals.made;
        terminals.tabs.push(Tab { number, view });
        self.tools.terminal_focus_wanted = true;
        self.upsert_surface(thread, SurfaceId::Terminal(number), cx);
    }

    fn terminal_event(
        &mut self,
        thread: ThreadId,
        view: Entity<TerminalView>,
        event: &TerminalEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TerminalEvent::Attach(text) => {
                if let Some(index) = self.pane_for(thread) {
                    self.focus_pane(index);
                    let reply = Reply::new(text.trim(), "");
                    self.panes[index]
                        .composer
                        .update(cx, |composer, cx| composer.reply(reply, cx));
                    cx.notify();
                }
            }
            TerminalEvent::ContextMenu(at) => {
                // The rows read what is selected as the menu opens.
                self.tools.terminal_selected = view.read(cx).selection().is_some();
                self.open_context_menu(MenuTarget::Terminal(thread), *at, cx)
            }
            TerminalEvent::Exited => {
                if let Some(number) = self.terminal_number(thread, &view) {
                    self.end_terminal(thread, number, cx);
                    self.forget_surface(thread, &SurfaceId::Terminal(number));
                }
            }
            TerminalEvent::Title(_) => {}
        }
    }

    fn terminal_number(&self, thread: ThreadId, view: &Entity<TerminalView>) -> Option<u64> {
        self.tools
            .terminals
            .get(&thread)?
            .tabs
            .iter()
            .find(|tab| tab.view == *view)
            .map(|tab| tab.number)
    }

    /// Shell `number` of `thread` ends: its tab closed, or it exited.
    pub(in crate::cockpit) fn end_terminal(&mut self, thread: ThreadId, number: u64, cx: &mut Context<Self>) {
        let Some(terminals) = self.tools.terminals.get_mut(&thread) else {
            return;
        };
        terminals.tabs.retain(|tab| tab.number != number);
        if terminals.tabs.is_empty() {
            self.tools.terminals.remove(&thread);
        }
        cx.notify();
    }

    fn terminal_of(&self, thread: ThreadId, number: u64) -> Option<Entity<TerminalView>> {
        let terminals = self.tools.terminals.get(&thread)?;
        let tab = terminals.tabs.iter().find(|tab| tab.number == number)?;
        Some(tab.view.clone())
    }

    /// The shell `thread`'s Panel shows, if it shows one.
    fn active_terminal(&self, thread: ThreadId) -> Option<Entity<TerminalView>> {
        match self.active_surface(thread)? {
            SurfaceId::Terminal(number) => self.terminal_of(thread, number),
            _ => None,
        }
    }

    /// Whether the shown shell has the keyboard. The focus pump
    /// (`render`'s) leaves it alone, else it would hand the keyboard back to
    /// the Composer every frame.
    pub(in crate::cockpit) fn terminal_holds_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.focused_thread()
            .and_then(|thread| self.active_terminal(thread))
            .is_some_and(|view| view.focus_handle(cx).is_focused(window))
    }

    /// Every frame: the shown shell takes the keyboard it was opened for,
    /// once it has been drawn. A handle not yet in the window's tree cannot
    /// be focused; the wish waits a frame for it.
    pub(in crate::cockpit) fn tend_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tools.terminal_focus_wanted {
            return;
        }
        let Some(view) = self.focused_thread().and_then(|thread| self.active_terminal(thread)) else {
            self.tools.terminal_focus_wanted = false;
            return;
        };
        if view.read(cx).is_drawn() {
            self.tools.terminal_focus_wanted = false;
            let focus = view.focus_handle(cx);
            window.focus(&focus, cx);
        } else {
            cx.notify();
        }
    }

    pub(in crate::cockpit) fn run_terminal_verb(&mut self, thread: ThreadId, verb: Verb, cx: &mut Context<Self>) {
        let Some(view) = self.active_terminal(thread) else {
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
            Verb::ToggleBlink => self.tools.terminal_prefs.settings.blink ^= true,
            Verb::ToggleCopyOnSelect => self.tools.terminal_prefs.settings.copy_on_select ^= true,
            Verb::Cursor(cursor) => self.tools.terminal_prefs.settings.cursor = cursor,
            Verb::Scrollback(lines) => self.tools.terminal_prefs.scrollback = lines,
        }
        let prefs = &self.tools.terminal_prefs;
        let (settings, scrollback) = (prefs.settings, prefs.scrollback);
        for tab in self.tools.terminals.get(&thread).into_iter().flat_map(|t| &t.tabs) {
            tab.view.update(cx, |view, cx| view.configure(settings, scrollback, cx));
        }
        cx.notify();
    }

    /// The right-click menu's rows.
    pub(in crate::cockpit) fn terminal_context_rows(&self) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let selected = self.tools.terminal_selected;
        let row = |label: &'static str, chord: &'static str, verb: Verb, enabled: bool| {
            let mut item = menu::Item::new(label);
            if !chord.is_empty() {
                item = item.shortcut(chord);
            }
            item.disabled = !enabled;
            Some((item, MenuVerb::Terminal(verb)))
        };
        let mut rows = Vec::new();
        if selected {
            rows.push(row("Attach selection as context", "shift-cmd-L", Verb::Attach, true));
            rows.push(None);
        }
        rows.extend([
            row("Copy", "cmd-C", Verb::Copy, selected),
            row("Paste", "cmd-V", Verb::Paste, true),
            row("Select all", "cmd-A", Verb::SelectAll, true),
            None,
            row("Clear terminal", "", Verb::Clear, true),
        ]);
        rows
    }

    /// The Terminal `⋯` menu's rows.
    pub(in crate::cockpit) fn terminal_more_rows(&self) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let prefs = &self.tools.terminal_prefs;
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

    pub(in crate::cockpit) fn terminal_sub_rows(&self, sub: Sub) -> Vec<(menu::Item, MenuVerb)> {
        let prefs = &self.tools.terminal_prefs;
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

    /// Shell `number`'s Surface: its row (`⋯`), then the grid.
    pub(in crate::cockpit) fn terminal_pane(&self, thread: ThreadId, number: u64, cx: &mut Context<Self>) -> Div {
        let body = self.terminal_of(thread, number);
        let more = ferrite_pane::preview::head_door("terminal-more", "terminal-more".into(), icons::ELLIPSIS)
            .tooltip(ferrite_ui::menu::tooltip("More"))
            .on_click(cx.listener(move |view, event: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.open_context_menu(MenuTarget::TerminalMore(thread), event.position(), cx);
            }));
        div()
            .debug_selector(|| "terminal-pane".into())
            .flex()
            .flex_col()
            .size_full()
            .child(super::surface_bar().child(div().flex_1()).child(more))
            .child(div().flex_1().min_h_0().children(body))
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
        if let Some(view) = self.active_terminal(thread) {
            view.update(cx, |view, _| view.write(command));
            std::thread::sleep(std::time::Duration::from_millis(1500));
        }
    }
}

#[cfg(test)]
impl CockpitView {
    /// The numbers of the Thread's shells.
    pub(in crate::cockpit) fn terminal_numbers(&self, thread: ThreadId) -> Vec<u64> {
        self.tools
            .terminals
            .get(&thread)
            .map(|terminals| terminals.tabs.iter().map(|tab| tab.number).collect())
            .unwrap_or_default()
    }

    /// The shown shell's view.
    pub(in crate::cockpit) fn terminal_view(&self, thread: ThreadId) -> Option<Entity<TerminalView>> {
        self.active_terminal(thread)
    }
}

#[cfg(test)]
mod tests {
    use super::scrollback_label;

    #[test]
    fn scrollback_reads_with_thousands_separators() {
        assert_eq!(scrollback_label(1_000), "1,000 lines");
        assert_eq!(scrollback_label(10_000), "10,000 lines");
        assert_eq!(scrollback_label(100_000), "100,000 lines");
    }
}
