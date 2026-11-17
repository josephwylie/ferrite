#![cfg(unix)]

use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Output;
use std::sync::atomic::AtomicU64;

use sha2::{Digest, Sha256};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "ferrite-setup-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn script(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, format!("#!/bin/sh\n{contents}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    fn runtime(&self) -> PathBuf {
        self.0.join(".ferrite/runtime")
    }

    fn bootstrap(&self) -> Command {
        self.remote(&BOOTSTRAP.replace("@VERSION@", VERSION))
    }

    fn remote(&self, script: &str) -> Command {
        let mut command = Command::new("sh");
        command.arg("-c").arg(script);
        command.env("HOME", &self.0);
        command.env(
            "PATH",
            format!("{}:/usr/bin:/bin", self.0.join("bin").display()),
        );
        command.env("FERRITE_FIXTURE_RELEASE", self.0.join("release"));
        command
    }

    fn release(&self) {
        self.script("archive/ferrited", &format!(
            "case \"$1\" in --version) printf '%s\\n' 'ferrited {VERSION}' ;; bridge) printf '%s\\n' bridged ;; service) exit 1 ;; *) exit 2 ;; esac"
        ));
        fs::create_dir_all(self.0.join("release")).unwrap();
        let asset = format!("ferrited-{VERSION}-x86_64-unknown-linux-gnu.tar.gz");
        let archive = self.0.join("release").join(&asset);
        assert!(Command::new("tar")
            .args(["-czf"])
            .arg(&archive)
            .arg("-C")
            .arg(self.0.join("archive"))
            .arg("ferrited")
            .status()
            .unwrap()
            .success());
        let checksum = format!("{:x}", Sha256::digest(fs::read(&archive).unwrap()));
        fs::write(
            self.0.join("release/SHA256SUMS"),
            format!("{checksum}  {asset}\n"),
        )
        .unwrap();
        self.script(
            "bin/uname",
            "case \"$1\" in -s) echo Linux ;; -m) echo x86_64 ;; esac",
        );
        self.script("bin/curl", "while [ \"$#\" -gt 0 ]; do case \"$1\" in --output) output=$2; shift ;; https:*) url=$1 ;; esac; shift; done\ncp \"$FERRITE_FIXTURE_RELEASE/${url##*/}\" \"$output\"");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn bootstrap_installs_a_verified_release_then_reuses_it_without_downloading() {
    let fixture = Fixture::new();
    fixture.release();
    assert_eq!(success(fixture.bootstrap().output().unwrap()), "bridged\n");
    assert!(fixture
        .runtime()
        .join(format!("versions/{VERSION}/ferrited"))
        .is_file());
    fixture.script("bin/curl", "echo 'unexpected second download' >&2; exit 1");
    assert_eq!(success(fixture.bootstrap().output().unwrap()), "bridged\n");
    assert!(fs::read_dir(fixture.runtime()).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with('.')));
}

#[test]
fn bootstrap_reuses_a_prepared_path_executable_without_requiring_release_assets() {
    let fixture = Fixture::new();
    fixture.script(
        "bin/ferrited",
        "[ \"$1\" = bridge ] || exit 2; echo prepared",
    );
    fixture.script("bin/curl", "exit 99");
    assert_eq!(success(fixture.bootstrap().output().unwrap()), "prepared\n");
    assert!(!fixture.runtime().exists());
}

#[test]
fn bootstrap_and_bridges_skip_a_managed_binary_with_the_wrong_version() {
    let fixture = Fixture::new();
    fixture.script(
        &format!(".ferrite/runtime/versions/{VERSION}/ferrited"),
        "case \"$1\" in --version) echo 'invalid version' ;; *) exit 13 ;; esac",
    );
    fixture.script("bin/ferrited", "printf '%s\\n' \"$1\"");
    fixture.script("bin/curl", "exit 99");

    assert_eq!(success(fixture.bootstrap().output().unwrap()), "bridge\n");
    for mode in ["bridge", "api-bridge"] {
        assert_eq!(
            success(fixture.remote(&bridge_command(mode)).output().unwrap()),
            format!("{mode}\n")
        );
    }
}

