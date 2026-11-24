//! Files moved in pieces of [`CHUNK_SIZE`] over the Ferrite protocol (ADR
//! 0048). The receiver writes each file to a temporary file of its own,
//! hashes it as it arrives, and hands it over only once its length and
//! SHA-256 are what the sender said. Attachments and tree writes both move
//! files this way.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{File, Metadata};
use std::io::{self, Read, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use sha2::{Digest, Sha256};

pub const CHUNK_SIZE: usize = 256 * 1024;
/// A file nothing was heard of for this long is dropped, with its
/// temporary file.
pub const EXPIRY: Duration = Duration::from_secs(600);

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

    pub fn check(&self) -> io::Result<()> {
        if self.0.load(Ordering::Acquire) {
            Err(io::Error::new(io::ErrorKind::Interrupted, "Cancelled"))
        } else {
            Ok(())
        }
    }
}

struct Part<T> {
    path: PathBuf,
    file: File,
    size: u64,
    received: u64,
    hash: Sha256,
    touched: Instant,
    what: T,
}

impl<T> Drop for Part<T> {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Whether `name` is one a receiver gives a temporary file:
/// `.ferrite-<process>-<micros>-<nonce>.part`, each a number.
pub fn temporary(name: &OsStr) -> bool {
    let numbers = name
        .to_str()
        .and_then(|name| name.strip_prefix(".ferrite-")?.strip_suffix(".part"));
    let number = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    numbers.is_some_and(|numbers| {
        let parts: Vec<&str> = numbers.split('-').collect();
        parts.len() == 3 && parts.into_iter().all(number)
    })
}

/// A new temporary file in `dir`, named as [`temporary`] reads, made with
/// the permission bits `mode` where this side keeps them.
pub fn temporary_file(dir: &Path, mode: Option<u32>) -> io::Result<(PathBuf, File)> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    loop {
        let nonce = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!(
            ".ferrite-{}-{}-{nonce}.part",
            std::process::id(),
            crate::clock::now().timestamp_micros()
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        if let Some(mode) = mode {
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, mode);
        }
        #[cfg(not(unix))]
        let _ = mode;
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

/// Whether the temporary file `name` was left by a write that is over: the
/// process it is named for is gone, or nothing was written to it for
/// [`EXPIRY`], and no write takes a chunk after that.
pub fn stale(name: &OsStr, meta: &Metadata) -> bool {
    let quiet = meta
        .modified()
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age >= EXPIRY);
    let writer = name
        .to_str()
        .and_then(|name| name.strip_prefix(".ferrite-")?.split('-').next()?.parse().ok());
    quiet || writer.is_some_and(gone)
}

/// Whether no process `pid` runs. This process is never gone, and one this
/// side cannot ask about counts as running.
fn gone(pid: u32) -> bool {
    if pid == std::process::id() {
        return false;
    }
    #[cfg(unix)]
    {
        let Ok(pid) = libc::pid_t::try_from(pid) else {
            return false;
        };
        // SAFETY: signal 0 only asks whether the process exists.
        pid > 0
            && unsafe { libc::kill(pid, 0) } != 0
            && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{
            CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, STILL_ACTIVE,
        };
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        // SAFETY: a query-only handle, closed below once its exit code is read.
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return GetLastError() == ERROR_INVALID_PARAMETER;
            }
            let mut code = 0u32;
            let exited = GetExitCodeProcess(process, &mut code) != 0 && code != STILL_ACTIVE as u32;
            CloseHandle(process);
            exited
        }
    }
}

/// The files one sender has under way, each by its number, and what the
/// receiver keeps about each (`T`).
pub struct Incoming<T> {
    parts: HashMap<u64, Part<T>>,
    most: usize,
}

/// A file received whole, still at its temporary path. The receiver links
/// or renames it into place; dropping it removes what is left there.
pub struct Received<T> {
    part: Part<T>,
    pub sha256: String,
}

