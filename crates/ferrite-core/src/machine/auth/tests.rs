use super::*;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fa-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn broker(&self) -> (Broker, Receiver<Question>) {
        Broker::new(&self.0, Path::new("ferrite")).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn ask_on(
    broker: &Broker,
    prompt: &str,
    kind: PromptKind,
) -> JoinHandle<io::Result<Option<String>>> {
    let address = broker.address.clone();
    let prompt = prompt.to_owned();
    std::thread::spawn(move || request(&address, Request { prompt, kind }))
}

#[test]
fn password_and_host_confirmation_reach_only_their_helper() {
    let first = Fixture::new();
    let second = Fixture::new();
    let (one, questions_one) = first.broker();
    let (two, questions_two) = second.broker();
    let password = ask_on(&one, "devbox password:", PromptKind::Secret);
    let trust = ask_on(
        &two,
        "SHA256:fingerprint (yes/no)?",
        PromptKind::ConfirmHost,
    );
    let question = questions_one.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(question.prompt, "devbox password:");
    assert_eq!(question.kind, PromptKind::Secret);
    question.respond(Some("temporary-password".into())).unwrap();
    let question = questions_two.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(question.kind, PromptKind::ConfirmHost);
    question.respond(Some("yes".into())).unwrap();
    assert_eq!(
        password.join().unwrap().unwrap().as_deref(),
        Some("temporary-password")
    );
    assert_eq!(trust.join().unwrap().unwrap().as_deref(), Some("yes"));
    drop(one);
    drop(two);
    assert_eq!(std::fs::read_dir(&first.0).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(&second.0).unwrap().count(), 0);
}

#[test]
fn a_question_dropped_by_the_ui_cancels_the_helper() {
    let fixture = Fixture::new();
    let (broker, questions) = fixture.broker();
    let helper = ask_on(&broker, "password:", PromptKind::Secret);
    drop(questions.recv_timeout(Duration::from_secs(5)).unwrap());
    assert_eq!(helper.join().unwrap().unwrap(), None);
}

#[test]
fn dropping_the_broker_cancels_a_prompt_retained_by_the_ui() {
    let fixture = Fixture::new();
    let (broker, questions) = fixture.broker();
    let helper = ask_on(&broker, "password:", PromptKind::Secret);
    let question = questions.recv_timeout(Duration::from_secs(5)).unwrap();
    let (done, finished) = mpsc::channel();
    std::thread::spawn(move || {
        drop(broker);
        done.send(()).unwrap();
    });
    finished.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(question.respond(Some("too-late".into())).is_err());
    assert!(!matches!(helper.join().unwrap(), Ok(Some(_))));
}

#[test]
fn malformed_or_stalled_helpers_do_not_keep_the_broker_alive() {
    let fixture = Fixture::new();
    let (broker, _) = fixture.broker();
    let mut malformed = link::connect(&broker.address).unwrap();
    malformed.write_all(&u32::MAX.to_be_bytes()).unwrap();
    let mut result = [0];
    assert!(matches!(malformed.read(&mut result), Ok(0) | Err(_)));
    let _stalled = link::connect(&broker.address).unwrap();
    let (done, finished) = mpsc::channel();
    std::thread::spawn(move || {
        drop(broker);
        done.send(()).unwrap();
    });
    finished.recv_timeout(Duration::from_secs(5)).unwrap();
}

#[test]
fn prompts_expire_and_the_next_prompt_can_be_answered() {
    let fixture = Fixture::new();
    let (broker, questions) =
        Broker::with_timeout(&fixture.0, Path::new("ferrite"), Duration::from_millis(500)).unwrap();
    let expired = ask_on(&broker, "password:", PromptKind::Secret);
    let question = questions.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(!matches!(expired.join().unwrap(), Ok(Some(_))));
    assert_eq!(
        question.respond(Some("late".into())).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    let next = ask_on(&broker, "password:", PromptKind::Secret);
    questions
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .respond(Some("current".into()))
        .unwrap();
    assert_eq!(next.join().unwrap().unwrap().as_deref(), Some("current"));
}

#[test]
fn invalid_answers_cancel_without_returning_a_secret() {
    let fixture = Fixture::new();
    let (broker, questions) = fixture.broker();
    for (kind, answer) in [
        (PromptKind::Secret, "a".repeat(MAX_RESPONSE + 1)),
        (PromptKind::Secret, "secret\nsecond-line".into()),
        (PromptKind::ConfirmHost, "anything".into()),
    ] {
        let helper = ask_on(&broker, "SSH prompt", kind);
        let question = questions.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(question.respond(Some(answer)).is_err());
        assert_eq!(helper.join().unwrap().unwrap(), None);
    }
    let oversized = ask_on(&broker, &"p".repeat(MAX_PROMPT + 1), PromptKind::Secret);
    assert!(oversized.join().unwrap().is_err());
    assert!(questions.try_recv().is_err());
}

#[test]
fn configuring_a_command_does_not_change_global_environment() {
    let fixture = Fixture::new();
    let (broker, _) = fixture.broker();
    let before = std::env::var_os(HELPER_ENV);
    let mut command = Command::new("ssh");
    broker.configure(&mut command);
    let env: std::collections::HashMap<_, _> = command.get_envs().collect();
    assert_eq!(
        env[std::ffi::OsStr::new(HELPER_ENV)],
        Some(broker.address.as_os_str())
    );
    assert_eq!(
        env[std::ffi::OsStr::new("SSH_ASKPASS_REQUIRE")],
        Some(std::ffi::OsStr::new("force"))
    );
    assert_eq!(std::env::var_os(HELPER_ENV), before);
}

#[cfg(unix)]
#[test]
fn socket_is_private_and_long_settings_paths_still_work() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new();
    let long = fixture.0.join("a".repeat(120));
    let (broker, questions) = Broker::new(&long, Path::new("ferrite")).unwrap();
    let address = broker.address.clone();
    let directory = address.parent().unwrap().to_owned();
    assert_eq!(
        std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(&address).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let helper = ask_on(&broker, "password:", PromptKind::Secret);
    questions
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .respond(Some("secret".into()))
        .unwrap();
    assert_eq!(helper.join().unwrap().unwrap().as_deref(), Some("secret"));
    drop(broker);
    assert!(!address.exists());
    assert!(!directory.exists());
}
