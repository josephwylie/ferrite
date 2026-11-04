//! `update_plan`: the turn's plan, as Claude Desktop's Code host offers it
//! (`mcp__ccd_turn__update_plan`, `claude-desktop-transcript-handoff.md`
//! §12). The CLI's own checklist tools are off for current models in print
//! mode, and an agent without a plan tool and the guidance to use it never
//! plans; this is the plan half of the turn box `wrap_up` is the other half
//! of.
//!
//! Offered on Ferrite's in-band server (`providers::claude::mcp`), which
//! validates each call and, once the CLI reports it succeeded for Main,
//! turns it into the same `ProgressEvent`s every other plan becomes: a
//! replacement is a `Plan`, a step change a `PlanSteps`. One plan model.
//! Codex has a native `update_plan` of its own (`turn/plan/updated`);
//! [`GUIDANCE`] is what both are told about planning.

use serde_json::{json, Value};

use crate::progress::{PlanStep, StepStatus};

/// The tool's name inside Ferrite's server (`visual::SERVER`).
pub const TOOL: &str = "update_plan";
/// What Claude calls it.
pub const CLAUDE_NAME: &str = "mcp__ferrite__update_plan";

/// A plan holds 1 to this many steps.
pub const MAX_ITEMS: usize = 12;
/// A step's words, at most (Claude's limit; the guidance asks for far fewer).
pub const MAX_TEXT: usize = 160;

/// Whether a call (as the provider names it) is this tool.
pub fn is_tool(name: &str) -> bool {
    name == CLAUDE_NAME
}

/// When and how to plan, as the agent reads it: §12's guidance, as Claude
/// Desktop gives it (a plan with the turn's very first tool call, in the
/// same message; each step marked done as it finishes, with the call that
/// starts the next; short steps; replan after steering). Shared by both
/// providers, whose tools take different arguments, so it names none.
/// "Keep it current" alone was not enough: a Claude turn planned four
/// searches, ran them all, answered, and never moved the plan.
pub const GUIDANCE: &str = "The operator follows your progress on the plan Ferrite shows under \
your latest message, not by reading your messages, so keeping it is part of doing the task. Every \
request that edits files or runs more than one command gets a plan; only a single-command answer \
or pure conversation does not. Make it with `update_plan` in the same message as the turn's very \
first tool call, even one that only reads or lists files: the whole plan in order, 2-7 short \
imperative steps of a few words each (\"Read the parser\", \"Fix the off-by-one\", \"Run the \
tests\"), the step you are starting in progress and the rest pending. If you must look around \
first, make \"Look at how X is set up\" step one and refine the plan once you know. Never spend \
a message on `update_plan` alone. As each step finishes, mark it completed and the next one in \
progress, in the same message as the call that starts that next step; work you do in parallel \
still moves the plan as it lands. Keep exactly one step in progress while you work. Before your \
closing answer, the plan must say what you did: no step you finished is left in progress or \
pending. When the operator steers you or the scope changes, send the new whole plan, so it never \
shows an abandoned one. Do not also write the plan as a list in your message.";

/// The tool's description: the guidance, then how this tool's fields work.
pub fn description() -> String {
    format!(
        "{GUIDANCE}\n\n\
- First call: `scope` and `items` (each step `pending`, `in_progress` or `completed`). `scope` \
says what the request is to this conversation: `first_task` until you have finished a task here; \
then `new_task` for unrelated work, or `continuation` for everything else (the next iteration, \
acting on what you just found or suggested; when unsure, this one).\n\
- Next step: `{{\"advance\": true}}` alongside the call that begins it: it completes the step in \
progress and starts the next pending one. After the last step, advance alongside `wrap_up` \
(a `wrap_up` that is not blocked also checks off what is left).\n\
- Out of order: `{{\"done\": [2], \"start\": 4}}` (1-based step numbers).\n\
- Plan changed: `items` again, the new whole plan.\n\
The answer says where the plan now stands."
    )
}

