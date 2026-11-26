//! A window signs the Server's machine's CLIs in (`Op::SignIn`, ADR 0051),
//! against a script standing in for both CLIs: a file beside it stands for
//! each one's sign-in, another for the operator's approval on Codex's page,
//! and while Codex waits for that approval a process the script started
//! beats a heartbeat file.

use std::fs;
use std::path::Path;

use super::environment::answered;
use super::*;
use ferrite_core::cli_update::Status;
use ferrite_core::protocol::{CliBinary, Done, SignInStep, Withheld};
use ferrite_engine::providers::clis::Clis;

const CLI_SH: &str = r#"#!/bin/sh
here=$(dirname "$0")
beat() { while [ ! -f "$here/approved" ]; do : > "$here/heartbeat"; sleep 0.1; done; }
case "$1 $2" in
"auth status")
  if [ -f "$here/claude-signed-in" ]; then echo '{"loggedIn":true}'; exit 0; fi
  echo '{"loggedIn":false}'; exit 1 ;;
"auth login")
  printf "Opening browser to sign in\nIf the browser didn't open, visit:\nhttps://claude.ai/oauth/authorize?state=test\nPaste code here if prompted > "
  read -r code
  if [ "$code" = "right-code" ]; then : > "$here/claude-signed-in"; exit 0; fi
  echo "Couldn't log in: That code wasn't accepted."
  exit 1 ;;
"login status")
  if [ -f "$here/codex-signed-in" ]; then exit 0; fi
  echo 'Not logged in'; exit 1 ;;
"login --device-auth")
  beat >/dev/null 2>&1 &
  printf '\nWelcome to Codex\n1. Open this link in your browser\n   \033[94mhttps://auth.openai.com/codex/device\033[0m\n\n2. Enter this one-time code\n   \033[94mWXYZ-12345\033[0m\n\n'
  while [ ! -f "$here/approved" ]; do sleep 0.1; done
  : > "$here/codex-signed-in"; exit 0 ;;
esac
exit 2
"#;

const CLI_CMD: &str = r#"@echo off
set "here=%~dp0"
if "%1 %2"=="auth status" goto claude_status
if "%1 %2"=="auth login" goto claude_login
if "%1 %2"=="login status" goto codex_status
if "%1 %2"=="login --device-auth" goto codex_login
exit /b 2
:claude_status
if exist "%here%claude-signed-in" goto claude_in
echo {"loggedIn":false}
exit /b 1
:claude_in
echo {"loggedIn":true}
exit /b 0
:claude_login
echo Opening browser to sign in
echo If the browser didn't open, visit:
echo https://claude.ai/oauth/authorize?state=test
<nul set /p "=Paste code here if prompted > "
set "code="
set /p code=
if "%code%"=="right-code" goto claude_right
echo Couldn't log in: That code wasn't accepted.
exit /b 1
:claude_right
type nul > "%here%claude-signed-in"
exit /b 0
:codex_status
if exist "%here%codex-signed-in" exit /b 0
echo Not logged in
exit /b 1
:codex_login
start "" /b cmd /c "%here%beat.cmd" >nul 2>nul
echo Welcome to Codex
echo 1. Open this link in your browser
echo https://auth.openai.com/codex/device
echo 2. Enter this one-time code
echo WXYZ-12345
:codex_wait
if exist "%here%approved" goto codex_approved
ping -n 2 127.0.0.1 >nul
goto codex_wait
:codex_approved
type nul > "%here%codex-signed-in"
exit /b 0
"#;

const BEAT_CMD: &str = r#"@echo off
:beat
if exist "%~dp0approved" exit /b 0
type nul > "%~dp0heartbeat"
ping -n 2 127.0.0.1 >nul
goto beat
"#;

/// Longer than a beat of either script's heartbeat.
const BEAT_GAP: Duration = Duration::from_millis(2500);

/// The script, written into `dir`: what both CLIs run as.
fn fake_cli(dir: &Path) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    if cfg!(windows) {
        let crlf = |script: &str| script.replace('\n', "\r\n");
        fs::write(dir.join("beat.cmd"), crlf(BEAT_CMD)).unwrap();
        let cli = dir.join("cli.cmd");
        fs::write(&cli, crlf(CLI_CMD)).unwrap();
        return cli;
    }
    let cli = dir.join("cli");
    fs::write(&cli, CLI_SH).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).unwrap();
    }
    cli
}

/// A machine whose CLIs are the script, or that has none.
struct Machine {
    cli: Option<PathBuf>,
}

impl Clis for Machine {
    fn binary(&self, _: Provider) -> Option<CliBinary> {
        self.cli.clone().map(|path| CliBinary {
            version: "1.0.0".into(),
            path,
        })
    }

    fn check(&self, _: Provider) -> io::Result<Status> {
        Err(io::Error::other("no registry here"))
    }

    fn update(&self, _: Provider) -> Result<CliBinary, String> {
        Err("no installer here".into())
    }
}

/// A Server on a machine with the script, or with no CLI at all, and the
/// folder the script keeps its files in.
fn sign_in_world(name: &str, installed: bool) -> (Harness, PathBuf) {
    let dir = scratch(name);
    let machine = dir.join("machine");
    let cli = installed.then(|| fake_cli(&machine));
    let harness = serve_configured(
        dir,
        None,
        Tuning::default(),
        Box::new(move |server: &mut Server| server.clis = Arc::new(Machine { cli })),
    );
    (harness, machine)
}

