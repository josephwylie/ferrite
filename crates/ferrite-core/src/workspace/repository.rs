//! Which repository a checkout is, the same on every machine: T3 Code's
//! repository identity (`project/RepositoryIdentityResolver.ts` and
//! `normalizeGitRemoteUrl` in `packages/shared/src/git.ts` at 0fd48228).
//! Its key is the primary remote's `host/owner/name`, lower case, without a
//! port or `.git`, so `git@github.com:Acme/App.git` and
//! `https://github.com/acme/app` are one repository. A Delegate finds the
//! Project with its parent's repository on another machine by it (ADR
//! 0047), where `repo_key`, a path, means nothing.
//!
//! Read from the repository's git config without running git, as
//! `checkout_of` reads `.git`: `project.list` asks it of every Project.
//! T3's ssh alias lookup (`ssh -G`) and `insteadOf` rewrites are not
//! followed: a remote spelled through one keys as written.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::registry::Registry;
use crate::project::ProjectId;
use crate::protocol::Repositories;

/// The repository the checkout at `root` is, by its primary remote: `None`
/// outside git, or with no remote that names a host.
pub fn repository_key(root: &Path) -> Option<String> {
    let config = fs::read_to_string(common_dir(root)?.join("config")).ok()?;
    let remotes = remotes(&config);
    primary(&remotes).and_then(normalize)
}

/// Each Project's id and root, to read their repositories off the thread
/// that holds the registry.
pub fn project_roots(registry: &Registry) -> Vec<(ProjectId, PathBuf)> {
    registry
        .projects()
        .iter()
        .map(|project| (project.id, project.root.clone()))
        .collect()
}

pub fn repositories(projects: &[(ProjectId, PathBuf)]) -> Repositories {
    projects
        .iter()
        .filter_map(|(id, root)| Some((*id, repository_key(root)?)))
        .collect()
}

/// The git directory every worktree of `root`'s repository shares, read
/// as `checkout_of` reads it.
fn common_dir(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let pointer = fs::read_to_string(&dot_git).ok()?;
    let admin = root.join(pointer.trim().strip_prefix("gitdir:")?.trim());
    match fs::read_to_string(admin.join("commondir")) {
        Ok(common) => Some(admin.join(common.trim())),
        Err(_) => Some(admin),
    }
}

/// A remote's fetch URL and the mark `gh repo set-default` left on it.
#[derive(Debug, Default, PartialEq, Eq)]
struct Remote {
    url: Option<String>,
    resolved: Option<String>,
}

/// Every `[remote "name"]` section's `url` and `gh-resolved`, by name.
fn remotes(config: &str) -> BTreeMap<String, Remote> {
    let mut remotes: BTreeMap<String, Remote> = BTreeMap::new();
    let mut section: Option<String> = None;
    for line in config.lines().map(str::trim) {
        if let Some(header) = line.strip_prefix('[') {
            let header = header.split(']').next().unwrap_or_default();
            section = header
                .split_once(char::is_whitespace)
                .filter(|(kind, _)| kind.eq_ignore_ascii_case("remote"))
                .map(|(_, name)| name.trim().trim_matches('"').to_string());
            continue;
        }
        let Some(name) = &section else { continue };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = config_value(value);
        let remote = remotes.entry(name.clone()).or_default();
        match key.trim().to_ascii_lowercase().as_str() {
            // The first `url` is the one git fetches from.
            "url" if remote.url.is_none() => remote.url = Some(value),
            "gh-resolved" => remote.resolved = Some(value),
            _ => {}
        }
    }
    remotes
}

/// A config value as git reads it: quotes dropped, a comment cut off.
fn config_value(raw: &str) -> String {
    let mut value = String::new();
    let mut quoted = false;
    let mut chars = raw.chars();
    while let Some(char) = chars.next() {
        match char {
            '\\' => value.extend(chars.next()),
            '"' => quoted = !quoted,
            '#' | ';' if !quoted => break,
            char => value.push(char),
        }
    }
    value.trim().to_string()
}

/// T3's `pickPrimaryRemote`: the one remote `gh` marked as the repository
/// it reads PRs from, else `upstream`, else `origin`, else the first by
/// name. A remote at a local path is never one.
fn primary(remotes: &BTreeMap<String, Remote>) -> Option<&str> {
    let urls: BTreeMap<&str, &str> = remotes
        .iter()
        .filter_map(|(name, remote)| Some((name.as_str(), remote.url.as_deref()?)))
        .filter(|(_, url)| !is_local(url))
        .collect();
    let marks: Vec<(&str, &str)> = remotes
        .iter()
        .filter(|(name, _)| urls.contains_key(name.as_str()))
        .filter_map(|(name, remote)| Some((name.as_str(), remote.resolved.as_deref()?)))
        .collect();
    if let [(name, mark)] = marks.as_slice() {
        if *mark == "base" {
            return urls.get(name).copied();
        }
        // A mark naming a parent repository: the remote that fetches it.
        let host = normalize(urls[name]).and_then(|key| Some(key.split_once('/')?.0.to_string()));
        let wanted = host.map(|host| format!("{host}/{}", mark.to_lowercase()));
        if let Some(url) = urls.values().find(|url| normalize(url) == wanted) {
            return Some(url);
        }
    }
    ["upstream", "origin"]
        .into_iter()
        .find_map(|name| urls.get(name).copied())
        .or_else(|| urls.values().next().copied())
}

