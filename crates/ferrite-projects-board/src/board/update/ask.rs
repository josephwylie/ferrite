//! The one question box: a small question, y = yes, n or esc = no; enter never confirms. `valid`: is what the question
//! names still what "yes" would act on? Checked on every look at the tasks and again on y.
use crate::board::job::Job;
use crate::board::state::{Ask, State, Valid};
use crate::json::*;

/// Ask. `keys`: the answers' words (yes first); `detail`: what the question is about, shown quietly under it;
/// `danger`: a yes cannot be undone, so on the web board neither answer is the filled button.
pub fn confirm(s: &mut State, q: String, yes: Job, valid: Valid, keys: Vec<(&'static str, String)>, detail: String, danger: bool) {
    s.ask = Some(Ask { q, yes, no: None, valid, keys: if keys.is_empty() { vec![("sayYes", "yes".into()), ("sayNo", "no".into())] } else { keys }, detail, danger, nudge: false });
}
/// What a merge question is about: this pull request, at this reviewed commit, in this state.
pub fn merge_mark(s: &State, id: &str) -> String {
    let Some(x) = s.items.iter().find(|i| i.s("id") == id) else { return String::new() };
    let rv = x.at("review");
    serde_json::to_string(&json!([x.at("stage"), x.at("pr"), x.at("reviewedHead"), rv.at("score"), rv.at("recommend"), rv.at("pass"), rv.at("findings").as_array().map(|a| a.len()), rv.b("stale"), x.b("mergeFailed"), x.b("syncBlocked"), x.b("prFailure"), x.s("prState") == "failing", x.b("archivedAt")])).unwrap_or_default()
}
pub fn still_valid(s: &State, v: &Valid) -> bool { match v { Valid::Always => true, Valid::Mark(id, mark) => merge_mark(s, id) == *mark } }
