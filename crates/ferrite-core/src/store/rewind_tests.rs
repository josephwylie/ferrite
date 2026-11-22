//! A rewind in the log (schema 15): what it
//! takes out of recall and the turn count, where the next Session resumes,
//! and the fork it can become.

use super::compat_tests::scratch;
use super::*;

fn main() -> WorkspaceBinding {
    WorkspaceBinding::Main {
        checkout: "/fixture/repo".into(),
    }
}

/// Three prompts sent under ids `p1`..`p3`, on a Session named `sess`.
fn three_prompts(store: &Store) -> (ThreadId, ThreadWriter) {
    let (id, mut writer) = store.create(Provider::Claude, None, main()).unwrap();
    writer
        .record_event(
            &SessionEvent::Init {
                session_id: "sess".into(),
                model: "claude-opus-5".into(),
            },
            None,
        )
        .unwrap();
    for (text, prompt) in [("one", "p1"), ("two", "p2"), ("three", "p3")] {
        writer.record_prompt(text).unwrap();
        writer
            .record_prompt_observation("7:31 pm", Some(prompt))
            .unwrap();
    }
    writer.flush().unwrap();
    (id, writer)
}

#[test]
fn a_rewind_takes_its_prompts_out_of_recall_for_good() {
    let dir = scratch("rewind-recall");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = three_prompts(&store);
    writer.record_rewind("p2", 2).unwrap();
    let snapshot = store.load(id).unwrap();
    assert_eq!(snapshot.prompt_texts(), vec!["one"]);
    // A replay of the log shows the cut: the prompts it took never return.
    let mut transcript = crate::transcript::Transcript::default();
    for input in snapshot.inputs() {
        transcript.apply(input);
    }
    let prompts: Vec<_> = transcript
        .blocks()
        .iter()
        .filter_map(|block| match &block.body {
            crate::transcript::Body::Prompt(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(prompts, vec!["one"]);
    drop(writer);
    let _ = fs::remove_dir_all(&dir);
}

/// A resume at an earlier entry holds until the conversation moves on from
/// it: the next prompt takes an in-place one, the next Init a forked one.
#[test]
fn a_resume_point_holds_until_the_conversation_takes_it() {
    let dir = scratch("rewind-resume");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = three_prompts(&store);
    let point = ResumePoint {
        session: "sess".into(),
        at: Some("a1".into()),
        fork: false,
        drops: None,
    };
    writer.record_resume_at(&point).unwrap();
    let snapshot = store.load(id).unwrap();
    assert_eq!(snapshot.resume_at(), Some(point));
    assert_eq!(snapshot.resume_target(), Some("sess"));

    writer.record_prompt("four").unwrap();
    writer.flush().unwrap();
    assert_eq!(store.load(id).unwrap().resume_at(), None);

    let forked = ResumePoint {
        session: "sess".into(),
        at: Some("a1".into()),
        fork: true,
        drops: None,
    };
    writer.record_resume_at(&forked).unwrap();
    writer
        .record_event(
            &SessionEvent::Init {
                session_id: "sess-forked".into(),
                model: "claude-opus-5".into(),
            },
            None,
        )
        .unwrap();
    writer.flush().unwrap();
    let snapshot = store.load(id).unwrap();
    assert_eq!(snapshot.resume_at(), None);
    assert_eq!(snapshot.resume_target(), Some("sess-forked"));
    drop(writer);
    let _ = fs::remove_dir_all(&dir);
}

/// A fork is a new Thread holding the conversation before the prompt, not
/// the Session that held it: it takes up the provider's fork instead.
#[test]
fn a_fork_copies_the_conversation_before_its_prompt() {
    let dir = scratch("rewind-fork");
    let store = Store::open(&dir).unwrap();
    let (id, writer) = three_prompts(&store);
    let point = ResumePoint {
        session: "sess".into(),
        at: Some("a1".into()),
        fork: true,
        drops: None,
    };
    let fork = store.fork(id, Some("p2"), Some(&point)).unwrap();
    assert_ne!(fork, id);
    let snapshot = store.load(fork).unwrap();
    assert_eq!(snapshot.prompt_texts(), vec!["one"]);
    assert_eq!(snapshot.resume_at(), Some(point));
    assert_eq!(snapshot.resume_target(), Some("sess"));
    // The source is untouched.
    assert_eq!(
        store.load(id).unwrap().prompt_texts(),
        vec!["one", "two", "three"]
    );
    assert!(store.fork(id, Some("p9"), None).is_err());
    drop(writer);
    let _ = fs::remove_dir_all(&dir);
}

/// A fork of the whole conversation copies every prompt and resumes the
/// provider's conversation at its end, forked: the point names no entry,
/// and still stands until the fork's own Init takes it.
#[test]
fn a_whole_fork_copies_everything_and_resumes_at_the_end() {
    let dir = scratch("rewind-fork-whole");
    let store = Store::open(&dir).unwrap();
    let (id, writer) = three_prompts(&store);
    let point = ResumePoint {
        session: "sess".into(),
        at: None,
        fork: true,
        drops: None,
    };
    let fork = store.fork(id, None, Some(&point)).unwrap();
    let snapshot = store.load(fork).unwrap();
    assert_eq!(snapshot.prompt_texts(), vec!["one", "two", "three"]);
    assert_eq!(snapshot.resume_at(), Some(point));
    assert_eq!(snapshot.resume_target(), Some("sess"));
    drop(writer);
    let _ = fs::remove_dir_all(&dir);
}

/// An older log holds a prompt twice under one id: the prompt as sent, then
/// its echo (taken for a queued prompt starting, a779f0e3), the echo with
/// its skill's `/name` lost — and the rewind to it, counting one. The cut
/// still takes the prompt as sent, from recall and from the replay
/// (the record shapes of a real log, its words replaced).
#[test]
fn a_rewind_takes_the_prompt_its_old_echo_shares_an_id_with() {
    let dir = scratch("rewind-echo");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = store.create(Provider::Codex, None, main()).unwrap();
    writer.record_prompt("earlier").unwrap();
    writer
        .record_prompt_observation("12:40 pm", Some("p0"))
        .unwrap();
    writer.record_prompt("/skill-x just a test").unwrap();
    writer
        .record_prompt_observation("12:48 pm", Some("p1"))
        .unwrap();
    writer.record_prompt("just a test").unwrap();
    writer
        .record_prompt_observation("12:48 pm", Some("p1"))
        .unwrap();
    writer.record_rewind("p1", 1).unwrap();
    writer.flush().unwrap();
    let snapshot = store.load(id).unwrap();
    assert_eq!(snapshot.prompt_texts(), vec!["earlier"]);
    let mut transcript = crate::transcript::Transcript::default();
    for input in snapshot.inputs() {
        transcript.apply(input);
    }
    let prompts: Vec<_> = transcript
        .blocks()
        .iter()
        .filter_map(|block| match &block.body {
            crate::transcript::Body::Prompt(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(prompts, vec!["earlier"]);
    drop(writer);
    let _ = fs::remove_dir_all(&dir);
}

/// A fork is the operator's own and started here: it keeps neither its
/// source's owner nor the Thread on another machine that delegated it, nor
/// the start key that finds the source again, nor its cap.
#[test]
fn a_fork_drops_who_delegated_its_source_and_its_start_key() {
    let dir = scratch("fork-delegated");
    let store = Store::open(&dir).unwrap();
    let delegated_from = crate::DelegatedFrom {
        server: crate::server::ServerId::new("s-mac"),
        label: "mac".into(),
        thread: ThreadId::new(12),
        title: "Release checklist".into(),
    };
    let origin = Origin {
        owner: Some(crate::ThreadOwner::Task("t-1".into())),
        delegated_from: Some(delegated_from.clone()),
        request: Some("delegate:s-mac:12:k-1".into()),
        cap: Some(crate::runtime_mode::RuntimeMode::ApprovalRequired),
    };
    let (id, mut writer) = store
        .create_owned(Provider::Claude, None, None, main(), origin)
        .unwrap();
    writer.record_prompt("one").unwrap();
    writer.flush().unwrap();

    let fork = store.fork(id, None, None).unwrap();
    let forked = store.peek(fork).unwrap();
    assert_eq!(
        (forked.owner, forked.delegated_from, forked.request, forked.cap),
        (None, None, None, None)
    );
    assert_eq!(store.load(fork).unwrap().delegated_from(), None);
    let source = store.peek(id).unwrap();
    assert_eq!(source.delegated_from, Some(delegated_from));
    assert_eq!(
        source.cap,
        Some(crate::runtime_mode::RuntimeMode::ApprovalRequired)
    );
    assert_eq!(store.started_by("delegate:s-mac:12:k-1").unwrap(), Some(id));
    assert_eq!(store.started_by("delegate:s-mac:12:k-2").unwrap(), None);
    drop(writer);
    let _ = fs::remove_dir_all(&dir);
}

/// A watched PR's watch is kept on its Thread's header and comes back with
/// it; a fork watches nothing, since the watch's wakes are its source's
/// agent's. A wake is kept as a wake, never as the operator's prompt.
#[test]
fn a_fork_watches_nothing_and_a_wake_is_no_prompt() {
    let dir = scratch("rewind-fork-watch");
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = three_prompts(&store);
    let mut fact = crate::pr_watch::WatchFact::default();
    let target = crate::pr_watch::Target::of("acme/app", 7).unwrap();
    assert!(!fact.watch(target, None, None, 1));
    store
        .set_pr_watch(id, fact.clone(), Some(&mut writer))
        .unwrap();
    writer
        .record_wake(&crate::wake::Wake {
            text: "Update on pull request #7".into(),
            outcome: crate::wake::Outcome::Failed,
            summary: "#7: checks failed".into(),
        })
        .unwrap();
    writer.flush().unwrap();
    let snapshot = store.load(id).unwrap();
    assert_eq!(snapshot.pr_watch(), &fact);
    assert_eq!(snapshot.prompt_texts(), vec!["one", "two", "three"]);
    let fork = store.fork(id, None, None).unwrap();
    assert!(store.load(fork).unwrap().pr_watch().is_empty());
    drop(writer);
    let _ = fs::remove_dir_all(&dir);
}
