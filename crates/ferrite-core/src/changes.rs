//! A workspace's diff and index operations, executed by its owning Server.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::spawn::NoConsoleWindow;

use ferrite_reviewr::ferrite::{FileSides, Read};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", content = "base", rename_all = "snake_case")]
pub enum Scope {
    All,
    Uncommitted,
    Against(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Request {
    Read {
        root: PathBuf,
        scope: Scope,
    },
    SetStaged {
        root: PathBuf,
        path: String,
        staged: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Reply {
    Read {
        files: Vec<crate::protocol::WireReviewFile>,
        branches: Vec<String>,
        staged: Vec<String>,
    },
    Staged,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub files: Vec<FileSides>,
    pub branches: Vec<String>,
    pub staged: HashSet<String>,
}

pub fn execute(request: Request) -> Result<Reply, String> {
    match request {
        Request::Read { root, scope } => {
            let root = root_path(&root)?;
            let (read, base) = match scope {
                Scope::All => (Read::Branch, None),
                Scope::Uncommitted => (Read::Uncommitted, None),
                Scope::Against(base) => {
                    if base.is_empty() || base.starts_with('-') || base.contains('\0') {
                        return Err("Invalid comparison branch".into());
                    }
                    let commit = git(
                        &root,
                        &[
                            "rev-parse",
                            "--verify",
                            "--end-of-options",
                            &format!("{base}^{{commit}}"),
                        ],
                    )?;
                    (
                        Read::Branch,
                        Some(String::from_utf8_lossy(&commit).trim().to_string()),
                    )
                }
            };
            let files = ferrite_reviewr::ferrite::review_against(&root, &read, base.as_deref())?;
            let branches = ferrite_reviewr::git::list_branches(&root)
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(|branch| branch.name)
                .collect();
            let staged = git(&root, &["diff", "--cached", "--name-only", "-z", "--"])?
                .split(|byte| *byte == 0)
                .filter(|path| !path.is_empty())
                .map(|path| String::from_utf8_lossy(path).into_owned())
                .collect();
            Ok(Reply::Read {
                files: files.into_iter().map(Into::into).collect(),
                branches,
                staged,
            })
        }
        Request::SetStaged { root, path, staged } => {
            let root = root_path(&root)?;
            validate_path(&root, &path)?;
            if staged {
                git(&root, &["--literal-pathspecs", "add", "--", &path])?;
            } else {
                git(
                    &root,
                    &["--literal-pathspecs", "restore", "--staged", "--", &path],
                )?;
            }
            Ok(Reply::Staged)
        }
    }
}

fn root_path(root: &Path) -> Result<PathBuf, String> {
    if !root.is_absolute() {
        return Err("A Changes workspace needs an absolute path".into());
    }
    let root = dunce::canonicalize(root).map_err(|error| error.to_string())?;
    let top = git(&root, &["rev-parse", "--show-toplevel"])?;
    let top = dunce::canonicalize(String::from_utf8_lossy(&top).trim())
        .map_err(|error| error.to_string())?;
    if top != root {
        return Err("Changes must name the checkout root".into());
    }
    Ok(root)
}

fn validate_path(root: &Path, path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.contains(['\0', '\\'])
        || Path::new(path)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("Changes needs a file path within the checkout".into());
    }
    if Path::new(path)
        .components()
        .any(|part| part.as_os_str() == ".git")
    {
        return Err("Git metadata cannot be staged".into());
    }
    let joined = root.join(path);
    let mut parent = joined.parent();
    while let Some(directory) = parent {
        match dunce::canonicalize(directory) {
            Ok(resolved) if resolved.starts_with(root) => return Ok(()),
            Ok(_) => return Err("The file is outside the checkout".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                parent = directory.parent()
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("The file is outside the checkout".into())
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .no_console_window()
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

impl Reply {
    pub(crate) fn snapshot(self) -> Option<Snapshot> {
        match self {
            Self::Read {
                files,
                branches,
                staged,
            } => Some(Snapshot {
                files: files.into_iter().map(Into::into).collect(),
                branches,
                staged: staged.into_iter().collect(),
            }),
            Self::Staged => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("ferrite-changes-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "-q", "-b", "main"]).unwrap();
        git(&root, &["config", "user.name", "Test"]).unwrap();
        git(&root, &["config", "user.email", "test@example.invalid"]).unwrap();
        std::fs::write(root.join("a.txt"), "first\n").unwrap();
        git(&root, &["add", "."]).unwrap();
        git(&root, &["commit", "-qm", "first"]).unwrap();
        dunce::canonicalize(root).unwrap()
    }

    #[test]
    fn comparison_bases_are_per_call_and_do_not_change_repository_configuration() {
        let root = repo("bases");
        git(&root, &["checkout", "-qb", "feature"]).unwrap();
        std::fs::write(root.join("a.txt"), "second\n").unwrap();
        git(&root, &["commit", "-qam", "second"]).unwrap();
        std::fs::write(root.join("a.txt"), "third\n").unwrap();
        let config = std::fs::read(root.join(".git/config")).unwrap();
        let main = execute(Request::Read {
            root: root.clone(),
            scope: Scope::Against("main".into()),
        })
        .unwrap()
        .snapshot()
        .unwrap();
        let head = execute(Request::Read {
            root: root.clone(),
            scope: Scope::Against("feature".into()),
        })
        .unwrap()
        .snapshot()
        .unwrap();
        assert_eq!(main.files[0].sides.as_ref().unwrap().0, "first\n");
        assert_eq!(head.files[0].sides.as_ref().unwrap().0, "second\n");
        assert_eq!(std::fs::read(root.join(".git/config")).unwrap(), config);
    }

    #[test]
    fn stage_paths_are_literal_and_cannot_escape_the_checkout() {
        let root = repo("paths");
        for name in ["[literal].txt", "-option", "other.txt"] {
            std::fs::write(root.join(name), "new").unwrap();
        }
        for path in ["../outside", "/outside", ":(glob)*", ".git/config"] {
            let outcome = execute(Request::SetStaged {
                root: root.clone(),
                path: path.into(),
                staged: true,
            });
            assert!(outcome.is_err(), "{path}");
        }
        execute(Request::SetStaged {
            root: root.clone(),
            path: "[literal].txt".into(),
            staged: true,
        })
        .unwrap();
        assert_eq!(staged(root), HashSet::from(["[literal].txt".into()]));
    }

    #[cfg(unix)]
    #[test]
    fn a_path_with_a_newline_stages_literally() {
        let root = repo("newline");
        std::fs::write(root.join("line\nbreak.txt"), "new").unwrap();
        execute(Request::SetStaged {
            root: root.clone(),
            path: "line\nbreak.txt".into(),
            staged: true,
        })
        .unwrap();
        assert_eq!(staged(root), HashSet::from(["line\nbreak.txt".into()]));
    }

    fn staged(root: PathBuf) -> HashSet<String> {
        execute(Request::Read {
            root,
            scope: Scope::Uncommitted,
        })
        .unwrap()
        .snapshot()
        .unwrap()
        .staged
    }
}
