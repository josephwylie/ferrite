//! Ending a Unix shell without waiting on it. The PTY's own drop hangs the
//! shell up and waits for it before it closes the PTY, but a shell cannot
//! leave while output it wrote is unread, and an interactive one may ignore
//! the hangup until the PTY closes. So a dropped Terminal hands its loop,
//! its shell and a second handle on the PTY to a thread of their own: the
//! view never waits on a shell.

use std::fs::File;
use std::io::{self, Read};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use alacritty_terminal::event_loop::{EventLoop, State};
use alacritty_terminal::tty;

use super::Listener;

/// How long a shell gets to leave on a hangup before it is killed.
pub(super) const GRACE: Duration = Duration::from_secs(2);

/// The user's shell, run directly as a login shell. alacritty's macOS default
/// wraps it in `/usr/bin/login`, which is setuid root: while it runs, the
/// PTY's drop cannot signal it. The shell itself is ours to signal.
pub(super) fn login_shell() -> tty::Shell {
    let program = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    tty::Shell::new(program, vec!["-l".to_string()])
}

/// The shell's process id and a second handle on the PTY's master end, to
/// read what the shell writes after the loop has stopped.
pub(super) struct Hangup {
    shell: u32,
    master: File,
}

impl Hangup {
    pub(super) fn of(pty: &tty::Pty) -> io::Result<Self> {
        Ok(Self {
            shell: pty.child().id(),
            master: pty.file().try_clone()?,
        })
    }

    /// End the shell off this thread, once `io`, the loop told to shut
    /// down, has ended.
    pub(super) fn end(self, io: JoinHandle<(EventLoop<tty::Pty, Listener>, State)>) {
        std::thread::spawn(move || self.end_after(io));
    }

    /// Wait out the loop's end, then hang the shell up, reading what it
    /// still writes, and kill it if it stays; the PTY's drop then reaps it.
    fn end_after(mut self, io: JoinHandle<(EventLoop<tty::Pty, Listener>, State)>) {
        let Ok(ended) = io.join() else { return };
        let pid = self.shell as libc::pid_t;
        unsafe { libc::kill(pid, libc::SIGHUP) };
        let until = Instant::now() + GRACE;
        let mut killed = false;
        let mut unread = [0u8; 4096];
        while !has_exited(pid) {
            if !killed && Instant::now() >= until {
                unsafe { libc::kill(pid, libc::SIGKILL) };
                killed = true;
            }
            // The PTY is non-blocking: nothing to read is an error, and a wait.
            if !matches!(self.master.read(&mut unread), Ok(n) if n > 0) {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        drop(ended);
    }
}

/// Whether the child has exited, without reaping it: the PTY's drop does that.
fn has_exited(pid: libc::pid_t) -> bool {
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let seen = unsafe {
        libc::waitid(libc::P_PID, pid as libc::id_t, &mut info, libc::WEXITED | libc::WNOWAIT | libc::WNOHANG)
    };
    seen == 0 && info.si_signo == libc::SIGCHLD
}
