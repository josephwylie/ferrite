//! `ferrite_cockpit::cockpit::changes_frames`, on the main thread, in the
//! frame the app installs.
fn main() {
    // The motion kit moves, as in the app (`ferrite::launch`).
    ferrite_ui::motion::testing::drive();
    ferrite_ui::frame::install(ferrite_frame::Frame { menus: Vec::new });
    let cases: Vec<String> = std::env::args().skip(1).collect();
    ferrite_cockpit::cockpit::changes_frames::run(&cases);
}
