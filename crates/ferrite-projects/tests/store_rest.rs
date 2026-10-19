//! The store, what the ported lock test left out: the real lock (flock) leaves its lock files in the board's data
//! folder, one per name, and they stay there once let go.
use crate::common;
use common::*;

#[test]
fn the_real_lock_leaves_its_lock_files_in_the_data_folder() {
    let _w = world_with(Opts::default());
    let locks = sw::store::files::data_dir().join("locks");
    assert!(!locks.exists(), "nothing locked yet: no folder");
    let held = sw::store::files::hold_lock("x", 0).unwrap();
    assert!(locks.is_dir(), "only the real lock leaves lock files");
    drop(held);
    assert_eq!(sw::store::files::with_lock("y", || Ok(2)).unwrap(), 2);
    let names: Vec<String> = std::fs::read_dir(&locks).unwrap().map(|f| f.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.iter().all(|n| n.ends_with(".lock")), "{names:?}");
}
