//! Ferrite: the popup's tab watch asks nothing of herdr's socket. Upstream's popup is a herdr pane, closed the moment
//! herdr shows another tab (`run::start::watch_tab`, on herdr's `tab.focused` events). Ferrite's board is drawn in its
//! window, which takes it down itself, and its terminal's tab events are refused through the seam
//! (`socket::on_tab_shown`). So the watch connects to nothing and closes nothing; upstream's test of it
//! (`board_stay_keys.rs`) waits on a herdr that would answer and stays ignored.
use std::os::unix::net::UnixListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn the_tab_watch_asks_herdr_nothing_and_never_closes_the_board() {
    let dir = std::env::temp_dir().join(format!("ferrite-tab-watch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("herdr.sock");
    let herdr = UnixListener::bind(&path).unwrap();
    herdr.set_nonblocking(true).unwrap();

    let closed = Arc::new(AtomicUsize::new(0));
    let told = closed.clone();
    sw_board::board::run::start::watch_tab(Some(path.display().to_string()), "w9:t1", move || {
        told.fetch_add(1, Ordering::SeqCst);
    });
    // Where herdr's watch would have connected and asked by now, nothing came.
    std::thread::sleep(Duration::from_millis(200));
    let asked = herdr.accept();
    assert!(
        matches!(&asked, Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "herdr's socket was asked: {asked:?}"
    );
    assert_eq!(closed.load(Ordering::SeqCst), 0, "the board was told to close");
    let _ = std::fs::remove_dir_all(&dir);
}
