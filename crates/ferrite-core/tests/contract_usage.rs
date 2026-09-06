#![cfg(unix)]
#[allow(dead_code)]
#[path="support/provider_contract.rs"]mod support;
use ferrite_core::{SessionEvent,UsageScope};
use ferrite_core::transcript::{Transcript,Input};
use support::*;
use serde_json::json;

#[test]
fn context_only_refresh_cannot_reset_output_accounting() {
    let mut t=Transcript::default();
    let usage=|output|SessionEvent::TokenUsage{total_tokens:100,input_tokens:80,cached_input_tokens:0,output_tokens:output,reasoning_output_tokens:0,context_window:Some(1000)};
    t.apply(Input::Prompt("Hello".into()));t.apply(Input::Event(usage(10)));
    t.apply(Input::Event(SessionEvent::ContextUsage{total_tokens:200,context_window:Some(1000)}));
    t.apply(Input::Event(usage(15)));
    assert_eq!(t.turn_output_tokens(),15,"context occupancy has no output usage and must not imply a reset to zero");
    assert_eq!(t.usage().unwrap().total_tokens,100);
}

#[test]
fn native_accounting_details_keep_scope_and_all_counters() {
    for (provider,frame,scope) in [
        ("claude",json!({"type":"result","subtype":"success","session_id":"root","uuid":"usage","total_cost_usd":0.04,"usage":{"input_tokens":100,"cache_read_input_tokens":20,"cache_creation_input_tokens":0,"output_tokens":30}}),UsageScope::Turn),
        ("codex",json!({"method":"thread/tokenUsage/updated","params":{"threadId":"root","turnId":"turn","tokenUsage":{"last":{"totalTokens":80},"total":{"totalTokens":150,"inputTokens":100,"cachedInputTokens":20,"outputTokens":30,"reasoningOutputTokens":10},"modelContextWindow":180000}}}),UsageScope::Session),
    ] {
        let r=Replay::new(provider,vec![frame]);let a=fold(r.drain());let details=a.view().main().transcript().usage_details().expect("shared accounting details");
        assert_eq!(details.scope,scope);assert_eq!(details.input_tokens,100);assert_eq!(details.cached_input_tokens,20);assert_eq!(details.output_tokens,30);
        if provider=="codex"{assert_eq!(details.reasoning_output_tokens,10);}else{assert_eq!(a.view().main().transcript().last_cost(),Some(0.04));}
    }
}
