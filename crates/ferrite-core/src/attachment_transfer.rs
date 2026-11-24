//! Bounded attachment transfers and immutable server-owned retained content.

use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::sync::mpsc;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::chunked::{self, hex, Cancellation, Progress, CHUNK_SIZE};

pub const MAX_SIZE: u64 = crate::attachment::RETAIN_LIMIT;
const MAX_PENDING: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Retained {
    pub path: PathBuf,
    pub sha256: String,
    pub size: u64,
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

pub struct Session {
    store: PathBuf,
    /// Each upload's file name.
    incoming: chunked::Incoming<String>,
    last: u64,
}

impl Session {
    pub fn new(store: PathBuf) -> Self {
        Self {
            store,
            incoming: chunked::Incoming::new(MAX_PENDING),
            last: 0,
        }
    }

    pub fn expire(&mut self) {
        self.incoming.expire();
    }

    pub fn execute(&mut self, request: Request) -> io::Result<Reply> {
        self.expire();
        match request {
            Request::Begin { name, size } => {
                validate_name(&name)?;
                if size > MAX_SIZE {
                    return Err(invalid("Attachments must be 64 MiB or smaller"));
                }
                let dir = self.store.join("attachments").join(".uploads");
                private_dir(&dir)?;
                for entry in std::fs::read_dir(&dir)?.flatten() {
                    if entry.metadata().is_ok_and(|metadata| {
                        metadata.is_file() && chunked::stale(&entry.file_name(), &metadata)
                    }) {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
                self.last += 1;
                self.incoming.begin(self.last, &dir, size, None, name)?;
                Ok(Reply::Begun { upload: self.last })
            }
            Request::Chunk {
                upload,
                offset,
                data,
            } => {
                let offset = self.incoming.chunk(upload, offset, &data)?;
                Ok(Reply::Written { offset })
            }
            Request::Finish { upload, sha256 } => {
                let received = self.incoming.finish(upload, &sha256)?;
                received.sync()?;
                let dir = self.store.join("attachments").join(&received.sha256);
                private_dir(&dir)?;
                let path = std::path::absolute(dir.join(received.what()))?;
                match std::fs::hard_link(received.path(), &path) {
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
                        if copied != received.size() || hex(&hash.finalize()) != received.sha256 {
                            return Err(invalid("Retained attachment content does not match"));
                        }
                    }
                    Err(error) => return Err(error),
                }
                Ok(Reply::Retained(Retained {
                    path,
                    size: received.size(),
                    sha256: received.sha256.clone(),
                }))
            }
            Request::Cancel { upload } => {
                self.incoming.cancel(upload);
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
    // A Windows desktop checks a Unix Server's receipt, rooted but driveless.
    path.has_root()
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

pub type Exchange = Box<dyn FnMut(Request) -> io::Result<Reply>>;

pub fn upload(
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
            let file = std::fs::File::open(&path)?;
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
                let total = meta.len();
                let _ = progress_send.send(Progress { sent: 0, total });
                let mut pieces = chunked::Pieces::new(file, total);
                for piece in &mut pieces {
                    cancelled.check()?;
                    let (piece, data) = piece?;
                    match request(Request::Chunk {
                        upload,
                        offset: piece.start,
                        data,
                    })? {
                        Reply::Written { offset } if offset == piece.end => {
                            let _ = progress_send.send(Progress { sent: offset, total });
                        }
                        _ => return Err(invalid("The server did not accept the attachment chunk")),
                    }
                }
                let sha256 = pieces.sha256()?;
                cancelled.check()?;
                let Reply::Retained(retained) = request(Request::Finish {
                    upload,
                    sha256: sha256.clone(),
                })?
                else {
                    return Err(invalid("The server did not retain the attachment"));
                };
                cancelled.check()?;
                if retained.sha256 != sha256
                    || retained.size != total
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

pub fn read(
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
    fn a_file_that_grows_while_it_uploads_fails_and_retains_nothing() {
        let session = session("growing");
        let store = session.store.clone();
        let source = store.join("growing.bin");
        std::fs::write(&source, vec![1; CHUNK_SIZE + 1]).unwrap();
        let growing = source.clone();
        let upload = upload(source, move || {
            let mut session = session;
            Ok(Box::new(move |request: Request| {
                if matches!(request, Request::Chunk { offset: 0, .. }) {
                    use std::io::Write as _;
                    let mut file = std::fs::OpenOptions::new().append(true).open(&growing)?;
                    file.write_all(b"more")?;
                }
                session.execute(request)
            }) as Exchange)
        });
        assert!(upload.result.recv().unwrap().is_err());
        let retained = std::fs::read_dir(store.join("attachments"))
            .unwrap()
            .filter(|entry| entry.as_ref().unwrap().file_name() != ".uploads")
            .count();
        assert_eq!(retained, 0);
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
        assert_eq!(
            std::fs::read_dir(session.store.join("attachments/.uploads"))
                .unwrap()
                .count(),
            0
        );
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
