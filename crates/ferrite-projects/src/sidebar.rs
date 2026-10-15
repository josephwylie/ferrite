//! What the Herdr sidebar shows for projects: per agent row a name
//! (`--display-agent`); the project grouping of `grouping` (order and a bold
//! head row) in the agents and the Spaces list; a tab-bar count; the default
//! agent view, grouped by project.
//! Ferrite: the `config.toml` edit `configure` made to render them is gone;
//! Ferrite draws Projects natively.

use std::path::Path;

use crate::herdr::{CALL_TIMEOUT, Herdr};
use crate::project::{self, Project};
use crate::thread::{Group, Live, Thread};

pub const TOKEN_TTL_MS: u64 = 300_000;
/// Tokens this plugin wrote before 0.2.0; cleared on every report.
pub const OLD_TOKENS: [&str; 4] = ["project", "thread", "review", "rank"];
/// Minutes without a self-report after which a working row says "Nm quiet".
pub const QUIET_SECS: i64 = 300;

/// The short word of a group, as the sidebar and tab bar show it.
pub fn word(group: Group) -> &'static str {
    match group {
        Group::WaitingOnYou => "needs you",
        Group::ReadyForReview => "review",
        Group::Working => "working",
        Group::Landing => "landing",
        Group::Idle => "idle",
        Group::Resolved => "resolved",
    }
}

/// Groups that need the user: counted in the project row and the tab bar.
pub fn needs_you(group: Group) -> bool {
    matches!(group, Group::WaitingOnYou | Group::ReadyForReview)
}

