//! Whether a file in a draft can be sent as it looks, and what keeps the
//! bytes that were sent. A draft's files are paths, so each is inspected
//! where it came from (`inspect`) and a send is admitted only while none has
//! `Failed`. `retain` copies a file into the app's own store, named by its
//! content, so history, retry and a restored draft keep the bytes that went
//! out even after the source changes.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The most one image or PDF carries inline to Claude; a larger one goes as
/// a path.
pub const NATIVE_LIMIT: u64 = 5 * 1024 * 1024;
/// The most one prompt carries inline.
pub const INLINE_TOTAL: u64 = 20 * 1024 * 1024;
/// The largest file `retain` copies. A bigger one stays a live reference to
/// the path it came from.
pub const RETAIN_LIMIT: u64 = 64 * 1024 * 1024;

/// What the agent is on, relative to the computer the files are on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Destination {
    #[default]
    Local,
    /// A machine the window is attached to: only paths that are already on
    /// it can be read there.
    Remote,
}

/// Where a file in a draft came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// This computer: a picker, a drop or a pasted screenshot.
    Local,
    /// The Thread's own workspace, wherever that is.
    Workspace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Readiness {
    Ready,
    /// Goes out as a path the agent reads with its own tools, not as native
    /// content.
    Reference(Fallback),
    /// Cannot be sent until removed or repaired.
    Failed(Failure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fallback {
    /// An image or PDF over `NATIVE_LIMIT`.
    TooLarge(u64),
    /// An image the app draws but neither provider takes natively.
    NotNative,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Missing,
    Unreadable(String),
    /// A file of this computer, bound for another machine.
    NotOnMachine,
}

impl Readiness {
    pub fn blocks(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    /// One short phrase, for a tile that has no room for more.
    pub fn label(&self) -> Option<&'static str> {
        match self {
            Self::Ready => None,
            Self::Reference(_) => Some("Path only"),
            Self::Failed(Failure::Missing) => Some("Missing"),
            Self::Failed(Failure::Unreadable(_)) => Some("Unreadable"),
            Self::Failed(Failure::NotOnMachine) => Some("Not on machine"),
        }
    }

    /// What it means for the send, in words.
    pub fn explanation(&self) -> Option<String> {
        Some(match self {
            Self::Ready => return None,
            Self::Reference(Fallback::TooLarge(bytes)) => format!(
                "{} is over the {} an agent takes inline: it is sent as a path.",
                size(*bytes),
                size(NATIVE_LIMIT)
            ),
            Self::Reference(Fallback::NotNative) => {
                "Agents do not take this image format inline: it is sent as a path.".into()
            }
            Self::Failed(Failure::Missing) => "The file is gone. Restore it and retry, or remove it.".into(),
            Self::Failed(Failure::Unreadable(why)) => format!("The file cannot be read ({why}). Fix it and retry, or remove it."),
            Self::Failed(Failure::NotOnMachine) => {
                "This file is on this computer, and the agent is on another machine. Ferrite does not upload it: remove it.".into()
            }
        })
    }
}

fn size(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
}

/// `path` as a send would find it now. A file of this computer bound for a
/// machine is `NotOnMachine` without looking at it; a workspace path on a
/// machine cannot be looked at from here and is taken as it is.
pub fn inspect(path: &Path, source: Source, destination: Destination) -> Readiness {
    match (destination, source) {
        (Destination::Remote, Source::Local) => return Readiness::Failed(Failure::NotOnMachine),
        (Destination::Remote, Source::Workspace) => return Readiness::Ready,
        (Destination::Local, _) => {}
    }
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Readiness::Failed(Failure::Missing)
        }
        Err(error) => return Readiness::Failed(Failure::Unreadable(error.to_string())),
    };
    if meta.is_dir() {
        return Readiness::Ready;
    }
    if let Err(error) = std::fs::File::open(path) {
        return Readiness::Failed(Failure::Unreadable(error.to_string()));
    }
    let native = crate::prompt_files::image_type(path).is_some()
        || crate::prompt_files::pdf_type(path).is_some();
    if native && meta.len() > NATIVE_LIMIT {
        return Readiness::Reference(Fallback::TooLarge(meta.len()));
    }
    if is_other_image(path) {
        return Readiness::Reference(Fallback::NotNative);
    }
    Readiness::Ready
}

