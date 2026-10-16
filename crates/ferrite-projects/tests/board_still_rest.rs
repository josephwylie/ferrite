//! [real programs] A board that loads its new version keeps its screen: the old one gives back the modes it took but
//! not the full-screen view, and the new one draws over the old frame without entering or clearing it (recorded on a
//! real terminal: the board stood a line off, then the screen was empty, then the board was back). The new version that
//! never comes up gives the terminal its plain screen and its cursor back.
use crate::common;
use common::*;
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::time::{Duration, Instant};

struct Term { master: std::fs::File, child: std::process::Child, out: Vec<u8>, answered: usize }
fn winsize(fd: i32, cols: u16, rows: u16) { let ws = libc::winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 }; unsafe { libc::ioctl(fd, libc::TIOCSWINSZ as _, &ws) }; }
/// The program at `exe`, in a pseudo-terminal of its own, as a terminal runs it.
fn start(exe: &Path, cwd: &Path) -> Term {
    let (mut m, mut s) = (0, 0);
    assert_eq!(unsafe { libc::openpty(&mut m, &mut s, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) }, 0);
    winsize(m, 100, 30);
    let slave = unsafe { OwnedFd::from_raw_fd(s) };
    let mut c = std::process::Command::new(exe);
    c.current_dir(cwd).env("TERM", "xterm-256color").env("COLORTERM", "truecolor").env("LANG", "C.UTF-8").env_remove("NO_COLOR")
        .env("HERDR_SOCKET_PATH", "/nonexistent").env("HERDR_BIN_PATH", "/bin/false").env("HERDR_PROJECTS_STAMP_MS", "100")
        .stdin(slave.try_clone().unwrap()).stdout(slave.try_clone().unwrap()).stderr(slave);
    unsafe { c.pre_exec(|| { libc::setsid(); libc::ioctl(0, libc::TIOCSCTTY as _, 0); Ok(()) }); }
    let child = c.spawn().unwrap();
    unsafe { libc::fcntl(m, libc::F_SETFL, libc::fcntl(m, libc::F_GETFL) | libc::O_NONBLOCK) };
    Term { master: unsafe { std::fs::File::from_raw_fd(m) }, child, out: vec![], answered: 0 }
}
impl Term {
    /// What came out in the next `ms`, answering the colour questions a terminal answers.
    fn read(&mut self, ms: u64) {
        let until = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < until {
            let mut b = [0u8; 65536];
            match self.master.read(&mut b) { Ok(n) if n > 0 => self.out.extend_from_slice(&b[..n]), _ => std::thread::sleep(Duration::from_millis(2)) }
        }
        if String::from_utf8_lossy(&self.out[self.answered..]).contains("\x1b]10;?") { let _ = self.master.write_all(b"\x1b]10;rgb:cdcd/d6d6/f4f4\x1b\\\x1b]11;rgb:1e1e/1e1e/2e2e\x1b\\\x1b[?62;22c"); }
        self.answered = self.out.len();
    }
    /// Read until `done` holds for what came out since `from`, or `ms` have passed.
    fn until(&mut self, from: usize, ms: u64, done: impl Fn(&str) -> bool) -> bool { let end = Instant::now() + Duration::from_millis(ms); while Instant::now() < end { self.read(20); if done(&String::from_utf8_lossy(&self.out[from..])) { return true; } } false }
    fn ended(&mut self, ms: u64) -> Option<std::process::ExitStatus> { let end = Instant::now() + Duration::from_millis(ms); while Instant::now() < end { if let Some(s) = self.child.try_wait().unwrap() { self.read(50); return Some(s); } self.read(20); } None }
}
fn plain(s: &str) -> String { regex::Regex::new(r"\x1b\[[0-9;?<>=]*[A-Za-z]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)").unwrap().replace_all(s, "").into_owned() }
/// A copy of the program in a folder of the world's own: the one the test changes under the open board.
fn copy(w: &World) -> std::path::PathBuf {
    let exe = w.base.join("bin").join("herdr-projects");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_ferrite-projects"), &exe).unwrap();
    exe
}
/// The program's file changed (a merge into the checkout it runs from): its time moves on.
fn touch(exe: &Path) { let f = std::fs::File::open(exe).unwrap(); f.set_modified(std::time::SystemTime::now() + Duration::from_secs(60)).unwrap(); }

#[test]
fn real_programs_a_board_that_loads_its_new_version_keeps_its_screen_nothing_leaves_the_full_screen_view_nothing_clears_it() {
    let w = world();
    w.add("A task to see");
    let exe = copy(&w);
    let mut t = start(&exe, &w.root);
    assert!(t.until(0, 5000, |o| plain(o).contains("A task to see")), "the board drew nothing: {:?}", plain(&String::from_utf8_lossy(&t.out)));
    // A board opened by you starts as always: it enters its full-screen view.
    assert!(String::from_utf8_lossy(&t.out).contains("\x1b[?1049h"));
    t.read(300);
    let from = t.out.len();
    touch(&exe);
    // The board sees the change, waits for it to settle, and loads the new version: the old board gives the modes back
    // (bracketed paste), the new one takes them again and draws.
    assert!(t.until(from, 25000, |o| o.find("\x1b[?2004l").map(|i| o[i..].contains("\x1b[?2004h") && plain(&o[i..]).contains("A task to see")).unwrap_or(false)), "the board did not load its new version: {:?}", String::from_utf8_lossy(&t.out[from..]));
    t.read(300);
    let reload = String::from_utf8_lossy(&t.out[from..]).into_owned();
    // Nothing leaves the full-screen view, nothing enters it again, nothing clears it: the first frame is drawn over the old one.
    for (what, seq) in [("the full-screen view was left", "\x1b[?1049l"), ("the full-screen view was entered again", "\x1b[?1049h"), ("the screen was cleared", "\x1b[2J")] { assert!(!reload.contains(seq), "{what}: {reload:?}"); }
    assert!(t.child.try_wait().unwrap().is_none(), "the board is still open");
    // It still takes keys, and closing gives the terminal back as it was.
    let before = t.out.len();
    let _ = t.master.write_all(b"\x03"); t.read(150); let _ = t.master.write_all(b"\x03");
    let status = t.ended(5000).expect("the board did not close");
    assert!(status.success(), "{status:?}");
    let tail = String::from_utf8_lossy(&t.out[before..]).into_owned();
    assert!(tail.contains("\x1b[?1049l") && tail.contains("\x1b[?25h"), "{tail:?}");
}

#[test]
fn real_programs_a_new_version_that_never_comes_up_gives_the_terminal_its_plain_screen_and_its_cursor_back() {
    let w = world();
    w.add("A task to see");
    let exe = copy(&w);
    let mut t = start(&exe, &w.root);
    assert!(t.until(0, 5000, |o| plain(o).contains("A task to see")), "the board drew nothing");
    t.read(300);
    let from = t.out.len();
    // The program's file is gone (moved away mid-update): the board tries the new version, which cannot start.
    std::fs::remove_file(&exe).unwrap();
    let status = t.ended(25000).expect("the board did not try its new version");
    assert!(!status.success());
    let tail = String::from_utf8_lossy(&t.out[from..]).into_owned();
    let back = tail.rfind("\x1b[?1049l").expect("the plain screen was not given back");
    assert!(tail[back..].contains("\x1b[?25h") || tail[..back].rfind("\x1b[?25h").is_some(), "the cursor was not given back: {tail:?}");
    assert!(plain(&tail).contains("could not start its new version"), "{tail:?}");
}
