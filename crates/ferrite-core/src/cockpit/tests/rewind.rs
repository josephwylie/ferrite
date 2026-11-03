//! Edit, Rewind and Fork end to end (docs/research/edit-rewind.md): every
//! prompt goes out under an id, the agent is asked by it, and its answer
//! cuts the transcript, recall and the log, or opens a fork.

use super::*;

fn live(fake: &Fake) -> Sender<SessionEvent> {
    fake.streams.borrow().last().unwrap().clone()
}

/// The prompts `thread`'s Main transcript shows, with the ids they were
/// sent under.
fn prompts(cockpit: &Cockpit, thread: ThreadId) -> Vec<(String, Option<String>)> {
    cockpit
        .thread(thread)
        .unwrap()
        .transcript()
        .blocks()
        .iter()
        .filter_map(|block| match &block.body {
            Body::Prompt(text) => Some((text.clone(), block.prompt_id.clone())),
            _ => None,
        })
        .collect()
}

/// Two finished turns, `one` and `two`, on Session `sess-1`.
fn two_turns(name: &str) -> (Cockpit, Fake, ThreadId) {
    let (mut cockpit, fake) = cockpit(name);
    *fake.rewindable.borrow_mut() = true;
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
    cockpit.send(thread, "two".into());
    live(&fake).send(text("second")).unwrap();
    live(&fake).send(ended()).unwrap();
    cockpit.pump();
    (cockpit, fake, thread)
}

#[test]
fn an_edit_takes_its_turn_back_and_hands_its_words_to_the_composer() {
    let (mut cockpit, fake, thread) = two_turns("rewind-edit");
    let sent = prompts(&cockpit, thread);
    assert_eq!(sent.len(), 2);
    let (first, second) = (sent[0].1.clone().unwrap(), sent[1].1.clone().unwrap());
    assert_ne!(first, second, "every prompt goes out under its own id");

    cockpit.rewind(thread, &second, false, true).unwrap();
    cockpit.pump();

    let asked = fake.rewinds.borrow().last().unwrap().clone();
    assert_eq!(asked.prompt, second);
    assert_eq!(asked.latest.as_deref(), Some(second.as_str()));
    assert_eq!(asked.session.as_deref(), Some("sess-1"));
    assert!(!asked.files);
    assert_eq!(prompts(&cockpit, thread), vec![("one".into(), Some(first))]);
    assert_eq!(
        cockpit.take_retrieved_prompt(thread),
        Some(("two".into(), false))
    );
}

#[test]
fn a_rewind_of_code_says_what_came_back() {
    let (mut cockpit, _fake, thread) = two_turns("rewind-code");
    let first = prompts(&cockpit, thread)[0].1.clone().unwrap();
    cockpit.rewind(thread, &first, true, false).unwrap();
    cockpit.pump();
    assert!(prompts(&cockpit, thread).is_empty());
    assert_eq!(cockpit.take_retrieved_prompt(thread), None);
    let said = cockpit
        .thread(thread)
        .unwrap()
        .transcript()
        .blocks()
        .iter()
        .rev()
        .find_map(|block| match &block.body {
            Body::Notice(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    assert!(said.contains("1 file, +2 \u{2212}3 restored"), "{said}");
}

#[test]
fn a_rewind_never_races_a_running_turn() {
    let (mut cockpit, fake, thread) = two_turns("rewind-mid-turn");
    let first = prompts(&cockpit, thread)[0].1.clone().unwrap();
    cockpit.send(thread, "three".into());
    live(&fake).send(text("thi")).unwrap();
    cockpit.pump();
    let refused = cockpit.rewind(thread, &first, false, false).unwrap_err();
    assert!(refused.contains("turn is running"), "{refused}");
    assert!(fake.rewinds.borrow().is_empty());
}

#[test]
fn a_fork_opens_a_new_thread_with_the_conversation_before_its_prompt() {
    let (mut cockpit, _fake, thread) = two_turns("rewind-fork");
    let second = prompts(&cockpit, thread)[1].1.clone().unwrap();
    cockpit.fork(thread, &second).unwrap();
    cockpit.pump();
    let fork = cockpit
        .threads()
        .into_iter()
        .find(|open| *open != thread)
        .expect("the fork is open");
    assert_eq!(
        prompts(&cockpit, fork)
            .into_iter()
            .map(|(text, _)| text)
            .collect::<Vec<_>>(),
        vec!["one".to_string()]
    );
    // The source keeps everything.
    assert_eq!(prompts(&cockpit, thread).len(), 2);
}
