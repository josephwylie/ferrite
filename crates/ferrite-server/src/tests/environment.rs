//! A window's Settings against the Server it is on: the defaults the
//! Server's own file holds (`Op::ServerDefaults`) and the provider CLIs of
//! its machine (`Op::Cli`).

use super::*;
use ferrite_core::protocol::{CliAction, CliBinary, CliStatus, Done};
use ferrite_core::providers::update::Status;
use ferrite_core::server::clis::Clis;
use ferrite_core::settings::{ServerDefaults, Settings};

/// A machine whose CLIs the test scripts: the version each answers and
/// the installs it was asked for.
#[derive(Default)]
struct Machine {
    installed: Mutex<Vec<Provider>>,
}

fn copy(provider: Provider, version: &str) -> CliBinary {
    CliBinary {
        version: version.into(),
        path: PathBuf::from(format!("/opt/box/bin/{provider}")),
    }
}

impl Clis for Machine {
    fn binary(&self, provider: Provider) -> Option<CliBinary> {
        let after = self.installed.lock().unwrap().contains(&provider);
        Some(copy(provider, if after { "3.0.0" } else { "2.0.0" }))
    }

    fn check(&self, _: Provider) -> io::Result<Status> {
        Ok(Status::UpToDate {
            current: "2.0.0".into(),
        })
    }

    fn update(&self, provider: Provider) -> Result<CliBinary, String> {
        if provider == Provider::Codex {
            return Err("the installer failed".into());
        }
        self.installed.lock().unwrap().push(provider);
        Ok(copy(provider, "3.0.0"))
    }
}

fn machine_world(name: &str) -> (Harness, Arc<Machine>) {
    let machine = Arc::new(Machine::default());
    let clis = Arc::clone(&machine);
    let harness = serve_configured(
        scratch(name),
        None,
        Tuning::default(),
        Box::new(move |server: &mut Server| server.clis = clis),
    );
    (harness, machine)
}

fn answered<T>(
    window: &mut ferrite_client::cockpit::Cockpit,
    ask: impl FnOnce(
        &mut ferrite_client::cockpit::Cockpit,
    ) -> io::Result<std::sync::mpsc::Receiver<io::Result<T>>>,
) -> io::Result<T> {
    let answer = ask(window).unwrap();
    let landed = std::cell::RefCell::new(None);
    pump_until(window, |_| {
        if let Ok(read) = answer.try_recv() {
            *landed.borrow_mut() = Some(read);
        }
        landed.borrow().is_some()
    });
    landed.into_inner().unwrap()
}

/// The Server answers with what its own file says, and a write lands in
/// that file and comes back as the Server reread it.
#[test]
fn a_write_to_the_defaults_is_reread_from_the_servers_file() {
    let harness = start("defaults-reread");
    let on_disk = Settings {
        claude_permission_mode: Some("plan".into()),
        codex_sandbox: Some("read-only".into()),
        factory_tasks_at_once: 5,
        ..Default::default()
    };
    on_disk.save(&harness.dir).unwrap();
    let mut window = cockpit(&harness);

    let read = answered(&mut window, |w| w.server_defaults(w.first_machine(), None)).unwrap();
    assert_eq!(read, on_disk.server_defaults());

    let wanted = ServerDefaults {
        codex_approval_policy: "never".into(),
        factory_tasks_at_once: 2,
        ..read
    };
    let written = answered(&mut window, |w| w.server_defaults(w.first_machine(), Some(wanted.clone()))).unwrap();
    assert_eq!(written, wanted);
    let saved = Settings::load(&harness.dir).unwrap();
    assert_eq!(saved.server_defaults(), wanted);
    assert_eq!(
        saved.claude_permission_mode.as_deref(),
        Some("plan"),
        "what the write did not name stays"
    );
}

/// A probe answers with the copy on the Server's machine, and an update
/// installs there and reports the copy it left.
#[test]
fn the_cli_commands_name_the_servers_machine_and_binary() {
    let (harness, machine) = machine_world("cli-machine");
    let mut window = cockpit(&harness);

    let Done::Clis { clis } = answered(&mut window, |w| w.cli(w.first_machine(), CliAction::Versions)).unwrap() else {
        panic!("a probe answers with the copies");
    };
    assert_eq!(clis.len(), 2);
    assert_eq!(clis[0].binary, Some(copy(clis[0].provider, "2.0.0")));
    assert!(
        clis[0].status.is_none(),
        "a probe does not reach the registry"
    );

    let Done::Clis { clis } = answered(&mut window, |w| w.cli(w.first_machine(), CliAction::Check)).unwrap() else {
        panic!("a check answers with the copies");
    };
    assert!(matches!(clis[0].status, Some(CliStatus::UpToDate { .. })));

    let updated = answered(&mut window, |w| {
        w.cli(w.first_machine(), CliAction::Update {
            provider: Provider::Claude,
        })
    })
    .unwrap();
    match updated {
        Done::CliUpdated { provider, binary } => {
            assert_eq!(provider, Provider::Claude);
            assert_eq!(binary, copy(Provider::Claude, "3.0.0"));
        }
        other => panic!("an update answers with the copy it left: {other:?}"),
    }
    assert_eq!(*machine.installed.lock().unwrap(), [Provider::Claude]);

    let failed = answered(&mut window, |w| {
        w.cli(w.first_machine(), CliAction::Update {
            provider: Provider::Codex,
        })
    });
    assert!(failed.unwrap_err().to_string().contains("installer failed"));
}
