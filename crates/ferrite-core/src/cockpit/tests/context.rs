//! Agent context lives in the provider CLI (resumed by id) and in the log
//! (carried on a hand-over). ADR 0008's invariants 2 and 3, end to end
//! through park and revive.

use super::*;

fn live(fake: &Fake) -> Sender<SessionEvent> {
    fake.streams.borrow().last().unwrap().clone()
}

fn last_resume(fake: &Fake) -> Option<String> {
    fake.resumed.borrow().last().unwrap().clone()
}

/// Invariant 2: the id a revive resumes is exactly the newest the log
/// recorded — through parks, a conversation reset, and a provider switch
/// whose carry is still owed.
#[test]
fn invariant_resume_ids_persist_exactly() {
    let (mut cockpit, fake) = cockpit("invariant-resume");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.send(thread, "one".into());
    live(&fake)
        .send(SessionEvent::Init {
            session_id: "sess-1".into(),
            model: "claude-opus-5".into(),
        })
        .unwrap();
    live(&fake).send(text("first")).unwrap();
    live(&fake).send(ended()).unwrap();
    cockpit.pump();
    cockpit.park(thread).unwrap();
    cockpit.revive(thread).unwrap();
    assert_eq!(last_resume(&fake).as_deref(), Some("sess-1"));

    // A /clear names a new conversation; the next revive resumes that.
    live(&fake)
        .send(SessionEvent::ConversationReset {
            session_id: "sess-2".into(),
        })
        .unwrap();
    cockpit.pump();
    cockpit.park(thread).unwrap();
    cockpit.revive(thread).unwrap();
    assert_eq!(last_resume(&fake).as_deref(), Some("sess-2"));

    // A switch whose carry has not gone out: nothing to resume.
    cockpit
        .set_provider(
            thread,
            ProviderChoice {
                provider: Provider::Codex,
                model: None,
            },
        )
        .unwrap();
    cockpit.pump();
    cockpit.park(thread).unwrap();
    cockpit.revive(thread).unwrap();
    assert_eq!(fake.providers.borrow().last(), Some(&Provider::Codex));
    assert_eq!(last_resume(&fake), None);

    // Once the carry went out and Codex named its thread, that id resumes.
    cockpit.send(thread, "two".into());
    live(&fake)
        .send(SessionEvent::Init {
            session_id: "codex-1".into(),
            model: "gpt-5.4".into(),
        })
        .unwrap();
    live(&fake).send(ended()).unwrap();
    cockpit.pump();
    cockpit.park(thread).unwrap();
    cockpit.revive(thread).unwrap();
    assert_eq!(last_resume(&fake).as_deref(), Some("codex-1"));
}

/// A parked Thread has no writer left to sync it later: parking takes its
/// log through the drive's cache.
#[test]
fn parking_syncs_the_log_through_the_drive_cache() {
    let (mut cockpit, fake) = cockpit("park-full-sync");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    cockpit.send(thread, "one".into());
    live(&fake).send(ended()).unwrap();
    cockpit.pump();
    let (_, full) = cockpit.store.syncs();
    cockpit.park(thread).unwrap();
    assert_eq!(cockpit.store.syncs().1, full + 1);
}

/// Invariant 3: a hand-over after a revive carries what the whole log
/// holds, even when the history is longer than what Activity retains.
#[test]
fn invariant_a_hand_over_after_revive_carries_the_whole_history() {
    let (mut cockpit, fake) = cockpit("invariant-carry");
    let thread = cockpit.open(Provider::Claude, main_choice()).unwrap();
    let answer = |n: usize| format!("answer {n} {}", "z".repeat(700_000));
    let mut exchanges = Vec::new();
    for n in 0..8 {
        cockpit.send(thread, format!("question {n}"));
        live(&fake).send(text(&answer(n))).unwrap();
        live(&fake).send(ended()).unwrap();
        cockpit.pump();
        exchanges.push((format!("question {n}"), answer(n)));
    }
    cockpit.park(thread).unwrap();
    cockpit.revive(thread).unwrap();
    cockpit
        .set_provider(
            thread,
            ProviderChoice {
                provider: Provider::Codex,
                model: None,
            },
        )
        .unwrap();
    cockpit.pump();
    cockpit.send(thread, "next".into());
    assert_eq!(
        fake.sent.borrow().last().unwrap(),
        &format!("{}\n\nnext", carry_digest(Provider::Claude, &exchanges))
    );
    let _ = std::fs::remove_dir_all(cockpit.store.dir());
}

/// What the store costs on a real log, end to end through the Cockpit:
/// the parked row's lookups, a rename parked and live, and a revive (the
/// first one also upgrades an old log). Set `LAB_LOG` to a log to copy;
/// the copy's binding is pointed at a scratch checkout, so nothing outside
/// the scratch directory is touched. Run in release for real numbers.
#[test]
#[ignore = "needs LAB_LOG, a copy of a real Thread log"]
fn lab_store_costs_on_a_real_log() {
    use std::time::Instant;
    let Ok(source) = std::env::var("LAB_LOG") else {
        return;
    };
    let dir = scratch("lab-costs");
    let checkout = dir.join("checkout");
    std::fs::create_dir_all(dir.join("threads").join("3")).unwrap();
    std::fs::create_dir_all(&checkout).unwrap();
    // A clone on APFS: no second copy of the bytes until one is written.
    std::fs::copy(&source, dir.join("threads").join("3").join("log.jsonl")).unwrap();
    let store = Store::open(dir.join("threads")).unwrap();
    // Point the binding at the scratch checkout, so a revive asks no git of
    // the operator's own repos.
    store
        .set_workspace(
            ThreadId::new(3),
            &WorkspaceBinding::Main {
                checkout: checkout.clone(),
            },
            None,
        )
        .unwrap();
    let fake = Fake::default();
    let mut cockpit = Cockpit::new(store, Box::new(fake.clone()));
    let thread = ThreadId::new(3);
    let ms = |since: Instant| since.elapsed().as_secs_f64() * 1000.0;

    let started = Instant::now();
    cockpit.peek(thread).unwrap();
    eprintln!("LAB peek_ms={:.3}", ms(started));
    let started = Instant::now();
    let reader = cockpit.log_reader();
    let turns = reader.turn_count(thread).unwrap();
    let subagents = reader.subagent_count(thread).unwrap();
    eprintln!(
        "LAB parked_lookup_ms={:.1} turns={turns} subagents={subagents}",
        ms(started)
    );
    let started = Instant::now();
    cockpit.rename_thread(thread, "lab parked title").unwrap();
    eprintln!("LAB rename_parked_ms={:.1}", ms(started));
    for round in 0..2 {
        let started = Instant::now();
        cockpit.revive(thread).unwrap();
        eprintln!("LAB revive#{round}_ms={:.1}", ms(started));
        let started = Instant::now();
        cockpit.rename_thread(thread, "lab live title").unwrap();
        eprintln!("LAB rename_live#{round}_ms={:.1}", ms(started));
        let started = Instant::now();
        cockpit.park(thread).unwrap();
        eprintln!("LAB park#{round}_ms={:.1}", ms(started));
    }
    let started = Instant::now();
    let reader = cockpit.log_reader();
    let turns = reader.turn_count(thread).unwrap();
    let subagents = reader.subagent_count(thread).unwrap();
    eprintln!(
        "LAB parked_lookup_after_ms={:.1} turns={turns} subagents={subagents}",
        ms(started)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
