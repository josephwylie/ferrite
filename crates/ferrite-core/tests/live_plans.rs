//! Does an agent in Ferrite plan? Real `claude` and `codex` on a multi-step
//! task, folded into the transcript model as Ferrite folds them, printing
//! the plan each time it moves. Ignored by default: they cost money, need
//! auth and talk to a vendor service.
//!
//! `cargo test -p ferrite-core --test live_plans -- --ignored --nocapture`
//! With `FERRITE_PLAN_CAPTURE=<dir>` the provider's stdout is also kept
//! there (`claude.jsonl`, `codex.jsonl`), which is how the committed
//! `*-update-plan-*` fixtures were captured.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ferrite_core::activity::{Activity, ActivityInput};
use ferrite_core::progress::{PlanStep, StepStatus};
use ferrite_core::providers::{ClaudeConfig, ClaudeSession, CodexConfig, CodexSession, Session};
use ferrite_core::transcript::Input;
use ferrite_core::{DecisionAnswer, SessionEvent};

const TURN_TIMEOUT: Duration = Duration::from_secs(600);

const TASK: &str = "In this directory: create a Python module calc.py with add and mul \
functions, write test_calc.py with unittest tests for both, run the tests with python3, then \
write a README.md describing the module.";

/// A fresh, empty working folder.
fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ferrite-live-plan-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The provider's program, teeing its stdout into the capture folder when
/// one is named.
fn program(real: &str, capture: &str, dir: &Path) -> String {
    let Ok(folder) = std::env::var("FERRITE_PLAN_CAPTURE") else {
        return real.into();
    };
    std::fs::create_dir_all(&folder).unwrap();
    let out = Path::new(&folder).join(capture);
    let _ = std::fs::remove_file(&out);
    let wrapper = dir.with_extension("provider");
    std::fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\ncase \"$1\" in --version) exec {real} --version;; esac\n\
             {real} \"$@\" | tee '{}'\n",
            out.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    wrapper.display().to_string()
}

/// Run one turn, allowing every Decision, and return each distinct plan the
/// transcript model held as the turn went.
fn plans_through_a_turn(session: &mut dyn Session) -> Vec<Vec<PlanStep>> {
    let mut activity = Activity::default();
    activity.apply(ActivityInput::Connect { generation: 1 });
    activity.apply(ActivityInput::Main {
        input: Input::Prompt(TASK.into()),
        at: Instant::now(),
    });
    session.send(TASK).unwrap();
    let deadline = Instant::now() + TURN_TIMEOUT;
    let mut plans: Vec<Vec<PlanStep>> = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let event = session
            .events()
            .recv_timeout(left)
            .expect("the turn ends in time");
        if let SessionEvent::DecisionRequested { decision } = &event {
            session
                .respond_to_decision(
                    &decision.id,
                    DecisionAnswer::Allow {
                        input: decision.input.clone(),
                    },
                )
                .unwrap();
        }
        let ended = matches!(event, SessionEvent::TurnEnded { .. });
        if let SessionEvent::Closed { reason } = &event {
            panic!("session closed mid-turn: {reason}");
        }
        let at = Instant::now();
        activity.apply(match event {
            SessionEvent::Activity(event) => ActivityInput::Observe {
                generation: 1,
                event,
                at,
            },
            event => ActivityInput::Main {
                input: Input::Event(event),
                at,
            },
        });
        let plan = activity.view().main().transcript().progress().plan.clone();
        if !plan.is_empty() && plans.last() != Some(&plan) {
            println!("plan: {plan:?}");
            plans.push(plan);
        }
        if ended {
            return plans;
        }
    }
}

/// The plan was made, and moved: some step finished after the first plan.
fn assert_planned_and_worked(plans: &[Vec<PlanStep>]) {
    assert!(!plans.is_empty(), "the agent never planned");
    let done = |plan: &Vec<PlanStep>| {
        plan.iter()
            .filter(|step| step.status == StepStatus::Completed)
            .count()
    };
    assert!(
        plans.len() > 1 && done(plans.last().unwrap()) > done(&plans[0]),
        "the plan never moved: {plans:?}"
    );
}

#[test]
#[ignore = "spawns the real claude CLI"]
fn claude_plans_a_multi_step_task() {
    let dir = workspace("claude");
    let real = std::env::var("FERRITE_CLAUDE_BIN").unwrap_or_else(|_| "claude".into());
    let mut session = ClaudeSession::spawn(ClaudeConfig {
        program: program(&real, "claude.jsonl", &dir),
        cwd: Some(dir.clone()),
        permission_mode: Some("acceptEdits".into()),
        ..Default::default()
    })
    .unwrap();
    let plans = plans_through_a_turn(&mut session);
    assert_planned_and_worked(&plans);
}

#[test]
#[ignore = "spawns the real codex CLI"]
fn codex_plans_a_multi_step_task() {
    let dir = workspace("codex");
    let real = std::env::var("FERRITE_CODEX_BIN").unwrap_or_else(|_| "codex".into());
    let mut session = CodexSession::spawn(CodexConfig {
        program: program(&real, "codex.jsonl", &dir),
        cwd: Some(dir.clone()),
        approval_policy: Some("never".into()),
        sandbox: Some("workspace-write".into()),
        ..Default::default()
    })
    .unwrap();
    let plans = plans_through_a_turn(&mut session);
    assert_planned_and_worked(&plans);
}
