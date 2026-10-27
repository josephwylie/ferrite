// A GUI app, not a console one: without this Windows hands ferrite.exe its
// own console window at launch. Left on in debug builds so `cargo run`
// still prints.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use ferrite_core::server::{settings_dir, store_dir};

fn main() {
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
    ferrite::launch();
}
