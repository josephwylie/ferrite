//! Schedule arithmetic and what a start says, then the Factory's automatic
//! starts through its interface, the clock passed in.

use super::*;
use chrono::FixedOffset;

fn at(text: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(text).unwrap()
}

fn at_time(recurrence: Recurrence, time: &str) -> Schedule {
    Schedule {
        recurrence,
        time: Some(time.into()),
        ..Schedule::default()
    }
}

/// 2026-11-11 is a Wednesday.
#[test]
fn a_schedule_names_its_next_occurrence_in_the_hosts_zone() {
    let wednesday_noon = at("2026-11-11T12:00:00+11:00");
    let daily = at_time(Recurrence::Daily, "09:30");
    assert_eq!(
        daily.next_after(&wednesday_noon),
        Some(at("2026-11-12T09:30:00+11:00")),
        "today's has passed"
    );
    assert_eq!(
        at_time(Recurrence::Daily, "13:00").next_after(&wednesday_noon),
        Some(at("2026-11-11T13:00:00+11:00"))
    );
    let friday_evening = at("2026-11-13T18:00:00+11:00");
    assert_eq!(
        at_time(Recurrence::Weekdays, "09:00").next_after(&friday_evening),
        Some(at("2026-11-16T09:00:00+11:00")),
        "skips the weekend"
    );
    let weekly = Schedule {
        weekday: 1,
        ..at_time(Recurrence::Weekly, "08:00")
    };
    assert_eq!(
        weekly.next_after(&wednesday_noon),
        Some(at("2026-11-16T08:00:00+11:00")),
        "the next Monday"
    );
    let interval = Schedule {
        recurrence: Recurrence::Interval,
        interval_hours: 6,
        ..Schedule::default()
    };
    assert_eq!(
        interval.next_after(&at("2026-11-11T12:00:00+11:00")),
        Some(at("2026-11-11T18:00:00+11:00")),
        "on the hour, a multiple of six"
    );
    assert_eq!(
        interval.next_after(&at("2026-11-11T19:10:00+11:00")),
        Some(at("2026-11-12T00:00:00+11:00"))
    );
    assert_eq!(
        Schedule::default().next_after(&wednesday_noon),
        None,
        "no time chosen"
    );
}

#[test]
fn a_schedule_and_a_github_start_say_what_they_do() {
    assert_eq!(
        Schedule {
            weekday: 2,
            ..at_time(Recurrence::Weekly, "07:00")
        }
        .describe(),
        "Every Tuesday at 07:00"
    );
    let github = GitHubSource {
        repository: Some("acme/app".into()),
        trigger: Trigger::Labelled,
        label: Some("ready-for-agent".into()),
        extra_instructions: String::new(),
    };
    assert_eq!(
        github.describe(StartKind::GithubIssue),
        "About once a minute, Ferrite checks acme/app for open issues labelled \
         ready-for-agent, using this computer's GitHub CLI login. Each one starts one task."
    );
}

#[test]
fn a_start_missing_a_repository_label_or_time_is_refused() {
    let github = Start {
        kind: StartKind::GithubIssue,
        ..Start::default()
    };
    assert_eq!(
        github.problems(),
        vec![
            "Choose the repository to check.".to_string(),
            "Choose the label to check for.".to_string(),
        ]
    );
    let review_on_issue = Start {
        kind: StartKind::GithubIssue,
        github: GitHubSource {
            repository: Some("acme/app".into()),
            trigger: Trigger::ReviewRequested,
            ..GitHubSource::default()
        },
        ..Start::default()
    };
    assert_eq!(review_on_issue.problems().len(), 1);
    let schedule = Start {
        kind: StartKind::Schedule,
        ..Start::default()
    };
    assert_eq!(schedule.problems().len(), 2, "{:?}", schedule.problems());
    assert!(Start::default().problems().is_empty());
}

// ------------------------------------------------- the Factory's starts

mod factory {
    use std::collections::BTreeMap;

    use super::super::*;
    use crate::factory::{
        Answer, Catalog, Command, Effect, Factory, Refusal, RefusalCode, Source, Status, Template,
    };
    use crate::store::Provider;

    fn project() -> crate::workspace::registry::ProjectId {
        serde_json::from_value(1.into()).unwrap()
    }

