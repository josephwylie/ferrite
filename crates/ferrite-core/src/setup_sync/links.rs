//! Links in the operator's setup (ADR 0050). A link to a folder in the
//! table is an entry of its own, its target the path from its folder to
//! that place in the table's paths, so a skill linked into another folder
//! of the setup is linked there on every side, however each side spells
//! its home. It is made as a tree makes a link (`tree::make_link`): a
//! junction on Windows, which needs no privilege. A link to a file, or out
//! of the table, is followed, as it was.

use std::fs;
use std::path::{Component, Path, PathBuf};

use super::store::in_table;
use super::Item;

/// Where the items a link may point at are on this computer, each resolved.
pub(super) struct Places(pub(super) Vec<(PathBuf, Item)>);

impl Places {
    /// What the link at `link`, the setup path `path`, is listed with: the
    /// path from its folder to the folder in the table that it points at,
    /// `None` when it points at a file or out of the table.
    pub(super) fn target(&self, link: &Path, path: &str) -> Option<String> {
        let literal = fs::read_link(link).ok()?;
        let literal = dunce::simplified(&literal);
        let pointed = match literal.is_absolute() {
            true => literal.to_path_buf(),
            false => resolved(link.parent()?).join(literal),
        };
        let pointed = dunce::canonicalize(&pointed).ok().filter(|pointed| pointed.is_dir())?;
        let (place, item) = self
            .0
            .iter()
            .filter(|(place, _)| pointed.starts_with(place))
            .max_by_key(|(place, _)| place.components().count())?;
        let rest = slashed(pointed.strip_prefix(place).ok()?)?;
        let to = if rest.is_empty() { item.path.to_string() } else { format!("{}/{rest}", item.path) };
        if to == path || to.starts_with(&format!("{path}/")) || !in_table(&to) {
            return None;
        }
        Some(between(path, &to))
    }
}

/// `path` resolved as far as it is there, the rest of it after that.
pub(super) fn resolved(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut at = path;
    loop {
        if let Ok(found) = dunce::canonicalize(at) {
            return rest.into_iter().rev().fold(found, |found, name| found.join(name));
        }
        match (at.parent(), at.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                at = parent;
            }
            _ => return lexical(path),
        }
    }
}

/// `path` with its folder [`resolved`] and its own name kept, so a link
/// there is itself, not the folder it leads to.
pub(super) fn unfollowed(path: &Path) -> PathBuf {
    match (path.parent(), path.file_name()) {
        (Some(folder), Some(name)) => resolved(folder).join(name),
        _ => resolved(path),
    }
}

/// `path` with each `.` left out and each `..` taking the name before it.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            part => out.push(part),
        }
    }
    out
}

/// `path`'s names, `/` between them; `None` unless each is a plain name.
pub(super) fn slashed(path: &Path) -> Option<String> {
    let names: Option<Vec<&str>> = path
        .components()
        .map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    Some(names?.join("/"))
}

/// The path from the folder of the setup path `path` to the setup path
/// `to`, as a link there spells it.
fn between(path: &str, to: &str) -> String {
    let from: Vec<&str> = path.split('/').collect();
    let from = &from[..from.len() - 1];
    let to: Vec<&str> = to.split('/').collect();
    let shared = from.iter().zip(&to).take_while(|(one, other)| one == other).count();
    let up = std::iter::repeat_n("..", from.len() - shared);
    up.chain(to[shared..].iter().copied()).collect::<Vec<_>>().join("/")
}

/// The setup path a link at `path` to `target` points at: `None` unless it
/// stays in the home.
pub(super) fn joined(path: &str, target: &str) -> Option<String> {
    let mut names: Vec<&str> = path.split('/').collect();
    names.pop();
    for name in target.split('/') {
        match name {
            "" | "." => {}
            ".." => {
                names.pop()?;
            }
            name => names.push(name),
        }
    }
    Some(names.join("/")).filter(|to| !to.is_empty())
}

/// The folder `from` and `to`, both of this computer, lie in, and the path
/// from `from` to `to` in it, `/` between its names: `None` when they share
/// no root, as two Windows drives do not.
pub(super) fn relative(from: &Path, to: &Path) -> Option<(PathBuf, String)> {
    let (from, to) = (lexical(from), lexical(to));
    let shared = from.components().zip(to.components()).take_while(|(one, other)| one == other).count();
    if shared == 0 {
        return None;
    }
    let down = slashed(&to.components().skip(shared).collect::<PathBuf>())?;
    let up = std::iter::repeat_n("..", from.components().count() - shared);
    let path = up.chain(Some(down.as_str()).filter(|down| !down.is_empty())).collect::<Vec<_>>().join("/");
    Some((from.components().take(shared).collect(), path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_is_spelled_from_its_folder_and_read_back_to_where_it_points() {
        assert_eq!(between(".claude/skills/shared", ".agents/skills/shared"), "../../.agents/skills/shared");
        assert_eq!(between(".agents/skills/a", ".agents/skills/b"), "b");
        assert_eq!(joined(".claude/skills/shared", "../../.agents/skills/shared").as_deref(), Some(".agents/skills/shared"));
        assert_eq!(joined(".claude/skills/shared", "../../../etc/passwd"), None);
        let home = Path::new("/home/me");
        let at = relative(&home.join(".claude/skills"), &home.join(".agents/skills/shared"));
        assert_eq!(at, Some((home.to_path_buf(), "../../.agents/skills/shared".to_string())));
    }
}
