//! What the Factory puts in a new Workflow (ADR 0037, #221): a new
//! Step, and a template's Steps. A Workflow itself is in
//! `factory_board`.

use ferrite_core::factory_board::{
    Agent, Approval, Definition, Destination, MergeRoutes, Start, Step, StepId, Template, Workflow,
};

/// A Step as a new one is: named, everything else to fill in.
pub(super) fn new_step(id: StepId, name: &str, agent: Agent) -> Step {
    Step {
        id,
        name: name.into(),
        instructions: String::new(),
        skills: Vec::new(),
        agent,
        on_finished: Destination::Complete,
        on_unable: Destination::Human,
        fresh: false,
        outcome_routes: None,
    }
}

/// A template's Definition, its Steps numbered from `workflow`'s.
pub(super) fn from_template(template: Template, workflow: &mut Workflow, agent: &Agent) {
    let mut step = |name: &str, instructions: &str, skills: &[&str]| {
        let mut step = new_step(workflow.new_step_id(), name, agent.clone());
        step.instructions = instructions.into();
        step.skills = skills.iter().map(|skill| skill.to_string()).collect();
        step.on_finished = Destination::Next;
        step
    };
    workflow.draft = match template {
        Template::Blank => {
            let mut only = step("Agent", "", &[]);
            only.on_finished = Destination::Complete;
            Definition {
                name: "Untitled workflow".into(),
                steps: vec![only],
                approval: Approval::default(),
                start: Start::default(),
            }
        }
        Template::IssueToReviewedPr => {
            let triage = step(
                "Triager",
                "Use the triage skill on the incoming issue. Ask the owner about any missing \
                 behaviour before handing it on.",
                &["triage"],
            );
            let explore = step(
                "Explorer",
                "Explore the codebase and the issue discussion. Use codebase-design to prepare a \
                 plan and identify the existing test seam.",
                &["codebase-design"],
            );
            let implement = step(
                "Implementer",
                "Implement the agreed plan and any human feedback. Run the relevant project \
                 checks, then use pr to open or update the pull request.",
                &["implement", "pr"],
            );
            let mut review = step(
                "Reviewer",
                "Read the current PR, the issue and all human feedback. Use code-review. Verify \
                 the requested behaviour and the regression evidence. Report the commands you ran \
                 and what they showed.",
                &["code-review"],
            );
            review.on_finished = Destination::Approval;
            review.on_unable = Destination::Step(implement.id.clone());
            review.fresh = true;
            let mut merge = step(
                "Merge agent",
                "Read the PR reference, its current head, the approval and the feedback in the \
                 task context. Use your configured GitHub tools to check the PR and merge it when \
                 eligible. Confirm it actually merged before reporting success. Report conflicts, \
                 pending checks, missing access, new commits since approval or an unconfirmed \
                 result as that outcome.",
                &[],
            );
            let mut resolve = step(
                "Conflict resolver",
                "Resolve only the reported merge conflicts, keep the approved behaviour, push the \
                 result and hand the changed PR back for review.",
                &["implement"],
            );
            merge.on_finished = Destination::Complete;
            merge.on_unable = Destination::Human;
            merge.outcome_routes = Some(MergeRoutes {
                conflict: Destination::Step(resolve.id.clone()),
                ..MergeRoutes::default()
            });
            resolve.on_finished = Destination::Step(review.id.clone());
            Definition {
                name: "Issue to reviewed PR".into(),
                approval: Approval {
                    after: Destination::Step(merge.id.clone()),
                    rework_to: Destination::Step(implement.id.clone()),
                    ..Approval::default()
                },
                steps: vec![triage, explore, implement, review, merge, resolve],
                start: Start::default(),
            }
        }
    };
}
