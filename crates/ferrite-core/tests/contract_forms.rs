#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use ferrite_core::{DecisionAnswer, DecisionKind};
use serde_json::json;
use support::*;

#[test]
fn question_presentation_and_answers_are_provider_neutral() {
    for provider in ["claude", "codex"] {
        let frame = if provider == "claude" {
            json!({"type":"control_request","request_id":"q","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","tool_use_id":"tool","input":{"questions":[{"question":"Choose","header":"Choice","options":[{"label":"Red, green","description":"Both"},{"label":"Blue","description":"One"}]}]}}})
        } else {
            json!({"id":"q","method":"item/tool/requestUserInput","params":{"threadId":"root","turnId":"turn","itemId":"tool","questions":[{"id":"stable","question":"Choose","header":"Choice","options":[{"label":"Red, green","description":"Both"},{"label":"Blue","description":"One"}],"isOther":false,"isSecret":true}]}})
        };
        let mut r = Replay::new(provider, vec![frame]);
        let ds = decisions(&r.drain());
        let DecisionKind::Questions(qs) = &ds[0].kind else { panic!("adapter must fill the typed shared form") };
        assert_eq!(qs[0].question, "Choose");
        if provider == "codex" {
            assert!(qs[0].secret);
            assert!(!qs[0].allow_other);
        }
        r.session.respond_to_decision(&ds[0].id, DecisionAnswer::Questions {
            answers: vec![ferrite_core::questions::Answer { picks: vec![0], other: None }],
        }).unwrap();
        if provider == "codex" {
            let reply = r.wait_host(|v| v["id"] == "q" && v.get("result").is_some());
            assert_eq!(reply["result"], json!({"answers":{"stable":{"answers":["Red, green"]}}}));
        } else {
            let reply = r.wait_host(|v| v["type"] == "control_response");
            assert_eq!(reply["response"]["response"]["updatedInput"]["answers"], json!({"Choose":"Red, green"}));
        }
    }
}

#[test]
fn both_providers_offer_the_same_typed_elicitation_form() {
    for provider in ["claude", "codex"] {
        let schema = json!({"type":"object","properties":{"count":{"type":"integer","title":"Count","minimum":1,"maximum":3},"enabled":{"type":"boolean","title":"Enabled"}},"required":["count"]});
        let frame = if provider == "claude" {
            json!({"type":"control_request","request_id":"form","request":{"subtype":"elicitation","mcp_server_name":"server","message":"Configure search","mode":"form","requested_schema":schema}})
        } else {
            json!({"id":71,"method":"mcpServer/elicitation/request","params":{"threadId":"root","turnId":"turn","serverName":"server","mode":"form","message":"Configure search","requestedSchema":schema,"_meta":null}})
        };
        let mut r = Replay::new(provider, vec![frame]);
        let ds = decisions(&r.drain());
        assert_eq!(ds.len(), 1, "elicitation must not strand the provider");
        let DecisionKind::Form { fields } = &ds[0].kind else { panic!("MCP schema must normalize before the UI") };
        assert_eq!(fields.len(), 2);
        assert!(fields.iter().any(|f| f.id == "count" && f.required));
        assert!(r.session.respond_to_decision(&ds[0].id, DecisionAnswer::Form { values: json!({"count":9}) }).is_err(), "native field constraints must be enforced");
        r.session.respond_to_decision(&ds[0].id, DecisionAnswer::Form { values: json!({"count":2,"enabled":true}) }).unwrap();
        let reply = if provider == "codex" {
            r.wait_host(|v| v["id"] == 71 && v.get("result").is_some())["result"].clone()
        } else {
            r.wait_host(|v| v["type"] == "control_response")["response"]["response"].clone()
        };
        assert_eq!(reply["action"], "accept");
        assert_eq!(reply["content"], json!({"count":2,"enabled":true}));
    }
}

#[test]
fn resolved_native_question_cannot_be_answered_as_an_approval() {
    let mut r = Replay::new("codex", vec![
        json!({"id":"q","method":"item/tool/requestUserInput","params":{"threadId":"root","turnId":"turn","itemId":"tool","questions":[{"id":"q","question":"Why?","header":"Reason","options":null,"isOther":true,"isSecret":false}]}}),
        json!({"method":"serverRequest/resolved","params":{"threadId":"root","requestId":"q"}}),
    ]);
    let ds = decisions(&r.drain());
    assert_eq!(ds.len(), 1);
    assert!(r.session.respond_to_decision(&ds[0].id, DecisionAnswer::Deny { message: "cancel".into() }).is_err(), "resolved request must not fall through to another response schema");
}

