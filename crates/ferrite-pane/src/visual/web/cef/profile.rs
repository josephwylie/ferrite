//! CEF fixes its cache root at initialization. Hold the website profile claim
//! through shutdown, while keeping sealed visuals in a disposable sibling.
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct Profile {
    root: PathBuf,
    sealed: PathBuf,
    websites: Result<PathBuf, String>,
    remove_root: bool,
    // Dropped after our cleanup, and only after the owning engine shuts down.
    _claim: Option<File>,
}

impl Profile {
    pub(super) fn new() -> Self {
        let explicit = std::env::var_os("FERRITE_BROWSER_PROFILE");
        // Ordinary packaged smoke must never read or modify operator logins.
        if explicit.is_none() && std::env::args().any(|arg| arg == "--visual-smoke") {
            return Self::temporary(None);
        }
        let root = explicit
            .map(PathBuf::from)
            .unwrap_or_else(|| ferrite_core::server::store_dir().join("browser-profile"));
        match Self::durable(&root) {
            Ok(profile) => profile,
            Err(error) => Self::temporary(Some(error)),
        }
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn sealed(&self) -> &Path {
        &self.sealed
    }

    pub(super) fn websites(&self) -> Result<PathBuf, String> {
        self.websites.clone()
    }

    fn durable(root: &Path) -> Result<Self, String> {
        let unavailable = |error| {
            format!(
                "Website browser unavailable: cannot use profile {}: {error}. Close any other Ferrite instance using this profile, check its directory permissions, then restart this instance.",
                root.display()
            )
        };
        create_private_dir(root).map_err(|e| unavailable(e.to_string()))?;
        // CEF requires absolute cache paths; canonicalization also ensures
        // aliases contend on the same lock and sealed cleanup stays bounded.
        let root = canonical_root(root).map_err(|e| unavailable(e.to_string()))?;
        let claim = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(".ferrite-profile.lock"))
            .map_err(|e| unavailable(e.to_string()))?;
        claim.try_lock().map_err(|error| {
            unavailable(match error {
                fs::TryLockError::WouldBlock => {
                    "another Ferrite process owns the website profile".into()
                }
                fs::TryLockError::Error(error) => format!("exclusive profile lock failed: {error}"),
            })
        })?;
        let websites = root.join("websites");
        create_private_dir(&websites).map_err(|e| unavailable(e.to_string()))?;
        let sealed = unique_child(&root, "sealed").map_err(|e| unavailable(e.to_string()))?;
        Ok(Self {
            root,
            sealed,
            websites: Ok(websites),
            remove_root: false,
            _claim: Some(claim),
        })
    }

    fn temporary(unavailable: Option<String>) -> Self {
        // A failed temp directory is left for CEF initialization to report;
        // never switch back to the real website profile on any failure.
        let root = unique_path(&std::env::temp_dir(), "ferrite-cef");
        let _ = create_private_dir(&root);
        let root = canonical_root(&root).unwrap_or(root);
        let sealed = root.join("sealed");
        let _ = create_private_dir(&sealed);
        let websites = match unavailable {
            Some(error) => Err(error),
            None => {
                let websites = root.join("websites");
                create_private_dir(&websites)
                    .map(|()| websites)
                    .map_err(|e| format!("Isolated smoke profile unavailable: {e}"))
            }
        };
        Self {
            root,
            sealed,
            websites,
            remove_root: true,
            _claim: None,
        }
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        // Never recursively delete the durable root, website data or lock.
        let _ = fs::remove_dir_all(if self.remove_root {
            &self.root
        } else {
            &self.sealed
        });
    }
}

fn create_private_dir(path: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn unique_path(parent: &Path, prefix: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    parent.join(format!(
        "{prefix}-{}-{nonce}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn unique_child(parent: &Path, prefix: &str) -> std::io::Result<PathBuf> {
    let path = unique_path(parent, prefix);
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&path)?;
    Ok(path)
}

/// Resolve aliases for the claim without passing Windows verbatim paths to CEF.
fn canonical_root(path: &Path) -> std::io::Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        let mut components = canonical.components();
        let mut ordinary = match components.next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:\\", char::from(drive))),
                Prefix::VerbatimUNC(server, share) => {
                    let mut unc = PathBuf::from(r"\\");
                    unc.push(server);
                    unc.push(share);
                    unc
                }
                Prefix::Disk(_) | Prefix::UNC(_, _) => return Ok(canonical),
                _ => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "CEF profile requires a drive or UNC path",
                    ))
                }
            },
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "CEF profile must resolve to an absolute path",
                ))
            }
        };
        for component in components {
            if !matches!(component, Component::RootDir) {
                ordinary.push(component.as_os_str());
            }
        }
        Ok(ordinary)
    }
    #[cfg(not(windows))]
    Ok(canonical)
}
