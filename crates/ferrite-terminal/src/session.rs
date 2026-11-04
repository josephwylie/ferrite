//! One shell: `alacritty_terminal`'s emulator (`Term`) fed by its PTY event
//! loop on a thread of its own, with no window attached. The user's login
//! shell starts in the Thread's folder (the PTY's default shell on this
//! platform is the user's, as a login shell). Its events — new output, a
//! title, the shell's end — come out of `take_events`. Dropping the
//! session shuts the loop down, which ends the shell.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::thread::JoinHandle;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg, State};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty;
use std::sync::mpsc::{channel, Receiver, Sender};

/// Hands the emulator's events to whoever holds the receiver.
#[derive(Clone)]
pub(crate) struct Listener(Sender<Event>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let _ = self.0.send(event);
    }
}

pub struct Terminal {
    term: Arc<FairMutex<Term<Listener>>>,
    sender: EventLoopSender,
    events: Option<Receiver<Event>>,
    io: Option<JoinHandle<(EventLoop<tty::Pty, Listener>, State)>>,
    columns: u16,
    lines: u16,
}

impl Terminal {
    /// Start the user's login shell in `directory`, keeping `scrollback`
    /// lines of history, at `columns` by `lines` — the pane's size as the
    /// caller reckons it, so the shell's first prompt is drawn to fit; the
    /// view sets the measured size on its first frame.
    pub fn spawn(directory: &Path, scrollback: usize, (columns, lines): (u16, u16)) -> io::Result<Self> {
        let (events_tx, events) = channel();
        let listener = Listener(events_tx);
        let config = Config {
            scrolling_history: scrollback,
            ..Config::default()
        };
        let term = Arc::new(FairMutex::new(Term::new(
            config,
            &TermSize::new(columns as usize, lines as usize),
            listener.clone(),
        )));
        let options = tty::Options {
            working_directory: Some(directory.to_path_buf()),
            env: HashMap::from([
                ("TERM".to_string(), "xterm-256color".to_string()),
                ("COLORTERM".to_string(), "truecolor".to_string()),
            ]),
            ..tty::Options::default()
        };
        let size = WindowSize {
            num_lines: lines,
            num_cols: columns,
            cell_width: 1,
            cell_height: 1,
        };
        let pty = tty::new(&options, size, 0)?;
        let event_loop = EventLoop::new(term.clone(), listener, pty, options.drain_on_exit, false)?;
        let sender = event_loop.channel();
        let io = event_loop.spawn();
        Ok(Self {
            term,
            sender,
            events: Some(events),
            io: Some(io),
            columns,
            lines,
        })
    }

    /// The emulator's events, once: the view that draws this shell holds them.
    pub(crate) fn take_events(&mut self) -> Option<Receiver<Event>> {
        self.events.take()
    }

    /// Read or change the emulator under its lock.
    pub(crate) fn with<R>(&self, act: impl FnOnce(&mut Term<Listener>) -> R) -> R {
        act(&mut self.term.lock())
    }

