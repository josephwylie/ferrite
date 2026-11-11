//! Pull requests and merging (#223) through the module's interface: a
//! Task's PR and its revisions, approval tied to a revision, evidence,
//! routes by merge result, and the worktree tidied once merged.

use super::*;

fn start_on(
    factory: &mut Factory,
    workflow: Option<&str>,
    pr: Option<&str>,
) -> Result<Answer, Refusal> {
    factory.command(
        Command::Start {
            project: project(),
            instructions: "Ship the flag".into(),
            agent: agent(),
            request: None,
            workflow: workflow.map(str::to_string),
            pr: pr.map(str::to_string),
        },
        6,
    )
}

fn task_id(answer: Result<Answer, Refusal>) -> TaskId {
    match answer {
        Ok(Answer::Task(task)) => task,
        other => panic!("{other:?}"),
    }
}

fn the<'a>(factory: &'a Factory, id: &str) -> &'a Task {
    factory.task(id).expect("the task")
}

fn revision(
    factory: &mut Factory,
    worker: ThreadId,
    head: &str,
    pr: Option<&str>,
) -> Result<Answer, Refusal> {
    factory.command(
        Command::ReportRevision {
            worker,
            head: head.into(),
            pr: pr.map(str::to_string),
        },
        8,
    )
}

const HEAD_1: &str = "1111111aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HEAD_2: &str = "2222222bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// AC: a Task accepts a PR reference at start, as owner/repository#number
/// or a URL, validated; the first Step is told it.
#[test]
fn a_task_takes_its_pr_at_start_as_a_reference_or_a_url() {
    let mut factory = Factory::in_memory();
    let (id, _) = saved(&mut factory, &[("Build", "Build it")]);
    for given in [
        "acme/widgets#12",
        " https://github.com/acme/widgets/pull/12 ",
        "https://github.com/acme/widgets/pull/12/files",
        "github.com/acme/widgets/pull/12",
    ] {
        let task = task_id(start_on(&mut factory, Some(&id), Some(given)));
        let pr = the(&factory, &task).delivery.pr.clone().expect("its PR");
        assert_eq!((pr.repository.as_str(), pr.number), ("acme/widgets", 12));
        assert_eq!(pr.url(), "https://github.com/acme/widgets/pull/12");
        assert!(pr.revisions.is_empty(), "no head is known yet");
    }
    let before = factory.board().tasks.len();
    for bad in [
        "acme/widgets",
        "#12",
        "acme#12",
        "acme/widgets#0",
        "acme/widgets#x",
        "acme/wid gets#3",
        "https://gitlab.com/acme/widgets/pull/1",
        "https://github.com/acme/widgets/issues/12",
    ] {
        let refused = start_on(&mut factory, Some(&id), Some(bad)).unwrap_err();
        assert_eq!(refused.code, RefusalCode::Invalid, "{bad}");
    }
    assert_eq!(factory.board().tasks.len(), before, "a bad PR starts nothing");

    let one_off = task_id(start_on(&mut factory, None, Some("acme/widgets#7")));
    assert_eq!(the(&factory, &one_off).delivery.pr.as_ref().unwrap().number, 7);

    let effects = factory.step(7, &BTreeMap::new());
    let prompts: Vec<String> = starts(&effects).into_iter().map(|(_, _, p)| p).collect();
    assert!(
        prompts.iter().all(|prompt| prompt.contains("## Pull request")),
        "{prompts:?}"
    );
    assert!(prompts
        .iter()
        .any(|prompt| prompt.contains("https://github.com/acme/widgets/pull/7")));
}

/// AC: a Worker can report the PR and each new revision; revisions are
/// kept, each with who reported it. A repeat changes nothing.
#[test]
fn a_worker_reports_the_pr_and_each_revision_with_who_reported_it() {
    let mut factory = Factory::in_memory();
    let (id, _) = saved(&mut factory, &[("Build", "Build it"), ("Fix", "Fix it")]);
    let task = task_id(start_on(&mut factory, Some(&id), None));
    factory.step(7, &BTreeMap::new());
    started(&mut factory, &task, FIRST);

    let refused = revision(&mut factory, FIRST, HEAD_1, None).unwrap_err();
    assert_eq!(refused.code, RefusalCode::Invalid, "which PR is it?");
    let refused = revision(&mut factory, FIRST, "not-a-sha", Some("acme/widgets#12")).unwrap_err();
    assert_eq!(refused.code, RefusalCode::Invalid);

    assert_eq!(
        revision(&mut factory, FIRST, HEAD_1, Some("acme/widgets#12")),
        Ok(Answer::Changed(true))
    );
    assert_eq!(
        revision(&mut factory, FIRST, HEAD_1, None),
        Ok(Answer::Changed(false)),
        "the same head again is no new revision"
    );
    let refused = revision(&mut factory, FIRST, HEAD_2, Some("acme/widgets#99")).unwrap_err();
    assert_eq!(refused.code, RefusalCode::Invalid, "the Task has its PR");
    assert_eq!(
        revision(&mut factory, SECOND, HEAD_2, None),
        Err(Refusal::new(
            RefusalCode::NotAWorker,
            "this Thread is no Task's Worker"
        ))
    );

    let effects = finished(&mut factory, FIRST, "Opened the PR");
    let prompt = &starts(&effects)[0].2;
    assert!(
        prompt.contains("acme/widgets#12") && prompt.contains("revision 1"),
        "every Step is told the PR and its revision: {prompt}"
    );
    assert!(prompt.contains("1111111"), "{prompt}");
    started(&mut factory, &task, SECOND);
    assert_eq!(
        revision(&mut factory, FIRST, HEAD_2, None),
        Ok(Answer::Changed(false)),
        "an earlier Step's Worker reports into the past"
    );
    revision(&mut factory, SECOND, HEAD_2, None).unwrap();

    let pr = the(&factory, &task).delivery.pr.clone().unwrap();
    let kept: Vec<(u32, &str, &str)> = pr
        .revisions
        .iter()
        .map(|each| (each.revision, each.head.as_str(), each.by.as_str()))
        .collect();
    assert_eq!(kept, [(1, HEAD_1, "Build"), (2, HEAD_2, "Fix")]);
    assert_eq!(pr.current().unwrap().revision, 2);
    let activity = &the(&factory, &task).activity;
    assert!(activity
        .iter()
        .any(|entry| entry.by == Actor::Agent
            && entry.text.contains("revision 2")
            && entry.revision == Some(2)));
}
