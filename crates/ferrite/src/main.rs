// A GUI app, not a console one: without this Windows hands ferrite.exe its
// own console window at launch. Left on in debug builds so `cargo run`
// still prints.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ferrite::run();
}