#[test]
fn bootstrap_rejects_corrupt_and_missing_release_assets_without_publishing() {
    let fixture = Fixture::new();
    fixture.release();
    let asset = format!("ferrited-{VERSION}-x86_64-unknown-linux-gnu.tar.gz");
    fs::write(
        fixture.0.join("release/SHA256SUMS"),
        format!("{}  {asset}\n", "0".repeat(64)),
    )
    .unwrap();
    let output = fixture.bootstrap().output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("SHA256 verification"));
    assert!(!fixture
        .runtime()
        .join(format!("versions/{VERSION}"))
        .exists());
    fs::remove_file(fixture.0.join("release/SHA256SUMS")).unwrap();
    let output = fixture.bootstrap().output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("publishes server assets"));
}

#[test]
fn bootstrap_rejects_unsupported_platforms_and_recovers_a_dead_lock() {
    let fixture = Fixture::new();
    fixture.release();
    fixture.script("bin/uname", "echo unsupported");
    let output = fixture.bootstrap().output().unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unavailable for unsupported/unsupported")
    );
    fixture.script(
        "bin/uname",
        "case \"$1\" in -s) echo Linux ;; -m) echo x86_64 ;; esac",
    );
    fs::create_dir_all(fixture.runtime()).unwrap();
    std::os::unix::fs::symlink(
        libc::pid_t::MAX.to_string(),
        fixture.runtime().join(format!(".setup-{VERSION}.lock")),
    )
    .unwrap();
    assert_eq!(success(fixture.bootstrap().output().unwrap()), "bridged\n");
}

fn protocol(version: &str) -> String {
    format!(
        "{{\"type\":\"welcome\",\"generation\":1,\"server\":{{\"version\":\"{version}\",\"executable\":\"/ferrited\"}},\"boot_id\":\"fixture\"}}\n{{\"type\":\"snapshot\",\"boot_id\":\"fixture\",\"revision\":0,\"threads\":[],\"roster\":{{\"open\":[],\"parked\":[]}},\"decisions\":[]}}\n"
    )
}

fn attempt(command: Command, timeout: Duration) -> Attempt {
    attempt_with_reconnect(command, None, timeout)
}

fn attempt_with_reconnect(
    command: Command,
    reconnect: Option<Command>,
    timeout: Duration,
) -> Attempt {
    let (_, questions) = mpsc::channel();
    Attempt::launch(
        Machine::new("fixture"),
        command,
        reconnect,
        None,
        questions,
        timeout,
    )
    .unwrap()
}

