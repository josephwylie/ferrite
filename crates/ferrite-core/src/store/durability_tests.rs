//! ADR 0009's mechanics: one writing process per store, how syncs order
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
    store.settle_all();
    assert_eq!(store.syncs(), (barriers + 1, full));
}

/// Barriers bound what a crash costs; the interval bounds what a power cut
/// costs. A log written and left alone is fully synced once the interval
/// passes — by the worker, with nothing more written.
#[test]
fn a_written_log_is_fully_synced_within_the_interval() {
    let store = Store::open(scratch("sync-interval"))
        .unwrap()
        .full_sync_every(std::time::Duration::from_millis(50));
    let (_, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    writer.record_event(&turn_end(), None).unwrap();
    let (_, full) = store.syncs();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while store.syncs().1 == full {
        assert!(std::time::Instant::now() < deadline, "never fully synced");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Parking leaves nothing behind a barrier only: everything written,
/// including what earlier boundaries only ordered, goes through the drive's
/// cache, even when the park itself has nothing new to write.
#[test]
fn a_full_flush_covers_what_barriers_only_ordered() {
    let store = Store::open(scratch("sync-park")).unwrap();
    let (_, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    writer.record_event(&turn_end(), None).unwrap();
    store.settle_all();
    let (barriers, full) = store.syncs();
    writer.flush_fully().unwrap();
    assert_eq!(store.syncs(), (barriers, full + 1));
    writer.flush_fully().unwrap();
    assert_eq!(store.syncs(), (barriers, full + 1), "nothing new to sync");
}

/// A current-schema log as some crash left it: its header and whole
/// records, then `tail`.
fn current_log(tail: &str) -> String {
    format!(
        "{}\n{}\n{}\n{tail}",
        format_args!(
            r#"{{"schema":{SCHEMA_VERSION},"provider":"codex","workspace":null,"session_project_root":null,"model":null,"project_id":null,"title":null,"effort":null}}"#
        ),
        r#"{"type":"prompt","text":"before the crash"}"#,
        r#"{"type":"turn_ended","outcome":"completed","cost_usd":null}"#,
    )
}

#[cfg(unix)]
fn inode(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path).unwrap().ino()
}

/// A crash's fragment is cut off where it lies: the log keeps its inode and
/// every byte before the tear, is not parsed whole twice, and
/// the fragment itself is kept beside it.
#[test]
fn a_torn_tail_is_cut_off_in_place_and_read_once() {
    let dir = scratch("repair-torn");
    let planted = current_log(r#"{"type":"text","te"#);
    plant(&dir, 4, &planted);
    let log = dir.join("4").join("log.jsonl");
    #[cfg(unix)]
    let before = inode(&log);
    let store = Store::open(&dir).unwrap();
    let read = store.bytes_read();

    let mut writer = store.writer(ThreadId::new(4)).unwrap();

    assert!(
        store.bytes_read() - read <= 2 * planted.len() as u64,
        "the log was parsed whole more than once"
    );
    #[cfg(unix)]
    assert_eq!(inode(&log), before, "the log was rewritten, not repaired");
    let readable = planted.len() - r#"{"type":"text","te"#.len();
    assert_eq!(fs::read(&log).unwrap(), &planted.as_bytes()[..readable]);
    assert_eq!(
        fs::read_to_string(dir.join("4").join(format!("log.damaged-{readable}.jsonl"))).unwrap(),
        r#"{"type":"text","te"#
    );
    writer.record_prompt("after the crash").unwrap();
    writer.flush().unwrap();
    assert_eq!(
        store.load(ThreadId::new(4)).unwrap().prompt_texts(),
        vec!["before the crash", "after the crash"]
    );
}

/// A record torn between its last brace and its newline is whole: it keeps
/// its place and gets its newline.
#[test]
fn a_last_record_missing_only_its_newline_is_kept() {
    let dir = scratch("repair-newline");
    let planted = current_log(r#"{"type":"prompt","text":"last"}"#);
    plant(&dir, 4, &planted);
    let log = dir.join("4").join("log.jsonl");
    let store = Store::open(&dir).unwrap();
    drop(store.writer(ThreadId::new(4)).unwrap());
    assert_eq!(fs::read_to_string(&log).unwrap(), format!("{planted}\n"));
    assert_eq!(
        store.load(ThreadId::new(4)).unwrap().prompt_texts(),
        vec!["before the crash", "last"]
    );
}

/// Damage inside a log hides the records after it from every reader. They
/// are cut so appends can be read again — but kept, every byte, first.
#[test]
fn damaged_records_are_kept_before_they_are_cut() {
    let dir = scratch("repair-damaged");
    let damaged = concat!(
        "not a record\n",
        r#"{"type":"prompt","text":"hidden by the damage"}"#,
        "\n"
    );
    let planted = current_log(damaged);
    plant(&dir, 4, &planted);
    let store = Store::open(&dir).unwrap();
    drop(store.writer(ThreadId::new(4)).unwrap());
    let readable = planted.len() - damaged.len();
    assert_eq!(
        fs::read(dir.join("4").join("log.jsonl")).unwrap(),
        &planted.as_bytes()[..readable]
    );
    assert_eq!(
        fs::read_to_string(dir.join("4").join(format!("log.damaged-{readable}.jsonl"))).unwrap(),
        damaged
    );
}

/// If what would be cut cannot be kept, nothing is cut: the writer is
/// refused and the log is exactly as it was.
#[test]
fn a_repair_that_cannot_keep_the_damage_cuts_nothing() {
    let dir = scratch("repair-refused");
    let planted = current_log("not a record\n");
    plant(&dir, 4, &planted);
    let readable = planted.len() - "not a record\n".len();
    // Something already stands where the damage would be kept.
    fs::create_dir(dir.join("4").join(format!("log.damaged-{readable}.jsonl"))).unwrap();
    let store = Store::open(&dir).unwrap();
    assert!(store.writer(ThreadId::new(4)).is_err());
    assert_eq!(fs::read_to_string(dir.join("4").join("log.jsonl")).unwrap(), planted);
}

fn temps(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("log.jsonl.tmp-"))
        .collect()
}

/// An upgrade cut short at any step — a crash mid-write included — leaves
/// one whole log or the other, never a lost record; the next open finishes
/// it, sweeps what the crash left, and keeps every record byte for byte.
#[test]
fn an_interrupted_upgrade_never_loses_a_record() {
    use super::compat_tests::{history_of, observed, record_bytes, FIXTURES};
    for step in [
        ReplaceStep::Write,
        ReplaceStep::Sync,
        ReplaceStep::Rename,
        ReplaceStep::Durable,
    ] {
        for (name, fixture, _, _) in FIXTURES {
            let schema = parse(ThreadId::new(0), fixture.as_bytes())
                .unwrap()
                .snapshot
                .schema;
            if schema == SCHEMA_VERSION {
                continue; // Nothing to upgrade.
            }
            let dir = scratch(&format!("upgrade-fault-{step:?}-{name}"));
            plant(&dir, 5, fixture);
            let id = ThreadId::new(5);
            let thread_dir = dir.join("5");
            let log = thread_dir.join("log.jsonl");
            let store = Store::open(&dir).unwrap();
            let before = observed(&store, id);

            store.faults().fail_replace_at(Some(step));
            assert!(store.writer(id).is_err(), "{step:?} {name}");
            match step {
                ReplaceStep::Write => {
                    assert_eq!(temps(&thread_dir).len(), 1, "a crash leaves its temp");
                    assert_eq!(fs::read_to_string(&log).unwrap(), *fixture);
                }
                ReplaceStep::Sync | ReplaceStep::Rename => {
                    assert!(temps(&thread_dir).is_empty());
                    assert_eq!(fs::read_to_string(&log).unwrap(), *fixture);
                }
                ReplaceStep::Durable => assert!(temps(&thread_dir).is_empty()),
            }
            // Whichever log is in place shows exactly what the old one did.
            let reopened = Store::open(&dir).unwrap();
            assert_eq!(
                history_of(&observed(&reopened, id)),
                history_of(&before),
                "{step:?} {name}"
            );

            store.faults().fail_replace_at(None);
            drop(reopened.writer(id).unwrap());
            assert!(temps(&thread_dir).is_empty(), "the crash's temp is swept");
            assert_eq!(observed(&reopened, id), before, "{step:?} {name}");
            assert!(record_bytes(&fs::read(&log).unwrap())
                .starts_with(&record_bytes(fixture.as_bytes())));
            let _ = fs::remove_dir_all(&dir);
        }
    }
}

/// A Thread whose log runs to about `megabytes`, in turns of ~10 KB — long
/// enough that the writer has marked it every megabyte or so.
fn long_thread(store: &Store, megabytes: usize) -> (ThreadId, ThreadWriter) {
    let (id, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    for turn in 0..megabytes * 100 {
        if turn % 25 == 0 {
            // Real turns pause; the worker syncs (and marks) in between.
            store.settle_all();
        }
        writer.record_prompt(&format!("turn {turn}")).unwrap();
        writer
            .record_event(
                &SessionEvent::TextDelta {
                    text: "w".repeat(10_000),
                },
                None,
            )
            .unwrap();
        writer.record_event(&turn_end(), None).unwrap();
    }
    // The worker marks behind its barriers: let it catch up.
    store.settle_all();
    (id, writer)
}

fn lines(log: &Path) -> Vec<String> {
    fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

/// Renaming a Thread appends one line. The log keeps its inode and every
/// byte it had — live or parked.
#[test]
fn an_amendment_appends_and_moves_no_earlier_byte() {
    let dir = scratch("amend-append");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    writer.record_prompt("hello").unwrap();
    writer.record_event(&turn_end(), None).unwrap();
    let log = dir.join(id.to_string()).join("log.jsonl");
    let before = fs::read(&log).unwrap();
    #[cfg(unix)]
    let inode_before = inode(&log);

    store.set_title(id, "live".into(), Some(&mut writer)).unwrap();
    drop(writer);
    store.set_title(id, "parked".into(), None).unwrap();

    assert!(fs::read(&log).unwrap().starts_with(&before));
    #[cfg(unix)]
    assert_eq!(inode(&log), inode_before);
    assert!(lines(&log)
        .iter()
        .any(|line| line.starts_with(r#"{"type":"facts""#) && line.contains(r#""title":"live""#)));
    assert_eq!(store.peek(id).unwrap().title.as_deref(), Some("parked"));
    assert_eq!(store.load(id).unwrap().title(), Some("parked"));
    assert_eq!(
        lines(&log)[0],
        before.split(|b| *b == b'\n').next().map(|l| String::from_utf8_lossy(l).into_owned()).unwrap(),
        "the header line is never rewritten"
    );
}

/// On a long log, a peek, a parked rename and a reopen read back only to
/// the newest mark — never the whole log.
#[test]
fn peeks_amendments_and_reopens_read_only_the_tail_of_a_long_log() {
    let dir = scratch("amend-tail");
    let store = Store::open(&dir).unwrap();
    let (id, writer) = long_thread(&store, 4);
    drop(writer);
    let size = fs::metadata(dir.join(id.to_string()).join("log.jsonl"))
        .unwrap()
        .len();
    assert!(size > 4_000_000);
    let bound = 2 * MARK_SPACING + 256 * 1024;
    let peek = || drop(store.peek(id).unwrap());
    let rename = || store.set_title(id, "renamed".into(), None).unwrap();
    let reopen = || drop(store.writer(id).unwrap());
    let acts: [(&str, &dyn Fn()); 3] = [
        ("peek", &peek),
        ("parked rename", &rename),
        ("reopen", &reopen),
    ];
    for (what, act) in acts {
        let start = store.bytes_read();
        act();
        let read = store.bytes_read() - start;
        assert!(read <= bound, "{what} read {read} bytes of a {size} byte log");
    }
    assert_eq!(store.peek(id).unwrap().title.as_deref(), Some("renamed"));
}

/// Setters send changes, not restatements: whatever the caller believes
/// the provider is, a model change or a rename after a provider switch
/// keeps the switch (ADR 0009, review finding 3).
#[test]
fn a_change_after_a_provider_switch_never_undoes_it() {
    let dir = scratch("amend-after-switch");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    writer.record_prompt("hello").unwrap();
    writer.record_event(&turn_end(), None).unwrap();
    store
        .hand_over(id, Provider::Codex, Some("gpt-5.4".into()), &mut writer)
        .unwrap();
    store
        .set_title(id, "after the switch".into(), Some(&mut writer))
        .unwrap();
    drop(writer);
    store
        .set_tuning(id, Some("gpt-5.5".into()), Some("high".into()), None)
        .unwrap();

    let meta = store.peek(id).unwrap();
    assert_eq!(meta.provider, Provider::Codex);
    assert_eq!(meta.model.as_deref(), Some("gpt-5.5"));
    assert_eq!(meta.title.as_deref(), Some("after the switch"));
    let snapshot = store.load(id).unwrap();
    assert_eq!(snapshot.provider(), Provider::Codex);
    assert_eq!(snapshot.resume_target(), None, "the switch's carry is owed");
}

/// A switch commits in one line: the header still names the old provider,
/// and a log cut right after the switch already names the new one.
#[test]
fn a_handover_line_alone_commits_the_switch() {
    let dir = scratch("handover-line");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    writer.record_prompt("hello").unwrap();
    writer.record_event(&turn_end(), None).unwrap();
    store
        .hand_over(id, Provider::Codex, None, &mut writer)
        .unwrap();
    drop(writer);
    let log = dir.join(id.to_string()).join("log.jsonl");
    let all = lines(&log);
    assert!(all[0].contains(r#""provider":"claude""#), "the header is never rewritten");
    let switch = all
        .iter()
        .position(|line| line.starts_with(r#"{"type":"handover""#))
        .unwrap();
    assert!(all[switch].contains(r#""facts":{"provider":"codex""#));
    fs::write(&log, all[..=switch].join("\n") + "\n").unwrap();
    assert_eq!(store.peek(id).unwrap().provider, Provider::Codex);
    assert_eq!(store.load(id).unwrap().provider(), Provider::Codex);
}

/// An amendment reported as failed is taken back off the log, so it can
/// never land later with some other flush.
#[test]
fn a_failed_amendment_never_lands_later() {
    use std::sync::atomic::Ordering;
    let dir = scratch("amend-failed");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    writer.record_prompt("hello").unwrap();
    writer.record_event(&turn_end(), None).unwrap();
    let log = dir.join(id.to_string()).join("log.jsonl");
    let before = fs::read(&log).unwrap();

    store.faults().fail_commit.store(true, Ordering::SeqCst);
    assert!(store
        .set_title(id, "refused".into(), Some(&mut writer))
        .is_err());
    store.faults().fail_commit.store(false, Ordering::SeqCst);
    assert_eq!(fs::read(&log).unwrap(), before, "taken back off the log");

    writer.record_prompt("later").unwrap();
    writer.record_event(&turn_end(), None).unwrap();
    writer.flush_fully().unwrap();
    assert!(!fs::read_to_string(&log).unwrap().contains("refused"));
    assert_eq!(store.peek(id).unwrap().title, None);
}

/// When even taking a failed amendment back fails, the writer refuses to
/// append after it until the Thread is reopened — and repaired.
#[test]
fn a_failed_rollback_closes_the_writer_until_reopened() {
    use std::sync::atomic::Ordering;
    let dir = scratch("amend-rollback-failed");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    store.faults().fail_commit.store(true, Ordering::SeqCst);
    store.faults().fail_rollback.store(true, Ordering::SeqCst);
    assert!(store
        .set_title(id, "stuck".into(), Some(&mut writer))
        .is_err());
    store.faults().fail_commit.store(false, Ordering::SeqCst);
    store.faults().fail_rollback.store(false, Ordering::SeqCst);
    writer.record_prompt("after").unwrap();
    assert!(writer.flush().is_err(), "closed to appends");
    drop(writer);
    let mut reopened = store.writer(id).unwrap();
    reopened.record_prompt("after the reopen").unwrap();
    reopened.flush().unwrap();
    assert_eq!(store.load(id).unwrap().prompt_texts(), vec!["after the reopen"]);
}

/// Marks are bookkeeping: each restates the facts as the log has them at
/// that point, and a log with every mark removed reads exactly the same.
#[test]
fn marks_restate_the_facts_and_change_nothing_a_reader_sees() {
    use super::compat_tests::observed;
    let dir = scratch("marks-derived");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = long_thread(&store, 2);
    store.set_title(id, "midway".into(), Some(&mut writer)).unwrap();
    for turn in 0..150 {
        if turn % 25 == 0 {
            store.settle_all();
        }
        writer.record_prompt(&format!("more {turn}")).unwrap();
        writer
            .record_event(&SessionEvent::TextDelta { text: "m".repeat(10_000) }, None)
            .unwrap();
        writer.record_event(&turn_end(), None).unwrap();
    }
    writer.flush_fully().unwrap();
    drop(writer);
    let log = dir.join(id.to_string()).join("log.jsonl");
    let all = lines(&log);
    let mut facts = serde_json::from_str::<Header>(&all[0]).unwrap().facts();
    let mut marks = 0;
    for line in &all[1..] {
        let record: Record = serde_json::from_str(line).unwrap();
        if let Record::Mark { facts: restated, .. } = &record {
            assert_eq!(restated, &facts, "a mark restates the fold");
            marks += 1;
        } else if let Some(changed) = record.facts() {
            facts = changed.clone();
        }
    }
    assert!(marks >= 3, "{marks} marks in a 3.5 MB log");

    let unmarked: Vec<&String> = all
        .iter()
        .filter(|line| !line.starts_with(r#"{"type":"mark""#))
        .collect();
    let bare = scratch("marks-derived-bare");
    super::compat_tests::plant(
        &bare,
        id.get(),
        &(unmarked.iter().map(|line| line.as_str()).collect::<Vec<_>>().join("\n") + "\n"),
    );
    assert_eq!(observed(&Store::open(&bare).unwrap(), id), observed(&store, id));
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&bare);
}

/// After a crash, what follows the newest mark is all that is in doubt: a
/// hole there is kept and cut away, reading no further back than the mark.
#[test]
fn a_reopen_repairs_what_follows_the_newest_mark() {
    let dir = scratch("repair-after-mark");
    let store = Store::open(&dir).unwrap();
    let (id, writer) = long_thread(&store, 3);
    drop(writer);
    let log = dir.join(id.to_string()).join("log.jsonl");
    let good = fs::read(&log).unwrap();
    let damage = "\0\0\0\0 a hole a power cut left\n{\"type\":\"prompt\",\"text\":\"after the hole\"}\n";
    let mut file = OpenOptions::new().append(true).open(&log).unwrap();
    file.write_all(damage.as_bytes()).unwrap();
    drop(file);

    let start = store.bytes_read();
    drop(store.writer(id).unwrap());
    assert!(store.bytes_read() - start <= 2 * MARK_SPACING + 256 * 1024);
    assert_eq!(fs::read(&log).unwrap(), good);
    assert_eq!(
        fs::read_to_string(
            dir.join(id.to_string())
                .join(format!("log.damaged-{}.jsonl", good.len()))
        )
        .unwrap(),
        damage
    );
    let _ = fs::remove_dir_all(&dir);
}

/// An old log is upgraded once, ends in a mark of its facts, and is never
/// rewritten again.
#[test]
fn an_old_log_upgrades_once_and_ends_in_a_mark() {
    let dir = scratch("upgrade-mark");
    let (_, v8, _, _) = super::compat_tests::FIXTURES[1];
    plant(&dir, 6, v8);
    let store = Store::open(&dir).unwrap();
    let id = ThreadId::new(6);
    drop(store.writer(id).unwrap());
    let log = dir.join("6").join("log.jsonl");
    let all = lines(&log);
    assert!(all[0].starts_with(&format!(r#"{{"schema":{SCHEMA_VERSION},"#)));
    let last: Record = serde_json::from_str(all.last().unwrap()).unwrap();
    let Record::Mark { facts, .. } = last else {
        panic!("the upgrade ends in a mark: {}", all.last().unwrap());
    };
    assert_eq!(facts.provider, Provider::Codex);
    assert_eq!(facts.model.as_deref(), Some("gpt-5.4"), "the old handover is not refolded");
    #[cfg(unix)]
    let before = inode(&log);
    drop(store.writer(id).unwrap());
    #[cfg(unix)]
    assert_eq!(inode(&log), before, "upgraded once");
}

/// A parked row's counts come off the log's tail: the newest mark, plus
/// whatever was written after it.
#[test]
fn a_parked_thread_s_counts_come_from_its_tail() {
    let dir = scratch("summary-tail");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    for text in ["one", "two", "three"] {
        writer.record_prompt(text).unwrap();
    }
    writer
        .record_event(
            &SessionEvent::ConversationReset {
                session_id: "s-2".into(),
            },
            None,
        )
        .unwrap();
    writer.record_prompt("four").unwrap();
    writer.note_subagents(4);
    writer.flush_fully().unwrap();
    let summary = store.peek(id).unwrap().summary.unwrap();
    assert_eq!((summary.turns, summary.prompted, summary.subagents), (1, true, Some(4)));

    // After the last mark: counted from the lines that follow it.
    writer.record_prompt("five").unwrap();
    writer.record_event(&turn_end(), None).unwrap();
    let summary = store.peek(id).unwrap().summary.unwrap();
    assert_eq!((summary.turns, summary.subagents), (2, Some(4)));
    assert_eq!(store.load(id).unwrap().prompt_texts().len(), 2);
}

/// Whatever mix of prompts, resets and marks a log holds, its tail counts
/// agree with a full load.
#[test]
fn tail_counts_agree_with_a_full_load() {
    for seed in 0..24u64 {
        let dir = scratch(&format!("summary-property-{seed}"));
        let store = Store::open(&dir).unwrap();
        let (id, mut writer) = store.create(Provider::Codex, None, main()).unwrap();
        let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        for step in 0..60 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            match (state >> 33) % 7 {
                0 => writer
                    .record_event(
                        &SessionEvent::ConversationReset {
                            session_id: format!("s-{step}"),
                        },
                        None,
                    )
                    .unwrap(),
                1 => writer.flush_fully().unwrap(),
                2 => writer.record_event(&turn_end(), None).unwrap(),
                _ => writer.record_prompt(&format!("p{step}")).unwrap(),
            }
        }
        writer.flush().unwrap();
        let summary = store.peek(id).unwrap().summary.unwrap();
        let snapshot = store.load(id).unwrap();
        assert_eq!(summary.turns, snapshot.prompt_texts().len(), "seed {seed}");
        assert_eq!(
            summary.prompted,
            snapshot.inputs().iter().any(|input| matches!(input, Input::Prompt(_))),
            "seed {seed}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}

/// A parked pre-13 log is counted once, by its caller's replay, and
/// remembered beside the logs — and an upgrade keeps what was counted.
#[test]
fn an_old_parked_log_is_counted_once_and_the_count_survives_its_upgrade() {
    let dir = scratch("summary-old");
    let (_, v11, _, _) = super::compat_tests::FIXTURES[2];
    plant(&dir, 7, v11);
    let store = Store::open(&dir).unwrap();
    let id = ThreadId::new(7);
    let replays = std::cell::Cell::new(0);
    let count = |_: &ThreadSnapshot| {
        replays.set(replays.get() + 1);
        3
    };
    let first = store.summary(id, count).unwrap();
    assert_eq!((first.turns, first.subagents), (2, Some(3)));
    let read = store.bytes_read();
    let again = store.summary(id, count).unwrap();
    assert_eq!(again, first);
    assert_eq!(replays.get(), 1, "remembered, not replayed again");
    assert!(store.bytes_read() - read <= 64 * 1024, "only the header was read");
    assert!(dir.join(SUMMARIES).is_file());

    // The first rename upgrades the log; its mark keeps the count.
    store.set_title(id, "renamed".into(), None).unwrap();
    let upgraded = store.summary(id, count).unwrap();
    assert_eq!((upgraded.turns, upgraded.subagents), (2, Some(3)));
    assert_eq!(replays.get(), 1);
}

/// Whether a parked Thread is locked — any prompt ever sent — is known
/// from the tail of a long log, not a full load.
#[test]
fn a_parked_lock_check_reads_only_the_tail() {
    let dir = scratch("summary-lock");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = long_thread(&store, 3);
    writer.flush_fully().unwrap();
    drop(writer);
    let read = store.bytes_read();
    assert!(store.prompted(id).unwrap());
    assert!(store.bytes_read() - read <= 2 * MARK_SPACING + 256 * 1024);
    let _ = fs::remove_dir_all(&dir);
}

/// No caller waits on a drive: a turn's end is written at once and synced
/// by the store's worker, however slow the sync.
#[test]
fn a_boundary_never_waits_for_its_sync() {
    let dir = scratch("worker-boundary");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    *worker::lock(&store.faults().sync_delay) = std::time::Duration::from_millis(400);
    let (barriers, _) = store.syncs();
    let started = std::time::Instant::now();
    writer.record_prompt("go").unwrap();
    writer.record_event(&turn_end(), None).unwrap();
    assert!(started.elapsed() < std::time::Duration::from_millis(150));
    assert_eq!(store.load(id).unwrap().prompt_texts(), vec!["go"], "written at once");
    store.settle_all();
    assert_eq!(store.syncs().0, barriers + 1, "synced by the worker");
    *worker::lock(&store.faults().sync_delay) = std::time::Duration::ZERO;
}

/// A mark vouches for every line before it, so the worker writes one only
/// once a sync has covered all of them.
#[test]
fn a_mark_never_reaches_the_log_before_its_barrier() {
    let dir = scratch("worker-marks");
    let store = Store::open(&dir).unwrap();
    let (_, writer) = long_thread(&store, 3);
    drop(writer);
    store.settle_all();
    let journal = store.journal();
    let marks: Vec<u64> = journal
        .iter()
        .filter(|(what, _)| *what == "mark")
        .map(|(_, at)| *at)
        .collect();
    assert!(marks.len() >= 2, "{journal:?}");
    for (index, (what, at)) in journal.iter().enumerate() {
        if *what == "mark" {
            assert!(
                journal[..index]
                    .iter()
                    .any(|(what, synced)| *what == "synced" && synced >= at),
                "the mark at {at} came before a sync covered it: {journal:?}"
            );
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

/// A panic must not lose what a writer accepted and had not yet written.
#[test]
fn a_panic_rescue_writes_what_writers_hold() {
    let dir = scratch("rescue");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    writer
        .record_event(&SessionEvent::TextDelta { text: "mid-turn".into() }, None)
        .unwrap();
    assert!(store.load(id).unwrap().inputs().is_empty(), "only buffered");
    store.rescue();
    assert_eq!(
        store.load(id).unwrap().inputs(),
        vec![Input::Event(SessionEvent::TextDelta { text: "mid-turn".into() })]
    );
    drop(writer);
}

/// Renaming a Thread — live or parked — never waits on the drive: the
/// line is written at once, the worker syncs it.
#[test]
fn an_amendment_never_waits_for_the_drive() {
    let dir = scratch("amend-async");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    let (parked, parked_writer) = store.create(Provider::Codex, None, main()).unwrap();
    drop(parked_writer);
    store.settle_all();
    *worker::lock(&store.faults().sync_delay) = std::time::Duration::from_millis(400);
    let started = std::time::Instant::now();
    store.set_title(id, "live".into(), Some(&mut writer)).unwrap();
    store.set_title(parked, "parked".into(), None).unwrap();
    assert!(started.elapsed() < std::time::Duration::from_millis(250));
    assert_eq!(store.peek(id).unwrap().title.as_deref(), Some("live"));
    assert_eq!(store.peek(parked).unwrap().title.as_deref(), Some("parked"));
    *worker::lock(&store.faults().sync_delay) = std::time::Duration::ZERO;
    store.settle_all();
}

/// A parked-row lookup reads on a background thread while the UI thread
/// writes: the two never wait on each other's locks.
#[test]
fn reads_on_another_thread_never_deadlock_with_writes() {
    let dir = scratch("worker-locks");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    let (parked, parked_writer) = store.create(Provider::Codex, None, main()).unwrap();
    parked_writer.park();
    let reader = store.clone();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop = done.clone();
    let reading = std::thread::spawn(move || {
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            reader.peek(parked).unwrap();
            reader.peek(id).unwrap();
        }
    });
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_millis(500) {
        writer.record_prompt("again").unwrap();
        writer.record_event(&turn_end(), None).unwrap();
    }
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    reading.join().unwrap();
    store.rescue();
}

/// Deleting a Thread the worker is still finishing lets go of its log
/// first: on Windows no directory with an open handle can be removed.
#[test]
fn deleting_a_parking_thread_lets_go_of_its_log() {
    let dir = scratch("delete-parking");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    writer.record_prompt("hello").unwrap();
    writer.record_event(&turn_end(), None).unwrap();
    *worker::lock(&store.faults().sync_delay) = std::time::Duration::from_millis(300);
    writer.park();
    store.delete(id).unwrap();
    *worker::lock(&store.faults().sync_delay) = std::time::Duration::ZERO;
    let path = store.log_path(id);
    let worker = store.shared.worker.get().unwrap();
    assert!(
        worker.held().iter().all(|writer| worker::lock(writer).path != path),
        "a writer still holds the deleted Thread's log"
    );
    assert!(!dir.join(id.to_string()).exists());
}