fn step(window: &mut ferrite_client::cockpit::Cockpit, provider: Provider, step: SignInStep) -> io::Result<Done> {
    answered(window, |w| w.sign_in(w.first_machine(), provider, step))
}

fn signed_in(window: &mut ferrite_client::cockpit::Cockpit, provider: Provider) -> Option<bool> {
    match step(window, provider, SignInStep::Status).unwrap() {
        Done::SignInStatus { signed_in, .. } => signed_in,
        other => panic!("a status answers with the status: {other:?}"),
    }
}

/// Wait for the heartbeat to come, from none.
fn beating(machine: &Path) {
    let heartbeat = machine.join("heartbeat");
    let _ = fs::remove_file(&heartbeat);
    let deadline = Instant::now() + PATIENCE;
    while !heartbeat.exists() {
        assert!(Instant::now() < deadline, "the CLI's tree never beat");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Wait for a gap with no heartbeat in it: nothing the CLI started lives.
fn stops_beating(machine: &Path) {
    let heartbeat = machine.join("heartbeat");
    let deadline = Instant::now() + PATIENCE;
    loop {
        let _ = fs::remove_file(&heartbeat);
        std::thread::sleep(BEAT_GAP);
        if !heartbeat.exists() {
            return;
        }
        assert!(Instant::now() < deadline, "the CLI's tree still beats");
    }
}

#[test]
fn claude_signs_in_there_with_the_code_its_page_gave() {
    let (harness, _) = sign_in_world("sign-in-claude", true);
    let mut window = cockpit(&harness);
    assert_eq!(signed_in(&mut window, Provider::Claude), Some(false));

    let page = step(&mut window, Provider::Claude, SignInStep::Start).unwrap();
    assert_eq!(
        page,
        Done::SignInPage {
            provider: Provider::Claude,
            url: "https://claude.ai/oauth/authorize?state=test".into(),
            user_code: None,
            paste: true,
        }
    );
    let wrong = step(
        &mut window,
        Provider::Claude,
        SignInStep::Finish {
            code: Some(Withheld::new("wrong-code")),
        },
    )
    .unwrap_err();
    assert_eq!(wrong.to_string(), "Couldn't log in: That code wasn't accepted.");
    assert_eq!(signed_in(&mut window, Provider::Claude), Some(false));

    step(&mut window, Provider::Claude, SignInStep::Start).unwrap();
    let done = step(
        &mut window,
        Provider::Claude,
        SignInStep::Finish {
            code: Some(Withheld::new("right-code")),
        },
    )
    .unwrap();
    assert_eq!(done, Done::SignedIn { provider: Provider::Claude });
    assert_eq!(signed_in(&mut window, Provider::Claude), Some(true));
}

#[test]
fn codex_signs_in_there_once_its_code_is_approved_on_the_page() {
    let (harness, machine) = sign_in_world("sign-in-codex", true);
    let mut window = cockpit(&harness);
    assert_eq!(signed_in(&mut window, Provider::Codex), Some(false));

    let page = step(&mut window, Provider::Codex, SignInStep::Start).unwrap();
    assert_eq!(
        page,
        Done::SignInPage {
            provider: Provider::Codex,
            url: "https://auth.openai.com/codex/device".into(),
            user_code: Some(Withheld::new("WXYZ-12345")),
            paste: false,
        }
    );
    let finished = window
        .sign_in(window.first_machine(), Provider::Codex, SignInStep::Finish { code: None })
        .unwrap();
    let waited = Instant::now() + Duration::from_millis(500);
    while Instant::now() < waited {
        window.pump();
        assert!(finished.try_recv().is_err(), "it finished before the page approved it");
        std::thread::sleep(Duration::from_millis(20));
    }
    fs::write(machine.join("approved"), "").unwrap();
    let done = std::cell::RefCell::new(None);
    pump_until(&mut window, |_| {
        if let Ok(answer) = finished.try_recv() {
            *done.borrow_mut() = Some(answer);
        }
        done.borrow().is_some()
    });
    assert_eq!(done.into_inner().unwrap().unwrap(), Done::SignedIn { provider: Provider::Codex });
    assert_eq!(signed_in(&mut window, Provider::Codex), Some(true));
}

#[test]
fn a_cancel_or_its_window_going_stops_the_cli_and_all_it_started() {
    let (harness, machine) = sign_in_world("sign-in-cancel", true);
    let mut window = cockpit(&harness);
    let mut other = cockpit(&harness);

    step(&mut window, Provider::Codex, SignInStep::Start).unwrap();
    beating(&machine);
    assert_eq!(
        step(&mut other, Provider::Codex, SignInStep::Cancel).unwrap(),
        Done::Changed { changed: false },
        "only the window that started it cancels it"
    );
    assert!(step(&mut other, Provider::Codex, SignInStep::Finish { code: None }).is_err());
    beating(&machine);
    assert_eq!(
        step(&mut window, Provider::Codex, SignInStep::Cancel).unwrap(),
        Done::Changed { changed: true }
    );
    stops_beating(&machine);

    step(&mut other, Provider::Codex, SignInStep::Start).unwrap();
    beating(&machine);
    drop(other);
    stops_beating(&machine);
}

#[test]
fn a_machine_without_the_cli_says_it_is_not_installed_there() {
    let (harness, _) = sign_in_world("sign-in-missing", false);
    let mut window = cockpit(&harness);
    assert_eq!(signed_in(&mut window, Provider::Claude), None);
    let error = step(&mut window, Provider::Claude, SignInStep::Start).unwrap_err();
    assert_eq!(error.to_string(), "Claude Code is not installed there");
}
