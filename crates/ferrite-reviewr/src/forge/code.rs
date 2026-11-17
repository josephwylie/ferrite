use std::path::Path;
use std::sync::atomic::AtomicBool;

use serde_json::Value;

use super::{GhError, PrView, READ_DEADLINE, gh, github_repository, graphql, within};
use crate::diff::{Notice, over_byte_budget};
use crate::ferrite::{FileSides, MAX_REVIEW};
use crate::git::RepoTarget;
use crate::model::{ChangeKind, ChangedFile};

/// Published files at the PR's merge base and head, with complete context on both sides.
pub fn diff_numbered(
    repo: &Path,
    config: &crate::config::PluginConfig,
    number: u64,
) -> Result<Vec<FileSides>, String> {
    let target =
        github_repository(repo, config).map_err(|view| crate::ferrite::pr_note(&view, "PR"))?;
    within(READ_DEADLINE, Err("The forge did not answer".into()), |cancelled| {
        read(repo, &target, number, cancelled)
            .map_err(|error| crate::ferrite::pr_note(&PrView::from(error), "PR"))
    })
}

fn json(repo: &Path, host: &str, endpoint: &str, cancelled: &AtomicBool) -> Result<Value, GhError> {
    let text = gh(repo, host, &["api", "--hostname", host, endpoint], cancelled)?;
    serde_json::from_str(&text).map_err(|error| GhError::Other(error.to_string()))
}

fn required<'a>(node: &'a Value, name: &str) -> Result<&'a str, GhError> {
    node.as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| GhError::Other(format!("The pull request did not include {name}")))
}

fn read(
    repo: &Path,
    target: &RepoTarget,
    number: u64,
    cancelled: &AtomicBool,
) -> Result<Vec<FileSides>, GhError> {
    let prefix = format!("repos/{}/{}", target.owner(), target.name());
    let endpoint = format!("{prefix}/pulls/{number}");
    let pr = json(repo, target.host(), &endpoint, cancelled)?;
    let base = required(&pr["base"]["sha"], "its base commit")?;
    let head = required(&pr["head"]["sha"], "its head commit")?;
    let compare = json(
        repo,
        target.host(),
        &format!("{prefix}/compare/{base}...{head}?per_page=1"),
        cancelled,
    )?;
    let old = required(&compare["merge_base_commit"]["sha"], "its merge base")?;
    let head_owner = pr["head"]["repo"]["owner"]["login"].as_str().unwrap_or(target.owner());
    let head_name = pr["head"]["repo"]["name"].as_str().unwrap_or(target.name());
    let mut files = Vec::new();
    for page in 1..=30 {
        let response = json(
            repo,
            target.host(),
            &format!("{endpoint}/files?per_page=100&page={page}"),
            cancelled,
        )?;
        let rows = response
            .as_array()
            .ok_or_else(|| GhError::Other("The forge did not return a file list".into()))?;
        files.extend(rows.iter().map(changed_file).collect::<Result<Vec<_>, _>>()?);
        if rows.len() < 100 {
            break;
        }
    }
    if pr["changed_files"].as_u64().is_some_and(|count| count != files.len() as u64) {
        return Err(GhError::Other("The forge did not return the complete PR file list".into()));
    }
    let mut result = Vec::with_capacity(files.len());
    let mut carried = 0usize;
    for chunk in files.chunks(20) {
        if carried >= MAX_REVIEW {
            result.extend(
                chunk.iter().cloned().map(|file| FileSides { file, sides: Err(Notice::TooLarge) }),
            );
            continue;
        }
        let (query, mut vars) = blob_query(chunk, old, head);
        vars.extend([
            ("owner".into(), target.owner().into()),
            ("name".into(), target.name().into()),
            ("headOwner".into(), head_owner.into()),
            ("headName".into(), head_name.into()),
        ]);
        let blobs = graphql(repo, target.host(), &query, &vars, cancelled)?;
        for (index, file) in chunk.iter().enumerate() {
            let old = &blobs["data"]["base"][format!("f{index}")];
            let new = &blobs["data"]["head"][format!("f{index}")];
            let mut file = file.clone();
            let sides = file_sides(&file, old, new).and_then(|(old, new)| {
                carried = carried.saturating_add(old.len() + new.len());
                if carried > MAX_REVIEW { Err(Notice::TooLarge) } else { Ok((old, new)) }
            });
            file.binary = sides == Err(Notice::Binary);
            result.push(FileSides { file, sides });
        }
    }
    let current = json(repo, target.host(), &endpoint, cancelled)?;
    if current["head"]["sha"] != pr["head"]["sha"] || current["base"]["sha"] != pr["base"]["sha"] {
        return Err(GhError::Other(
            "The pull request changed while reading its files. Refresh to read the new revision."
                .into(),
        ));
    }
    Ok(result)
}

