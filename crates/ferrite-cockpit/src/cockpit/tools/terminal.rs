//! The Terminal (Claude's, `terminal-open.png`, `terminal-tabs.png`,
//! `terminal-more-menu.png`, `terminal-menu.png`): a column of the dock
//! holding the user's shell in the Thread's folder — `ferrite-terminal`'s
//! grid — with nothing more than Claude's: tabs, copy and paste, and
//! `Attach as context`.
//!
//! - **Head:** `Terminal` and a `+`; with a second tab each is `Terminal N`
//!   with its own ×, the shown one on a pill. At the right `⋯`, expand and
//!   close. Closing a tab, or the column, ends its shell; a shell that exits
//!   on its own closes its tab.
//! - **`⋯`:** Cursor style ›, Blink cursor, Copy on select, Scrollback ›.
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
    name: SharedString,
    view: Entity<TerminalView>,
}

pub(crate) struct Terminals {
    tabs: Vec<Tab>,
    active: usize,
    /// Tabs made, which names the next.
    made: usize,
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
    /// The Terminal door opened for `thread`: its first shell starts.
    pub(in crate::cockpit) fn ensure_terminal(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if !self.tools.terminals.contains_key(&thread) {
            self.tools.terminals.insert(thread, Terminals { tabs: Vec::new(), active: 0, made: 0 });
            self.new_terminal_tab(thread, cx);
        }
    }

