//! Shared dispatch for the Rust executable and CEF's Windows bootstrap.

use ferrite_core::server::{settings_dir, store_dir};

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    #[cfg(feature = "browser-smoke")]
    if args.get(1).map(String::as_str) == Some("--browser-window-stop") {
        std::process::exit(stop_browser_smoke_server());
    }
    // `ferrite server` is the headless Server the window attaches to (ADR
    // 0025): it never starts a window, Chromium or the self-update.
    if args.get(1).map(String::as_str) == Some("server") {
        std::process::exit(ferrite_core::server::run(ferrite_core::server::Options {
            store: store_dir(),
            settings_dir: settings_dir(),
            version: env!("CARGO_PKG_VERSION").into(),
            after: args
                .iter()
                .position(|arg| arg == "--after")
                .and_then(|at| args.get(at + 1))
                .and_then(|pid| pid.parse().ok()),
        }));
    }
    // `ferrite thread list`, `ferrite agent prompt …`, `ferrite --skill`:
    // the automation API's CLI (ADR 0027), which never starts a window.
    if let Some(code) = ferrite_core::api::cli::run(&args, &store_dir(), &settings_dir()) {
        std::process::exit(code);
    }
    crate::launch();
}

/// The packaged probe's owner stops its isolated Server after the window exits.
/// Reuses the same public socket protocol as the ordinary server smoke test.
#[cfg(feature = "browser-smoke")]
fn stop_browser_smoke_server() -> i32 {
    use ferrite_core::link::{self, Client};
    use ferrite_core::protocol::{self, ClientMessage, Op, Peer, When};
    use std::time::{Duration, Instant};
    let stop = || -> std::io::Result<()> {
        let address = link::address(&store_dir());
        let mut attached = Client::attach(
            &address,
            ClientMessage::Hello {
                generation: protocol::GENERATION,
                client: Peer {
                    name: "packaged-window-owner".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                },
                capabilities: Vec::new(),
            },
        )?;
        attached
            .client
            .send(&ClientMessage::Command(protocol::Command {
                id: 1,
                op: Op::Stop { when: When::Now },
            }))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while link::connect(&address).is_ok() {
            if Instant::now() >= deadline {
                return Err(std::io::Error::other("isolated Server did not stop"));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    };
    match stop() {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("packaged window cleanup: {error}");
            1
        }
    }
}
