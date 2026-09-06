//! What the operator would plausibly say next, read off the last response.
//!
//! Neither provider offers this: Codex's app-server protocol carries no
//! suggestion of any kind, and Claude's stream-json carries none either. The
//! one native "here is what you might say" surface is the question tool, and
//! that already arrives as a Decision (see [`crate::questions`]). So the
//! suggestion here is derived, not received — and derived without inference,
//! which bounds how clever it is allowed to look. Every rule below either
//! fires on a signal that is unambiguous in the text or does not fire: a
//! wrong-but-confident placeholder costs the operator more than the generic
//! one it would have replaced.
//!
//! This module is wording-free on purpose. It names *what the Thread is
//! waiting on*; the renderer owns the sentence, as it does for every other
//! placeholder.

use crate::transcript::{Body, Span, Status, Transcript};

/// The longest offer this will quote back. Past this the phrase stops being
/// a placeholder and starts being a paragraph.
const PHRASE_MAX: usize = 48;

/// Openers that mark a sentence as the model offering to do something, so
/// "yes" is a complete answer to it. Matched case-insensitively against the
/// last sentence, longest first so `do you want me to` is not read as the
/// shorter `want me to` with a mangled remainder.
const OFFERS: [&str; 8] = [
    "would you like me to ",
    "do you want me to ",
    "do you want me to go ahead and ",
    "want me to ",
    "should i ",
    "shall i ",
    "can i ",
    "ok to ",
];

/// What the Composer's idle line should offer, most specific first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Followup {
    /// A Decision is pending; nothing else matters until it is answered.
    Decision,
    /// The Session is gone. Reviving is the only follow-up there is.
    Revive,
    /// The response ended by offering to do something, and the offer is a
    /// single unambiguous action. The phrase is the action, verbatim from
    /// the response minus its opener, so accepting it reads as the operator's
    /// own words rather than a paraphrase Ferrite invented.
    Accept(String),
    /// The response ended with a question that is not a plain offer — a
    /// choice, or anything else "yes" would not answer.
    Answer,
    /// No question, but the model's own plan still has steps outstanding.
    Continue { remaining: usize },
    /// Nothing specific. The generic line.
    Steer,
}

/// The follow-up this Thread invites, given whether a Decision pends.
///
/// A streaming Thread never yields anything but [`Followup::Steer`]: the
/// response it would be read from is still half-written, and a suggestion
/// drawn from a partial sentence is worse than no suggestion.
pub fn suggest(pending: bool, transcript: Option<&Transcript>) -> Followup {
    if pending {
        return Followup::Decision;
    }
    let Some(transcript) = transcript else {
        return Followup::Steer;
    };
    match transcript.status() {
        Status::Closed => return Followup::Revive,
        Status::Streaming => return Followup::Steer,
        Status::Idle | Status::Blocked => {}
    }
    if let Some(text) = last_answer(transcript) {
        if let Some(followup) = from_question(&text) {
            return followup;
        }
    }
    if let Some(todos) = transcript.todos() {
        if todos.done < todos.total {
            return Followup::Continue {
                remaining: todos.total - todos.done,
            };
        }
    }
    Followup::Steer
}

/// The last prose the model wrote in the current turn, or None if the
/// transcript's tail is the operator's own prompt (nothing has been answered
/// since) or holds no prose at all.
///
/// Tool rows, thinking, notices and cost lines are stepped over rather than
/// stopped at: a response that ends "…want me to run it?" and then runs one
/// last read is still a response ending in an offer.
fn last_answer(transcript: &Transcript) -> Option<String> {
    for block in transcript.blocks().iter().rev() {
        match &block.body {
            Body::Prompt(_) => return None,
            Body::Paragraph { spans } | Body::Bullet { spans } => {
                let text = join(spans);
                if !text.trim().is_empty() {
                    return Some(text);
                }
            }
            _ => {}
        }
    }
    None
}