/// The tool's input schema (§12's input contract; Ferrite has no linked
/// sessions, so a step names none).
pub fn input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "scope": {
                "type": "string",
                "enum": ["first_task", "continuation", "new_task"],
                "description": "Required with items: whether this plan is the first, a replan of the same work, or new work."
            },
            "items": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_ITEMS,
                "description": "The whole plan, replacing any before it.",
                "items": {
                    "type": "object",
                    "properties": {
                        "content": {"type": "string", "maxLength": MAX_TEXT, "description": "A short step."},
                        "status": {"type": "string", "enum": ["pending", "in_progress", "completed"]}
                    },
                    "required": ["content", "status"],
                    "additionalProperties": false
                }
            },
            "advance": {
                "type": "boolean",
                "description": "Complete the step in progress and start the next."
            },
            "done": {
                "type": "array",
                "items": {"type": "integer", "minimum": 1},
                "description": "1-based numbers of steps now completed."
            },
            "start": {
                "type": "integer",
                "minimum": 1,
                "description": "1-based number of the step now in progress."
            }
        },
        "additionalProperties": false
    })
}

/// One accepted call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    /// `items`: the whole plan, replacing any before it.
    Replace(Vec<PlanStep>),
    /// Step changes on the plan as it stands, by 1-based number.
    Steps(Steps),
}

/// Step changes, applied in this order: `done`, `start`, then `advance`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Steps {
    pub done: Vec<usize>,
    pub start: Option<usize>,
    pub advance: bool,
}

/// A call as the server accepts it: the update, what to answer the agent
/// (where the plan now stands, so a step left behind shows), and what to
/// tell it about the parts that were not applied as asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    pub update: Update,
    pub answer: String,
    pub warnings: Vec<String>,
}