impl<T> Received<T> {
    pub fn path(&self) -> &Path {
        &self.part.path
    }

    pub fn what(&self) -> &T {
        &self.part.what
    }

    pub fn size(&self) -> u64 {
        self.part.size
    }

    /// Wait until its bytes are on disk.
    pub fn sync(&self) -> io::Result<()> {
        self.part.file.sync_all()
    }
}

impl<T> Incoming<T> {
    /// At most `most` files under way at once.
    pub fn new(most: usize) -> Self {
        Self {
            parts: HashMap::new(),
            most,
        }
    }

    pub fn expire(&mut self) {
        self.parts.retain(|_, part| part.touched.elapsed() < EXPIRY);
    }

    /// Receive `size` bytes as file `id` into a new temporary file in `dir`,
    /// made with the permission bits `mode` where this side keeps them. An
    /// `id` already under way is refused, and that file dropped too.
    pub fn begin(
        &mut self,
        id: u64,
        dir: &Path,
        size: u64,
        mode: Option<u32>,
        what: T,
    ) -> io::Result<()> {
        self.expire();
        if self.parts.remove(&id).is_some() {
            return Err(invalid("That upload was already under way"));
        }
        if self.parts.len() >= self.most {
            return Err(invalid("Too many uploads are under way"));
        }
        let (path, file) = temporary_file(dir, mode)?;
        self.parts.insert(
            id,
            Part {
                path,
                file,
                size,
                received: 0,
                hash: Sha256::new(),
                touched: Instant::now(),
                what,
            },
        );
        Ok(())
    }

    /// Write the base64 `data` at `offset`, which must be where the file
    /// has reached: the offset reached after it. Anything out of order
    /// drops the file.
    pub fn chunk(&mut self, id: u64, offset: u64, data: &str) -> io::Result<u64> {
        self.expire();
        if data.len() > CHUNK_SIZE.div_ceil(3) * 4 {
            self.parts.remove(&id);
            return Err(invalid("A chunk is over 256 KiB"));
        }
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(data) else {
            self.parts.remove(&id);
            return Err(invalid("A chunk is not base64"));
        };
        let part = self.parts.get_mut(&id).ok_or_else(unknown)?;
        if bytes.is_empty()
            || bytes.len() > CHUNK_SIZE
            || offset != part.received
            || part.received.saturating_add(bytes.len() as u64) > part.size
        {
            self.parts.remove(&id);
            return Err(invalid("A chunk is out of order or past the declared size"));
        }
        if let Err(error) = part.file.write_all(&bytes) {
            self.parts.remove(&id);
            return Err(error);
        }
        part.hash.update(&bytes);
        part.received += bytes.len() as u64;
        part.touched = Instant::now();
        Ok(part.received)
    }

    /// The file, once all of it arrived and it hashes to `sha256`.
    pub fn finish(&mut self, id: u64, sha256: &str) -> io::Result<Received<T>> {
        self.expire();
        let part = self.parts.remove(&id).ok_or_else(unknown)?;
        let actual = hex(&part.hash.clone().finalize());
        if part.received != part.size || sha256 != actual {
            return Err(invalid("The length or SHA-256 did not match"));
        }
        Ok(Received {
            part,
            sha256: actual,
        })
    }

    pub fn cancel(&mut self, id: u64) {
        self.parts.remove(&id);
    }
}

/// The first `size` bytes of a file to send, in pieces of at most
/// [`CHUNK_SIZE`]: each its place in the file and its base64.
pub struct Pieces<R> {
    from: io::Take<R>,
    size: u64,
    offset: u64,
    hash: Sha256,
    buffer: Vec<u8>,
}

impl<R: Read> Pieces<R> {
    pub fn new(from: R, size: u64) -> Self {
        Self {
            from: from.take(size),
            size,
            offset: 0,
            hash: Sha256::new(),
            buffer: vec![0; CHUNK_SIZE],
        }
    }

