//! A Provider's CLI signed in on the machine that runs it (ADR 0051), by
//! the CLI's own sign-in: `claude auth login` shows a page and reads back
//! the code that page gives, and `codex login --device-auth` shows a page
//! and a code to enter there, then waits for the operator's approval.
//! Nothing comes from another computer: the machine gets a sign-in of its
//! own.
//!
//! One thread runs each sign-in, from finding the CLI to its end. What the
//! CLI writes and the step that finishes it reach that thread on one
//! channel. The CLI's process tree sits in a slot the handle shares, so
//! dropping the handle stops it at once, whatever the thread is doing.

use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use ferrite_core::protocol::Withheld;
use ferrite_core::provider::Provider;

use crate::providers::clis::Clis;
use crate::providers::process_tree::{within, ProcessTree, DRAIN};

/// How long a CLI has to say where to sign in.
const PAGE: Duration = Duration::from_secs(60);
/// How long a sign-in may wait on the operator from its page on: past a
/// device code's own 15 minutes.
const FINISH: Duration = Duration::from_secs(20 * 60);
/// How long a status command has to answer.
const STATUS: Duration = Duration::from_secs(20);
/// How often the thread looks at the CLI between what it writes.
const TICK: Duration = Duration::from_millis(50);
/// How long a CLI that answered a code with a line, and runs on, is quiet
/// before it is taken to be asking for the code again: Claude refuses a
/// code cut short at once, then waits.
const ASKING: Duration = Duration::from_secs(3);
/// What a pipe is read in: Claude's prompt ends no line.
const CHUNK: usize = 4096;
/// What a step hears when the sign-in ends before answering it.
const STOPPED: &str = "The sign-in stopped";

/// Where a started sign-in goes on, as its CLI said it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub url: String,
    /// What the operator enters on the page, when the CLI shows a code.
    pub user_code: Option<Withheld>,
    /// The page shows a code that the CLI reads back (`SignIn::finish`).
    pub paste: bool,
}

fn login_args(provider: Provider) -> &'static [&'static str] {
    match provider {
        Provider::Claude => &["auth", "login"],
        // Plain `codex login` waits on a callback to localhost, which only
        // a browser on the machine itself could reach.
        Provider::Codex => &["login", "--device-auth"],
    }
}

fn status_args(provider: Provider) -> &'static [&'static str] {
    match provider {
        Provider::Claude => &["auth", "status", "--json"],
        Provider::Codex => &["login", "status"],
    }
}

/// The CLI at `program`, run as an agent's child is, its colours off.
fn cli(program: &Path, args: &[&str]) -> Command {
    let mut command = ferrite_core::spawn::agent(program);
    command.args(args).env("NO_COLOR", "1");
    command
}

/// Whether `provider`'s CLI at `program` is signed in: none when it cannot
/// be run or does not answer in its time.
pub fn status(provider: Provider, program: &Path) -> Option<bool> {
    let output = within(cli(program, status_args(provider)), STATUS).ok()?;
    match provider {
        Provider::Claude => serde_json::from_slice::<serde_json::Value>(&output.stdout).ok()?["loggedIn"].as_bool(),
        Provider::Codex => Some(output.status.success()),
    }
}

/// Where the sign-in goes on, once `said`, all the CLI has written so far,
/// names it in whole lines: Claude's first address, whose page gives back a
/// code; Codex's first address and the one-time code after it.
pub fn page(provider: Provider, said: &str) -> Option<Page> {
    let plain = plain(said);
    let whole = plain.rsplit_once('\n').map_or("", |(whole, _)| whole);
    let mut lines = whole.lines().map(str::trim);
    let url = lines.find(|line| line.starts_with("https://"))?.to_string();
    match provider {
        Provider::Claude => Some(Page {
            url,
            user_code: None,
            paste: true,
        }),
        Provider::Codex => lines.find(|line| user_code(line)).map(|code| Page {
            url,
            user_code: Some(Withheld::new(code)),
            paste: false,
        }),
    }
}

/// A device code as Codex shows one: `ABCD-EFGHI`.
fn user_code(line: &str) -> bool {
    let part = |part: &str, lengths: std::ops::RangeInclusive<usize>| {
        lengths.contains(&part.len())
            && part
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    };
    line.split_once('-')
        .is_some_and(|(head, tail)| part(head, 4..=4) && part(tail, 4..=6))
}