fn pr_number(url: &str) -> Option<&str> {
    url.rsplit('/').next().filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// Line 3 of a thread row: the group word and one fact.
pub fn state_line(group: Group, thread: &Thread, live: &Live, now: jiff::Timestamp) -> String {
    let percent = live.self_report.as_ref().and_then(|r| r.percent).filter(|p| *p < 100).map(|p| format!("~{p}%"));
    let pr = pr_number(&thread.pr).map(|n| format!("PR #{n}"));
    let fact = match group {
        Group::WaitingOnYou if !live.pane_exists => Some("pane closed".to_string()),
        Group::WaitingOnYou if thread.status == crate::thread::Status::Failed => Some("failed".to_string()),
        Group::WaitingOnYou if live.agent_state.as_deref() == Some("blocked") => Some("blocked".to_string()),
        Group::WaitingOnYou => percent,
        Group::ReadyForReview | Group::Landing => pr.or_else(|| Some("report".into())),
        Group::Working => match (&live.self_report, percent) {
            (_, Some(p)) if live.report_age_secs < QUIET_SECS => Some(p),
            _ => {
                let since = match &live.self_report {
                    Some(record) => live.report_age_secs.max(now.as_second() - record.reported_at),
                    None => crate::thread::seconds_since(&thread.last_state_change, now),
                };
                (since >= QUIET_SECS && !thread.prompt_pending).then(|| format!("{}m quiet", since / 60))
            }
        },
        Group::Idle | Group::Resolved => None,
    };
    match fact {
        Some(fact) => format!("{} · {fact}", word(group)),
        None => word(group).to_string(),
    }
}

/// Row 1 of a thread: its id and title, capped by Herdr at 80 characters.
pub fn thread_display(thread: &Thread) -> String {
    format!("{} · {}", thread.id, thread.title)
}

/// A coordinator's row name: the project's name, marked as its group's head
/// in the agents list (see `grouping::HEAD_MARK`).
pub fn coordinator_display(project: &Project) -> String {
    let name = project.read_project_md().map(|(s, _)| project::display_name(&s.name, &project.slug)).unwrap_or_else(|_| project.slug.clone());
    format!("{name}{}", crate::grouping::HEAD_MARK)
}

/// Reports one pane's row: display name, project and rank, with a TTL so the
/// row falls back to Herdr's own when the ticker stops. Never `--seq`: token
/// patches are per key and the grouping tokens come from `grouping`.
pub fn report_pane(herdr: &Herdr, pane: &str, display: &str, slug: &str, group: Group) {
    let ttl = TOKEN_TTL_MS.to_string();
    let rank = group.rank().to_string();
    let tokens = [format!("hp_project={slug}"), format!("hp_rank={rank}")];
    let mut args = vec!["pane", "report-metadata", pane, "--source", crate::herdr::SOURCE, "--display-agent", display, "--ttl-ms", &ttl];
    for token in &tokens {
        args.push("--token");
        args.push(token);
    }
    for old in OLD_TOKENS {
        args.push("--clear-token");
        args.push(old);
    }
    let _ = herdr.call(&args, CALL_TIMEOUT);
}

/// Clears every token and the display name this plugin set on a pane.
pub fn clear_pane(herdr: &Herdr, pane: &str) {
    if pane.is_empty() {
        return;
    }
    let mut args = vec!["pane", "report-metadata", pane, "--source", crate::herdr::SOURCE, "--clear-display-agent"];
    for name in ["hp_project", "hp_rank"].into_iter().chain(OLD_TOKENS) {
        args.push("--clear-token");
        args.push(name);
    }
    let _ = herdr.call(&args, CALL_TIMEOUT);
    crate::grouping::clear(herdr, "pane", pane);
}

/// `2 need you · 3 working`, `paused`, or `idle`.
pub fn project_line(groups: &[Group], paused: bool) -> String {
    if paused {
        return "paused".into();
    }
    let count = |f: &dyn Fn(Group) -> bool| groups.iter().filter(|g| f(**g)).count();
    let mut parts = Vec::new();
    let need = count(&|g| needs_you(g));
    if need > 0 {
        parts.push(format!("{need} need you"));
    }
    for (group, label) in [(Group::Working, "working"), (Group::Landing, "landing")] {
        let n = count(&|g| g == group);
        if n > 0 {
            parts.push(format!("{n} {label}"));
        }
    }
    if parts.is_empty() { "idle".into() } else { parts.join(" · ") }
}

/// Clears the grouping tokens of a Space.
pub fn clear_workspace(herdr: &Herdr, workspace: &str) {
    crate::grouping::clear(herdr, "workspace", workspace);
}

/// A card's sub-line: what the state line adds to Herdr's own state word,
/// then the agent's last activity. `working · ~40%` and `Writing tests` give
/// `~40% · Writing tests`; a bare `idle` or `working` gives nothing.
pub fn sub_line(state_line: &str, activity: &str) -> String {
    let fact = match state_line {
        "idle" | "working" | "resolved" => "",
        line => line.strip_prefix("working · ").unwrap_or(line),
    };
    [fact, activity.trim()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" · ")
}

/// The groups of a project's open threads, as the ticker last persisted them.
pub fn recorded_groups(project: &Project) -> Vec<Group> {
    crate::thread::list(project)
        .into_iter()
        .filter(|t| t.status != crate::thread::Status::Resolved)
        .filter_map(|t| Group::from_token(&t.last_group))
        .collect()
}

/// `needs-you --line`, the tab-bar entry: `projects: N need you`, from the
/// thread records the ticker persists. Nothing when none need the user or when
/// the ticker is not running (a stale count must not sit in the tab bar).
pub fn needs_you_line(root: &Path) -> Option<String> {
    if crate::ticker::lock_state(root) == crate::ticker::LockState::Free {
        return None;
    }
    let n: usize = project::list_slugs(root)
        .iter()
        .filter_map(|slug| Project::load(root, slug).ok())
        .filter(|p| p.status() == project::Status::Active)
        .map(|p| recorded_groups(&p).into_iter().filter(|g| needs_you(*g)).count())
        .sum();
    (n > 0).then(|| format!("projects: {n} need you"))
}

// ---------------------------------------------------------------- agent view

/// The default view: agents in project blocks (`hp_group`, see `grouping`),
/// each by need; agents without it last.
pub fn default_view() -> serde_json::Value {
    serde_json::json!({
        "source": crate::herdr::SOURCE,
        "label": "projects",
        "sort": [{ "field": { "token": "hp_group" }, "order": "asc" }, { "field": { "token": "hp_rank" }, "order": "asc" }],
    })
}

/// `focus <slug>`: only that project's agents, by need.
pub fn project_view(slug: &str) -> serde_json::Value {
    serde_json::json!({
        "source": crate::herdr::SOURCE,
        "label": format!("project: {slug}"),
        "filter": { "op": "eq", "field": { "token": "hp_project" }, "value": slug },
        "sort": [{ "field": { "token": "hp_group" }, "order": "asc" }, { "field": { "token": "hp_rank" }, "order": "asc" }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thread::Status;

    #[test]
    fn sub_lines_keep_what_herdrs_state_word_does_not_say() {
        assert_eq!(sub_line("working · ~40%", "Writing tests"), "~40% · Writing tests");
        assert_eq!(sub_line("working", ""), "");
        assert_eq!(sub_line("idle", "Done"), "Done");
        assert_eq!(sub_line("review · report", ""), "review · report");
        assert_eq!(sub_line("needs you · blocked", " Waiting for you "), "needs you · blocked · Waiting for you");
        assert_eq!(sub_line("", "Reading code"), "Reading code");
    }

    fn live(state: &str) -> Live {
        Live { pane_exists: true, agent_state: Some(state.into()), ..Live::default() }
    }

    #[test]
    fn state_lines_carry_the_word_and_one_fact() {
        let now: jiff::Timestamp = "2026-09-23T12:00:00Z".parse().unwrap();
        let t = Thread { status: Status::Open, last_state_change: "2026-09-23T11:40:00Z".into(), pr: "https://github.com/o/r/pull/4".into(), ..Thread::default() };
        let with = |activity: &str, percent: Option<u8>, age: i64, state: &str| {
            let record = crate::progress::Record { activity: activity.into(), percent, reported_at: now.as_second() - age, ..Default::default() };
            Live { self_report: Some(record), report_age_secs: age, ..live(state) }
        };
        assert_eq!(state_line(Group::WaitingOnYou, &t, &with("Waiting for you", Some(55), 10, "idle"), now), "needs you · ~55%");
        assert_eq!(state_line(Group::WaitingOnYou, &t, &live("blocked"), now), "needs you · blocked");
        assert_eq!(state_line(Group::WaitingOnYou, &t, &Live::default(), now), "needs you · pane closed");
        assert_eq!(state_line(Group::ReadyForReview, &t, &live("idle"), now), "review · PR #4");
        assert_eq!(state_line(Group::Landing, &t, &live("idle"), now), "landing · PR #4");
        assert_eq!(state_line(Group::Working, &t, &with("Testing", Some(40), 30, "working"), now), "working · ~40%");
        // A harness without self-reports, silent for 20 minutes.
        assert_eq!(state_line(Group::Working, &t, &live("working"), now), "working · 20m quiet");
        assert_eq!(state_line(Group::Working, &t, &with("Testing", Some(40), 720, "working"), now), "working · 12m quiet");
        assert_eq!(state_line(Group::Idle, &t, &live("idle"), now), "idle");
        let no_pr = Thread { pr: String::new(), ..t };
        assert_eq!(state_line(Group::ReadyForReview, &no_pr, &live("idle"), now), "review · report");
    }

    #[test]
    fn project_lines_count_what_needs_you() {
        use Group::*;
        assert_eq!(project_line(&[WaitingOnYou, ReadyForReview, Working, Working, Working, Idle], false), "2 need you · 3 working");
        assert_eq!(project_line(&[Landing], false), "1 landing");
        assert_eq!(project_line(&[Idle], false), "idle");
        assert_eq!(project_line(&[WaitingOnYou], true), "paused");
    }
}
