//! Merging, the rest of the old suite's checks: a merge conflict with your checkout waits for you as a question of
//! permission, not as a review (the rest of that test is lifecycle.rs).
use crate::common;
use common::*;
use serde_json::json;
use sw::json::*;

#[test]
fn a_merge_conflict_with_your_checkout_waits_for_you_and_is_not_shown_as_a_review() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.when": "never" }), ..Default::default() });
    w.script(json!({ "worker": [{ "files": { "a.txt": "from the agent\n" }, "commit": "agent", "note": "Changed a.txt." }] }));
    let id = w.add("Conflict");
    sw::store::settings::change::set_value("review.rounds", &json!(0), &sw::store::settings::lookup::Scope::task(&task(&id))).unwrap();
    delegate(&id);
    assert_eq!(settle(&id).s("stage"), "approval");
    std::fs::write(w.root.join("a.txt"), "from the human\n").unwrap(); git(&w.root, &["commit", "-qam", "human"]);
    sw::actions::finish_line::approve(&id, None, false).unwrap();
    let t = settle(&id);
    assert_eq!((t.s("stage"), t.at("mergeFailed")), ("approval", &json!(true)));
    let r = row(&id);
    assert_eq!((r.s("state"), r.s("kind"), r.at("asReview")), ("blocked", "permission", &json!(false)));
}
