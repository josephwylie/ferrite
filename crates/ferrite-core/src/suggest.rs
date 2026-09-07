//! Follow-up context and filtering, independent of provider protocol.
//! Codex predicts through the same bounded one-shot runner used by Titles.
//! Claude supplies native suggestions through its live Session instead. Missing CLIs, failed runs and refused replies leave the
//! generic idle line. No conversation is sent to a different Provider.

use std::sync::mpsc::Sender;

use crate::store::Provider;
use crate::transcript::{Body, Span, Transcript};
use crate::ThreadId;

/// What the model is asked to be. Deliberately narrow: the reply goes into a
/// text box as ghost text, and anything conversational reads as the agent
/// talking rather than as the operator's own draft.
const SYSTEM: &str = "You predict the operator's next message to a coding agent. \
Reply with ONLY that message: one short imperative line in the operator's voice, \
at most 12 words. No quotes, no explanation, no preamble.";

/// How much of either side of the last exchange is worth sending. The
/// prediction is about the shape of the turn, not its detail, and the whole
/// point of this path is that it stays cheap.
const CONTEXT_CHARS: usize = 1200;

/// The longest suggestion worth showing, and the most words — the ceilings
/// Claude Code's own suggestion filter uses.
const MAX_CHARS: usize = 100;
const MAX_WORDS: usize = 12;

/// One prediction, addressed to the Thread that asked for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub thread: ThreadId,
    /// The Thread's generation when the run started. A park, revive or
    /// provider switch moves the generation on, and a reply carrying the old
    /// one is answering a conversation that no longer exists.
    pub generation: u64,
    /// Reject replies superseded within the same Session.
    pub revision: u64,
    pub text: String,
}

/// Everything one run needs, resolved before spawning so the worker touches
/// no cockpit state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub thread: ThreadId,
    pub generation: u64,
    /// Reject replies superseded within the same Session.
    pub revision: u64,
    pub provider: Provider,
    /// The digest of the last exchange, from [`context`].
    pub context: String,
}

/// Fire one prediction. Returns immediately; the answer arrives on `replies`
/// if it arrives at all.
pub fn spawn(request: Request, replies: Sender<Suggestion>) {
    std::thread::spawn(move || {
        let Some(text) =
            crate::providers::oneshot::predict(request.provider, SYSTEM, &request.context)
                .filter(|text| accept(text))
        else {
            return;
        };
        // A closed receiver means the cockpit is gone. Nothing to report to.
        let _ = replies.send(Suggestion {
            thread: request.thread,
            generation: request.generation,
            revision: request.revision,
            text,
        });
    });
}

/// The exchange to predict from, or None when there is nothing to predict —
/// no agent response yet, or the operator's own prompt is the tail.
///
/// Both sides go in: what the operator asked shapes what they would ask next
/// at least as much as what the agent answered.
pub fn context(transcript: &Transcript) -> Option<String> {
    let mut answer: Option<String> = None;
    let mut prompt: Option<String> = None;
    for block in transcript.blocks().iter().rev() {
        match &block.body {
            Body::Prompt(text) => {
                // The operator's prompt is the tail: they have spoken since
                // the last response, so there is nothing to follow up.
                answer.as_ref()?;
                prompt = Some(text.clone());
                break;
            }
            Body::Paragraph { spans } | Body::Bullet { spans } | Body::Heading { spans, .. } => {
                let text = join(spans);
                if !text.trim().is_empty() {
                    let held = answer.get_or_insert_with(String::new);
                    // Walking backwards, so earlier prose goes in front.
                    held.insert(0, '\n');
                    held.insert_str(0, &text);
                }
            }
            _ => {}
        }
    }
    let answer = clip(answer?.trim());
    if answer.is_empty() {
        return None;
    }
    Some(match prompt {
        Some(prompt) => format!(
            "Operator asked:\n{}\n\nAgent replied:\n{answer}",
            clip(prompt.trim())
        ),
        None => format!("Agent replied:\n{answer}"),
    })
}

