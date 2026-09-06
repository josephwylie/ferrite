#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use ferrite_core::DecisionAnswer;
use serde_json::json;
use support::*;
#[test]
fn codex_native_question_is_answerable_with_original_wire_id() {
    let mut r = Replay::new(
        "codex",
        vec![
            json!({"id":"question-7","method":"item/tool/requestUserInput","params":{"threadId":"root","turnId":"turn","itemId":"tool","questions":[{"id":"choice","header":"Target","question":"Where?","options":[{"label":"Local","description":"This checkout"},{"label":"Remote","description":"Other checkout"}],"isOther":true,"isSecret":false}]}}),
        ],
    );
    let ds = decisions(&r.drain());
    assert_eq!(
        ds.len(),
        1,
        "native question must not leave the provider waiting invisibly"
    );
    assert!(ds[0].blocks_execution());
    r.session
        .respond_to_decision(
            &ds[0].id,
            DecisionAnswer::Allow {
                input: json!({"answers":{"choice":{"answers":["Local"]}}}),
            },
        )
        .unwrap();
    let reply = r.wait_host(|v| v["id"] == "question-7" && v.get("result").is_some());
    assert_eq!(
        reply["result"],
        json!({"answers":{"choice":{"answers":["Local"]}}})
    );
}
#[test]
fn codex_deny_network_amendment_is_never_offered_as_always_allow() {
    let r = Replay::new(
        "codex",
        vec![
            json!({"id":77,"method":"item/commandExecution/requestApproval","params":{"threadId":"root","turnId":"turn","itemId":"cmd","availableDecisions":["accept",{"applyNetworkPolicyAmendment":{"network_policy_amendment":{"host":"example.com","action":"deny"}}},"decline"]}}),
        ],
    );
    let ds = decisions(&r.drain());
    assert_eq!(ds.len(), 1);
    assert!(ds[0].standing_answer().is_none());
}
#[test]
fn claude_suppressed_standing_approval_is_not_offered() {
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"control_request","request_id":"approval","request":{"subtype":"can_use_tool","tool_name":"Bash","tool_use_id":"tool","input":{"command":"echo hello"},"suppress_always_allow_rule":true,"permission_suggestions":[{"type":"setMode","mode":"bypassPermissions","destination":"session"}]}}),
        ],
    );
    let ds = decisions(&r.drain());
    assert_eq!(ds.len(), 1);
    assert!(ds[0].standing_answer().is_none());
}

#[test]
fn codex_question_form_preserves_ids_and_labels_with_commas() {
    let mut r = Replay::new(
        "codex",
        vec![
            json!({"id":9,"method":"item/tool/requestUserInput","params":{"threadId":"root","turnId":"turn","itemId":"tool","questions":[{"id":"first","header":"One","question":"Choose","options":[{"label":"Red, green","description":"Together"},{"label":"Blue","description":"Alone"}],"isOther":true,"isSecret":false},{"id":"second","header":"Two","question":"Choose","options":null,"isOther":true,"isSecret":false}]}}),
        ],
    );
    let ds = decisions(&r.drain());
    assert_eq!(ds.len(), 1);
    let qs =
        ferrite_core::questions::parse(&ds[0].input).expect("native question is a usable form");
    let input = ferrite_core::questions::answered_input(
        &ds[0].input,
        &[
            ferrite_core::questions::Answer {
                picks: vec![0],
                other: None,
            },
            ferrite_core::questions::Answer {
                picks: vec![],
                other: Some("free text".into()),
            },
        ],
        &qs,
    );
    r.session
        .respond_to_decision(&ds[0].id, DecisionAnswer::Allow { input })
        .unwrap();
    let reply = r.wait_host(|v| v["id"] == 9 && v.get("result").is_some());
    assert_eq!(
        reply["result"],
        json!({"answers":{"first":{"answers":["Red, green"]},"second":{"answers":["free text"]}}})
    );
}
