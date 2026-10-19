//! Fixtures the crate's unit tests share.

/// A fresh repository on `main` with an identity, and a runner for git in it.
pub(crate) fn test_repo() -> (tempfile::TempDir, impl Fn(&[&str]) -> String) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().to_path_buf();
    let git = move |args: &[&str]| {
        let out = std::process::Command::new("git").arg("-C").arg(&repo).args(args).output();
        let out = out.unwrap();
        assert!(out.status.success(), "git {args:?}");
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    (dir, git)
}
