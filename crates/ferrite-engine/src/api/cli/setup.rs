//! `ferrite --machine <name> setup sync [--allow-removals]` (ADR 0048): sync
//! this computer's Claude and Codex setup with a Machine's, both ways, as
//! the window does on its own. It moves over the window's protocol, like
//! a tree, through the Machine's bridge.

use std::path::Path;

use serde_json::json;

use super::tree::attach;
use super::{ApiError, Code};
use ferrite_core::chunked::Cancellation;
use ferrite_core::machine::Machine;
use ferrite_core::setup_sync::{self, Home, Job, Outcome};

pub(super) const USAGE: &str = "  ferrite --machine <name> setup sync [--allow-removals]\n      \
    Sync this computer's Claude and Codex setup with the Machine's, both ways.\n      \
    --allow-removals goes on when it would remove more than half of an item, or all of one the other side lost.\n";

pub(super) fn run(machine: Option<Machine>, words: &[&str], store: &Path, settings_dir: &Path) -> i32 {
    let allow_removals = match words {
        [] | ["--help"] => {
            print!("Usage:\n{USAGE}");
            return 0;
        }
        ["sync"] => false,
        ["sync", "--allow-removals"] => true,
        _ => {
            eprint!("ferrite: no command `setup {}`\n\nUsage:\n{USAGE}", words.join(" "));
            return 2;
        }
    };
    let Some(machine) = machine else {
        eprint!("ferrite: `setup sync` takes --machine <name>: it syncs this computer's setup with that Machine's\n\nUsage:\n{USAGE}");
        return 2;
    };
    match sync(&machine, allow_removals, store, settings_dir) {
        Ok(outcome) => {
            println!("{}", json!(outcome));
            0
        }
        Err(error) => {
            eprintln!("{}", json!({ "error": error }));
            1
        }
    }
}

fn sync(machine: &Machine, allow_removals: bool, store: &Path, settings_dir: &Path) -> Result<Outcome, ApiError> {
    let failed = |error: std::io::Error| ApiError::new(Code::Failed, error.to_string());
    let job = Job {
        allow_removals,
        ..Job::for_machine(Home::from_env().map_err(failed)?, settings_dir, &machine.target)
    };
    let mut there = attach(Some(machine), "ferrite setup sync", store, settings_dir)?;
    setup_sync::run(&job, &mut there, &mut |_| {}, &Cancellation::default()).map_err(failed)
}