/// A remote that names no host: a path, or `file:`.
fn is_local(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("file:") {
        return true;
    }
    if lower.contains("://") {
        return false;
    }
    let bytes = lower.as_bytes();
    if bytes.len() > 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/') {
        return true;
    }
    // git reads `host:path` as scp-style only when the colon comes before
    // any slash.
    match (lower.find(':'), lower.find(['/', '\\'])) {
        (None, _) => true,
        (Some(colon), Some(slash)) => slash < colon,
        (Some(_), None) => false,
    }
}

/// T3's `normalizeGitRemoteUrl`: `host/path`, lower case, without a user,
/// a port or `.git`. `None` for a remote that names no repository on a
/// host.
fn normalize(url: &str) -> Option<String> {
    let lower = url.trim().trim_end_matches('/').to_lowercase();
    let trimmed = lower.strip_suffix(".git").unwrap_or(&lower);
    let (host, path) = match trimmed.split_once("://") {
        Some((scheme, rest)) if matches!(scheme, "ssh" | "https" | "http" | "git") => {
            let (authority, path) = rest.split_once('/')?;
            let host = authority.rsplit('@').next()?;
            let host = match host.strip_prefix('[') {
                Some(v6) => v6.split(']').next()?,
                None => host.split(':').next()?,
            };
            (host.to_string(), path.to_string())
        }
        Some(_) => return None,
        None => {
            if is_local(trimmed) {
                return None;
            }
            let (authority, path) = trimmed.split_once(':')?;
            let host = authority.rsplit('@').next()?;
            (host.to_string(), path.trim_start_matches('/').to_string())
        }
    };
    let segments: Vec<&str> = path.split('/').filter(|segment| !segment.is_empty()).collect();
    (!host.is_empty() && segments.len() > 1).then(|| format!("{host}/{}", segments.join("/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T3's spellings of one repository key the same.
    #[test]
    fn one_repository_keys_the_same_however_its_remote_is_spelled() {
        for url in [
            "git@github.com:Acme/App.git",
            "https://github.com/acme/app",
            "https://github.com/acme/app.git/",
            "ssh://git@github.com:22/acme/app.git",
            "https://user@github.com/Acme/App",
        ] {
            assert_eq!(normalize(url).as_deref(), Some("github.com/acme/app"), "{url}");
        }
        assert_eq!(normalize("/srv/git/app.git"), None);
        assert_eq!(normalize("C:\\repos\\app"), None);
        assert_eq!(normalize("file:///srv/app"), None);
        assert_eq!(normalize("https://github.com/app"), None, "no owner");
    }

    /// `gh`'s mark first, then `upstream`, `origin`, then the first by
    /// name; a remote at a local path is never chosen.
    #[test]
    fn the_primary_remote_is_t3s() {
        let config = |text: &str| remotes(text);
        let two = r#"
[core]
	bare = false
[remote "origin"]
	url = git@github.com:me/app.git
	fetch = +refs/heads/*:refs/remotes/origin/*
[remote "upstream"]
	url = "https://github.com/acme/app" # the project
"#;
        assert_eq!(primary(&config(two)), Some("https://github.com/acme/app"));
        let marked = format!("{two}[remote \"origin\"]\n\tgh-resolved = base\n");
        assert_eq!(primary(&config(&marked)), Some("git@github.com:me/app.git"));
        let parent = format!("{two}[remote \"origin\"]\n\tgh-resolved = Acme/App\n");
        assert_eq!(primary(&config(&parent)), Some("https://github.com/acme/app"));
        let local = "[remote \"origin\"]\n\turl = ../app.git\n[remote \"zed\"]\n\turl = git@gitlab.com:z/app\n";
        assert_eq!(primary(&config(local)), Some("git@gitlab.com:z/app"));
        assert_eq!(primary(&config("[core]\n\tbare = false\n")), None);
    }

    /// A linked worktree reads its main checkout's remotes.
    #[test]
    fn a_worktree_is_its_repositorys() {
        let dir = std::env::temp_dir().join(format!("ferrite-repo-key-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let main = dir.join("app");
        fs::create_dir_all(main.join(".git/worktrees/wt")).unwrap();
        fs::write(
            main.join(".git/config"),
            "[remote \"origin\"]\n\turl = git@github.com:Acme/App.git\n",
        )
        .unwrap();
        fs::write(main.join(".git/worktrees/wt/commondir"), "../..\n").unwrap();
        let worktree = dir.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", main.join(".git/worktrees/wt").display()),
        )
        .unwrap();
        assert_eq!(repository_key(&main).as_deref(), Some("github.com/acme/app"));
        assert_eq!(repository_key(&worktree).as_deref(), Some("github.com/acme/app"));
        assert_eq!(repository_key(&dir), None, "outside git");
        let _ = fs::remove_dir_all(&dir);
    }
}