    /// Bytes for the shell's stdin.
    pub(crate) fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        let _ = self.sender.send(Msg::Input(bytes.into()));
    }

    /// Text pasted: bracketed where the shell asked for it, and with line
    /// ends as the terminal sends them (`\r`).
    pub(crate) fn paste(&self, text: &str) {
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        if self.with(|term| term.mode().contains(TermMode::BRACKETED_PASTE)) {
            self.write(format!("\x1b[200~{text}\x1b[201~").into_bytes());
        } else {
            self.write(text.into_bytes());
        }
    }

    /// The grid's size in cells, and the cell's in pixels, as the pane
    /// measures them.
    pub(crate) fn resize(&mut self, columns: u16, lines: u16, cell_width: u16, cell_height: u16) {
        let (columns, lines) = (columns.max(2), lines.max(1));
        if (columns, lines) == (self.columns, self.lines) {
            return;
        }
        self.columns = columns;
        self.lines = lines;
        self.with(|term| term.resize(TermSize::new(columns as usize, lines as usize)));
        let _ = self.sender.send(Msg::Resize(WindowSize {
            num_lines: lines,
            num_cols: columns,
            cell_width,
            cell_height,
        }));
    }

    pub(crate) fn size(&self) -> (u16, u16) {
        (self.columns, self.lines)
    }

    /// Scroll the view through the scrollback.
    pub(crate) fn scroll(&self, scroll: Scroll) {
        self.with(|term| term.scroll_display(scroll));
    }

    /// Begin a selection of `kind` at the cell under a press.
    pub(crate) fn select_from(&self, kind: SelectionType, at: Point, side: Side) {
        self.with(|term| term.selection = Some(Selection::new(kind, at, side)));
    }

    /// Grow the selection to the cell under the pointer.
    pub(crate) fn select_to(&self, at: Point, side: Side) {
        self.with(|term| {
            if let Some(selection) = term.selection.as_mut() {
                selection.update(at, side);
            }
        });
    }

    pub(crate) fn clear_selection(&self) {
        self.with(|term| term.selection = None);
    }

    /// Select every line, scrollback included.
    pub(crate) fn select_all(&self) {
        self.with(|term| {
            let top = Point::new(term.topmost_line(), Column(0));
            let bottom = Point::new(term.bottommost_line(), term.last_column());
            let mut selection = Selection::new(SelectionType::Simple, top, Side::Left);
            selection.update(bottom, Side::Right);
            term.selection = Some(selection);
        });
    }

    /// The selected text, if any.
    pub(crate) fn selected_text(&self) -> Option<String> {
        self.with(|term| term.selection_to_string()).filter(|text| !text.is_empty())
    }

    /// Clear the screen and the scrollback, and ask the shell to redraw its
    /// prompt (the shell's own `clear`: ^L).
    pub(crate) fn clear(&self) {
        self.with(|term| {
            term.selection = None;
            term.grid_mut().clear_history();
        });
        self.write(&b"\x0c"[..]);
    }

    /// Everything the grid holds, lines trimmed of their trailing blanks:
    /// what the tests read.
    #[cfg(test)]
    pub(crate) fn text(&self) -> String {
        self.with(|term| {
            let (start, end) = (
                Point::new(term.topmost_line(), Column(0)),
                Point::new(term.bottommost_line(), term.last_column()),
            );
            term.bounds_to_string(start, end)
        })
    }

    /// The cell at screen row `line` (from the top), column `column`.
    pub(crate) fn point(line: usize, column: usize, display_offset: usize) -> Point {
        Point::new(Line(line as i32 - display_offset as i32), Column(column))
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.sender.send(Msg::Shutdown);
        // The loop ends its thread on the message; the PTY's drop hangs the
        // shell up.
        drop(self.io.take());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait_for(terminal: &Terminal, needle: &str) -> String {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            let text = terminal.text();
            if text.contains(needle) || Instant::now() > until {
                return text;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrite-terminal-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    #[test]
    fn the_shell_starts_in_the_folder_and_runs_what_is_written() {
        let dir = scratch("cwd");
        let terminal = Terminal::spawn(&dir, 1000, (80, 24)).unwrap();
        terminal.write(&b"pwd; echo ferrite-$((6*7))\r"[..]);
        let text = wait_for(&terminal, "ferrite-42");
        assert!(text.contains("ferrite-42"), "{text}");
        assert!(text.contains(dir.to_str().unwrap()), "in its folder: {text}");
    }

    #[test]
    fn a_resize_reaches_the_shell() {
        let terminal_dir = scratch("size");
        let mut terminal = Terminal::spawn(&terminal_dir, 1000, (80, 24)).unwrap();
        terminal.resize(100, 30, 8, 16);
        assert_eq!(terminal.size(), (100, 30));
        terminal.write(&b"stty size\r"[..]);
        let text = wait_for(&terminal, "30 100");
        assert!(text.contains("30 100"), "{text}");
    }

    #[test]
    fn a_selection_copies_its_text_and_paste_types_it() {
        let dir = scratch("select");
        let terminal = Terminal::spawn(&dir, 1000, (80, 24)).unwrap();
        terminal.paste("echo hello-selected\n");
        let text = wait_for(&terminal, "\nhello-selected");
        assert!(text.contains("\nhello-selected"), "the shell ran it: {text}");
        terminal.select_all();
        let selected = terminal.selected_text().expect("a selection");
        assert!(selected.contains("hello-selected"));
        terminal.clear_selection();
        assert_eq!(terminal.selected_text(), None);
    }

    #[test]
    fn dropping_it_ends_the_shell() {
        let dir = scratch("kill");
        let terminal = Terminal::spawn(&dir, 1000, (80, 24)).unwrap();
        terminal.write(&b"echo $$ > pid.txt\r"[..]);
        let pid_file = dir.join("pid.txt");
        let until = Instant::now() + Duration::from_secs(10);
        while !pid_file.exists() || std::fs::read_to_string(&pid_file).unwrap_or_default().trim().is_empty() {
            assert!(Instant::now() < until, "the shell never wrote its pid");
            std::thread::sleep(Duration::from_millis(20));
        }
        let pid: i32 = std::fs::read_to_string(&pid_file).unwrap().trim().parse().unwrap();
        drop(terminal);
        let alive = || std::process::Command::new("kill").args(["-0", &pid.to_string()]).status().unwrap().success();
        let until = Instant::now() + Duration::from_secs(10);
        while alive() {
            assert!(Instant::now() < until, "the shell outlived its terminal");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
