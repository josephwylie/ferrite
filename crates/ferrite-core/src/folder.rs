//! A folder on a machine as a window browses it (ADR 0026): its entries,
//! and a typed path's `~` spelled out for the computer that reads it.
//! Reading one is the Server's (`workspace::list_dir`).

use std::path::{Path, PathBuf};

/// `~` and `~/…` spelled out against this computer's home directory, so a
/// path typed for a machine is read where it is meant; any other path as
/// it is.
pub fn expand_home(path: &Path) -> PathBuf {
    let Ok(rest) = path.strip_prefix("~") else {
        return path.to_path_buf();
    };
    // Windows spells the home directory USERPROFILE, not HOME.
    match std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        Some(home) => PathBuf::from(home).join(rest),
        None => path.to_path_buf(),
    }
}

/// One entry `list_dir` read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    /// A directory, or a link to one.
    pub directory: bool,
}

/// A directory's entries, in name order, and the path they were read at
/// with `~` spelled out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub path: PathBuf,
    pub entries: Vec<DirEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A path typed for a machine leads with `~` only as a whole component;
    /// it is spelled out against the home of the computer that reads it.
    #[test]
    fn expand_home_spells_out_a_leading_tilde_only() {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .expect("a home");
        assert_eq!(expand_home(Path::new("~")), home);
        assert_eq!(expand_home(Path::new("~/src")), home.join("src"));
        assert_eq!(
            expand_home(Path::new("~other/src")),
            Path::new("~other/src")
        );
        assert_eq!(expand_home(Path::new("/src/~")), Path::new("/src/~"));
    }
}
