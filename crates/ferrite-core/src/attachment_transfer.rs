//! Bounded attachment transfers and immutable server-owned retained content.

use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CHUNK_SIZE: usize = 256 * 1024;
pub const MAX_SIZE: u64 = crate::attachment::RETAIN_LIMIT;
const EXPIRY: Duration = Duration::from_secs(600);
const MAX_PENDING: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Retained {
    pub path: PathBuf,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub sent: u64,
    pub total: u64,
}

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(crate) fn check(&self) -> io::Result<()> {
        if self.0.load(Ordering::Acquire) {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Attachment upload cancelled",
            ))
        } else {
            Ok(())
        }
    }
}

pub struct Upload {
    pub progress: mpsc::Receiver<Progress>,
    pub result: mpsc::Receiver<io::Result<Retained>>,
    pub cancel: Cancellation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Request {
    Begin {
        name: String,
        size: u64,
    },
    Chunk {
        upload: u64,
        offset: u64,
        data: String,
    },
    Finish {
        upload: u64,
        sha256: String,
    },
    Cancel {
        upload: u64,
    },
    Read {
        path: PathBuf,
        offset: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Reply {
    Begun { upload: u64 },
    Written { offset: u64 },
    Retained(Retained),
    Cancelled,
    Bytes { data: String, size: u64 },
}

struct Pending {
    path: PathBuf,
    file: std::fs::File,
    name: String,
    size: u64,
    received: u64,
    hash: Sha256,
    touched: Instant,
}

impl Drop for Pending {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(crate) struct Session {
    store: PathBuf,
    next: u64,
    pending: HashMap<u64, Pending>,
}

impl Session {
    pub(crate) fn new(store: PathBuf) -> Self {
        Self {
            store,
            next: 0,
            pending: HashMap::new(),
        }
    }

    pub(crate) fn expire(&mut self) {
        self.pending
            .retain(|_, pending| pending.touched.elapsed() < EXPIRY);
    }

    pub(crate) fn execute(&mut self, request: Request) -> io::Result<Reply> {
        self.expire();
        match request {
            Request::Begin { name, size } => {
                validate_name(&name)?;
                if size > MAX_SIZE {
                    return Err(invalid("Attachments must be 64 MiB or smaller"));
                }
                if self.pending.len() >= MAX_PENDING {
                    return Err(invalid("Too many attachment uploads are pending"));
                }
                let dir = self.store.join("attachments").join(".uploads");
                private_dir(&dir)?;
                for entry in std::fs::read_dir(&dir)?.flatten() {
                    if entry.metadata().is_ok_and(|metadata| {
                        metadata.is_file()
                            && metadata.modified().is_ok_and(|modified| {
                                modified.elapsed().is_ok_and(|age| age >= EXPIRY)
                            })
                    }) {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
                static NEXT: AtomicU64 = AtomicU64::new(0);
                let (path, file) = loop {
                    let nonce = NEXT.fetch_add(1, Ordering::Relaxed);
                    let path = dir.join(format!(
                        "{}-{}-{nonce}.part",
                        std::process::id(),
                        crate::clock::now().timestamp_micros()
                    ));
                    match std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)
                    {
                        Ok(file) => break (path, file),
                        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                        Err(error) => return Err(error),
                    }
                };
                self.next += 1;
                self.pending.insert(
                    self.next,
                    Pending {
                        path,
                        file,
                        name,
                        size,
                        received: 0,
                        hash: Sha256::new(),
                        touched: Instant::now(),
                    },
                );
                Ok(Reply::Begun { upload: self.next })
            }
            Request::Chunk {
                upload,
                offset,
                data,
            } => {
                if data.len() > CHUNK_SIZE.div_ceil(3) * 4 {
                    self.pending.remove(&upload);
                    return Err(invalid("Attachment chunk exceeds 256 KiB"));
                }
                let bytes = match base64::engine::general_purpose::STANDARD.decode(data) {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        self.pending.remove(&upload);
                        return Err(invalid("Invalid attachment chunk"));
                    }
                };
                let pending = self
                    .pending
                    .get_mut(&upload)
                    .ok_or_else(|| invalid("Attachment upload expired"))?;
                if bytes.is_empty()
                    || bytes.len() > CHUNK_SIZE
                    || offset != pending.received
                    || pending.received.saturating_add(bytes.len() as u64) > pending.size
                {
                    self.pending.remove(&upload);
                    return Err(invalid(
                        "Attachment chunk is out of sequence or exceeds its declared size",
                    ));
                }
                if let Err(error) = pending.file.write_all(&bytes) {
                    self.pending.remove(&upload);
                    return Err(error);
                }
                pending.hash.update(&bytes);
                pending.received += bytes.len() as u64;
                pending.touched = Instant::now();
                Ok(Reply::Written {
                    offset: pending.received,
                })
            }
            Request::Finish { upload, sha256 } => {
                let mut pending = self
                    .pending
                    .remove(&upload)
                    .ok_or_else(|| invalid("Attachment upload expired"))?;
                let actual = hex(&pending.hash.clone().finalize());
                if pending.received != pending.size || sha256 != actual {
                    return Err(invalid("Attachment length or SHA-256 did not match"));
                }
                pending.file.flush()?;
                pending.file.sync_all()?;
                let dir = self.store.join("attachments").join(&actual);
                private_dir(&dir)?;
                let path = std::path::absolute(dir.join(&pending.name))?;
                match std::fs::hard_link(&pending.path, &path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                        let mut existing = std::fs::File::open(&path)?;
                        let mut hash = Sha256::new();
                        let mut copied = 0;
                        let mut buffer = vec![0; CHUNK_SIZE];
                        loop {
                            let count = existing.read(&mut buffer)?;
                            if count == 0 {
                                break;
                            }
                            copied += count as u64;
                            if copied > MAX_SIZE {
                                return Err(invalid("Retained attachment exceeds its bounds"));
                            }
                            hash.update(&buffer[..count]);
                        }
                        if copied != pending.size || hex(&hash.finalize()) != actual {
                            return Err(invalid("Retained attachment content does not match"));
                        }
                    }
                    Err(error) => return Err(error),
                }
                Ok(Reply::Retained(Retained {
                    path,
                    sha256: actual,
                    size: pending.size,
                }))
            }
            Request::Cancel { upload } => {
                self.pending.remove(&upload);
                Ok(Reply::Cancelled)
            }
            Request::Read { path, offset } => {
                let root = dunce::canonicalize(self.store.join("attachments"))?;
                let path = dunce::canonicalize(path)?;
                if !is_retained_path(&path) {
                    return Err(invalid("Not a retained attachment"));
                }
                let relative = path
                    .strip_prefix(&root)
                    .map_err(|_| invalid("Not a retained attachment"))?;
                let components: Vec<_> = relative.components().collect();
                if components.len() != 2 || components[0].as_os_str() == ".uploads" {
                    return Err(invalid("Not a retained attachment"));
                }
                let mut file = std::fs::File::open(path)?;
                let metadata = file.metadata()?;
                let size = metadata.len();
                if !metadata.is_file() || size > MAX_SIZE || offset > size {
                    return Err(invalid("Attachment preview exceeds its bounds"));
                }
                file.seek(SeekFrom::Start(offset))?;
                let mut bytes = Vec::with_capacity(CHUNK_SIZE);
                file.take(CHUNK_SIZE as u64).read_to_end(&mut bytes)?;
                Ok(Reply::Bytes {
                    data: base64::engine::general_purpose::STANDARD.encode(bytes),
                    size,
                })
            }
        }
    }
}

fn private_dir(path: &Path) -> io::Result<()> {
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub fn is_retained_path(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    let Some(hash) = parent.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    path.is_absolute()
        && matches!(hash.len(), 24 | 64)
        && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        && parent
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == "attachments")
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| validate_name(name).is_ok())
}

fn validate_name(name: &str) -> io::Result<()> {
    let mut components = Path::new(name).components();
    if name.is_empty()
        || name.len() > 240
        || name.contains(['/', '\\', ':'])
        || name.chars().any(char::is_control)
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
        || name == ".part"
    {
        return Err(invalid("An attachment needs a single file name"));
    }
    Ok(())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(crate) type Exchange = Box<dyn FnMut(Request) -> io::Result<Reply>>;

pub(crate) fn upload(
    path: PathBuf,
    connect: impl FnOnce() -> io::Result<Exchange> + Send + 'static,
) -> Upload {
    let (progress_send, progress) = crate::doorbell::channel();
    let (result_send, result) = crate::doorbell::channel();
    let cancel = Cancellation::default();
    let cancelled = cancel.clone();
    std::thread::spawn(move || {
        let result = (|| {
            cancelled.check()?;
            let mut file = std::fs::File::open(&path)?;
            let meta = file.metadata()?;
            if !meta.is_file() || meta.len() > MAX_SIZE {
                return Err(invalid("Choose a file of 64 MiB or smaller"));
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| invalid("Invalid attachment name"))?
                .to_string();
            validate_name(&name)?;
            let mut request = connect()?;
            cancelled.check()?;
            let Reply::Begun { upload } = request(Request::Begin {
                name,
                size: meta.len(),
            })?
            else {
                return Err(invalid("The server did not begin the attachment upload"));
            };
            let result = (|| {
                let mut hash = Sha256::new();
                let mut buffer = vec![0; CHUNK_SIZE];
                let mut sent = 0;
                let _ = progress_send.send(Progress {
                    sent,
                    total: meta.len(),
                });
                loop {
                    cancelled.check()?;
                    let count = file.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    let bytes = &buffer[..count];
                    let Reply::Written { offset } = request(Request::Chunk {
                        upload,
                        offset: sent,
                        data: base64::engine::general_purpose::STANDARD.encode(bytes),
                    })?
                    else {
                        return Err(invalid("The server did not accept the attachment chunk"));
                    };
                    sent += count as u64;
                    if offset != sent {
                        return Err(invalid(
                            "The server returned an incorrect attachment offset",
                        ));
                    }
                    hash.update(bytes);
                    let _ = progress_send.send(Progress {
                        sent,
                        total: meta.len(),
                    });
                }
                cancelled.check()?;
                let sha256 = hex(&hash.finalize());
                let Reply::Retained(retained) = request(Request::Finish {
                    upload,
                    sha256: sha256.clone(),
                })?
                else {
                    return Err(invalid("The server did not retain the attachment"));
                };
                cancelled.check()?;
                if retained.sha256 != sha256
                    || retained.size != sent
                    || !is_retained_path(&retained.path)
                    || retained
                        .path
                        .parent()
                        .and_then(Path::file_name)
                        .and_then(|name| name.to_str())
                        != Some(sha256.as_str())
                {
                    return Err(invalid("The server returned an invalid attachment receipt"));
                }
                Ok(retained)
            })();
            if result.is_err() {
                let _ = request(Request::Cancel { upload });
            }
            result
        })();
        let _ = result_send.send(result);
    });
    Upload {
        progress,
        result,
        cancel,
    }
}

pub(crate) fn read(
    path: PathBuf,
    mut request: impl FnMut(Request) -> io::Result<Reply>,
) -> io::Result<Vec<u8>> {
    if !is_retained_path(&path) {
        return Err(invalid("Not a retained attachment"));
    }
    let digest = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap()
        .to_ascii_lowercase();
    let mut bytes = Vec::new();
    let mut expected = None;
    loop {
        let Reply::Bytes { data, size } = request(Request::Read {
            path: path.clone(),
            offset: bytes.len() as u64,
        })?
        else {
            return Err(invalid("The server did not return attachment content"));
        };
        if size > MAX_SIZE
            || expected.is_some_and(|expected| expected != size)
            || data.len() > CHUNK_SIZE.div_ceil(3) * 4
        {
            return Err(invalid("Attachment preview exceeds its bounds"));
        }
        expected = Some(size);
        let chunk = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|_| invalid("Invalid attachment preview"))?;
        if chunk.len() > CHUNK_SIZE
            || bytes.len() as u64 + chunk.len() as u64 > size
            || (chunk.is_empty() && bytes.len() as u64 != size)
        {
            return Err(invalid("Invalid attachment preview length"));
        }
        bytes.extend(chunk);
        if bytes.len() as u64 == size {
            if !hex(&Sha256::digest(&bytes)).starts_with(&digest) {
                return Err(invalid("Retained attachment SHA-256 did not match"));
            }
            return Ok(bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(name: &str) -> Session {
        let store =
            std::env::temp_dir().join(format!("ferrite-transfer-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&store);
        std::fs::create_dir_all(&store).unwrap();
        Session::new(dunce::canonicalize(store).unwrap())
    }

    fn begin(session: &mut Session, size: u64) -> u64 {
        let Reply::Begun { upload } = session
            .execute(Request::Begin {
                name: "capture.png".into(),
                size,
            })
            .unwrap()
        else {
            panic!("begin");
        };
        upload
    }

    fn send(session: &mut Session, upload: u64, offset: u64, bytes: &[u8]) -> io::Result<Reply> {
        session.execute(Request::Chunk {
            upload,
            offset,
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
        })
    }

    #[test]
    fn chunk_boundaries_retain_exact_content_and_read_it_in_bounded_pieces() {
        let mut session = session("boundaries");
        for size in [0, 1, CHUNK_SIZE - 1, CHUNK_SIZE, CHUNK_SIZE + 1] {
            let bytes = vec![size as u8; size];
            let upload = begin(&mut session, size as u64);
            for (at, bytes) in bytes.chunks(CHUNK_SIZE).enumerate() {
                send(&mut session, upload, (at * CHUNK_SIZE) as u64, bytes).unwrap();
            }
            let Reply::Retained(retained) = session
                .execute(Request::Finish {
                    upload,
                    sha256: hex(&Sha256::digest(&bytes)),
                })
                .unwrap()
            else {
                panic!("retained");
            };
            assert!(is_retained_path(&retained.path));
            assert_eq!(std::fs::read(&retained.path).unwrap(), bytes);
            assert_eq!(
                read(retained.path, |request| session.execute(request)).unwrap(),
                bytes
            );
        }
        assert!(session.pending.is_empty());
        assert_eq!(
            std::fs::read_dir(session.store.join("attachments/.uploads"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn invalid_names_lengths_offsets_and_hashes_never_publish_content() {
        let mut session = session("invalid");
        for name in [
            "",
            "..",
            ".",
            "../file",
            "/file",
            "dir/file",
            "dir\\file",
            "bad\0file",
        ] {
            assert!(
                session
                    .execute(Request::Begin {
                        name: name.into(),
                        size: 1
                    })
                    .is_err(),
                "{name:?}"
            );
        }
        assert!(session
            .execute(Request::Begin {
                name: "large".into(),
                size: MAX_SIZE + 1
            })
            .is_err());
        let upload = begin(&mut session, 2);
        assert!(send(&mut session, upload, 1, b"a").is_err());
        let upload = begin(&mut session, CHUNK_SIZE as u64 + 1);
        assert!(send(&mut session, upload, 0, &vec![0; CHUNK_SIZE + 1]).is_err());
        let upload = begin(&mut session, 2);
        send(&mut session, upload, 0, b"ab").unwrap();
        assert!(session
            .execute(Request::Finish {
                upload,
                sha256: "wrong".into()
            })
            .is_err());
        let upload = begin(&mut session, 2);
        send(&mut session, upload, 0, b"a").unwrap();
        assert!(session
            .execute(Request::Finish {
                upload,
                sha256: hex(&Sha256::digest(b"a"))
            })
            .is_err());
        assert!(session.pending.is_empty());
        assert_eq!(
            std::fs::read_dir(session.store.join("attachments/.uploads"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn cancellation_expiry_and_disconnect_remove_scratch_files() {
        let mut session = session("cleanup");
        let first = begin(&mut session, 4);
        let second = begin(&mut session, 4);
        let first_path = session.pending[&first].path.clone();
        let second_path = session.pending[&second].path.clone();
        assert_ne!(first_path, second_path);
        session.execute(Request::Cancel { upload: first }).unwrap();
        assert!(!first_path.exists());
        session.pending.get_mut(&second).unwrap().touched = Instant::now() - EXPIRY;
        session.expire();
        assert!(!second_path.exists());
        let final_upload = begin(&mut session, 4);
        let final_path = session.pending[&final_upload].path.clone();
        drop(session);
        assert!(!final_path.exists());
    }

    #[test]
    fn concurrent_same_name_uploads_keep_distinct_content_and_verify_existing_content() {
        let mut session = session("concurrent");
        let first = begin(&mut session, 3);
        let second = begin(&mut session, 3);
        send(&mut session, first, 0, b"one").unwrap();
        send(&mut session, second, 0, b"two").unwrap();
        let finish = |session: &mut Session, upload, bytes: &[u8]| {
            let Reply::Retained(retained) = session
                .execute(Request::Finish {
                    upload,
                    sha256: hex(&Sha256::digest(bytes)),
                })
                .unwrap()
            else {
                panic!("retained");
            };
            retained
        };
        let first = finish(&mut session, first, b"one");
        let second = finish(&mut session, second, b"two");
        assert_ne!(first.path, second.path);
        assert_eq!(std::fs::read(&first.path).unwrap(), b"one");
        assert_eq!(std::fs::read(&second.path).unwrap(), b"two");
        let duplicate = begin(&mut session, 3);
        send(&mut session, duplicate, 0, b"one").unwrap();
        assert_eq!(finish(&mut session, duplicate, b"one"), first);
    }

    #[test]
    fn preview_refuses_unmanaged_paths_and_content_over_the_limit() {
        let mut session = session("preview-bounds");
        let outside = session.store.join("private");
        std::fs::write(&outside, b"secret").unwrap();
        std::fs::create_dir_all(session.store.join("attachments")).unwrap();
        assert!(session
            .execute(Request::Read {
                path: outside,
                offset: 0
            })
            .is_err());
        let dir = session.store.join("attachments").join("a".repeat(64));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("large.png");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(MAX_SIZE + 1)
            .unwrap();
        assert!(session
            .execute(Request::Read {
                path: path.clone(),
                offset: 0
            })
            .is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_corrupted_retained_preview_is_rejected() {
        let mut session = session("preview-corrupt");
        let upload = begin(&mut session, 3);
        send(&mut session, upload, 0, b"one").unwrap();
        let Reply::Retained(retained) = session
            .execute(Request::Finish {
                upload,
                sha256: hex(&Sha256::digest(b"one")),
            })
            .unwrap()
        else {
            panic!("retained");
        };
        std::fs::write(&retained.path, b"two").unwrap();
        assert!(read(retained.path, |request| session.execute(request)).is_err());
    }

    #[test]
    fn a_file_exactly_at_the_limit_is_accepted_in_chunks() {
        let mut session = session("limit");
        let upload = begin(&mut session, MAX_SIZE);
        let bytes = vec![7; CHUNK_SIZE];
        let mut hash = Sha256::new();
        for offset in (0..MAX_SIZE).step_by(CHUNK_SIZE) {
            send(&mut session, upload, offset, &bytes).unwrap();
            hash.update(&bytes);
        }
        let Reply::Retained(retained) = session
            .execute(Request::Finish {
                upload,
                sha256: hex(&hash.finalize()),
            })
            .unwrap()
        else {
            panic!("retained");
        };
        assert_eq!(retained.size, MAX_SIZE);
        assert_eq!(std::fs::metadata(&retained.path).unwrap().len(), MAX_SIZE);
        std::fs::remove_file(retained.path).unwrap();
    }
}
