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
