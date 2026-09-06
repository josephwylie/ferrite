#![cfg(unix)]
#[allow(dead_code)]
#[path="support/provider_contract.rs"]mod support;
use support::*;
use serde_json::json;

#[test]
fn native_patch_update_replaces_hunks_without_finishing_the_tool() {
    let r=Replay::new("codex",vec![
        json!({"method":"item/started","params":{"threadId":"root","turnId":"turn","item":{"id":"edit","type":"fileChange","changes":[],"status":"inProgress"}}}),
        json!({"method":"item/fileChange/patchUpdated","params":{"threadId":"root","turnId":"turn","itemId":"edit","changes":[{"path":"a.txt","kind":{"type":"update","move_path":null},"diff":"@@ -1 +1 @@\n-old\n+new\n"}]}}),
        json!({"method":"item/fileChange/patchUpdated","params":{"threadId":"root","turnId":"turn","itemId":"edit","changes":[{"path":"b.txt","kind":{"type":"add"},"diff":"latest\n"}]}}),
    ]);
    let a=fold(r.drain());let t=a.view().main().transcript().blocks().iter().find_map(|b|match &b.body{ferrite_core::transcript::Body::Tool(t)=>Some(t),_=>None}).unwrap();
    assert_eq!(t.diffs.len(),1);assert_eq!(t.diffs[0].path,"b.txt");assert_eq!(t.state,ferrite_core::transcript::ToolState::Running);
}

#[test]
fn native_turn_diff_is_a_replacement_snapshot_not_an_invented_tool() {
    let r=Replay::new("codex",vec![
        json!({"method":"turn/diff/updated","params":{"threadId":"root","turnId":"turn","diff":"old diff"}}),
        json!({"method":"turn/diff/updated","params":{"threadId":"root","turnId":"turn","diff":"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n"}}),
    ]);
    let a=fold(r.drain());let t=a.view().main().transcript();
    let d=t.turn_diff().expect("shared UI must have native aggregate diff");
    assert_eq!(d.turn_id,"turn");assert_eq!(d.diff,"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n");
    assert!(!t.blocks().iter().any(|b|matches!(&b.body,ferrite_core::transcript::Body::Tool(_))));
}