    fn agent() -> crate::factory::Agent {
        crate::factory::Agent {
            provider: Provider::Claude,
            model: None,
            effort: None,
        }
    }

    const HOUR: i64 = 3_600_000;

    /// A Workflow of one Step that starts as `start` says, saved as
    /// version 1.
    fn saved(factory: &mut Factory, start: Start) -> String {
        let Ok(Answer::Workflow(workflow)) = factory.command(
            Command::CreateWorkflow {
                project: project(),
                template: Template::Blank,
                agent: agent(),
            },
            0,
        ) else {
            panic!("no workflow");
        };
        let step = factory.workflow(&workflow).unwrap().draft.steps[0]
            .id
            .clone();
        factory
            .command(
                Command::EditStep {
                    workflow: workflow.clone(),
                    step,
                    change: crate::factory::StepChange {
                        instructions: Some("Do the work".into()),
                        ..Default::default()
                    },
                },
                0,
            )
            .unwrap();
        factory
            .command(
                Command::EditStart {
                    workflow: workflow.clone(),
                    start,
                },
                0,
            )
            .unwrap();
        save(factory, &workflow).unwrap();
        workflow
    }

    fn save(factory: &mut Factory, workflow: &str) -> Result<Answer, Refusal> {
        factory.command(
            Command::SaveWorkflow {
                workflow: workflow.into(),
                catalog: Catalog::default(),
            },
            0,
        )
    }

    fn turn(factory: &mut Factory, workflow: &str, turn: Turn, now_ms: i64) {
        factory
            .command(
                Command::Automation {
                    workflow: workflow.into(),
                    turn,
                },
                now_ms,
            )
            .unwrap();
    }

    fn step(factory: &mut Factory, now_ms: i64) -> Vec<Effect> {
        factory.step(now_ms, &BTreeMap::new())
    }

    fn automation(factory: &Factory, workflow: &str) -> Automation {
        factory.workflow(workflow).unwrap().automation.clone()
    }

    fn tasks(factory: &Factory) -> Vec<crate::factory::Task> {
        factory.board().tasks
    }

