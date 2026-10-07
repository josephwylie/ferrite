//! ADR 0008's mechanics: one writing process per store, how syncs order
//! the log, and how repair and upgrade keep every committed byte.

use super::compat_tests::{plant, scratch};
use super::*;

fn main() -> WorkspaceBinding {
    WorkspaceBinding::Main {
        checkout: "/fixture/repo".into(),
    }
}

/// Another process's claim on the store: a lock on a separate open of the
/// claim file, exactly what a second Ferrite's `Store::open` would hold.
fn claimed_elsewhere(dir: &Path) -> File {
    let other = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(CLAIM))
        .unwrap();
    other.lock().unwrap();
    other
}

/// A dev build beside the installed app: the second Ferrite reads every
/// Thread and changes none of them, and says why. Once the other lets go,
/// a later open may write.
#[test]
fn a_second_process_reads_the_store_but_never_writes_it() {
    let dir = scratch("claim-second");
    plant(
        &dir,
        4,
        concat!(
            r#"{"schema":12,"provider":"claude","workspace":null,"session_project_root":null,"model":null,"project_id":null,"title":null,"effort":null}"#,
            "\n",
            r#"{"type":"prompt","text":"hello"}"#,
            "\n",
        ),
    );
    let id = ThreadId::new(4);
    let log = dir.join("4").join("log.jsonl");
    let before = fs::read(&log).unwrap();

    let other = claimed_elsewhere(&dir);
    let store = Store::open(&dir).unwrap();
    assert!(store.read_only().is_some());
    assert_eq!(store.peek(id).unwrap().provider, Provider::Claude);
    assert_eq!(store.load(id).unwrap().prompt_texts(), vec!["hello"]);
    assert!(store.create(Provider::Claude, None, main()).is_err());
    assert!(store.writer(id).is_err());
    assert!(store.set_title(id, "renamed".into(), None).is_err());
    assert!(store.mark_open(id).is_err());
    assert!(store.delete(id).is_err());
    assert_eq!(fs::read(&log).unwrap(), before, "a read-only store wrote");
    assert_eq!(store.thread_ids().unwrap(), vec![id]);

    drop((other, store));
    let store = Store::open(&dir).unwrap();
    assert_eq!(store.read_only(), None, "the claim is free again");
    store.writer(id).unwrap();
}

/// Within one process every spelling of a store shares one claim, so a
/// second handle on it (a reader thread, the Groups file) is not mistaken
/// for a second Ferrite.
#[test]
fn every_spelling_of_a_store_shares_one_claim() {
    let dir = scratch("claim-spellings");
    fs::create_dir_all(dir.join("sub")).unwrap();
    let plain = Store::open(&dir).unwrap();
    let dotted = Store::open(dir.join("sub").join("..")).unwrap();
    assert!(std::sync::Arc::ptr_eq(&plain.shared, &dotted.shared));
    assert_eq!(dotted.read_only(), None);
    #[cfg(unix)]
    {
        let link = scratch("claim-spellings-link");
        std::os::unix::fs::symlink(&dir, &link).unwrap();
        let linked = Store::open(&link).unwrap();
        assert!(std::sync::Arc::ptr_eq(&plain.shared, &linked.shared));
        fs::remove_file(&link).unwrap();
    }
}

/// A file renamed over a log is written under a name no other writer
/// uses — not the fixed `log.jsonl.tmp` two Ferrites once shared.
#[test]
fn a_replaced_log_is_written_under_a_name_of_its_own() {
    let dir = scratch("unique-temp");
    let store = Store::open(&dir).unwrap();
    let (id, writer) = store.create(Provider::Claude, None, main()).unwrap();
    drop(writer);
    // Someone else's temp file, in the way of a shared name.
    fs::create_dir(dir.join(id.to_string()).join("log.jsonl.tmp")).unwrap();
    store.set_title(id, "renamed".into(), None).unwrap();
    assert_eq!(store.peek(id).unwrap().title.as_deref(), Some("renamed"));
}

fn turn_end() -> SessionEvent {
    SessionEvent::TurnEnded {
        outcome: crate::TurnOutcome::Completed,
        cost_usd: None,
    }
}

/// A Thread is durable through a power cut from the moment it exists.
#[test]
fn creating_a_thread_syncs_it_through_the_drive_cache() {
    let store = Store::open(scratch("sync-create")).unwrap();
    let (barriers, full) = store.syncs();
    store.create(Provider::Claude, None, main()).unwrap();
    assert_eq!(store.syncs(), (barriers, full + 1));
}

/// A turn's end orders the log behind a barrier — cheap enough that no
/// keystroke waits on it — and never empties the drive's cache.
#[test]
fn a_boundary_orders_the_log_without_a_full_sync() {
    let store = Store::open(scratch("sync-boundary")).unwrap();
    let (_, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    let (barriers, full) = store.syncs();
    writer.record_prompt("go").unwrap();
    writer.record_event(&turn_end(), None).unwrap();
    assert_eq!(store.syncs(), (barriers + 1, full));
}

/// Barriers bound what a crash costs; the interval bounds what a power cut
/// costs. A log written past it gets a full sync at its next flush.
#[test]
fn a_written_log_is_fully_synced_within_the_interval() {
    let store = Store::open(scratch("sync-interval"))
        .unwrap()
        .full_sync_every(std::time::Duration::from_millis(50));
    let (_, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    writer.record_event(&turn_end(), None).unwrap();
    let (barriers, full) = store.syncs();
    std::thread::sleep(std::time::Duration::from_millis(80));
    writer.record_event(&turn_end(), None).unwrap();
    assert_eq!(store.syncs(), (barriers, full + 1));
}

/// Parking leaves nothing behind a barrier only: everything written,
/// including what earlier boundaries only ordered, goes through the drive's
/// cache, even when the park itself has nothing new to write.
#[test]
fn a_full_flush_covers_what_barriers_only_ordered() {
    let store = Store::open(scratch("sync-park")).unwrap();
    let (_, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    writer.record_event(&turn_end(), None).unwrap();
    let (barriers, full) = store.syncs();
    writer.flush_fully().unwrap();
    assert_eq!(store.syncs(), (barriers, full + 1));
    writer.flush_fully().unwrap();
    assert_eq!(store.syncs(), (barriers, full + 1), "nothing new to sync");
}
