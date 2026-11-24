//! A model or effort choice reaches the Session a Thread already runs.

use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use ferrite_core::providers::{applied, Session, SettingAck};
use ferrite_core::store::{Provider, Store};
use ferrite_core::workspace::WorkspaceChoice;

use super::{Cockpit, SpawnRequest, Spawner};

#[derive(Default)]
struct NativeCalls {
    spawns: usize,
    model: Option<String>,
    efforts: Vec<Option<String>>,
}

struct NativeProvider {
    calls: Arc<Mutex<NativeCalls>>,
    events: Receiver<ferrite_core::SessionEvent>,
    sender: Sender<ferrite_core::SessionEvent>,
}

impl Session for NativeProvider {
    fn events(&self) -> &Receiver<ferrite_core::SessionEvent> {
        &self.events
    }
    fn send(&mut self, _: &str) -> io::Result<()> {
        let model = self
            .calls
            .lock()
            .unwrap()
            .model
            .clone()
            .unwrap_or("default".into());
        self.sender
            .send(ferrite_core::SessionEvent::TextDelta { text: model })
            .unwrap();
        self.sender
            .send(ferrite_core::SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
        Ok(())
    }
    fn interrupt(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn respond_to_decision(&mut self, _: &str, _: ferrite_core::DecisionAnswer) -> io::Result<()> {
        Ok(())
    }
    fn set_model(&mut self, model: Option<&str>) -> io::Result<SettingAck> {
        if model == Some("unavailable") {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "native model unavailable",
            ));
        }
        self.calls.lock().unwrap().model = model.map(str::to_owned);
        Ok(applied())
    }
    fn set_effort(&mut self, effort: Option<&str>) -> io::Result<SettingAck> {
        self.calls
            .lock()
            .unwrap()
            .efforts
            .push(effort.map(str::to_owned));
        Ok(applied())
    }
}

/// Native Sessions, and the operator's effort for every provider.
struct Native {
    calls: Arc<Mutex<NativeCalls>>,
    effort: Option<String>,
}
impl Native {
    fn new(calls: Arc<Mutex<NativeCalls>>) -> Box<Self> {
        Box::new(Self {
            calls,
            effort: None,
        })
    }
}
impl Spawner for Native {
    fn default_effort(&self, _: Provider) -> Option<String> {
        self.effort.clone()
    }
    fn spawn(&mut self, _: SpawnRequest) -> io::Result<Box<dyn Session>> {
        let (sender, events) = mpsc::channel();
        self.calls.lock().unwrap().spawns += 1;
        sender
            .send(ferrite_core::SessionEvent::Init {
                session_id: "same-native-conversation".into(),
                model: "initial".into(),
            })
            .unwrap();
        Ok(Box::new(NativeProvider {
            calls: self.calls.clone(),
            events,
            sender,
        }))
    }
}

fn native_cockpit(
    name: &str,
    provider: Provider,
    spawner: Box<Native>,
) -> (Cockpit, ferrite_core::ThreadKey, PathBuf) {
    
    let path = std::env::temp_dir().join(format!(
        "ferrite-native-{name}-{}-{provider:?}",
        std::process::id()
    ));
    let mut cockpit = Cockpit::new(Store::open(path.join("store")).unwrap(), spawner);
    let thread = cockpit
        .open(
            cockpit.first_machine(),
            provider,
            WorkspaceChoice::Main {
                checkout: path.clone(),
            },
        )
        .unwrap();
    cockpit.pump();
    let _ = cockpit.send(thread, "first turn".into());
    cockpit.pump();
    (cockpit, thread, path)
}

#[test]
fn a_model_choice_tunes_the_native_conversation_in_place() {
    for provider in [Provider::Claude, Provider::Codex] {
        let calls = Arc::new(Mutex::new(NativeCalls::default()));
        let (mut cockpit, thread, path) =
            native_cockpit("model", provider, Native::new(calls.clone()));
        cockpit.set_model(thread, Some("selected".into())).now().unwrap();
        assert_eq!(
            calls.lock().unwrap().spawns,
            1,
            "a model choice must not spawn a replacement"
        );
        assert_eq!(calls.lock().unwrap().model.as_deref(), Some("selected"));
        assert_eq!(
            cockpit.peek(thread).unwrap().model.as_deref(),
            Some("selected")
        );
        assert_eq!(
            cockpit.thread(thread).unwrap().transcript().session_id(),
            Some("same-native-conversation")
        );
        let error = cockpit
            .set_model(thread, Some("unavailable".into())).now()
            .unwrap_err();
        assert!(error.to_string().contains("native model unavailable"));
        assert_eq!(calls.lock().unwrap().spawns, 1);
        assert_eq!(
            cockpit.peek(thread).unwrap().model.as_deref(),
            Some("selected")
        );
        let _ = cockpit.send(thread, "after rejected model change".into());
        cockpit.pump();
        assert_eq!(
            cockpit.thread(thread).unwrap().transcript().session_id(),
            Some("same-native-conversation")
        );
        cockpit.set_model(thread, None).now().unwrap();
        assert_eq!(calls.lock().unwrap().model, None);
        assert_eq!(calls.lock().unwrap().spawns, 1);
        drop(cockpit);
        std::fs::remove_dir_all(path).unwrap();
    }
}

/// A cleared effort sets a live Session to the operator's, as the
/// spawner says it is now.
#[test]
fn a_cleared_effort_is_the_operators_default() {
    for provider in [Provider::Claude, Provider::Codex] {
        let calls = Arc::new(Mutex::new(NativeCalls::default()));
        let spawner = Box::new(Native {
            calls: calls.clone(),
            effort: Some("high".into()),
        });
        let (mut cockpit, thread, path) = native_cockpit("effort", provider, spawner);
        cockpit.set_effort(thread, Some("max".into())).now().unwrap();
        cockpit.set_effort(thread, None).now().unwrap();
        assert_eq!(
            calls.lock().unwrap().efforts,
            [Some("max".into()), Some("high".into())]
        );
        drop(cockpit);
        std::fs::remove_dir_all(path).unwrap();
    }
}
