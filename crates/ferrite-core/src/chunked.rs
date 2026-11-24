//! Files moved in pieces of [`CHUNK_SIZE`] over the Ferrite protocol (ADR
//! 0048). The receiver writes each file to a temporary file of its own,
//! hashes it as it arrives, and hands it over only once its length and
//! SHA-256 are what the sender said. Attachments and tree writes both move
//! files this way.

use std::collections::HashMap;
use std::fs::File;
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

    /// Receive `size` bytes as file `id` into a new temporary file in `dir`.
    /// An `id` already under way is refused, and that file dropped too.
    pub fn begin(&mut self, id: u64, dir: &Path, size: u64, what: T) -> io::Result<()> {
        self.expire();
        if self.parts.remove(&id).is_some() {
            return Err(invalid("That upload was already under way"));
        }
        if self.parts.len() >= self.most {
            return Err(invalid("Too many uploads are under way"));
        }
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let (path, file) = loop {
            let nonce = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = dir.join(format!(
                ".ferrite-{}-{}-{nonce}.part",
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

    /// The SHA-256 of the pieces, for the receiver's finish, once they
    /// came to all `size` bytes.
    pub fn sha256(self) -> io::Result<String> {
        if self.offset != self.size {
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
        incoming.begin(1, &dir, 4, ()).unwrap();
        incoming.begin(2, &dir, 4, ()).unwrap();
        assert_eq!(files(&dir), 2);
        incoming.cancel(1);
        assert_eq!(files(&dir), 1);
        incoming.parts.get_mut(&2).unwrap().touched = Instant::now() - EXPIRY;
        incoming.expire();
        assert_eq!(files(&dir), 0);
        incoming.begin(3, &dir, 4, ()).unwrap();
        assert!(incoming.begin(3, &dir, 4, ()).is_err(), "a number already under way");
        assert_eq!(files(&dir), 0, "and the file under it dropped");
        incoming.begin(4, &dir, 4, ()).unwrap();
        drop(incoming);
        assert_eq!(files(&dir), 0);
    }

    #[test]
    fn what_is_sent_in_pieces_arrives_whole_and_hashed() {
        let dir = dir("whole");
        let bytes: Vec<u8> = (0..CHUNK_SIZE * 2 + 5).map(|at| at as u8).collect();
        let mut incoming = Incoming::new(1);
        incoming.begin(7, &dir, bytes.len() as u64, "kept").unwrap();
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
