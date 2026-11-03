//! A rewind in the log (schema 15, docs/research/edit-rewind.md): what it
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
    };
    let fork = store.fork(id, None, Some(&point)).unwrap();
    let snapshot = store.load(fork).unwrap();
    assert_eq!(snapshot.prompt_texts(), vec!["one", "two", "three"]);
    assert_eq!(snapshot.resume_at(), Some(point));
    assert_eq!(snapshot.resume_target(), Some("sess"));
    drop(writer);
    let _ = fs::remove_dir_all(&dir);
}
