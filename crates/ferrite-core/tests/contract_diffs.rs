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

#[test]
fn native_diff_updates_survive_store_replay() {
    use ferrite_core::{store::{Store,Provider}, workspace::WorkspaceBinding, activity::Activity};
    let r=Replay::new("codex",vec![
        json!({"method":"item/started","params":{"threadId":"root","turnId":"turn","item":{"id":"edit","type":"fileChange","changes":[],"status":"inProgress"}}}),
        json!({"method":"item/fileChange/patchUpdated","params":{"threadId":"root","turnId":"turn","itemId":"edit","changes":[{"path":"a.txt","kind":{"type":"add"},"diff":"persisted\n"}]}}),
        json!({"method":"turn/diff/updated","params":{"threadId":"root","turnId":"turn","diff":"persisted aggregate"}}),
    ]);
    let dir=std::env::temp_dir().join(format!("ferrite-diff-replay-{}",std::process::id()));
    let store=Store::open(&dir).unwrap();let(id,mut writer)=store.create(Provider::Codex,None,WorkspaceBinding::Main{checkout:std::env::temp_dir()}).unwrap();
    for event in r.drain(){writer.record_event(&event,None).unwrap();}writer.flush().unwrap();
    let mut a=Activity::default();for input in store.load(id).unwrap().activity_inputs(){a.apply(input);}
    assert_eq!(a.view().main().transcript().turn_diff().unwrap().diff,"persisted aggregate");
    assert!(a.view().main().transcript().blocks().iter().any(|b| matches!(&b.body,ferrite_core::transcript::Body::Tool(t) if t.diffs.len()==1 && t.diffs[0].path=="a.txt")));
    drop(writer);std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn child_patch_updates_target_the_childs_scoped_tool() {
    let r=Replay::new("codex",vec![
        json!({"method":"thread/started","params":{"thread":{"id":"child","parentThreadId":"root"}}}),
        json!({"method":"item/started","params":{"threadId":"child","turnId":"turn","item":{"id":"edit","type":"fileChange","changes":[],"status":"inProgress"}}}),
        json!({"method":"item/fileChange/patchUpdated","params":{"threadId":"child","turnId":"turn","itemId":"edit","changes":[{"path":"child.txt","kind":{"type":"add"},"diff":"child\n"}]}}),
        json!({"method":"turn/diff/updated","params":{"threadId":"child","turnId":"turn","diff":"child aggregate"}}),
    ]);
    let a=fold(r.drain());let child=a.view().children().into_iter().next().unwrap();
    assert!(child.transcript().blocks().iter().any(|b|matches!(&b.body,ferrite_core::transcript::Body::Tool(t) if t.diffs.len()==1 && t.diffs[0].path=="child.txt")));
    assert_eq!(child.transcript().turn_diff().unwrap().diff,"child aggregate");
    assert!(a.view().main().transcript().turn_diff().is_none());
}