fn join(spans: &[Span]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

/// The follow-up a block of prose invites, or None if it does not end in a
/// question at all.
fn from_question(text: &str) -> Option<Followup> {
    let sentence = last_sentence(text)?;
    match offer_phrase(sentence) {
        Some(phrase) => Some(Followup::Accept(phrase)),
        None => Some(Followup::Answer),
    }
}

/// The final sentence of `text` without its question mark, or None if `text`
/// does not end in one.
fn last_sentence(text: &str) -> Option<&str> {
    let text = text.trim_end();
    let body = text.strip_suffix('?')?;
    // Terminators of the *previous* sentence. `.rfind` over a char set is
    // enough here: an abbreviation mid-sentence only ever shortens the
    // phrase, which the offer rules then reject or quote short.
    let start = body.rfind(['.', '!', '?', '\n']).map_or(0, |at| at + 1);
    let sentence = body[start..].trim();
    (!sentence.is_empty()).then_some(sentence)
}

/// The action inside an offer sentence, or None if the sentence is not a
/// single-action offer.
///
/// A remaining phrase containing a free-standing "or" is refused: "should I
/// use tokio or async-std?" is a choice, and answering it "yes" answers
/// nothing.
fn offer_phrase(sentence: &str) -> Option<String> {
    // ASCII openers, compared with `eq_ignore_ascii_case` against the
    // original rather than against a lowercased copy: case folding is not
    // length-preserving in general, and the byte offset has to land back in
    // `sentence` to quote it verbatim.
    let opener = OFFERS
        .iter()
        .filter(|opener| {
            sentence
                .as_bytes()
                .get(..opener.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(opener.as_bytes()))
        })
        .max_by_key(|opener| opener.len())?;
    let rest = sentence[opener.len()..].trim();
    if rest.is_empty() || rest.contains('\n') || has_or(rest) {
        return None;
    }
    Some(clip(rest))
}

fn has_or(rest: &str) -> bool {
    rest.split(|c: char| !c.is_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("or"))
}

/// The phrase at placeholder length, cut on a word boundary so a clipped
/// offer never ends mid-word.
fn clip(phrase: &str) -> String {
    if phrase.chars().count() <= PHRASE_MAX {
        return phrase.to_string();
    }
    let head: String = phrase.chars().take(PHRASE_MAX).collect();
    match head.rfind(' ') {
        Some(cut) => head[..cut].trim_end().to_string(),
        None => head,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::{PlanStep, ProgressEvent, StepStatus};
    use crate::transcript::Input;
    use crate::{SessionEvent, TurnOutcome};

    /// A Transcript holding one finished agent answer.
    fn answered(text: &str) -> Transcript {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("do the thing".into()));
        transcript.apply(Input::Event(SessionEvent::TextDelta { text: text.into() }));
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        }));
        transcript
    }

    /// A plan of `total` steps with the first `done` finished.
    fn plan(done: usize, total: usize) -> Input {
        Input::Event(SessionEvent::Progress {
            event: ProgressEvent::Plan {
                steps: (0..total)
                    .map(|at| PlanStep {
                        text: format!("step {at}"),
                        status: if at < done {
                            StepStatus::Completed
                        } else {
                            StepStatus::Pending
                        },
                    })
                    .collect(),
                explanation: String::new(),
            },
        })
    }

    #[test]
    fn a_decision_outranks_everything_the_response_said() {
        let transcript = answered("Want me to run the tests?");
        assert_eq!(suggest(true, Some(&transcript)), Followup::Decision);
    }

    #[test]
    fn a_closed_session_offers_only_reviving() {
        let mut transcript = answered("Want me to run the tests?");
        transcript.apply(Input::Event(SessionEvent::Closed {
            reason: "the CLI exited".into(),
        }));
        assert_eq!(suggest(false, Some(&transcript)), Followup::Revive);
    }

    /// The whole point: a trailing offer becomes the operator's next line.
    #[test]
    fn a_trailing_offer_becomes_an_acceptance() {
        for (response, phrase) in [
            ("Want me to run the tests?", "run the tests"),
            ("Should I open a PR?", "open a PR"),
            ("Shall I revert it?", "revert it"),
            ("Can I delete the fixture?", "delete the fixture"),
            ("Would you like me to explain?", "explain"),
            ("Done. Do you want me to push it?", "push it"),
            ("OK to force-push?", "force-push"),
        ] {
            assert_eq!(
                suggest(false, Some(&answered(response))),
                Followup::Accept(phrase.into()),
                "{response}"
            );
        }
    }

    /// "Yes" answers nothing when the question is a choice, so the offer
    /// rules stand down rather than guess a side.
    #[test]
    fn a_choice_is_never_read_as_an_offer() {
        for response in [
            "Should I use tokio or async-std?",
            "Want me to revert it or patch it?",
        ] {
            assert_eq!(
                suggest(false, Some(&answered(response))),
                Followup::Answer,
                "{response}"
            );
        }
    }

    #[test]
    fn any_other_trailing_question_invites_an_answer() {
        let transcript = answered("Which provider should this Thread use?");
        assert_eq!(suggest(false, Some(&transcript)), Followup::Answer);
    }

    /// Only a question *ending* the response counts: prose that merely
    /// contains a question mark earlier is not a question put to the
    /// operator.
    #[test]
    fn only_a_trailing_question_counts() {
        let transcript = answered("Should I have asked? I went ahead and fixed it.");
        assert_eq!(suggest(false, Some(&transcript)), Followup::Steer);
    }

    /// The offer is read off the sentence, not off the paragraph, so the
    /// phrase never drags the sentences before it along.
    #[test]
    fn the_phrase_starts_at_its_own_sentence() {
        let transcript = answered("The build is green. Want me to tag it?");
        assert_eq!(
            suggest(false, Some(&transcript)),
            Followup::Accept("tag it".into())
        );
    }

    #[test]
    fn a_long_offer_is_clipped_on_a_word_boundary() {
        let offer =
            "Want me to rewrite the provider adapter, the wire decoder and every fixture under it";
        let transcript = answered(&format!("{offer}?"));
        let Followup::Accept(phrase) = suggest(false, Some(&transcript)) else {
            panic!("expected an acceptance");
        };
        assert!(phrase.chars().count() <= PHRASE_MAX, "{phrase}");
        assert!(!phrase.ends_with(' '), "{phrase}");
        assert!(offer.contains(&phrase), "{phrase} is not part of the offer");
    }

    /// A half-written sentence is not a response. Nothing is read off a
    /// Thread that is still streaming.
    #[test]
    fn a_streaming_thread_suggests_nothing() {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt("do the thing".into()));
        transcript.apply(Input::Event(SessionEvent::TextDelta {
            text: "Want me to run the tests?".into(),
        }));
        assert_eq!(transcript.status(), Status::Streaming);
        assert_eq!(suggest(false, Some(&transcript)), Followup::Steer);
    }

    /// The operator's own prompt is the tail after they send one, and their
    /// prompt is not a response to follow up.
    #[test]
    fn an_unanswered_prompt_is_not_a_response() {
        let mut transcript = answered("Want me to run the tests?");
        transcript.apply(Input::Prompt("no, do something else".into()));
        assert_eq!(suggest(false, Some(&transcript)), Followup::Steer);
    }

    #[test]
    fn an_unfinished_plan_is_the_fallback_when_nothing_was_asked() {
        let mut transcript = answered("Fixed the decoder.");
        transcript.apply(plan(1, 4));
        assert_eq!(
            suggest(false, Some(&transcript)),
            Followup::Continue { remaining: 3 }
        );
        transcript.apply(plan(4, 4));
        assert_eq!(suggest(false, Some(&transcript)), Followup::Steer);
    }

    /// A question outranks the plan: the Thread is waiting on the operator,
    /// not on itself.
    #[test]
    fn a_question_outranks_an_unfinished_plan() {
        let mut transcript = answered("Want me to run the tests?");
        transcript.apply(plan(1, 4));
        assert_eq!(
            suggest(false, Some(&transcript)),
            Followup::Accept("run the tests".into())
        );
    }

    #[test]
    fn a_draft_with_no_transcript_suggests_nothing() {
        assert_eq!(suggest(false, None), Followup::Steer);
    }

    /// Multi-byte prose must not panic the clip.
    #[test]
    fn multibyte_prose_is_safe() {
        let transcript = answered(
            "Want me to rename it to \u{201c}f\u{fc}nf\u{201d} everywhere, \
             including the \u{fc}bergreifend fixtures?",
        );
        let Followup::Accept(phrase) = suggest(false, Some(&transcript)) else {
            panic!("expected an acceptance");
        };
        assert!(phrase.chars().count() <= PHRASE_MAX, "{phrase}");
    }
}