/// Images by extension that the providers do not take natively.
fn is_other_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "heic" | "heif" | "avif" | "tif" | "tiff" | "svg" | "bmp" | "ico"
            )
        })
}

/// Whether `path` is already a file `retain` made.
pub fn is_retained(path: &Path, store: &Path) -> bool {
    path.starts_with(store.join(DIR))
}

const DIR: &str = "attachments";

/// Copy `path`'s bytes into `store`, in a directory named for their hash,
/// and give the copy's path: the same bytes under the same name are one
/// file, and a copy is never written over. A file over `RETAIN_LIMIT` is
/// not copied; its own path comes back.
pub fn retain(path: &Path, store: &Path) -> io::Result<PathBuf> {
    if is_retained(path, store) {
        return Ok(path.to_path_buf());
    }
    let meta = std::fs::metadata(path)?;
    if meta.is_dir() || meta.len() > RETAIN_LIMIT {
        return Ok(path.to_path_buf());
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    std::fs::File::open(path)?.read_to_end(&mut bytes)?;
    retain_bytes(&bytes, path.file_name().unwrap_or_default(), store)
}

/// `retain` for bytes that have no file: a pasted screenshot.
pub fn retain_bytes(bytes: &[u8], name: &std::ffi::OsStr, store: &Path) -> io::Result<PathBuf> {
    let hash = Sha256::digest(bytes);
    let dir = store.join(DIR).join(hex(&hash[..12]));
    let path = dir.join(name);
    if path.exists() {
        return Ok(path);
    }
    std::fs::create_dir_all(&dir)?;
    // A partly written file would be kept as the attachment's bytes, so the
    // name appears only once every byte is on disk.
    let scratch = dir.join(".part");
    std::fs::File::create(&scratch)?.write_all(bytes)?;
    std::fs::rename(&scratch, &path)?;
    Ok(path)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ferrite-attachment-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_file_fails_and_a_repaired_one_is_ready() {
        let dir = scratch("repair");
        let path = dir.join("shot.png");
        let local = |path: &Path| inspect(path, Source::Local, Destination::Local);
        assert_eq!(local(&path), Readiness::Failed(Failure::Missing));
        std::fs::write(&path, b"png").unwrap();
        assert_eq!(local(&path), Readiness::Ready);
    }

    #[test]
    fn what_goes_as_a_path_is_said_before_the_send() {
        let dir = scratch("fallbacks");
        let big = dir.join("big.png");
        std::fs::File::create(&big)
            .unwrap()
            .set_len(NATIVE_LIMIT + 1)
            .unwrap();
        let heic = dir.join("camera.heic");
        std::fs::write(&heic, b"x").unwrap();
        let local = |path: &Path| inspect(path, Source::Local, Destination::Local);
        assert_eq!(
            local(&big),
            Readiness::Reference(Fallback::TooLarge(NATIVE_LIMIT + 1))
        );
        assert_eq!(local(&heic), Readiness::Reference(Fallback::NotNative));
        assert!(!local(&heic).blocks());
    }

    #[test]
    fn a_file_of_this_computer_does_not_go_to_a_machine() {
        let dir = scratch("remote");
        let path = dir.join("notes.txt");
        std::fs::write(&path, b"x").unwrap();
        assert_eq!(
            inspect(&path, Source::Local, Destination::Remote),
            Readiness::Failed(Failure::NotOnMachine)
        );
        assert_eq!(
            inspect(&path, Source::Workspace, Destination::Remote),
            Readiness::Ready
        );
    }

    #[test]
    fn a_retained_copy_keeps_the_bytes_the_source_loses() {
        let dir = scratch("retain");
        let store = dir.join("store");
        let source = dir.join("shot.png");
        std::fs::write(&source, b"first").unwrap();
        let kept = retain(&source, &store).unwrap();
        std::fs::write(&source, b"second").unwrap();
        let again = retain(&source, &store).unwrap();
        assert_eq!(std::fs::read(&kept).unwrap(), b"first");
        assert_eq!(std::fs::read(&again).unwrap(), b"second");
        assert_ne!(kept, again);
        assert_eq!(retain(&kept, &store).unwrap(), kept);
    }
}
