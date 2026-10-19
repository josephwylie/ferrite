//! The reviewer's look at a plan (`plan.approve`: reviewer): the plan is checked against the task's description, and
//! the same gate decides. It is a reviewer like the one that reviews the work (in a tab named `Plan review`), with
//! another brief: there is no change to look at yet, only the plan.
use super::plan_file;
use crate::board_rule::reviewer_board_rule;
use crate::err::Res;
use crate::finish::review::{run_review_with, Reviewed};
use crate::finish::review_once::Plan;
use crate::gate;
use crate::json::*;
use crate::runner::Guard;
use std::path::Path;

pub fn plan_brief(t: &Obj, dir: &Path) -> String {
    let desc: String = t.s("description").chars().take(3000).collect();
    [
        "You are an adversarial reviewer of a plan. An agent was given the task below and wrote a plan for it; nothing is built yet, and nothing is built before the plan is approved. Your job is to find what is wrong, missing or risky in the plan before forty minutes are spent building it.".to_string(),
        format!("The plan is in {}. You are in the folder the work will be done in, with all your tools: read the plan, then read the code it speaks of. Check it against the task: does it cover everything that was asked, and only that? Did its writer understand the task? Does it name the right files and places, and do they exist as it says? Is something that will break left out? Would building exactly this plan do what was asked? Ignore taste and style.", plan_file(t).display()),
        "You are here to review, not to fix. Do not change the plan, and change, commit, push or merge nothing in the repository: whatever you alter is put back.".into(),
        reviewer_board_rule(),
        format!("Task: {}", t.s("title")), if desc.is_empty() { String::new() } else { format!("Details:\n{desc}") },
        "First answer one fixed question, true or false: would building this plan do what the task asked, all of it? That answer is `meets`.".into(),
        "Then report what you found as a list of findings, one fault of the plan each. A finding has a `subject` (one sentence saying what is wrong with the plan), a `where` (the part of the plan, or the file it is about), a `proof` (what you read or ran that shows it; leave it empty if you did not show it) and a `severity`, chosen by what the fault would do:\n- critical: built as planned, the work would not do what was asked, would break something that works, or risks security or data loss; or the plan misunderstands the task.\n- major: a part of the task the plan leaves out; a step that cannot work as written; a claim about the code that is false.\n- minor: it would work, but a step is vague or a better way is plain.\n- trivial: a note. Nothing to do.".into(),
        "You do not decide whether the plan is built or goes back: the board decides from your findings by a fixed rule. A critical finding with an empty proof is counted as major. If nothing qualifies, return an empty list and say so. Do not invent findings to have something to report.".into(),
        format!("Last of all, save your verdict as JSON in {}/verdict.json, then stop. The board waits for that file. Its shape: {{\"meets\": <true or false>, \"text\": \"<two or three plain sentences: your overall judgement of the plan>\", \"findings\": [{{\"severity\": \"<critical, major, minor or trivial>\", \"subject\": \"<one sentence: what is wrong>\", \"where\": \"<the part of the plan or the file>\", \"proof\": \"<what you read or ran; may be empty>\"}}, ...]}}  (findings may be an empty list; do not invent findings or proof)", dir.display()),
    ].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n\n")
}
/// Runs it: a review stamped with whether the plan may be built, an error, or aborted.
pub fn review_plan(id: &str, g: &mut Guard, ch: &Obj) -> Res<Reviewed> {
    run_review_with(id, g, "", None, false, ch, "", Some(&Plan { brief: Box::new(plan_brief) }))
}
/// What its writer is told when the plan fails the gate.
pub fn points_of(rv: &Value) -> String { format!("{}.\n\n{}", gate::result(rv, "merge"), gate::findings_text(rv, false)) }