fn join(spans: &[Span]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

/// `text` at [`CONTEXT_CHARS`], keeping the *end* — the tail of a response is
/// what invites the follow-up; the head is usually preamble.
fn clip(text: &str) -> String {
    let count = text.chars().count();
    if count <= CONTEXT_CHARS {
        return text.to_string();
    }
    text.chars()
        .skip(count - CONTEXT_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Whether a reply is worth putting in the box.
///
/// These are Claude Code's own suggestion filters, the ones its
/// `--prompt-suggestions` path applies before emitting: the failure modes are
/// the same because the job is the same, and a suggestion the vendor would
/// suppress is not one Ferrite should show. A rejected reply leaves the
/// generic line, which is the right outcome — ghost text the operator would
/// never have typed is worse than no ghost text at all.
pub(crate) fn accept(text: &str) -> bool {
    if text.is_empty() || text.chars().count() >= MAX_CHARS {
        return false;
    }
    let words = text.split_whitespace().count();
    if words > MAX_WORDS {
        return false;
    }
    let lower = text.to_lowercase();
    // One-word replies are noise unless the word answers on its own.
    const WHOLE_ANSWERS: [&str; 16] = [
        "yes", "yeah", "yep", "yea", "yup", "sure", "ok", "okay", "push", "commit", "deploy",
        "stop", "continue", "check", "exit", "quit",
    ];
    if words < 2
        && !text.starts_with('/')
        && !WHOLE_ANSWERS.contains(&lower.trim_end_matches(['.', '!']))
    {
        return false;
    }
    // Prose, not a prompt: markdown, or a second sentence.
    if text.contains(['\n', '*']) || multiple_sentences(text) {
        return false;
    }
    // The model answering as itself rather than predicting the operator.
    const CLAUDE_VOICE: [&str; 20] = [
        "let me ",
        "i'll ",
        "i've ",
        "i'm ",
        "i can ",
        "i would ",
        "i think ",
        "i notice ",
        "here's ",
        "here is ",
        "here are ",
        "that's ",
        "this is ",
        "this will ",
        "you can ",
        "you should ",
        "you could ",
        "sure, ",
        "of course",
        "certainly",
    ];
    if CLAUDE_VOICE.iter().any(|open| lower.starts_with(open)) {
        return false;
    }
    // Closing pleasantries end a conversation; they never open the next turn.
    const EVALUATIVE: [&str; 12] = [
        "thanks",
        "thank you",
        "looks good",
        "sounds good",
        "that works",
        "that worked",
        "that's all",
        "nice",
        "great",
        "perfect",
        "makes sense",
        "awesome",
    ];
    !EVALUATIVE.iter().any(|phrase| lower.contains(phrase))
}

/// Whether `text` runs to a second sentence — a terminator, whitespace, and a
/// capital.
fn multiple_sentences(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    chars.windows(3).any(|window| {
        matches!(window[0], '.' | '!' | '?')
            && window[1].is_whitespace()
            && window[2].is_uppercase()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::Input;
    use crate::{SessionEvent, TurnOutcome};

    fn answered(prompt: &str, text: &str) -> Transcript {
        let mut transcript = Transcript::default();
        transcript.apply(Input::Prompt(prompt.into()));
        transcript.apply(Input::Event(SessionEvent::TextDelta { text: text.into() }));
        transcript.apply(Input::Event(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: None,
        }));
        transcript
    }

    /// Both halves of the exchange go to the predictor, labelled.
    #[test]
    fn the_context_carries_the_prompt_and_the_answer() {
        let transcript = answered("fix the decoder", "Fixed it. Want me to run the tests?");
        let context = context(&transcript).expect("an exchange to predict from");
        assert!(context.contains("fix the decoder"), "{context}");
        assert!(context.contains("Want me to run the tests?"), "{context}");
        assert!(context.starts_with("Operator asked:"), "{context}");
    }

    /// The operator has spoken since the last answer, so there is no response
    /// to follow up and no run to pay for.
    #[test]
    fn an_unanswered_prompt_is_not_worth_predicting() {
        let mut transcript = answered("fix the decoder", "Fixed it.");
        transcript.apply(Input::Prompt("now do the other one".into()));
        assert_eq!(context(&transcript), None);
    }

    #[test]
    fn an_empty_transcript_is_not_worth_predicting() {
        assert_eq!(context(&Transcript::default()), None);
    }

    /// Long responses are clipped from the front: the tail is what invites
    /// the follow-up.
    #[test]
    fn a_long_answer_keeps_its_tail() {
        let long = format!("{}\nWant me to run the tests?", "x".repeat(4000));
        let transcript = answered("go", &long);
        let context = context(&transcript).expect("an exchange");
        assert!(context.contains("Want me to run the tests?"), "{context}");
        assert!(
            context.chars().count() < 3000,
            "{}",
            context.chars().count()
        );
    }

    /// The vendor's own filters, because the failure modes are the vendor's
    /// own: a suggestion Claude Code would suppress is not one to show.
    #[test]
    fn the_filter_refuses_what_the_operator_would_never_type() {
        for refused in [
            "",
            "Sure, I can help with that",
            "Let me run the tests for you",
            "Here's what I would do next",
            "thanks",
            "looks good",
            "Run the tests. Then open a PR.",
            "Run the tests\nand open a PR",
            "**Run the tests**",
            "Rewrite the provider adapter and the wire decoder and every fixture beneath it too",
            "it",
        ] {
            assert!(!accept(refused), "should have been refused: {refused:?}");
        }
        for kept in [
            "Run the tests and report back.",
            "yes",
            "Open a PR against main",
            "/code-review",
            "revert it",
        ] {
            assert!(accept(kept), "should have been kept: {kept:?}");
        }
    }
}
