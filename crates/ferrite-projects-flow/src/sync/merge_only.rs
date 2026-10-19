//! Only the merge of main? Has the branch, since the commit a review looked at, done nothing but take main in? Then
//! that review still describes the task's own work, and only the tests need to run again (finish/). The rule:
//!   1. Every commit on the branch since the reviewed one is a merge commit with two parents: the branch as it was,
//!      and a commit of main (`target`, or one it holds). No ordinary commit, no rebase, no other branch.
//!   2. Each of those merges is what git itself makes of its two parents, file for file. Only the files git could not
//!      merge by itself (the conflicts) may differ: resolving them is what the merge needed.
//! Anything else, and anything that cannot be read, is "no": then the work is reviewed. The answer says why.
use crate::flow::git::head_of;
use crate::json::*;
use crate::store::repos::{git_full, git_out};
use crate::store::tasks::one_line;
use std::path::Path;

pub fn merge_only(tree: &str, reviewed: &str, target: &str) -> Value {
    let no = |why: String| json!({ "ok": false, "why": why });
    let short = |c: &str| c.chars().take(7).collect::<String>();
    if tree.is_empty() || reviewed.is_empty() || target.is_empty() { return no("the reviewed commit or main is not known".into()); }
    let dir = Path::new(tree);
    let run = || -> crate::err::Res<Value> {
        let head = head_of(dir);
        if head.is_empty() || head == reviewed { return Ok(no("nothing was merged since the review".into())); }
        if git_out(dir, &["merge-base", "--is-ancestor", reviewed, &head]).is_err() { return Ok(no("the branch no longer holds the reviewed commit".into())); }
        let chain: Vec<String> = git_out(dir, &["rev-list", "--first-parent", "--reverse", &format!("{reviewed}..{head}")])?.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
        let mut resolved: Vec<String> = vec![];
        let mut prev = reviewed.to_string();
        for c in &chain {
            let parents: Vec<String> = git_out(dir, &["show", "-s", "--format=%P", c])?.split_whitespace().map(String::from).collect();
            if parents.len() != 2 || parents[0] != prev { return Ok(no(format!("{} is new work, not a merge of main", short(c)))); }
            if git_out(dir, &["merge-base", "--is-ancestor", &parents[1], target]).is_err() { return Ok(no(format!("{} merges something that is not main", short(c)))); }
            // The same merge, made again by git in memory: the tree it gives, and the files it could not merge.
            let (status, out, _) = git_full(dir, &["merge-tree", "--write-tree", "--name-only", "--no-messages", "-z", &parents[0], &parents[1]], 60000);
            if status != 0 && status != 1 { return Ok(no(format!("the merge {} could not be made again to compare", short(c)))); }
            let parts: Vec<String> = out.split('\0').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
            let auto = parts.first().cloned().unwrap_or_default();
            if auto.len() < 40 || !auto.bytes().all(|b| b.is_ascii_hexdigit()) { return Ok(no(format!("the merge {} could not be made again to compare", short(c)))); }
            let conflicts = &parts[1..];
            let differs: Vec<String> = git_out(dir, &["diff", "--name-only", "--no-renames", "-z", &auto, &format!("{c}^{{tree}}")])?.split('\0').filter(|x| !x.is_empty()).map(String::from).collect();
            let extra: Vec<&String> = differs.iter().filter(|f| !conflicts.contains(f)).collect();
            if !extra.is_empty() { return Ok(no(format!("the merge {} changes more than its conflicts needed ({}{})", short(c), extra.iter().take(3).map(|s| s.as_str()).collect::<Vec<_>>().join(", "), if extra.len() > 3 { ", …" } else { "" }))); }
            for f in differs { if !resolved.contains(&f) { resolved.push(f); } }
            prev = c.clone();
        }
        if chain.is_empty() || prev != head { return Ok(no("the reviewed commit is not on the branch's own line".into())); }
        Ok(json!({ "ok": true, "from": reviewed, "head": head, "merges": chain.len(), "resolved": resolved }))
    };
    run().unwrap_or_else(|e| no(format!("git could not say ({})", one_line(&e.msg, 160))))
}