    /// The SHA-256 of the pieces, once they came to all `size` bytes and
    /// the file held no more.
    pub fn sha256(self) -> io::Result<String> {
        let mut rest = self.from.into_inner();
        if self.offset != self.size || fill(&mut rest, &mut [0])? != 0 {
            return Err(invalid("The file changed while it was sent"));
        }
        Ok(hex(&self.hash.finalize()))
    }
}

impl<R: Read> Iterator for Pieces<R> {
    type Item = io::Result<(Range<u64>, String)>;

    fn next(&mut self) -> Option<Self::Item> {
        let count = match fill(&mut self.from, &mut self.buffer) {
            Ok(0) => return None,
            Ok(count) => count,
            Err(error) => return Some(Err(error)),
        };
        let bytes = &self.buffer[..count];
        self.hash.update(bytes);
        let piece = self.offset..self.offset + count as u64;
        self.offset = piece.end;
        Some(Ok((piece, base64::engine::general_purpose::STANDARD.encode(bytes))))
    }
}

fn fill(from: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match from.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn unknown() -> io::Error {
    invalid("That upload failed or expired")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrite-chunked-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn files(dir: &Path) -> usize {
        std::fs::read_dir(dir).unwrap().count()
    }

    #[test]
    fn cancellation_expiry_and_dropping_remove_the_temporary_files() {
        let dir = dir("cleanup");
        let mut incoming = Incoming::new(4);
        incoming.begin(1, &dir, 4, None, ()).unwrap();
        incoming.begin(2, &dir, 4, None, ()).unwrap();
        assert_eq!(files(&dir), 2);
        incoming.cancel(1);
        assert_eq!(files(&dir), 1);
        incoming.parts.get_mut(&2).unwrap().touched = Instant::now() - EXPIRY;
        incoming.expire();
        assert_eq!(files(&dir), 0);
        incoming.begin(3, &dir, 4, None, ()).unwrap();
        assert!(incoming.begin(3, &dir, 4, None, ()).is_err(), "a number already under way");
        assert_eq!(files(&dir), 0, "and the file under it dropped");
        incoming.begin(4, &dir, 4, None, ()).unwrap();
        drop(incoming);
        assert_eq!(files(&dir), 0);
    }

    #[test]
    fn a_temporary_file_is_stale_once_the_process_writing_it_is_gone() {
        let dir = dir("writer");
        let mut writer = std::process::Command::new("git")
            .args(["hash-object", "--stdin"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let name = format!(".ferrite-{}-1-2.part", writer.id());
        let path = dir.join(&name);
        std::fs::write(&path, "half").unwrap();
        let stale = |name: &str| super::stale(OsStr::new(name), &std::fs::metadata(&path).unwrap());
        assert!(!stale(&name), "its writer still runs");
        drop(writer.stdin.take());
        writer.wait().unwrap();
        drop(writer);
        assert!(stale(&name), "its writer is gone");
        assert!(!stale(&format!(".ferrite-{}-1-2.part", std::process::id())));
    }

    #[test]
    fn what_is_sent_in_pieces_arrives_whole_and_hashed() {
        let dir = dir("whole");
        let bytes: Vec<u8> = (0..CHUNK_SIZE * 2 + 5).map(|at| at as u8).collect();
        let mut incoming = Incoming::new(1);
        incoming.begin(7, &dir, bytes.len() as u64, None, "kept").unwrap();
        let mut pieces = Pieces::new(&bytes[..], bytes.len() as u64);
        let mut chunks = 0;
        for piece in &mut pieces {
            let (piece, data) = piece.unwrap();
            chunks += 1;
            assert_eq!(incoming.chunk(7, piece.start, &data).unwrap(), piece.end);
        }
        assert_eq!(chunks, 3);
        let received = incoming.finish(7, &pieces.sha256().unwrap()).unwrap();
        assert_eq!(*received.what(), "kept");
        assert_eq!(std::fs::read(received.path()).unwrap(), bytes);
        assert_eq!(received.sha256, hex(&Sha256::digest(&bytes)));
    }
}
