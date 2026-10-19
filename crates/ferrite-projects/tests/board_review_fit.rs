//! A task at work in a short side panel: the settings outlast every text (what the agent said, the review's summary,
//! then the description give way first), the ones that say who works and who reviews stay longest with a line that
//! counts the rest, and where the task stands and what it is doing now stay whole at every height, wrapped, never cut.
use crate::common;
use crate::review_kit;
use common::*;
use review_kit::*;
use serde_json::json;
use sw::json::*;

fn busy(v: &V) -> Obj {
    let now = v.now;
    with(&v.pr(), json!({ "reviewRound": 3, "review": { "score": 4, "recommend": true, "text": "Fine now.", "stale": false }, "reviews": [
        { "round": 1, "at": now - 7200000, "score": 2, "recommend": false, "text": "Crashes on start.", "problems": ["game.js: null plane."], "checks": ["Ran it."], "limitations": [], "head": "1111111aaaa" },
        { "round": 2, "at": now - 3600000, "error": "the reviewer finished without a verdict" },
        { "round": 3, "at": now - 60000, "score": 4, "recommend": true, "text": "Fine now.", "problems": [], "checks": ["Ran the tests: all pass."], "limitations": ["Not tried on a phone."], "head": "abcdef12345" }],
        "title": "Build a flying game with a very long name that wraps onto two lines", "state": "working", "kind": null, "stage": "working", "progress": 40, "eta": 12, "msg": "Running the whole test suite again after the last fix",
        "said": "Working on it.", "description": "Long. ".repeat(200), "usage": { "in": 1000, "out": 2000, "context": 5000, "window": 200000 }, "link": { "pane": "p1", "workspace": "w1" } }))
}
const SETTINGS: [&str; 7] = ["Agent", "Plan first", "Reviewer", "Review when", "Send back", "Merge", "Also"];
const STATE: &str = " State        ◐ working · for 0s";
const PROGRESS: &str = " Progress     ██████░░░░░░░░░ 40% · 12m left";
const NOW: [&str; 2] = [" Now          Running the whole test suite", "              again after the last fix"];

#[test]
fn a_short_window_the_settings_outlast_every_text_and_the_status_stays_whole() {
    let _w = world();
    let mut v = V::new();
    let busy = busy(&v);
    for fit in [40, 30, 24, 20, 16, 12, 8, 6] {
        let lines = v.panel(&busy, 45, false, fit);
        let at = format!("with room for {fit} lines:\n{}", lines.join("\n"));
        assert!(lines.len() <= fit, "fits in {at}");
        let st = lines.iter().position(|l| l == STATE).unwrap_or_else(|| panic!("the state, {at}"));
        assert_eq!(lines[st + 1..st + 4].to_vec(), vec![PROGRESS, NOW[0], NOW[1]], "the status and what it is doing, whole, {at}");
        let kept: Vec<&str> = SETTINGS.iter().copied().filter(|f| lines.iter().any(|l| l.starts_with(&format!(" {f} ")))).collect();
        let more = lines.iter().find(|l| has(l, r"more settings? · ctrl\+e shows"));
        let want: Vec<&str> = if fit >= 30 { SETTINGS.to_vec() } else if fit == 24 { vec!["Agent", "Plan first", "Reviewer", "Review when", "Send back", "Merge"] } else if fit == 20 { vec!["Agent", "Reviewer"] } else { vec![] };
        assert_eq!(kept, want, "the settings {at}");
        assert_eq!(more.map(String::as_str), if fit == 24 { Some(" 7 more settings · ctrl+e shows them") } else if fit == 20 { Some(" 13 more settings · ctrl+e shows them") } else { None }, "how many settings have no room, {at}");
        assert_eq!(lines.iter().any(|l| l == " Settings ──────────────────────────────────"), !kept.is_empty(), "the settings keep their headline while one of them is there, {at}");
        assert_eq!(lines.iter().any(|l| l == " Description ───────────────────────────────"), fit >= 40, "the description {at}");
        assert_eq!(lines.iter().any(|l| l == " Description · alt+t shows it"), fit == 30, "one line for the description {at}");
        assert!(!lines.iter().any(|l| l.starts_with(" The agent said")), "what the agent said gives way before a setting does, {at}");
        assert_eq!(lines.iter().any(|l| l.starts_with(" Review 3     ✓ 4/5, says merge")), fit > 6, "the latest review {at}");
        assert_eq!(lines.iter().any(|l| l == "              Fine now."), fit == 16, "the review's summary {at}");
        assert_eq!(lines.iter().any(|l| l == " Status ────────────────────────────────────"), fit >= 12, "the headline {at}");
        assert_eq!(lines.iter().any(|l| l.starts_with(" #1 Build a flying game")), fit >= 6, "the title {at}");
        assert!(!lines.iter().any(|l| has(l, r"^[ ▌] {13}") && l.ends_with('…')) && !lines.iter().any(|l| SETTINGS.iter().any(|k| l.starts_with(&format!(" {k} "))) && l.ends_with('…')), "no fact is cut with an ellipsis, {at}");
        assert_eq!(lines.last().map(String::as_str), Some(" alt+t shows the whole task"), "{at}");
    }
    // shorter than the status and that text with the last line: the last line goes, not a word of the text; then the
    // progress, then the state; only a text longer than the panel is cut, and says so
    let mut at = |fit| v.panel(&busy, 45, false, fit);
    assert_eq!(at(5), vec![STATE, PROGRESS, NOW[0], NOW[1], " alt+t shows the whole task"]);
    assert_eq!(at(4), vec![STATE, PROGRESS, NOW[0], NOW[1]]);
    assert_eq!(at(3), vec![STATE, NOW[0], NOW[1]]); assert_eq!(at(2), NOW.to_vec());
    assert_eq!(at(1), vec![" Now          Running the whole test suite …"]);
}

#[test]
fn a_long_line_of_what_it_is_doing_is_wrapped_under_itself_and_never_cut() {
    let _w = world();
    let mut v = V::new();
    let long = with(&busy(&v), json!({ "msg": "Found cause: second device resizing board; now checking every caller of the resize handler for the same mistake" }));
    for (width, fit) in [(45, 30), (30, 30), (30, 10), (20, 0)] {
        let lines = v.panel(&long, width, false, fit);
        let at = format!("{width} columns in {fit} lines:\n{}", lines.join("\n"));
        let from = lines.iter().position(|l| l.starts_with(" Now")).unwrap_or_else(|| panic!("{at}"));
        let next = lines.iter().enumerate().position(|(n, l)| n > from && (has(l, r"^ \S") || l.is_empty())).unwrap_or(lines.len());
        let now = &lines[from..next];
        assert!(now.len() > 2 && lines.iter().all(|l| sw::text::width(l) <= width), "wrapped inside {at}");
        assert_eq!(now.iter().map(|l| l.trim_start_matches(" Now").trim()).filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" "), long.s("msg"), "nothing cut at {at}");
        assert!(lines.iter().any(|l| l.contains("◐ working")), "the state at {at}");
        if fit == 10 { assert!(lines.len() <= fit && !lines.iter().any(|l| l.starts_with(" Build") || l.starts_with(" Project")), "{at}"); }
    }
    m(&v.panel(&busy(&v), 45, false, 30).join("\n"), r"\n Status ─+\n State {8}◐ working · for 0s\n Progress {5}█+░+ 40% · 12m left\n Now {10}Running the whole test suite\n[ ▌] {13}again after the last fix\n"); // with room, what it is doing is there, a fact under Status
}
