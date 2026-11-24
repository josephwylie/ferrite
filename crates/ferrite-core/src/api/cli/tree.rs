//! `ferrite [--machine <name>] tree send <local-dir> <remote-dir> [--skip
//! <path>]…` (ADR 0048): make a directory where the Server runs hold what
//! one on this computer holds, sending only what differs. A tree moves
//! over the window's protocol, not the API: on a machine, through the
//! window's bridge, so `ferrited bridge` answers it there.

use std::path::{Path, PathBuf};

use serde_json::json;

use super::{unreachable, ApiError, Code};
use crate::chunked::Cancellation;
use crate::link;
use crate::machine::{self, Bridge, Machine};
use crate::protocol::{self, ClientMessage, Peer};
use crate::tree::{self, Root, Sent};

pub(super) const USAGE: &str = "  ferrite tree send <local-dir> <remote-dir> [--skip <path>]…\n      \
    Make <remote-dir>, where the Server runs, hold what <local-dir> holds,\n      \
    sending only what differs. Each --skip is a path inside both, left as it is.\n";

pub(super) fn run(machine: Option<Machine>, words: &[&str], store: &Path, settings_dir: &Path) -> i32 {
    let (source, dest, skip) = match words {
        [] | ["--help"] => {
            print!("Usage:\n{USAGE}");
            return 0;
        }
        ["send", args @ ..] => match parse(args) {
            Ok(parsed) => parsed,
            Err(error) => {
                eprint!("ferrite: {error}\n\nUsage:\n{USAGE}");
                return 2;
            }
        },
        [verb, ..] => {
            eprint!("ferrite: no command `tree {verb}`\n\nUsage:\n{USAGE}");
            return 2;
        }
    };
    let here = std::env::current_dir().unwrap_or_default();
    let source = here.join(source);
    let dest = match machine {
        None if !dest.is_absolute() && !dest.starts_with("~") => here.join(dest),
        _ => dest,
    };
    match send(machine.as_ref(), &source, dest, &skip, store, settings_dir) {
        Ok(sent) => {
            println!("{}", json!(sent));
            0
        }
        Err(error) => {
            eprintln!("{}", json!({ "error": error }));
            1
        }
    }
}

/// The local directory, the remote one, and the paths to skip.
fn parse(args: &[&str]) -> Result<(PathBuf, PathBuf, Vec<String>), String> {
    let mut skip = Vec::new();
    let mut dirs = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match *arg {
            "--skip" => skip.push(args.next().ok_or("--skip needs a path")?.to_string()),
            arg => match arg.strip_prefix("--skip=") {
                Some(path) => skip.push(path.to_string()),
                None if arg.starts_with("--") => return Err(format!("`tree send` takes no {arg}")),
                None => dirs.push(PathBuf::from(arg)),
            },
        }
    }
    match <[PathBuf; 2]>::try_from(dirs) {
        Ok([source, dest]) => Ok((source, dest, skip)),
        Err(_) => Err("`tree send` takes a local directory and a remote one".into()),
    }
}

fn send(
    machine: Option<&Machine>,
    source: &Path,
    dest: PathBuf,
    skip: &[String],
    store: &Path,
    settings_dir: &Path,
) -> Result<Sent, ApiError> {
    let hello = ClientMessage::Hello {
        generation: protocol::GENERATION,
        client: Peer {
            name: "ferrite tree send".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
        capabilities: Vec::new(),
    };
    let bridge = machine.map(|machine| Bridge::for_machine(machine, settings_dir));
    let attached = match &bridge {
        Some(bridge) => bridge.attach(hello),
        None => link::Client::attach(&link::address(store), hello),
    }
    .map_err(|error| match machine::failure(&error) {
        Some(failure) => unreachable(failure),
        None => ApiError::new(Code::ServerUnavailable, format!("no Ferrite server answered: {error}")),
    })?;
    tree::mirror(
        source,
        &Root::Path(dest),
        skip,
        &mut tree::Remote::new(attached.client),
        &mut |_| {},
        &Cancellation::default(),
    )
    .map_err(|error| ApiError::new(Code::Failed, error.to_string()))
}