fn terminal(attempt: &mut Attempt) -> Vec<Event> {
    let until = Instant::now() + Duration::from_secs(5);
    let mut events = Vec::new();
    while Instant::now() < until {
        if let Some(event) = attempt.poll() {
            let finished = matches!(&event, Event::Ready(_) | Event::Failed(_));
            events.push(event);
            if finished {
                return events;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("setup produced no terminal event: {events:?}");
}

#[test]
fn setup_requires_welcome_and_snapshot_and_warns_without_restarting_an_older_server() {
    let fixture = Fixture::new();
    let output = fixture.0.join("protocol");
    fs::write(&output, protocol("0.3.0")).unwrap();
    let executable = fixture.script(
        "server",
        &format!(
            "read hello; cat {}; sleep 30",
            quoted(output.to_str().unwrap())
        ),
    );
    let mut attempt = attempt(Command::new(executable), Duration::from_secs(2));
    let events = terminal(&mut attempt);
    assert!(events
        .iter()
        .any(|event| matches!(event, Event::Warning(message) if message.contains("0.3.0"))));
    assert!(matches!(events.last(), Some(Event::Ready(machine)) if machine.target == "fixture"));
    assert!(verify(protocol(VERSION).lines().next().unwrap().as_bytes()).is_err());
    assert!(verify(b"not a server\n".as_slice()).is_err());
    assert!(verify(
        protocol(VERSION)
            .replace("\"generation\":1", "\"generation\":999")
            .as_bytes()
    )
    .unwrap_err()
    .to_string()
    .contains("incompatible protocol"));
}

#[test]
fn setup_without_a_shared_sign_in_requires_a_verified_batch_connection() {
    let fixture = Fixture::new();
    let output = fixture.0.join("protocol");
    fs::write(&output, protocol(VERSION)).unwrap();
    let authenticated = fixture.script(
        "authenticated",
        &format!(
            "read hello; cat {}; sleep 30",
            quoted(output.to_str().unwrap())
        ),
    );
    let needs_password = fixture.script(
        "needs-password",
        "read hello; echo 'Permission denied (publickey,password).' >&2; exit 255",
    );
    let mut rejected = attempt_with_reconnect(
        Command::new(&authenticated),
        Some(Command::new(needs_password)),
        Duration::from_secs(2),
    );
    let events = terminal(&mut rejected);
    assert!(
        matches!(events.last(), Some(Event::Failed(message)) if message.contains("ssh-agent") && message.contains("Permission denied"))
    );
    assert!(!events.iter().any(|event| matches!(event, Event::Ready(_))));

    let incomplete = fixture.script("incomplete", "read hello; echo 'not a server'");
    let mut rejected = attempt_with_reconnect(
        Command::new(&authenticated),
        Some(Command::new(incomplete)),
        Duration::from_secs(2),
    );
    assert!(matches!(
        terminal(&mut rejected).last(),
        Some(Event::Failed(_))
    ));

    let mut accepted = attempt_with_reconnect(
        Command::new(&authenticated),
        Some(Command::new(&authenticated)),
        Duration::from_secs(2),
    );
    assert!(matches!(
        terminal(&mut accepted).last(),
        Some(Event::Ready(_))
    ));
}

#[test]
fn setup_deadlines_and_drop_stop_the_owned_process() {
    let fixture = Fixture::new();
    let pid = fixture.0.join("pid");
    let executable = fixture.script(
        "sleep",
        &format!(
            "echo $$ > {}; read hello; sleep 30",
            quoted(pid.to_str().unwrap())
        ),
    );
    let mut timed = attempt(Command::new(&executable), Duration::from_millis(50));
    assert!(
        matches!(terminal(&mut timed).last(), Some(Event::Failed(message)) if message.contains("timed out"))
    );
    fs::remove_file(&pid).ok();
    let active = attempt(Command::new(&executable), Duration::from_secs(30));
    let until = Instant::now() + Duration::from_secs(2);
    while !pid.exists() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
    }
    let child = fs::read_to_string(&pid)
        .unwrap()
        .trim()
        .parse::<libc::pid_t>()
        .unwrap();
    drop(active);
    while unsafe { libc::kill(child, 0) } == 0 && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_ne!(
        unsafe { libc::kill(child, 0) },
        0,
        "cancelled SSH child remained alive"
    );
}

#[test]
fn cancelling_bootstrap_removes_its_staging_and_lock() {
    use std::os::unix::process::CommandExt;
    let fixture = Fixture::new();
    fixture.release();
    fixture.script("bin/curl", "sleep 30");
    let mut command = fixture.bootstrap();
    command
        .process_group(0)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    let staged = |root: &Path| {
        fs::read_dir(root).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().starts_with(".install-"))
        })
    };
    while !staged(&fixture.runtime()) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(staged(&fixture.runtime()));
    unsafe {
        libc::kill(-(child.id() as libc::pid_t), libc::SIGTERM);
    }
    assert!(!child.wait().unwrap().success());
    assert!(!staged(&fixture.runtime()));
    assert!(!fixture
        .runtime()
        .join(format!(".setup-{VERSION}.lock"))
        .symlink_metadata()
        .is_ok());
}
