//! Where an attempt works: a worktree and a branch of its own (`work/12a-…`), and a tab of its own (`Agent A`) in the
//! task's one herdr space. The first attempt to start makes that space, as herdr makes any worktree's; each later one
//! gets a worktree beside it and a tab in the same space.
use crate::err::Res;
use crate::flow;
use crate::herdr::Herdr;
use crate::json::*;
use crate::store::attempts::attempts_of;
use crate::store::files::{board_env, with_lock_wait};
use crate::store::repos::{cwd_of, git_slow};
use crate::store::tasks::{get, update_wait, Change, Task};
use std::path::{Path, PathBuf};

/// Answers as `herdr worktree create` does: { workspace, tab, root_pane, worktree: { path } }. One attempt at a time
/// per task: two that start together must not both make the space.
pub fn open_place(api: &Herdr, root: &Path, t: &Task, branch: &str, from: &str) -> Res<Value> {
    with_lock_wait(&format!("space:{}", t.s("attemptOf")), 180000, || {
        let p = get(t.s("attemptOf"));
        let space = p.as_ref().filter(|p| p.at("link").b("workspace")).and_then(|p| api.workspace_get(p.at("link").s("workspace")));
        let Some(space) = space else {
            let label: String = p.as_ref().map(|p| p.t("title")).unwrap_or_else(|| t.t("title")).chars().take(60).collect();
            let res = flow::spaces::opening_worktree(api, root, || api.worktree_create(&root.display().to_string(), branch, from, &label))?;
            let dir = if res.at("worktree").b("path") { res.at("worktree").t("path") } else { res.at("workspace").at("worktree").t("checkout_path") };
            let ws = res.at("workspace").t("workspace_id");
            update_wait(t.s("attemptOf"), 10000, |p| { p.obj_mut("link").put("workspace", ws.clone()); p.obj_mut("link").put("ownsWorkspace", true); p.put("spaceDir", Path::new(&dir).parent().map(|x| x.display().to_string()).unwrap_or_default()); Ok(Change::Write) })?;
            return Ok(res);
        };
        // Beside the first one's worktree, named as herdr names one: the branch with its slashes as dashes.
        let p = p.unwrap_or_default();
        let base: PathBuf = if p.b("spaceDir") { PathBuf::from(p.s("spaceDir")) } else {
            attempts_of(&p).iter().find(|c| c.b("worktree")).and_then(|c| Path::new(c.s("worktree")).parent().map(|x| x.to_path_buf())).unwrap_or_else(|| inside_project(root))
        };
        let name: String = branch.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '-' }).collect();
        let dir = base.join(name);
        if !dir.exists() { git_slow(root, &["worktree", "add", "--no-track", "-b", branch, &dir.display().to_string(), from], 120000)?; }
        let ws = space.t("workspace_id");
        let r = api.tab_create(&ws, &dir.display().to_string(), &flow::tabs::agent_tab(Some(t)), &board_env())?;
        let tab = if r.at("tab").b("tab_id") { r.at("tab").t("tab_id") } else { r.at("root_pane").t("tab_id") };
        let pane = if r.at("root_pane").is_object() { Some(r.at("root_pane").clone()) } else { api.panes(Some(&ws))?.into_iter().find(|x| x.s("tab_id") == tab) };
        let Some(pane) = pane else { if !tab.is_empty() { api.tab_close(&tab); } crate::fail!("herdr opened no terminal for attempt {}", t.s("letter")) };
        Ok(json!({ "workspace": { "workspace_id": ws }, "tab": { "tab_id": if tab.is_empty() { pane.t("tab_id") } else { tab } }, "root_pane": pane, "worktree": { "path": dir.display().to_string() } }))
    })
}
/// Ferrite: where an attempt with no worktree beside it goes, as Ferrite's Server places any worktree it makes: inside
/// the project, at `<root>/.worktrees/`, kept out of its status by the repo's `info/exclude` (never its `.gitignore`).
fn inside_project(root: &Path) -> PathBuf {
    crate::delegate::worktree::exclude_dir(root, "/.worktrees");
    root.join(".worktrees")
}
/// A tab for an attempt whose terminal is gone, in the task's space, in its own worktree.
pub fn reopen_place(api: &Herdr, t: &Task) -> Res<Option<Value>> {
    let ws = if t.at("link").b("workspace") { t.at("link").t("workspace") } else { get(t.s("attemptOf")).map(|p| p.at("link").t("workspace")).unwrap_or_default() };
    if ws.is_empty() || api.workspace_get(&ws).is_none() { return Ok(None); }
    let r = api.tab_create(&ws, &cwd_of(t).display().to_string(), &flow::tabs::agent_tab(Some(t)), &board_env())?;
    if r.at("root_pane").is_object() { return Ok(Some(r.at("root_pane").clone())); }
    Ok(api.panes(Some(&ws))?.into_iter().find(|x| x.s("tab_id") == r.at("tab").s("tab_id")))
}

#[cfg(test)]
mod tests {
    use super::inside_project;
    use std::process::Command;

    fn git(dir: &std::path::Path, args: &[&str]) -> String {
        let out = Command::new("git").current_dir(dir).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    }

    /// An attempt's worktree with no other beside it lives inside its project, and the repo's status stays clean.
    #[test]
    fn an_attempt_with_nothing_beside_it_works_inside_the_project_excluded_not_ignored() {
        let root = std::env::temp_dir().join(format!("ferrite-attempt-place-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "-q"]);

        let base = inside_project(&root);
        assert_eq!(base, root.join(".worktrees"));
        std::fs::create_dir_all(base.join("work-12a")).unwrap();
        std::fs::write(base.join("work-12a").join("f"), "x").unwrap();
        inside_project(&root);

        assert_eq!(git(&root, &["status", "--porcelain"]), "");
        assert!(!root.join(".gitignore").exists());
        let exclude = std::fs::read_to_string(root.join(".git/info/exclude")).unwrap();
        assert_eq!(exclude.lines().filter(|l| *l == "/.worktrees/").count(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }
}
