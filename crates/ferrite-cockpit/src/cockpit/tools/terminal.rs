//! The Terminal in the window: the root's side of
//! `ferrite_tools::terminal::TerminalsView`, every Thread's shells, which
//! the Panel draws as the body of whichever shell it shows. The view reads
//! the shell shown through a `Source` the root makes over itself
//! (`terminal_inputs`); what it asks (a menu, Attach, an address opened, a
//! shell started again, a tab gone) it says as a `TerminalsEvent`. Starting
//! a shell needs the Thread's folder and the machine, so the root starts it
//! and hands it to the view.

use ferrite_core::replies::Reply;
use ferrite_pane::link_target::LinkTarget;
use ferrite_tools::terminal::{TerminalInputs, TerminalsEvent, TerminalsView, Verb};
use ferrite_ui::source::Source;
use gpui::{AppContext as _, Context, Entity, Window};

use crate::cockpit::context_menu::MenuTarget;
use crate::cockpit::menu_button::{Door, Edge};
use crate::cockpit::surfaces::SurfaceId;
use crate::cockpit::CockpitView;
use ferrite_core::ThreadKey;

impl CockpitView {
    /// The window's shells, heard from their making.
    pub(in crate::cockpit) fn new_terminals(cx: &mut Context<Self>) -> Entity<TerminalsView> {
        let inputs = Source::of(&cx.entity(), |root: &Self, _: &(), cx| root.terminal_inputs(cx));
        let terminals = cx.new(|_| TerminalsView::new(inputs));
        cx.subscribe(&terminals, Self::on_terminals_event).detach();
        terminals
    }

    /// One more shell in the Thread's folder, its Surface opened and
    /// shown (⌘J's first, the `+` menu's Terminal).
    pub(in crate::cockpit) fn new_terminal_surface(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        let Some(directory) = self.thread_path(thread, cx) else {
            return;
        };
        let machine = self.cockpit.read(cx).bridge(thread.machine()).cloned();
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
            TerminalsEvent::More { thread } => {
                self.toggle_door(Door::new(MenuTarget::TerminalMore(*thread), Edge::Right), cx)
            }
            TerminalsEvent::Open { thread, address } => self.open_terminal_address(*thread, address, cx),
            TerminalsEvent::Retry { thread, number } => {
                let Some(directory) = self.thread_path(*thread, cx) else {
                    return;
                };
                let machine = self.cockpit.read(cx).bridge(thread.machine()).cloned();
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
    fn open_terminal_address(&mut self, thread: ThreadKey, address: &str, cx: &mut Context<Self>) {
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
    fn shown_terminal(&self, thread: ThreadKey) -> Option<u64> {
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
                .is_some_and(|number| {
                    self.terminals.read(cx).holds_focus(thread, number, window, cx)
                })
        })
    }

    /// A Terminal menu row, on the shell `thread`'s Panel shows.
    pub(in crate::cockpit) fn run_terminal_verb(&mut self, thread: ThreadKey, verb: Verb, cx: &mut Context<Self>) {
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
            more: self.doors().door(MenuTarget::TerminalMore(thread)),
        })
    }
}

#[cfg(feature = "visual-reference")]
impl CockpitView {
    /// A reference scene runs `command` in the shown shell and gives it a
    /// moment to answer.
    pub(in crate::cockpit) fn fixture_terminal_command(
        &mut self,
        thread: ThreadKey,
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