#[test]
fn permission_profile_approval_grants_only_the_requested_profile() {
    let mut r = Replay::new("codex", vec![json!({"id":92,"method":"item/permissions/requestApproval","params":{"threadId":"root","turnId":"turn","itemId":"permissions","environmentId":null,"startedAtMs":1,"cwd":"/workspace","reason":"Fetch dependencies","permissions":{"network":{"enabled":true},"fileSystem":null}}})]);
    let ds = decisions(&r.drain());
    assert_eq!(ds.len(), 1);
    r.session.respond_to_decision(&ds[0].id, DecisionAnswer::Allow { input: ds[0].input.clone() }).unwrap();
    let reply = r.wait_host(|v| v["id"] == 92 && v.get("result").is_some());
    assert_eq!(reply["result"], json!({"permissions":{"network":{"enabled":true}},"scope":"turn"}));
}

#[test]
fn native_approval_cannot_offer_or_send_an_unavailable_allow_choice() {
    let mut r = Replay::new("codex", vec![json!({"id":15,"method":"item/commandExecution/requestApproval","params":{"threadId":"root","turnId":"turn","itemId":"cmd","availableDecisions":["decline"]}})]);
    let ds = decisions(&r.drain());
    assert!(!ds[0].policy.allow);
    assert!(ds[0].policy.deny);
    assert!(r.session.respond_to_decision(&ds[0].id, DecisionAnswer::Allow { input: json!({}) }).is_err());
    r.session.respond_to_decision(&ds[0].id, DecisionAnswer::Deny { message:"No".into() }).unwrap();
    assert_eq!(r.wait_host(|v| v["id"] == 15 && v.get("result").is_some())["result"]["decision"], "decline");
}

#[test]
fn unknown_handles_are_never_encoded_as_tool_approvals() {
    for provider in ["claude", "codex"] {
        let mut r = Replay::new(provider, vec![]);
        r.drain();
        assert!(r.session.respond_to_decision("\"unknown\"", DecisionAnswer::Allow { input: json!({}) }).is_err());
    }
}

#[test]
fn claude_question_can_be_declined_without_leaving_it_pending() {
    let mut r = Replay::new("claude", vec![json!({"type":"control_request","request_id":"q","request":{"subtype":"can_use_tool","tool_name":"AskUserQuestion","tool_use_id":"tool","input":{"questions":[{"question":"Choose","options":[{"label":"A"},{"label":"B"}]}]}}})]);
    let ds = decisions(&r.drain());
    r.session.respond_to_decision(&ds[0].id, DecisionAnswer::Deny { message:"Skip this question".into() }).unwrap();
    let reply = r.wait_host(|v| v["type"] == "control_response");
    assert_eq!(reply["response"]["response"]["behavior"], "deny");
    assert_eq!(reply["response"]["response"]["message"], "Skip this question");
}

#[test]
fn titled_mcp_enum_choices_preserve_distinct_values_and_labels() {
    let schema = json!({"type":"object","properties":{"region":{"type":"string","oneOf":[{"const":"au","title":"Australia"},{"const":"us","title":"United States"}]}},"required":["region"]});
    for mode in ["form", "openai/form", "openaiForm"] {
        let mut r = Replay::new("codex", vec![json!({"id":71,"method":"mcpServer/elicitation/request","params":{"threadId":"root","turnId":"turn","serverName":"server","mode":mode,"message":"Choose region","requestedSchema":schema,"_meta":null}})]);
        let ds = decisions(&r.drain());
        assert_eq!(ds.len(), 1);
        let DecisionKind::Form { fields } = &ds[0].kind else { panic!("typed form expected") };
        let ferrite_core::FormFieldKind::Enum { options, .. } = &fields[0].kind else { panic!("native enum cannot become free text") };
        assert_eq!(options[0].value, "au");
        assert_eq!(options[0].label, "Australia");
        r.session.respond_to_decision(&ds[0].id, DecisionAnswer::Form { values: json!({"region":"au"}) }).unwrap();
        assert_eq!(r.wait_host(|v| v["id"] == 71 && v.get("result").is_some())["result"]["content"]["region"], "au");
    }
}

#[test]
fn unreadable_elicitation_remains_visible_and_cancellable() {
    let mut r = Replay::new("claude", vec![json!({"type":"control_request","request_id":"unsupported","request":{"subtype":"elicitation","mcp_server_name":"server","message":"Configure","mode":"form","requested_schema":{"type":"object","properties":{"future":{"type":"future-widget"}}}}})]);
    let ds = decisions(&r.drain());
    assert_eq!(ds.len(), 1, "an unsupported form must never strand a native request invisibly");
    assert!(!ds[0].policy.allow);
    r.session.respond_to_decision(&ds[0].id, DecisionAnswer::Cancel).unwrap();
    assert_eq!(r.wait_host(|v| v["type"] == "control_response")["response"]["response"]["action"], "cancel");
}