impl Update {
    /// Validate a call's input against the plan's statuses as they stand
    /// (`None` when unknown, as after a resume: step numbers are then
    /// checked against [`MAX_ITEMS`]). `Err` is what the agent is told.
    pub fn parse(input: &Value, current: Option<&[StepStatus]>) -> Result<Accepted, String> {
        let mut warnings = Vec::new();
        let incremental = ["advance", "done", "start"]
            .iter()
            .any(|field| !input[field].is_null());
        if let Some(items) = input.get("items").filter(|items| !items.is_null()) {
            if !matches!(
                input["scope"].as_str(),
                Some("first_task" | "continuation" | "new_task")
            ) {
                return Err(
                    "`scope` is required with `items`: first_task, continuation or new_task."
                        .into(),
                );
            }
            let items = items.as_array().ok_or("`items` must be a list of steps.")?;
            if items.is_empty() || items.len() > MAX_ITEMS {
                return Err(format!("A plan has 1 to {MAX_ITEMS} steps."));
            }
            let steps = items
                .iter()
                .enumerate()
                .map(|(at, item)| {
                    let text = item["content"].as_str().map(str::trim).unwrap_or_default();
                    if text.is_empty() {
                        return Err(format!("Step {} has no `content`.", at + 1));
                    }
                    if text.chars().count() > MAX_TEXT {
                        return Err(format!(
                            "Step {} is over {MAX_TEXT} characters; keep steps short.",
                            at + 1
                        ));
                    }
                    let status = status(&item["status"])
                        .ok_or_else(|| format!("Step {} has no valid `status`.", at + 1))?;
                    Ok(PlanStep {
                        text: text.into(),
                        status,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            if incremental {
                warnings.push(
                    "`items` replaced the plan; `advance`, `done` and `start` were ignored.".into(),
                );
            }
            let statuses: Vec<_> = steps.iter().map(|step| step.status).collect();
            warn_on_parallel(&statuses, &mut warnings);
            return Ok(Accepted {
                answer: format!("Plan shown ({} steps).", steps.len()),
                update: Update::Replace(steps),
                warnings,
            });
        }
        if !incremental {
            return Err("Pass `scope` and `items` to set the plan, or `advance`, `done` or `start` to move through it.".into());
        }
        let limit = current.map_or(MAX_ITEMS, <[_]>::len);
        let number = |value: &Value, field: &str| -> Result<usize, String> {
            value
                .as_u64()
                .map(|n| n as usize)
                .filter(|n| (1..=limit).contains(n))
                .ok_or_else(|| format!("`{field}` takes step numbers from 1 to {limit}."))
        };
        let done = match &input["done"] {
            Value::Null => Vec::new(),
            Value::Array(values) => values
                .iter()
                .map(|value| number(value, "done"))
                .collect::<Result<_, _>>()?,
            _ => return Err("`done` must be a list of step numbers.".into()),
        };
        let start = match &input["start"] {
            Value::Null => None,
            value => Some(number(value, "start")?),
        };
        let advance = match &input["advance"] {
            Value::Null => false,
            Value::Bool(advance) => *advance,
            _ => return Err("`advance` must be true or false.".into()),
        };
        let steps = Steps {
            done,
            start,
            advance,
        };
        let answer = match current {
            Some(current) => {
                let mut after = current.to_vec();
                steps.apply(&mut after);
                warn_on_parallel(&after, &mut warnings);
                format!("Plan updated: {}.", standing(&after))
            }
            None => "Plan updated.".into(),
        };
        Ok(Accepted {
            update: Update::Steps(steps),
            answer,
            warnings,
        })
    }
}

impl Steps {
    /// Apply to a plan's statuses (§12: `advance` completes the first step
    /// in progress, then starts the next pending step after it, or else the
    /// first pending one). Numbers past the plan's end change nothing.
    pub fn apply(&self, statuses: &mut [StepStatus]) {
        for &number in &self.done {
            if let Some(status) = number.checked_sub(1).and_then(|at| statuses.get_mut(at)) {
                *status = StepStatus::Completed;
            }
        }
        let start = self.start.and_then(|number| number.checked_sub(1));
        if let Some(status) = start.and_then(|at| statuses.get_mut(at)) {
            *status = StepStatus::InProgress;
        }
        if self.advance {
            let current = statuses
                .iter()
                .position(|status| *status == StepStatus::InProgress);
            if let Some(at) = current {
                statuses[at] = StepStatus::Completed;
            }
            let after = current.map_or(0, |at| at + 1);
            let next = statuses[after.min(statuses.len())..]
                .iter()
                .position(|status| *status == StepStatus::Pending)
                .map(|offset| after + offset)
                .or_else(|| {
                    statuses
                        .iter()
                        .position(|status| *status == StepStatus::Pending)
                });
            if let Some(at) = next {
                statuses[at] = StepStatus::InProgress;
            }
        }
    }
}

/// Where a plan stands, as the answer says it: the step in progress, or
/// that every step is done, or how to go on when none is in progress.
fn standing(statuses: &[StepStatus]) -> String {
    let total = statuses.len();
    let current: Vec<_> = (1..=total)
        .filter(|n| statuses[n - 1] == StepStatus::InProgress)
        .collect();
    let done = statuses
        .iter()
        .filter(|status| **status == StepStatus::Completed)
        .count();
    match current[..] {
        [step] => format!("step {step} of {total} is in progress"),
        [] if done == total => "every step is done".into(),
        [] => format!("no step is in progress ({done} of {total} done); `start` one, or `advance`"),
        _ => {
            let steps: Vec<_> = current.iter().map(ToString::to_string).collect();
            format!("steps {} of {total} are in progress", steps.join(", "))
        }
    }
}

/// §12: more than one step in progress is discouraged, not refused.
fn warn_on_parallel(statuses: &[StepStatus], warnings: &mut Vec<String>) {
    if statuses
        .iter()
        .filter(|status| **status == StepStatus::InProgress)
        .count()
        > 1
    {
        warnings.push("More than one step is in progress; keep one at a time.".into());
    }
}

fn status(value: &Value) -> Option<StepStatus> {
    Some(match value.as_str()? {
        "pending" => StepStatus::Pending,
        "in_progress" => StepStatus::InProgress,
        "completed" => StepStatus::Completed,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use StepStatus::*;

    fn steps(input: Value, current: Option<&[StepStatus]>) -> Steps {
        match Update::parse(&input, current).unwrap().update {
            Update::Steps(steps) => steps,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn items_replace_the_plan_and_need_a_scope() {
        let input = json!({"scope": "first_task", "items": [
            {"content": " Read the parser ", "status": "in_progress"},
            {"content": "Fix it", "status": "pending"},
        ]});
        let accepted = Update::parse(&input, None).unwrap();
        assert_eq!(
            accepted.update,
            Update::Replace(vec![
                PlanStep {
                    text: "Read the parser".into(),
                    status: InProgress
                },
                PlanStep {
                    text: "Fix it".into(),
                    status: Pending
                },
            ])
        );
        assert_eq!(accepted.answer, "Plan shown (2 steps).");
        assert!(accepted.warnings.is_empty());
        let unscoped = json!({"items": [{"content": "x", "status": "pending"}]});
        assert!(Update::parse(&unscoped, None)
            .unwrap_err()
            .contains("scope"));
    }

    #[test]
    fn a_plan_has_one_to_twelve_short_steps() {
        let plan = |n: usize, text: &str| {
            let items: Vec<_> = (0..n)
                .map(|_| json!({"content": text, "status": "pending"}))
                .collect();
            Update::parse(&json!({"scope": "new_task", "items": items}), None)
        };
        assert!(plan(0, "x").is_err());
        assert!(plan(12, "x").is_ok());
        assert!(plan(13, "x").is_err());
        assert!(plan(1, &"x".repeat(160)).is_ok());
        assert!(plan(1, &"x".repeat(161)).is_err());
        assert!(plan(1, " ").is_err());
    }

    /// Replacement wins over step changes in the same call, with a warning;
    /// two steps in progress are warned about, not refused.
    #[test]
    fn replacement_wins_and_parallel_steps_only_warn() {
        let input = json!({"scope": "continuation", "advance": true, "items": [
            {"content": "a", "status": "in_progress"},
            {"content": "b", "status": "in_progress"},
        ]});
        let accepted = Update::parse(&input, Some(&[Pending; 5])).unwrap();
        assert!(matches!(accepted.update, Update::Replace(_)));
        assert_eq!(accepted.warnings.len(), 2, "{:?}", accepted.warnings);
    }

    #[test]
    fn step_numbers_are_checked_against_the_known_plan_or_the_maximum() {
        assert!(Update::parse(&json!({"done": [4]}), Some(&[Pending; 3])).is_err());
        assert!(Update::parse(&json!({"start": 0}), Some(&[Pending; 3])).is_err());
        assert!(Update::parse(&json!({"done": [12]}), None).is_ok());
        assert!(Update::parse(&json!({"done": [13]}), None).is_err());
        assert!(Update::parse(&json!({"done": ["1"]}), None).is_err());
        assert!(Update::parse(&json!({}), None).is_err());
    }

    #[test]
    fn advance_completes_the_current_step_and_starts_the_next() {
        let mut plan = [Completed, InProgress, Pending, Pending];
        steps(json!({"advance": true}), Some(&plan)).apply(&mut plan);
        assert_eq!(plan, [Completed, Completed, InProgress, Pending]);

        // Nothing in progress: the first pending step starts.
        let mut plan = [Pending, Completed, Pending];
        steps(json!({"advance": true}), Some(&plan)).apply(&mut plan);
        assert_eq!(plan, [InProgress, Completed, Pending]);

        // Nothing pending after the current step: wrap to the first.
        let mut plan = [Pending, InProgress];
        steps(json!({"advance": true}), Some(&plan)).apply(&mut plan);
        assert_eq!(plan, [InProgress, Completed]);
    }

    /// The answer says where the plan now stands, so a step left behind
    /// is the agent's to see.
    #[test]
    fn the_answer_says_where_the_plan_stands() {
        let answer = |input: Value, current: Option<&[StepStatus]>| {
            Update::parse(&input, current).unwrap().answer
        };
        let advance = json!({"advance": true});
        assert_eq!(
            answer(advance.clone(), Some(&[InProgress, Pending, Pending])),
            "Plan updated: step 2 of 3 is in progress."
        );
        assert_eq!(
            answer(advance.clone(), Some(&[Completed, InProgress])),
            "Plan updated: every step is done."
        );
        assert_eq!(
            answer(json!({"done": [1]}), Some(&[InProgress, Pending])),
            "Plan updated: no step is in progress (1 of 2 done); `start` one, or `advance`."
        );
        assert_eq!(answer(advance, None), "Plan updated.");
    }

    #[test]
    fn done_and_start_set_steps_by_number() {
        let mut plan = [InProgress, Pending, Pending];
        steps(json!({"done": [1], "start": 3}), Some(&plan)).apply(&mut plan);
        assert_eq!(plan, [Completed, Pending, InProgress]);
        // A number past the plan (known only as "at most 12") is no change.
        steps(json!({"done": [9]}), None).apply(&mut plan);
        assert_eq!(plan, [Completed, Pending, InProgress]);
    }
}
