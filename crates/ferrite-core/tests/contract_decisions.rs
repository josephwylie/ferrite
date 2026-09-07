#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use ferrite_core::DecisionAnswer;
use serde_json::json;
use support::*;

#[test]
fn codex_computer_use_approval_exposes_session_persistence_and_returns_native_meta() {
    let mut r = Replay::new(
        "codex",
        vec![json!({
            "id":"computer-use", "method":"mcpServer/elicitation/request",
            "params":{"threadId":"root","serverName":"computer-use","mode":"form",
                "message":"Allow Computer Use to use Ferrite?",
                "requestedSchema":{"type":"object","properties":{}},
                "_meta":{"codex_approval_kind":"mcp_tool_call","persist":["session","always"]}}
        })],
    );
    let ds = decisions(&r.drain());
    assert!(matches!(ds[0].kind, ferrite_core::DecisionKind::Approval));
    let choice = ds[0]
        .suggestions
        .iter()
        .find(|choice| choice.label == "Allow for this session")
        .expect("native persistence must be offered by the shared approval UI");
    r.session
        .respond_to_decision(
            &ds[0].id,
            DecisionAnswer::Choose {
                value: choice.value.clone(),
            },
        )
        .unwrap();
    let reply = r.wait_host(|v| v["id"] == "computer-use" && v.get("result").is_some());
    assert_eq!(
        reply["result"],
        json!({
            "action":"accept","content":null,"_meta":{"persist":"session"}
        })
    );
}

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
fn codex_mcp_approval_never_invents_or_implicitly_selects_persistence() {
    for persist in [
        json!("session"),
        json!(["session"]),
        json!(null),
        json!(["unknown"]),
    ] {
        let mut r = Replay::new(
            "codex",
            vec![json!({
                "id":90,"method":"mcpServer/elicitation/request",
                "params":{"threadId":"root","mode":"openai/form","message":"Allow access?",
                    "requestedSchema":null,
                    "_meta":{"codex_approval_kind":"mcp_tool_call","persist":persist}}
            })],
        );
        let ds = decisions(&r.drain());
        assert!(matches!(ds[0].kind, ferrite_core::DecisionKind::Approval));
        assert!(!ds[0]
            .suggestions
            .iter()
            .any(|choice| choice.label == "Always allow"));
        assert!(r.session.respond_to_decision(&ds[0].id, DecisionAnswer::Choose {
            value: json!({"action":"accept","content":null,"_meta":{"persist":"always"}}),
        }).is_err(), "unadvertised persistence must be rejected");
        r.session
            .respond_to_decision(&ds[0].id, DecisionAnswer::Allow { input: json!(null) })
            .unwrap();
        assert_eq!(
            r.wait_host(|v| v["id"] == 90 && v.get("result").is_some())["result"],
            json!({"action":"accept","content":null,"_meta":null}),
            "ordinary Allow must remain one-shot, even when persistence was offered"
        );
    }
}

#[test]
fn codex_mcp_approval_always_and_cancel_keep_distinct_native_replies() {
    for (label, expected) in [
        (
            "Always allow",
            json!({"action":"accept","content":null,"_meta":{"persist":"always"}}),
        ),
        (
            "Cancel",
            json!({"action":"cancel","content":null,"_meta":null}),
        ),
    ] {
        let mut r = Replay::new(
            "codex",
            vec![json!({
                "id":91,"method":"mcpServer/elicitation/request",
                "params":{"threadId":"root","mode":"form","message":"Allow access?",
                    "requestedSchema":{"type":"object","properties":{}},
                    "_meta":{"codex_approval_kind":"mcp_tool_call","persist":"always"}}
            })],
        );
        let ds = decisions(&r.drain());
        assert!(!ds[0]
            .suggestions
            .iter()
            .any(|choice| choice.label == "Allow for this session"));
        let choice = ds[0]
            .suggestions
            .iter()
            .find(|choice| choice.label == label)
            .unwrap();
        assert_eq!(choice.standing, label == "Always allow");
        r.session
            .respond_to_decision(
                &ds[0].id,
                DecisionAnswer::Choose {
                    value: choice.value.clone(),
                },
            )
            .unwrap();
        assert_eq!(
            r.wait_host(|v| v["id"] == 91 && v.get("result").is_some())["result"],
            expected
        );
    }
}

#[test]
fn codex_mcp_approval_metadata_does_not_discard_required_form_fields() {
    let mut r = Replay::new(
        "codex",
        vec![json!({
            "id":92,"method":"mcpServer/elicitation/request",
            "params":{"threadId":"root","mode":"form","message":"Specify access",
                "requestedSchema":{"type":"object","properties":{"scope":{"type":"string"}},"required":["scope"]},
                "_meta":{"codex_approval_kind":"mcp_tool_call","persist":["session","always"]}}
        })],
    );
    let ds = decisions(&r.drain());
    assert!(matches!(&ds[0].kind, ferrite_core::DecisionKind::Form { fields } if fields.len()==1));
    assert!(ds[0].suggestions.is_empty());
    assert!(r
        .session
        .respond_to_decision(&ds[0].id, DecisionAnswer::Allow { input: json!(null) })
        .is_err());
    r.session
        .respond_to_decision(
            &ds[0].id,
            DecisionAnswer::Form {
                values: json!({"scope":"read"}),
            },
        )
        .unwrap();
    assert_eq!(
        r.wait_host(|v| v["id"] == 92 && v.get("result").is_some())["result"],
        json!({"action":"accept","content":{"scope":"read"},"_meta":null})
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

#[test]
fn native_approval_options_have_labels_and_keep_distinct_response_tokens() {
    let mut r = Replay::new(
        "codex",
        vec![
            json!({"id":83,"method":"item/commandExecution/requestApproval","params":{"threadId":"root","turnId":"turn","itemId":"cmd","availableDecisions":["acceptForSession",{"applyNetworkPolicyAmendment":{"network_policy_amendment":{"host":"example.com","action":"deny"}}},"cancel"]}}),
        ],
    );
    let ds = decisions(&r.drain());
    assert!(
        !ds[0].policy.allow,
        "plain accept is absent; a standing choice is separate"
    );
    assert_eq!(
        ds[0].suggestions.len(),
        3,
        "all native choices must remain usable, including deny-policy and cancel"
    );
    assert!(ds[0].suggestions[0]
        .label
        .to_lowercase()
        .contains("session"));
    assert!(ds[0].suggestions[1].label.contains("example.com"));
    assert_eq!(
        ds[0].standing_answer(),
        Some(&json!("acceptForSession")),
        "deny options must never be assigned to the always shortcut"
    );
    r.session
        .respond_to_decision(
            &ds[0].id,
            DecisionAnswer::Choose {
                value: ds[0].suggestions[1].value.clone(),
            },
        )
        .unwrap();
    assert_eq!(
        r.wait_host(|v| v["id"] == 83 && v.get("result").is_some())["result"]["decision"],
        json!({"applyNetworkPolicyAmendment":{"network_policy_amendment":{"host":"example.com","action":"deny"}}})
    );
}