/// `text` without its terminal escapes: CSI sequences, colours among them,
/// and OSC strings.
fn plain(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(char) = chars.next() {
        if char != '\x1b' {
            plain.push(char);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for char in chars.by_ref() {
                    if ('@'..='~').contains(&char) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(char) = chars.next() {
                    if char == '\x07' {
                        break;
                    }
                    if char == '\x1b' {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    plain
}

fn last_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map(str::to_string)
}

/// The last line `said` holds whole, as a person reads it: a prompt the CLI
/// asks again on is no line yet.
fn whole_last_line(said: &[u8]) -> Option<String> {
    let plain = plain(&String::from_utf8_lossy(said));
    last_line(plain.rsplit_once('\n')?.0)
}

/// How a `finish` ended, short of failing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finished {
    /// The CLI ended, and says it is signed in.
    SignedIn,
    /// The CLI refused the code on sight, as it said, and waits for another.
    Refused(String),
}

/// A sign-in running on this machine. Dropped, it stops its CLI and all
/// the CLI started before it returns.
pub struct SignIn {
    steps: Sender<Heard>,
    tree: Arc<Mutex<Slot>>,
}

impl SignIn {
    /// Start `provider`'s sign-in, the CLI found through `clis`, on a
    /// thread of its own: `started` hears where it goes on, or why not.
    pub fn start(
        provider: Provider,
        clis: Arc<dyn Clis>,
        started: impl FnOnce(Result<Page, String>) + Send + 'static,
    ) -> Self {
        let (steps, heard) = mpsc::channel();
        let tree = Arc::new(Mutex::new(Slot::Unstarted));
        let run = Run {
            provider,
            tree: Arc::clone(&tree),
            heard,
            said: Vec::new(),
            open: 0,
            stdin: None,
        };
        let tell = steps.clone();
        thread::spawn(move || run.run(&*clis, tell, Answer::new(started)));
        Self { steps, tree }
    }

    /// Hand the CLI `code`, when its page gave one, and wait for it to end
    /// signed in, or to refuse the code: `finished` hears which.
    pub fn finish(&self, code: Option<Withheld>, finished: impl FnOnce(Result<Finished, String>) + Send + 'static) {
        // A thread that already ended answers as the step drops.
        let _ = self.steps.send(Heard::Finish(code, Answer::new(finished)));
    }
}

impl Drop for SignIn {
    fn drop(&mut self) {
        let gone = std::mem::replace(&mut *lock(&self.tree), Slot::Stopped);
        drop(gone);
    }
}

/// The CLI's process tree, as the handle and the thread share it.
enum Slot {
    /// The thread is still finding the CLI.
    Unstarted,
    Running(ProcessTree),
    /// The handle went: nothing may run.
    Stopped,
}

fn lock(tree: &Mutex<Slot>) -> MutexGuard<'_, Slot> {
    tree.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What reaches a sign-in's thread.
enum Heard {
    /// A piece of what the CLI wrote, on either stream.
    Said(Vec<u8>),
    /// One of its streams ended.
    Shut,
    Finish(Option<Withheld>, Answer<Finished>),
}

/// Who waits on a step: answered once, by the step, or else as it drops.
struct Answer<T>(Option<Box<dyn FnOnce(Result<T, String>) + Send>>);

impl<T> Answer<T> {
    fn new(answer: impl FnOnce(Result<T, String>) + Send + 'static) -> Self {
        Self(Some(Box::new(answer)))
    }

    fn give(mut self, result: Result<T, String>) {
        if let Some(answer) = self.0.take() {
            answer(result);
        }
    }
}

impl<T> Drop for Answer<T> {
    fn drop(&mut self) {
        if let Some(answer) = self.0.take() {
            answer(Err(STOPPED.into()));
        }
    }
}

/// How the CLI stands.
enum Now {
    Running,
    Ended(ExitStatus),
    Stopped,
}

/// One sign-in, as its thread runs it.
struct Run {
    provider: Provider,
    tree: Arc<Mutex<Slot>>,
    heard: Receiver<Heard>,
    /// All the CLI wrote, both streams as they came.
    said: Vec<u8>,
    /// Its streams not yet ended.
    open: usize,
    /// Kept open to the end: the CLI may read its code at any time.
    stdin: Option<ChildStdin>,
}

impl Run {
    fn run(mut self, clis: &dyn Clis, tell: Sender<Heard>, started: Answer<Page>) {
        let Some(binary) = clis.binary(self.provider) else {
            return started.give(Err(self.missing()));
        };
        if let Err(error) = self.spawn(&binary.path, tell) {
            return started.give(Err(error));
        }
        let page = self.until_page();
        let shown = page.is_ok();
        started.give(page);
        if !shown {
            return;
        }
        let by = Instant::now() + FINISH;
        while let Some((code, finished)) = self.until_finish(by) {
            let ended = self.finish(&binary.path, code, by);
            let again = matches!(ended, Ok(Finished::Refused(_)));
            finished.give(ended);
            if !again {
                return;
            }
        }
    }

    fn missing(&self) -> String {
        format!("{} is not installed there", self.provider.cli_name())
    }

    /// Start the CLI's sign-in, its streams read into the thread's channel,
    /// unless the handle went while the CLI was being found.
    fn spawn(&mut self, program: &Path, tell: Sender<Heard>) -> Result<(), String> {
        let mut command = cli(program, login_args(self.provider));
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut tree = ProcessTree::spawn(command).map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => self.missing(),
            _ => format!("{} could not start: {error}", self.provider.cli_name()),
        })?;
        let (stdin, stdout, stderr) = tree.pipes();
        self.stdin = stdin;
        let pipes: [Option<Box<dyn Read + Send>>; 2] = [
            stdout.map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
            stderr.map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
        ];
        for pipe in pipes.into_iter().flatten() {
            self.open += 1;
            read(pipe, tell.clone());
        }
        let mut slot = lock(&self.tree);
        if matches!(*slot, Slot::Stopped) {
            return Err(STOPPED.into());
        }
        *slot = Slot::Running(tree);
        Ok(())
    }

    /// Wait for the CLI to say where to sign in, for at most `PAGE`.
    fn until_page(&mut self) -> Result<Page, String> {
        let by = Instant::now() + PAGE;
        loop {
            if let Some(page) = page(self.provider, &String::from_utf8_lossy(&self.said)) {
                return Ok(page);
            }
            match self.now() {
                Now::Running if Instant::now() >= by => {
                    self.stop();
                    return Err(format!("{} did not say where to sign in", self.provider.cli_name()));
                }
                Now::Running => {}
                Now::Ended(_) => {
                    self.drain();
                    return Err(self.why(0));
                }
                Now::Stopped => return Err(STOPPED.into()),
            }
            drop(self.hear(TICK));
        }
    }

    /// Wait for the step that finishes the sign-in, until `by`.
    fn until_finish(&mut self, by: Instant) -> Option<(Option<Withheld>, Answer<Finished>)> {
        loop {
            if let Some(finish) = self.hear(TICK) {
                return Some(finish);
            }
            if matches!(self.now(), Now::Stopped) {
                return None;
            }
            if Instant::now() >= by {
                self.stop();
                return None;
            }
        }
    }

    /// Hand the CLI `code`, wait for it to end, and confirm it is signed in;
    /// or, when it answers the code with a line and runs on, quiet, take
    /// that line as its refusal.
    fn finish(&mut self, program: &Path, code: Option<Withheld>, by: Instant) -> Result<Finished, String> {
        let name = self.provider.cli_name();
        let since = self.said.len();
        let pasted = code.is_some();
        if let (Some(stdin), Some(code)) = (self.stdin.as_mut(), code) {
            let _ = writeln!(stdin, "{}", code.reveal()).and_then(|()| stdin.flush());
        }
        let mut heard_at = Instant::now();
        let status = loop {
            match self.now() {
                Now::Ended(status) => break status,
                Now::Stopped => return Err(STOPPED.into()),
                Now::Running if Instant::now() >= by => {
                    self.stop();
                    return Err(format!("{name} did not finish signing in in time, so it was stopped"));
                }
                Now::Running if pasted && heard_at.elapsed() >= ASKING => {
                    if let Some(said) = whole_last_line(&self.said[since..]) {
                        return Ok(Finished::Refused(said));
                    }
                }
                Now::Running => {}
            }
            let before = self.said.len();
            if let Some((_, again)) = self.hear(TICK) {
                again.give(Err("The sign-in is already finishing".into()));
            }
            if self.said.len() > before {
                heard_at = Instant::now();
            }
        };
        self.drain();
        if !status.success() {
            return Err(self.why(since));
        }
        match self::status(self.provider, program) {
            Some(true) => Ok(Finished::SignedIn),
            _ => Err(format!("{name} ended, but it is not signed in")),
        }
    }

    /// Take what reaches the thread within `wait`: what the CLI wrote is
    /// kept, and a finish is handed back.
    fn hear(&mut self, wait: Duration) -> Option<(Option<Withheld>, Answer<Finished>)> {
        match self.heard.recv_timeout(wait) {
            Ok(Heard::Said(bytes)) => self.said.extend_from_slice(&bytes),
            Ok(Heard::Shut) => self.open = self.open.saturating_sub(1),
            Ok(Heard::Finish(code, answer)) => return Some((code, answer)),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => thread::sleep(wait),
        }
        None
    }

    /// What the CLI still had to write once it ended, for at most `DRAIN`.
    fn drain(&mut self) {
        let by = Instant::now() + DRAIN;
        while self.open > 0 && Instant::now() < by {
            drop(self.hear(TICK));
        }
    }

    /// Why the CLI ended, as the last line it wrote since `since` says,
    /// else the last it wrote at all.
    fn why(&self, since: usize) -> String {
        let said = |bytes: &[u8]| last_line(&plain(&String::from_utf8_lossy(bytes)));
        said(&self.said[since..])
            .or_else(|| said(&self.said))
            .unwrap_or_else(|| format!("{} could not sign in", self.provider.cli_name()))
    }

    fn now(&self) -> Now {
        match &mut *lock(&self.tree) {
            Slot::Running(tree) => match tree.try_wait() {
                Ok(Some(status)) => Now::Ended(status),
                Ok(None) | Err(_) => Now::Running,
            },
            Slot::Stopped => Now::Stopped,
            Slot::Unstarted => Now::Running,
        }
    }

    fn stop(&self) {
        if let Slot::Running(tree) = &mut *lock(&self.tree) {
            tree.stop();
        }
    }
}

/// Read `pipe` on a thread of its own, in pieces as they come, until it ends.
fn read(mut pipe: Box<dyn Read + Send>, tell: Sender<Heard>) {
    thread::spawn(move || {
        let mut chunk = [0; CHUNK];
        while let Ok(read @ 1..) = pipe.read(&mut chunk) {
            if tell.send(Heard::Said(chunk[..read].to_vec())).is_err() {
                return;
            }
        }
        let _ = tell.send(Heard::Shut);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE: &str = "Opening browser to sign in\u{2026}\nIf the browser didn't open, visit:\nhttps://claude.ai/oauth/authorize?code=true&state=s3nt\nPaste code here if prompted > ";

    const CODEX: &str = "\nWelcome to Codex [v0.171.0]\n\n1. Open this link in your browser and sign in to your account\n   \x1b[94mhttps://auth.openai.com/codex/device\x1b[0m\n\n2. Enter this one-time code \x1b[90m(expires in 15 minutes)\x1b[0m\n   \x1b[94mWXYZ-12345\x1b[0m\n\n";

    #[test]
    fn claudes_page_is_its_first_address_and_its_code_is_pasted_back() {
        assert_eq!(
            page(Provider::Claude, CLAUDE),
            Some(Page {
                url: "https://claude.ai/oauth/authorize?code=true&state=s3nt".into(),
                user_code: None,
                paste: true,
            })
        );
    }

    #[test]
    fn codexs_page_carries_the_code_shown_under_its_address() {
        assert_eq!(
            page(Provider::Codex, CODEX),
            Some(Page {
                url: "https://auth.openai.com/codex/device".into(),
                user_code: Some(Withheld::new("WXYZ-12345")),
                paste: false,
            })
        );
    }

    #[test]
    fn no_page_until_its_lines_are_whole() {
        let cut = CLAUDE.find("s3nt").unwrap();
        assert_eq!(page(Provider::Claude, &CLAUDE[..cut]), None, "an address cut short");
        let cut = CODEX.find("WXYZ").unwrap();
        assert_eq!(page(Provider::Codex, &CODEX[..cut]), None, "the address without its code");
        assert_eq!(page(Provider::Codex, "Welcome to Codex\n"), None);
    }

    #[test]
    fn only_a_device_codes_shape_reads_as_one() {
        assert!(user_code("ABCD-EFGHI") && user_code("AB12-3456") && user_code("ABCD-EFGHIJ"));
        assert!(!user_code("abcd-efghi") && !user_code("ABC-DEFGH") && !user_code("ABCD-EFGHIJK"));
        assert!(!user_code("(expires in 15 minutes)"));
    }

    #[test]
    fn what_a_cli_writes_reads_without_its_escapes() {
        assert_eq!(plain("\x1b[94mhttps://a\x1b[0m"), "https://a");
        assert_eq!(plain("\x1b]8;;https://a\x07link\x1b]8;;\x1b\\"), "link");
        assert_eq!(last_line("Couldn't log in: bad code\n\n"), Some("Couldn't log in: bad code".into()));
        assert_eq!(whole_last_line(b"Cut short.\nPaste code here > "), Some("Cut short.".into()));
        assert_eq!(whole_last_line(b"Paste code here > "), None, "a prompt is no line");
    }
}
