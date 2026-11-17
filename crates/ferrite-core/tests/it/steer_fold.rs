//! A prompt folded into the running turn (a steer, QS4/HD5): the turn goes
//! on, its clock still its own (`claude-steer-2.1.318`: the folded
//! message starts mid-turn and one result ends both).

use ferrite_core::transcript::{Body, Input, Transcript};
use ferrite_core::SessionEvent;

#[test]
fn a_folded_prompt_keeps_the_turns_clock() {
    let mut transcript = Transcript::default();
    transcript.apply(Input::Prompt("run three commands".into()));
    let started = transcript.turn_started().expect("the turn runs");
    transcript.apply(Input::Event(SessionEvent::TextDelta {
        text: "running".into(),
    }));
    transcript.apply(Input::Prompt("also run a fourth".into()));
    assert_eq!(transcript.turn_started(), Some(started), "one turn, one clock");
    let prompts = transcript
        .blocks()
        .iter()
        .filter(|block| matches!(block.body, Body::Prompt(_)))
        .count();
    assert_eq!(prompts, 2, "the folded prompt still shows where it joined");
}
