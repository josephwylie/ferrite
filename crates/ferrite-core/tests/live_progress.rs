//! Opt-in native adapter smoke checks. No external service mock, no model
//! API: the existing provider Sessions own each agent’s native harness.
use ferrite_core::{
    providers::{ClaudeConfig, ClaudeSession, CodexConfig, CodexSession, Session},
    transcript::{Input, Transcript},
    SessionEvent, TurnOutcome,
};
use std::{
    fs,
    time::{Duration, Instant},
};

fn probe(provider: &str) {
    let cwd = std::env::temp_dir().join(format!(
        "ferrite-native-progress-{provider}-{}",
        std::process::id()
    ));
    fs::create_dir_all(&cwd).unwrap();
    fs::write(
        cwd.join("README.md"),
        "Ferrite displays native provider progress.\n",
    )
    .unwrap();
    fs::write(
        cwd.join("notes.txt"),
        "Progress headings must stay visible while tools run.\n",
    )
    .unwrap();
    let mut session: Box<dyn Session> = match provider {
        "codex" => Box::new(
            CodexSession::spawn(CodexConfig {
                cwd: Some(cwd.clone()),
                sandbox: Some("read-only".into()),
                approval_policy: Some("never".into()),
                ..Default::default()
            })
            .unwrap(),
        ),
        _ => Box::new(
            ClaudeSession::spawn(ClaudeConfig {
                cwd: Some(cwd.clone()),
                permission_mode: Some("plan".into()),
                ..Default::default()
            })
            .unwrap(),
        ),
    };
    let prompt = "Read README.md and notes.txt in this directory, using your native file-reading tools. Briefly say what you are checking as you work. Then give one sentence explaining their shared requirement. Do not edit files, launch subagents, or search the internet.";
    let mut transcript = Transcript::default();
    transcript.apply(Input::Prompt(prompt.into()));
    session.send(prompt).unwrap();
    let deadline = Instant::now() + Duration::from_secs(180);
    let (mut headings, mut tools, mut output, mut captions) = (0, 0, 0, Vec::new());
    loop {
        let event = session
            .events()
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("native turn deadline");
        let outcome = if let SessionEvent::TurnEnded { outcome, .. } = &event {
            Some(outcome.clone())
        } else {
            None
        };
        match &event {
            SessionEvent::ReasoningSummaryPart {
                snapshot: false, ..
            }
            | SessionEvent::ReasoningSummaryDelta { .. }
            | SessionEvent::ThinkingDelta { .. } => headings += 1,
            SessionEvent::ToolStarted { .. } => tools += 1,
            SessionEvent::ToolOutputDelta { .. } => output += 1,
            SessionEvent::Closed { reason } => panic!("native process closed: {reason}"),
            SessionEvent::DecisionRequested { .. } => {
                panic!("read-only probe unexpectedly requires a decision")
            }
            _ => {}
        }
        transcript.apply(Input::Event(event));
        if let Some(caption) = transcript.progress().caption() {
            if captions.last() != Some(&caption) {
                captions.push(caption);
            }
        }
        if let Some(outcome) = outcome {
            assert_eq!(outcome, TurnOutcome::Completed);
            break;
        }
    }
    println!(
        "{provider}: {headings} thinking/summary deltas, {tools} native tools, {output} output deltas, {} caption changes",
        captions.len()
    );
    for caption in captions.iter().take(12) {
        println!("  {caption}");
    }
    assert!(tools > 0, "probe must exercise native tools");
    assert_eq!(transcript.progress().caption(), None);
    assert_eq!(
        ferrite_core::docview::Instruments::of(&transcript).running,
        0
    );
    drop(session);
    fs::remove_dir_all(cwd).unwrap();
}

#[test]
#[ignore = "uses the installed Claude CLI and its authenticated account"]
fn native_claude_progress() {
    probe("claude");
}
#[test]
#[ignore = "uses the installed Codex app-server and its authenticated account"]
fn native_codex_progress() {
    probe("codex");
}
