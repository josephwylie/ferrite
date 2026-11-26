//! A child the Server runs for the operator, held with everything it
//! starts: a process group of its own on Unix, a job on Windows, so a stop
//! reaches all of it. Dropped, it is stopped.

use std::io::{self, Read};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(25);
/// How long a stopped run's output is waited for: whatever it started
/// that outlived the stop may still hold its pipes.
pub(crate) const DRAIN: Duration = Duration::from_secs(1);

pub(crate) struct ProcessTree {
    child: Child,
    /// Its exit was read: its pid, and its group's, may be another's now.
    reaped: bool,
    /// A `.cmd` shim's child is cmd.exe with the tool beneath it: only a
    /// job reaches the tree.
    #[cfg(windows)]
    job: Option<crate::providers::job::SessionJob>,
}

impl ProcessTree {
    pub(crate) fn spawn(mut command: Command) -> io::Result<Self> {
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let child = command.spawn()?;
        #[cfg(windows)]
        let job = crate::providers::job::SessionJob::assign(&child).ok();
        Ok(Self {
            child,
            reaped: false,
            #[cfg(windows)]
            job,
        })
    }

    /// The child's pipes, each one the command piped.
    pub(crate) fn pipes(&mut self) -> (Option<ChildStdin>, Option<ChildStdout>, Option<ChildStderr>) {
        (self.child.stdin.take(), self.child.stdout.take(), self.child.stderr.take())
    }

    /// How the child ended, once it has.
    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let status = self.child.try_wait()?;
        self.reaped |= status.is_some();
        Ok(status)
    }

    /// Stop the child and everything it started, and reap it: its process
    /// group on Unix, its job on Windows, or the child alone when it could
    /// not be put in one. A child that already ended is left be.
    pub(crate) fn stop(&mut self) {
        if self.reaped {
            return;
        }
        #[cfg(unix)]
        // SAFETY: a plain syscall on the group `process_group(0)` made,
        // whose id is the child's pid; the child is not yet reaped.
        unsafe {
            libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL)
        };
        #[cfg(windows)]
        match &self.job {
            Some(job) => job.terminate(),
            None => drop(self.child.kill()),
        }
        let _ = self.child.wait();
        self.reaped = true;
    }
}

impl Drop for ProcessTree {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Run `command` to its end, with nothing on its input: what it wrote, or
/// `TimedOut` once `deadline` passed, when it is stopped with everything it
/// started.
pub(crate) fn within(mut command: Command, deadline: Duration) -> io::Result<Output> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut tree = ProcessTree::spawn(command)?;
    let (_, stdout, stderr) = tree.pipes();
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        let (tx, rx) = mpsc::channel();
        if let Some(mut pipe) = pipe {
            thread::spawn(move || {
                let mut bytes = Vec::new();
                let _ = pipe.read_to_end(&mut bytes);
                let _ = tx.send(bytes);
            });
        }
        rx
    };
    let stdout = read(stdout.map(|pipe| Box::new(pipe) as Box<dyn Read + Send>));
    let stderr = read(stderr.map(|pipe| Box::new(pipe) as Box<dyn Read + Send>));
    let ends = Instant::now() + deadline;
    let status = loop {
        if let Some(status) = tree.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= ends {
            tree.stop();
            break None;
        }
        thread::sleep(POLL);
    };
    let drained = Instant::now() + DRAIN;
    let output = |rx: mpsc::Receiver<Vec<u8>>| rx.recv_timeout(drained.saturating_duration_since(Instant::now())).unwrap_or_default();
    let (stdout, stderr) = (output(stdout), output(stderr));
    match status {
        Some(status) => Ok(Output { status, stdout, stderr }),
        None => Err(io::Error::new(io::ErrorKind::TimedOut, "it did not finish in its time")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shell that starts a command that never ends, as a tool waiting on
    /// a prompt does, and holds its pipes open.
    fn never_ending() -> Command {
        if cfg!(windows) {
            let mut command = Command::new("cmd");
            command.args(["/C", "ping -n 60 127.0.0.1 & echo done"]);
            command
        } else {
            let mut command = Command::new("sh");
            command.args(["-c", "sleep 60; echo done"]);
            command
        }
    }

    #[test]
    fn a_run_that_does_not_end_is_stopped_with_all_it_started() {
        let started = Instant::now();
        let error = within(never_ending(), Duration::from_millis(500)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(
            started.elapsed() < Duration::from_millis(500) + DRAIN,
            "the command it started held the pipes: it was not stopped ({:?})",
            started.elapsed()
        );

        let mut echo = Command::new(if cfg!(windows) { "cmd" } else { "sh" });
        echo.args(if cfg!(windows) { ["/C", "echo said"] } else { ["-c", "echo said"] });
        let output = within(echo, Duration::from_secs(30)).unwrap();
        assert!(output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "said", "{output:?}");
    }
}
