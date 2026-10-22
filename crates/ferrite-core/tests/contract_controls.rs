//! Parent-owned acceptance tests for native controls through the shared Cockpit.
use std::io;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};

use ferrite_core::cockpit::{Cockpit, SpawnRequest, Spawner};
use ferrite_core::providers::{applied, Session, SettingAck};
use ferrite_core::store::{Provider, Store};
use ferrite_core::workspace::WorkspaceChoice;
use ferrite_core::{DecisionAnswer, SessionEvent, TurnOutcome};

#[derive(Default)]
struct Calls {
    streams: Vec<mpsc::Sender<SessionEvent>>,
    models: Vec<Option<String>>,
    reject: bool,
    controls: Vec<ferrite_core::SessionControl>,
}

#[derive(Clone, Default)]
struct Native(Arc<Mutex<Calls>>);

struct Live {
    calls: Native,
    events: mpsc::Receiver<SessionEvent>,
}

impl Session for Live {
    fn events(&self) -> &mpsc::Receiver<SessionEvent> {
        &self.events
    }
    fn send(&mut self, _: &str) -> io::Result<()> {
        Ok(())
    }
    fn interrupt(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn respond_to_decision(&mut self, _: &str, _: DecisionAnswer) -> io::Result<()> {
        Ok(())
    }
    fn supports_control(&self, _: ferrite_core::ControlKind) -> bool {
        true
    }
    fn control(&mut self, control: ferrite_core::SessionControl) -> io::Result<()> {
        self.calls.0.lock().unwrap().controls.push(control);
        Ok(())
    }
    fn set_model(&mut self, model: Option<&str>) -> io::Result<SettingAck> {
        let mut calls = self.calls.0.lock().unwrap();
        if calls.reject {
            return Err(io::Error::other("native model unavailable"));
        }
        calls.models.push(model.map(str::to_owned));
        Ok(applied())
    }
}

impl Spawner for Native {
    fn spawn(&mut self, request: SpawnRequest) -> io::Result<Box<dyn Session>> {
        let (sender, events) = mpsc::channel();
        sender
            .send(SessionEvent::Init {
                session_id: "native-session".into(),
                model: request.model.unwrap_or("initial").into(),
            })
            .unwrap();
        self.0.lock().unwrap().streams.push(sender);
        Ok(Box::new(Live {
            calls: self.clone(),
            events,
        }))
    }
}

/// A folder of the test's own, removed once it is done. Removal is best
/// effort, as across these tests: on Windows a file the Store just wrote
/// may still be held (its writer's worker finishing a sync, a scanner
/// reading it) when the test ends, and that is no failure of the test.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        // A folder left by an earlier run whose process id this one reuses.
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn exercise_model_control(reject: bool) {
    let scratch = Scratch::new(&format!("ferrite-native-control-{reject}"));
    let path = &scratch.path;
    let native = Native::default();
    let mut cockpit = Cockpit::new(
        Store::open(path.join("store")).unwrap(),
        Box::new(native.clone()),
    );
    let thread = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: path.clone(),
            },
        )
        .unwrap();
    cockpit.pump();
    cockpit.send(thread, "hello".into());
    native.0.lock().unwrap().streams[0]
        .send(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    cockpit.pump();
    let previous = cockpit.peek(thread).unwrap().model;
    native.0.lock().unwrap().reject = reject;
    let result = cockpit.set_model(thread, Some("selected".into()));
    assert_eq!(
        native.0.lock().unwrap().streams.len(),
        1,
        "native model control must keep the live Session"
    );
    if reject {
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("native model unavailable"));
        assert_eq!(cockpit.peek(thread).unwrap().model, previous);
        assert_eq!(cockpit.thread(thread).unwrap().model(), previous.as_deref());
    } else {
        result.unwrap();
        assert_eq!(native.0.lock().unwrap().models, [Some("selected".into())]);
        assert_eq!(
            cockpit.peek(thread).unwrap().model.as_deref(),
            Some("selected")
        );
        assert_eq!(cockpit.thread(thread).unwrap().model(), Some("selected"));
        assert_eq!(
            cockpit.thread(thread).unwrap().transcript().session_id(),
            Some("native-session")
        );
    }
}

#[test]
fn native_model_control_updates_the_header_without_replacing_the_session() {
    exercise_model_control(false);
}

#[test]
fn rejected_native_model_control_preserves_the_session_and_header() {
    exercise_model_control(true);
}

#[test]
fn shared_cockpit_routes_capability_controls_without_replacing_or_finishing_session() {
    let scratch = Scratch::new("ferrite-native-route");
    let path = &scratch.path;
    let native = Native::default();
    let mut cockpit = Cockpit::new(
        Store::open(path.join("store")).unwrap(),
        Box::new(native.clone()),
    );
    let thread = cockpit
        .open(
            Provider::Claude,
            WorkspaceChoice::Main {
                checkout: path.clone(),
            },
        )
        .unwrap();
    cockpit.pump();
    assert!(cockpit
        .thread(thread)
        .unwrap()
        .supports_control(ferrite_core::ControlKind::RefreshContext));
    cockpit
        .control(thread, ferrite_core::SessionControl::RefreshContext)
        .unwrap();
    assert_eq!(
        native.0.lock().unwrap().controls,
        [ferrite_core::SessionControl::RefreshContext]
    );
    assert_eq!(native.0.lock().unwrap().streams.len(), 1);
    assert_eq!(
        cockpit.thread(thread).unwrap().transcript().session_id(),
        Some("native-session")
    );
}
