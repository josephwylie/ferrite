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

/// What an accepted call is answered with, before any warnings: the plan
/// is drawn from the call itself.
pub const ANSWER: &str = "Plan updated; the operator sees it.";

/// Whether a call (as the provider names it) is this tool.
pub fn is_tool(name: &str) -> bool {
    name == CLAUDE_NAME
}

/// When and how to plan, as the agent reads it: §12's guidance (plan at the
/// first meaningful action, with that action; short steps; keep it current;
/// replan after steering). Shared by both providers, whose tools take
/// different arguments, so it names none.
pub const GUIDANCE: &str = "The operator watches your work in Ferrite, which shows your plan as a \
checklist under the turn. For any task with more than one meaningful step, make a plan with \
`update_plan` at your first meaningful action: call it together with that first piece of real work, \
not on its own and not after the work is done. Keep it to 2-7 short steps of a few words each \
(\"Read the parser\", \"Fix the off-by-one\", \"Run the tests\"). Keep exactly one step in progress \
while you work, and update the plan as each step finishes. When the operator steers you or the \
scope changes, update the plan to match. Skip the plan for a one-step task or a question you can \
just answer.";

/// The tool's description: the guidance, then how this tool's fields work.
pub fn description() -> String {
    format!(
        "{GUIDANCE}\n\nTo set the plan, pass `scope` and `items` (each step `pending`, \
`in_progress` or `completed`): `first_task` for the first plan of this conversation, \
`continuation` when you replan the same work (including acting on the previous result), \
`new_task` for genuinely separate work. To move through it, pass only step numbers (1-based): \
`advance: true` completes the step in progress and starts the next one; `done` completes steps; \
`start` starts one."
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

/// A call as the server accepts it: the update, and what to tell the agent
/// about the parts that were not applied as asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    pub update: Update,
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
        if let Some(current) = current {
            let mut after = current.to_vec();
            steps.apply(&mut after);
            warn_on_parallel(&after, &mut warnings);
        }
        Ok(Accepted {
            update: Update::Steps(steps),
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