fn changed_file(node: &Value) -> Result<ChangedFile, GhError> {
    let path = required(&node["filename"], "a changed file's path")?.to_string();
    let kind = match node["status"].as_str() {
        Some("added") => ChangeKind::Added,
        Some("removed") => ChangeKind::Deleted,
        Some("renamed") => ChangeKind::Renamed,
        Some("copied") => ChangeKind::Copied,
        _ => ChangeKind::Modified,
    };
    Ok(ChangedFile {
        path,
        kind,
        additions: node["additions"].as_u64().unwrap_or_default().min(u32::MAX as u64) as u32,
        deletions: node["deletions"].as_u64().unwrap_or_default().min(u32::MAX as u64) as u32,
        previous_path: node["previous_filename"].as_str().map(str::to_string),
        binary: false,
        old_size: 0,
        new_size: None,
    })
}

fn blob_query(files: &[ChangedFile], old: &str, new: &str) -> (String, Vec<(String, String)>) {
    use std::fmt::Write;
    let mut query =
        "query($owner:String!,$name:String!,$headOwner:String!,$headName:String!".to_string();
    let mut vars = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let _ = write!(query, ",$old{index}:String!,$new{index}:String!");
        vars.push((
            format!("old{index}"),
            format!("{old}:{}", file.previous_path.as_deref().unwrap_or(&file.path)),
        ));
        vars.push((format!("new{index}"), format!("{new}:{}", file.path)));
    }
    query.push_str("){base:repository(owner:$owner,name:$name){");
    for side in ["old", "new"] {
        if side == "new" {
            query.push_str("}head:repository(owner:$headOwner,name:$headName){");
        }
        for index in 0..files.len() {
            let _ = write!(
                query,
                "f{index}:object(expression:${side}{index}){{... on Blob{{text isBinary byteSize}}}}"
            );
        }
    }
    query.push_str("}}");
    (query, vars)
}

fn file_sides(file: &ChangedFile, old: &Value, new: &Value) -> Result<(String, String), Notice> {
    let text = |node: &Value, absent: bool| {
        if absent {
            return Ok(String::new());
        }
        if node["isBinary"].as_bool() == Some(true) {
            return Err(Notice::Binary);
        }
        if node["byteSize"].as_u64().is_some_and(|size| size > crate::diff::MAX_BYTES as u64) {
            return Err(Notice::TooLarge);
        }
        node["text"].as_str().map(str::to_string).ok_or(Notice::Unreadable)
    };
    let old = text(old, matches!(file.kind, ChangeKind::Added | ChangeKind::Untracked))?;
    let new = text(new, file.kind == ChangeKind::Deleted)?;
    if over_byte_budget(old.len() + new.len()) {
        return Err(Notice::TooLarge);
    }
    Ok((old, new))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn published_files_keep_full_context_and_the_original_rename_path() {
        let file = changed_file(&json!({"filename":"new.rs", "previous_filename":"old.rs", "status":"renamed", "additions":1, "deletions":1})).unwrap();
        let (_, vars) = blob_query(std::slice::from_ref(&file), "merge-base", "published-head");
        assert_eq!(
            vars,
            [
                ("old0".into(), "merge-base:old.rs".into()),
                ("new0".into(), "published-head:new.rs".into())
            ]
        );
        let old = "unchanged before\nold\nunchanged after\n";
        let new = "unchanged before\nnew\nunchanged after\n";
        assert_eq!(
            file_sides(&file, &json!({"text":old}), &json!({"text":new})),
            Ok((old.into(), new.into()))
        );
    }

    #[test]
    fn missing_binary_and_large_blobs_never_become_blank_file_context() {
        let file = changed_file(&json!({"filename":"file", "status":"modified"})).unwrap();
        assert_eq!(
            file_sides(&file, &Value::Null, &json!({"text":"new"})),
            Err(Notice::Unreadable)
        );
        assert_eq!(file_sides(&file, &json!({"isBinary":true}), &Value::Null), Err(Notice::Binary));
        assert_eq!(
            file_sides(&file, &json!({"byteSize":3_000_000}), &Value::Null),
            Err(Notice::TooLarge)
        );
        let added = changed_file(&json!({"filename":"new", "status":"added"})).unwrap();
        assert_eq!(
            file_sides(&added, &Value::Null, &json!({"text":"new\n"})),
            Ok((String::new(), "new\n".into()))
        );
    }
}
