//! Shared dispatch for the Rust executable and CEF's Windows bootstrap.

use ferrite_core::server::{settings_dir, store_dir};

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
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