    fn workers(effects: &[Effect]) -> usize {
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::StartWorker { .. }))
            .count()
    }

    /// Every hour on the hour, told to tidy up.
    fn hourly() -> Start {
        Start {
            kind: StartKind::Schedule,
            schedule: Schedule {
                recurrence: Recurrence::Interval,
                interval_hours: 1,
                instructions: "Tidy the docs".into(),
                ..Schedule::default()
            },
            ..Start::default()
        }
    }

    /// Midnight, host-local, on Wednesday 2026-11-11: the schedules are
    /// the host's.
    fn t0() -> i64 {
        use chrono::TimeZone;
        chrono::Local
            .with_ymd_and_hms(2026, 11, 11, 0, 0, 0)
            .earliest()
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn saving_starts_nothing_and_the_switch_starts_on_its_own() {
        let mut factory = Factory::in_memory();
        let workflow = saved(&mut factory, hourly());
        assert_eq!(automation(&factory, &workflow).switch, Switch::Off);
        assert!(step(&mut factory, t0() + 5 * HOUR).is_empty());
        assert!(tasks(&factory).is_empty(), "off starts nothing");

        turn(&mut factory, &workflow, Turn::On, t0() + 10);
        assert_eq!(automation(&factory, &workflow).due_ms, Some(t0() + HOUR));
        assert!(step(&mut factory, t0() + HOUR - 1).is_empty(), "not yet");
        let effects = step(&mut factory, t0() + HOUR);
        let made = tasks(&factory);
        assert_eq!(made.len(), 1, "one run");
        assert_eq!(made[0].instructions, "Tidy the docs");
        assert_eq!(
            made[0].source,
            Some(Source::Schedule {
                workflow: workflow.clone()
            })
        );
        assert_eq!(workers(&effects), 1, "{effects:?}");
        assert_eq!(
            automation(&factory, &workflow).due_ms,
            Some(t0() + 2 * HOUR)
        );

        turn(&mut factory, &workflow, Turn::Pause, t0() + HOUR + 5);
        step(&mut factory, t0() + 2 * HOUR);
        assert_eq!(tasks(&factory).len(), 1, "paused starts nothing");
        turn(
            &mut factory,
            &workflow,
            Turn::Resume,
            t0() + 2 * HOUR + 30 * 60_000,
        );
        assert_eq!(
            automation(&factory, &workflow).due_ms,
            Some(t0() + 3 * HOUR)
        );
    }

    /// A run's Task takes the next id, as an operator's does: the two
    /// never share one.
    #[test]
    fn a_scheduled_task_and_a_later_one_never_share_an_id() {
        let mut factory = Factory::in_memory();
        let workflow = saved(&mut factory, hourly());
        turn(&mut factory, &workflow, Turn::On, t0() + 10);
        step(&mut factory, t0() + HOUR);
        let Ok(Answer::Task(later)) = factory.command(
            Command::Start {
                project: project(),
                instructions: "Fix the toolbar".into(),
                agent: agent(),
                request: None,
                workflow: None,
                pr: None,
            },
            t0() + HOUR + 5,
        ) else {
            panic!("no Task");
        };
        let ids: Vec<String> = tasks(&factory).into_iter().map(|task| task.id).collect();
        assert_eq!(ids.len(), 2, "{ids:?}");
        assert_ne!(ids[0], ids[1], "{ids:?}");
        assert!(ids.contains(&later));
    }

    #[test]
    fn turning_on_needs_a_saved_automatic_start() {
        let mut factory = Factory::in_memory();
        let workflow = saved(&mut factory, Start::default());
        let refused = factory.command(
            Command::Automation {
                workflow: workflow.clone(),
                turn: Turn::On,
            },
            0,
        );
        assert!(
            matches!(
                refused,
                Err(Refusal {
                    code: RefusalCode::Invalid,
                    ..
                })
            ),
            "{refused:?}"
        );
    }

    #[test]
    fn saving_a_different_start_turns_automatic_starts_off() {
        let mut factory = Factory::in_memory();
        let workflow = saved(&mut factory, hourly());
        turn(&mut factory, &workflow, Turn::On, t0());
        let mut later = hourly();
        later.schedule.interval_hours = 2;
        factory
            .command(
                Command::EditStart {
                    workflow: workflow.clone(),
                    start: later,
                },
                0,
            )
            .unwrap();
        assert_eq!(
            automation(&factory, &workflow).switch,
            Switch::On,
            "a draft"
        );
        save(&mut factory, &workflow).unwrap();
        assert_eq!(automation(&factory, &workflow).switch, Switch::Off);
    }

    #[test]
    fn a_run_missed_while_asleep_is_skipped_or_run_once() {
        for (missed, runs) in [(Missed::Skip, 0), (Missed::RunOnce, 1)] {
            let mut factory = Factory::in_memory();
            let mut start = hourly();
            start.schedule.missed = missed;
            let workflow = saved(&mut factory, start);
            turn(&mut factory, &workflow, Turn::On, t0());
            // Asleep from before one until half past three.
            step(&mut factory, t0() + 3 * HOUR + 30 * 60_000);
            assert_eq!(tasks(&factory).len(), runs, "{missed:?}");
            assert_eq!(
                automation(&factory, &workflow).due_ms,
                Some(t0() + 4 * HOUR),
                "{missed:?}: the next one is the next after now"
            );
        }
    }

    #[test]
    fn a_run_due_while_the_last_runs_is_skipped_or_queued() {
        for (overlap, after) in [(Overlap::Skip, 1), (Overlap::Queue, 2)] {
            let mut factory = Factory::in_memory();
            let mut start = hourly();
            start.schedule.overlap = overlap;
            let workflow = saved(&mut factory, start);
            turn(&mut factory, &workflow, Turn::On, t0());
            step(&mut factory, t0() + HOUR);
            step(&mut factory, t0() + 2 * HOUR);
            assert_eq!(
                tasks(&factory).len(),
                1,
                "{overlap:?}: the first still runs"
            );
            let first = tasks(&factory)[0].id.clone();
            factory
                .command(Command::Stop { task: first }, t0() + 2 * HOUR + 1)
                .unwrap();
            step(&mut factory, t0() + 2 * HOUR + 2);
            assert_eq!(tasks(&factory).len(), after, "{overlap:?}");
        }
    }

    fn issues(repository: &str, label: &str) -> Start {
        Start {
            kind: StartKind::GithubIssue,
            github: GitHubSource {
                repository: Some(repository.into()),
                trigger: Trigger::Labelled,
                label: Some(label.into()),
                extra_instructions: "Open a PR that closes it.".into(),
            },
            ..Start::default()
        }
    }

    fn item(number: u64, title: &str) -> GitHubItem {
        GitHubItem {
            number,
            title: title.into(),
            body: format!("Body of {number}"),
            url: format!("https://github.com/acme/app/issues/{number}"),
        }
    }

    /// The reads the step asks for: (workflow, version, query).
    fn reads(effects: &[Effect]) -> Vec<(String, u32, GitHubQuery)> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::ReadGitHub {
                    workflow,
                    version,
                    query,
                } => Some((workflow.clone(), *version, query.clone())),
                _ => None,
            })
            .collect()
    }

    fn found(
        factory: &mut Factory,
        workflow: &str,
        result: Result<GitHubRead, String>,
        now_ms: i64,
    ) -> Answer {
        factory
            .command(
                Command::GitHubRead {
                    workflow: workflow.into(),
                    version: 1,
                    result,
                },
                now_ms,
            )
            .unwrap()
    }

    fn matches(items: Vec<GitHubItem>) -> Result<GitHubRead, String> {
        Ok(GitHubRead::Matches {
            etag: Some("\"e1\"".into()),
            items,
        })
    }

    #[test]
    fn a_github_start_is_read_about_once_a_minute_while_on() {
        let mut factory = Factory::in_memory();
        let workflow = saved(&mut factory, issues("acme/app", "ready"));
        assert!(
            reads(&step(&mut factory, t0())).is_empty(),
            "off reads nothing"
        );
        turn(&mut factory, &workflow, Turn::On, t0());
        let asked = reads(&step(&mut factory, t0()));
        assert_eq!(asked.len(), 1, "{asked:?}");
        let (_, version, query) = &asked[0];
        assert_eq!(*version, 1);
        assert_eq!(query.kind, StartKind::GithubIssue);
        assert_eq!(query.source.repository.as_deref(), Some("acme/app"));
        assert_eq!(query.etag, None);
        assert!(
            reads(&step(&mut factory, t0() + 1_000)).is_empty(),
            "one at a time"
        );
        found(&mut factory, &workflow, matches(Vec::new()), t0() + 2_000);
        assert!(
            reads(&step(&mut factory, t0() + 30_000)).is_empty(),
            "not yet a minute"
        );
        let again = reads(&step(&mut factory, t0() + POLL_MS));
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].2.etag.as_deref(), Some("\"e1\""), "conditional");

        turn(&mut factory, &workflow, Turn::Pause, t0() + POLL_MS + 1);
        found(
            &mut factory,
            &workflow,
            Ok(GitHubRead::Unchanged),
            t0() + POLL_MS + 2,
        );
        assert!(
            reads(&step(&mut factory, t0() + 5 * POLL_MS)).is_empty(),
            "paused"
        );
    }

    #[test]
    fn each_matching_issue_starts_one_task_and_seeing_it_again_starts_nothing() {
        let mut factory = Factory::in_memory();
        let workflow = saved(&mut factory, issues("acme/app", "ready"));
        turn(&mut factory, &workflow, Turn::On, t0());
        step(&mut factory, t0());
        found(&mut factory, &workflow, matches(Vec::new()), t0() + 1);
        assert!(tasks(&factory).is_empty());

        step(&mut factory, t0() + POLL_MS);
        found(
            &mut factory,
            &workflow,
            matches(vec![item(7, "Login fails"), item(8, "Logout fails")]),
            t0() + POLL_MS + 1,
        );
        let made = tasks(&factory);
        assert_eq!(made.len(), 2);
        assert_eq!(made[0].title, "Login fails");
        for expected in [
            "Login fails",
            "Body of 7",
            "Open a PR that closes it.",
            "issues/7",
        ] {
            assert!(
                made[0].instructions.contains(expected),
                "{expected}: {}",
                made[0].instructions
            );
        }
        assert_eq!(
            made[0].source,
            Some(Source::Issue {
                repository: "acme/app".into(),
                number: 7,
                url: "https://github.com/acme/app/issues/7".into(),
            })
        );
        assert_eq!(made[0].workflow.as_ref().unwrap().id, workflow);

        step(&mut factory, t0() + 2 * POLL_MS);
        let revision = factory.revision();
        let answer = found(
            &mut factory,
            &workflow,
            matches(vec![item(7, "Login fails"), item(8, "Logout fails")]),
            t0() + 2 * POLL_MS + 1,
        );
        assert_eq!(answer, Answer::Changed(false));
        assert_eq!(
            factory.revision(),
            revision,
            "the same ones again change nothing"
        );
        step(&mut factory, t0() + 3 * POLL_MS);
        assert_eq!(
            found(
                &mut factory,
                &workflow,
                Ok(GitHubRead::Unchanged),
                t0() + 3 * POLL_MS + 1
            ),
            Answer::Changed(false)
        );
        assert_eq!(
            factory.revision(),
            revision,
            "an unchanged poll does nothing"
        );
        assert_eq!(tasks(&factory).len(), 2);
    }

    #[test]
    fn a_failed_read_is_reported_and_retried_at_the_next_check() {
        let mut factory = Factory::in_memory();
        let workflow = saved(&mut factory, issues("acme/app", "ready"));
        turn(&mut factory, &workflow, Turn::On, t0());
        step(&mut factory, t0());
        found(
            &mut factory,
            &workflow,
            Err("HTTP 404: Not Found".into()),
            t0() + 1,
        );
        assert_eq!(
            automation(&factory, &workflow).error.as_deref(),
            Some("HTTP 404: Not Found")
        );
        assert!(tasks(&factory).is_empty(), "never a started Task");
        assert!(reads(&step(&mut factory, t0() + 2)).is_empty());
        assert_eq!(
            reads(&step(&mut factory, t0() + POLL_MS + 1)).len(),
            1,
            "retried"
        );
        found(
            &mut factory,
            &workflow,
            matches(Vec::new()),
            t0() + POLL_MS + 2,
        );
        assert_eq!(automation(&factory, &workflow).error, None, "cleared");
    }

    #[test]
    fn turning_on_over_a_backlog_asks_once_defaulting_to_new_ones_only() {
        for take in [false, true] {
            let mut factory = Factory::in_memory();
            let workflow = saved(&mut factory, issues("acme/app", "ready"));
            turn(&mut factory, &workflow, Turn::On, t0());
            step(&mut factory, t0());
            found(
                &mut factory,
                &workflow,
                matches(vec![item(1, "Old one")]),
                t0() + 1,
            );
            assert!(
                tasks(&factory).is_empty(),
                "nothing taken before the answer"
            );
            assert_eq!(automation(&factory, &workflow).backlog.len(), 1);
            factory
                .command(
                    Command::Backlog {
                        workflow: workflow.clone(),
                        take,
                    },
                    t0() + 2,
                )
                .unwrap();
            assert!(automation(&factory, &workflow).backlog.is_empty());
            assert_eq!(tasks(&factory).len(), usize::from(take), "take: {take}");
            step(&mut factory, t0() + POLL_MS);
            found(
                &mut factory,
                &workflow,
                matches(vec![item(1, "Old one"), item(2, "New one")]),
                t0() + POLL_MS + 1,
            );
            let made = tasks(&factory);
            assert_eq!(made.last().unwrap().title, "New one");
            assert_eq!(made.len(), 1 + usize::from(take), "asked once: {take}");
            assert!(automation(&factory, &workflow).backlog.is_empty());
        }
    }

    #[test]
    fn a_pull_request_becomes_its_tasks_pr() {
        let mut factory = Factory::in_memory();
        let start = Start {
            kind: StartKind::GithubPr,
            github: GitHubSource {
                repository: Some("acme/app".into()),
                trigger: Trigger::ReviewRequested,
                ..GitHubSource::default()
            },
            ..Start::default()
        };
        let workflow = saved(&mut factory, start);
        turn(&mut factory, &workflow, Turn::On, t0());
        step(&mut factory, t0());
        found(&mut factory, &workflow, matches(Vec::new()), t0() + 1);
        step(&mut factory, t0() + POLL_MS);
        let pr = GitHubItem {
            number: 31,
            title: "Speed up search".into(),
            body: "Caches the index.".into(),
            url: "https://github.com/acme/app/pull/31".into(),
        };
        found(
            &mut factory,
            &workflow,
            matches(vec![pr]),
            t0() + POLL_MS + 1,
        );
        let made = &tasks(&factory)[0];
        assert_eq!(
            made.source,
            Some(Source::PullRequest {
                repository: "acme/app".into(),
                number: 31,
                url: "https://github.com/acme/app/pull/31".into(),
            })
        );
        assert_eq!(
            made.pull_request(),
            Some("https://github.com/acme/app/pull/31")
        );
        let carried = made
            .delivery
            .pr
            .as_ref()
            .expect("the PR is the Task's (#223)");
        assert_eq!(
            (carried.repository.as_str(), carried.number),
            ("acme/app", 31)
        );
        assert!(
            made.instructions.contains("gh pr checkout 31"),
            "{}",
            made.instructions
        );
    }

    fn submit(
        factory: &mut Factory,
        workflow: &str,
        identity: &str,
        instructions: &str,
    ) -> Result<Answer, Refusal> {
        factory.command(
            Command::Submit {
                workflow: workflow.into(),
                identity: identity.into(),
                instructions: instructions.into(),
            },
            t0(),
        )
    }

    #[test]
    fn a_script_submission_is_validated_and_its_identity_deduplicates() {
        let mut factory = Factory::in_memory();
        let manual = saved(&mut factory, Start::default());
        let external = saved(
            &mut factory,
            Start {
                kind: StartKind::External,
                ..Start::default()
            },
        );
        let refused = |answer: Result<Answer, Refusal>| answer.unwrap_err().code;
        assert_eq!(
            refused(submit(&mut factory, &external, "JIRA-1", "Fix it")),
            RefusalCode::NotNow,
            "off"
        );
        assert_eq!(
            refused(submit(&mut factory, &manual, "JIRA-1", "Fix it")),
            RefusalCode::Invalid,
            "not a script's Workflow"
        );
        turn(&mut factory, &external, Turn::On, t0());
        assert_eq!(
            refused(submit(&mut factory, &external, "", "Fix it")),
            RefusalCode::Invalid
        );
        assert_eq!(
            refused(submit(&mut factory, &external, "JIRA-1", "  ")),
            RefusalCode::Invalid
        );
        let Answer::Task(first) = submit(&mut factory, &external, "JIRA-1", "Fix it").unwrap()
        else {
            panic!("a new Task");
        };
        let again = submit(&mut factory, &external, "JIRA-1", "Fix it again").unwrap();
        assert_eq!(
            again,
            Answer::Duplicate(first),
            "a repeated identity is the existing Task, and says so"
        );
        assert_eq!(tasks(&factory).len(), 1);
        assert_eq!(
            tasks(&factory)[0].source,
            Some(Source::Script {
                identity: "JIRA-1".into()
            })
        );
    }

    #[test]
    fn the_limit_keeps_the_excess_queued_and_releases_it() {
        let mut factory = Factory::in_memory();
        assert_eq!(factory.limit(), 3, "the default");
        factory.set_limit(2);
        for n in 0..3 {
            factory
                .command(
                    Command::Start {
                        project: project(),
                        instructions: format!("Task {n}"),
                        agent: agent(),
                        request: None,
                        workflow: None,
                        pr: None,
                    },
                    t0(),
                )
                .unwrap();
        }
        assert_eq!(workers(&step(&mut factory, t0())), 2);
        let statuses: Vec<Status> = tasks(&factory).iter().map(|task| task.status).collect();
        assert_eq!(statuses, [Status::Working, Status::Working, Status::Queued]);
        assert_eq!(
            workers(&step(&mut factory, t0() + 1)),
            0,
            "still two working"
        );
        let first = tasks(&factory)[0].id.clone();
        factory
            .command(Command::Stop { task: first }, t0() + 2)
            .unwrap();
        assert_eq!(workers(&step(&mut factory, t0() + 3)), 1, "released");
        assert_eq!(tasks(&factory)[2].status, Status::Working);
    }
}
