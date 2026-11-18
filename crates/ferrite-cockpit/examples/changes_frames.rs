//! `ferrite_cockpit::cockpit::changes_frames`, on the main thread, in the
//! frame the app installs.
fn main() {
    ferrite_ui::frame::install(ferrite_frame::Frame { menus: Vec::new });
    let cases: Vec<String> = std::env::args().skip(1).collect();
    ferrite_cockpit::cockpit::changes_frames::run(&cases);
}