    /// `+`: one more shell in the Thread's folder, shown.
    fn new_terminal_tab(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(directory) = self.thread_path(thread) else {
            return;
        };
        let prefs = &self.tools.terminal_prefs;
        let (settings, scrollback) = (prefs.settings, prefs.scrollback);
        let Ok(shell) = Terminal::spawn(&directory, scrollback) else {
            return;
        };
        let view = cx.new(|cx| TerminalView::new(shell, settings, cx));
        cx.subscribe(&view, move |cockpit, view, event: &TerminalEvent, cx| {
            cockpit.terminal_event(thread, view, event, cx)
        })
        .detach();
        let Some(terminals) = self.tools.terminals.get_mut(&thread) else {
            return;
        };
        terminals.made += 1;
        terminals.tabs.push(Tab {
            name: format!("Terminal {}", terminals.made).into(),
            view,
        });
        terminals.active = terminals.tabs.len() - 1;
        self.tools.terminal_focus_wanted = true;
        cx.notify();
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
                if let Some(at) = self.terminal_index(thread, &view) {
                    self.close_terminal_tab(thread, at, cx);
                }
            }
            TerminalEvent::Title(_) => {}
        }
    }

    fn terminal_index(&self, thread: ThreadId, view: &Entity<TerminalView>) -> Option<usize> {
        self.tools
            .terminals
            .get(&thread)?
            .tabs
            .iter()
            .position(|tab| tab.view == *view)
    }

    /// The tab's ×: its shell ends. The last one takes the column with it.
    pub(in crate::cockpit) fn close_terminal_tab(&mut self, thread: ThreadId, at: usize, cx: &mut Context<Self>) {
        let Some(terminals) = self.tools.terminals.get_mut(&thread) else {
            return;
        };
        if at < terminals.tabs.len() {
            terminals.tabs.remove(at);
        }
        if terminals.tabs.is_empty() {
            self.press_tool(Tool::Terminal, cx);
            return;
        }
        terminals.active = terminals.active.min(terminals.tabs.len() - 1);
        self.tools.terminal_focus_wanted = true;
        cx.notify();
    }

    /// The column went: every shell of the Thread ends.
    pub(in crate::cockpit) fn end_terminals(&mut self, thread: ThreadId) {
        self.tools.terminals.remove(&thread);
    }

    fn active_terminal(&self, thread: ThreadId) -> Option<Entity<TerminalView>> {
        let terminals = self.tools.terminals.get(&thread)?;
        Some(terminals.tabs.get(terminals.active)?.view.clone())
    }

    /// Every frame: the shown shell takes the keyboard it was opened for.
    pub(in crate::cockpit) fn tend_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread() else {
            return;
        };
        if std::mem::take(&mut self.tools.terminal_focus_wanted) {
            if let Some(view) = self.active_terminal(thread) {
                let focus = view.focus_handle(cx);
                window.focus(&focus, cx);
            }
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

    /// The Terminal column's body: its head, then the shown shell.
    pub(in crate::cockpit) fn terminal_pane(&self, thread: ThreadId, cx: &mut Context<Self>) -> Div {
        let Some(terminals) = self.tools.terminals.get(&thread) else {
            return div().flex_1();
        };
        let body = terminals.tabs.get(terminals.active).map(|tab| tab.view.clone());
        div()
            .debug_selector(|| "terminal-pane".into())
            .flex()
            .flex_col()
            .size_full()
            .child(self.terminal_head(thread, terminals, cx))
            .child(div().flex_1().min_h_0().children(body))
    }

    fn terminal_head(&self, thread: ThreadId, terminals: &Terminals, cx: &mut Context<Self>) -> Div {
        use theme::*;
        let several = terminals.tabs.len() > 1;
        let tabs = terminals.tabs.iter().enumerate().map(|(at, tab)| {
            let active = several && at == terminals.active;
            let id = SharedString::from(format!("terminal-tab-{at}"));
            div()
                .id(id.clone())
                .debug_selector(move || id.to_string())
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(SPACE_1))
                .h(px(PANEL_TAB_H))
                .pl(px(PANEL_TAB_PAD_X))
                .pr(px(if several { SPACE_1 } else { PANEL_TAB_PAD_X }))
                .rounded(px(R_CONTROL))
                .text_size(px(FS_LABEL))
                .text_color(rgb(if active || !several { INK_STRONG } else { INK_SUBTLE }))
                .when(active, |tab| tab.bg(paint::FIELD))
                .when(several, |tab| tab.cursor_pointer())
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    if let Some(terminals) = view.tools.terminals.get_mut(&thread) {
                        terminals.active = at;
                        view.tools.terminal_focus_wanted = true;
                        cx.notify();
                    }
                }))
                .child(if several { tab.name.clone() } else { "Terminal".into() })
                .children(several.then(|| {
                    let key = SharedString::from(format!("terminal-tab-close-{at}"));
                    let selector = key.to_string();
                    div()
                        .id(key.clone())
                        .debug_selector(move || selector.clone())
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(PANEL_TAB_H - SPACE_1))
                        .rounded(px(R_CONTROL))
                        .hover_control(key)
                        .child(icons::icon(icons::CLOSE, ICON_BUTTON_GLYPH, INK_SUBTLE))
                        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            view.close_terminal_tab(thread, at, cx);
                        }))
                }))
        });
        let door = |key: &'static str, glyph: &'static str, tip: &'static str| {
            ferrite_pane::preview::head_door(key, key.into(), glyph)
                .tooltip(ferrite_ui::menu::tooltip(tip))
        };
        let expanded = self.panel.expanded;
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(WIN_CHROME_H))
            .px(px(SIDE_CHAT_INSET))
            .pr(px(super::super::panel::tools_inset() - PANEL_PAD_R))
            .child(
                div()
                    .debug_selector(|| "terminal-tabs".into())
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(SPACE_0_5))
                    .overflow_hidden()
                    .children(tabs)
                    .child(door("terminal-new", icons::PLUS, "New terminal").on_click(cx.listener(
                        move |view, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            view.new_terminal_tab(thread, cx);
                        },
                    ))),
            )
            .child(door("terminal-more", icons::ELLIPSIS, "More").on_click(cx.listener(
                move |view, event: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.open_context_menu(MenuTarget::TerminalMore(thread), event.position(), cx);
                },
            )))
            .child(
                door(
                    "terminal-expand",
                    if expanded { icons::MINIMIZE } else { icons::MAXIMIZE },
                    if expanded { "Collapse" } else { "Expand" },
                )
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.panel.expanded = !view.panel.expanded;
                    cx.notify();
                })),
            )
            .child(door("terminal-close", icons::CLOSE, "Close").on_click(cx.listener(
                move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.press_tool(Tool::Terminal, cx);
                },
            )))
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
    /// The names of the Thread's terminal tabs.
    pub(in crate::cockpit) fn terminal_tab_names(&self, thread: ThreadId) -> Vec<String> {
        self.tools
            .terminals
            .get(&thread)
            .map(|terminals| terminals.tabs.iter().map(|tab| tab.name.to_string()).collect())
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
