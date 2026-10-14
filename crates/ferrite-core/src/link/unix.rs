//! The Unix socket: in the Store's directory (0700), the socket itself
//! 0600, and a peer whose uid is not ours refused before it says anything.

use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use super::TryClone;

pub struct Stream(UnixStream);

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl TryClone for Stream {
    fn try_clone(&self) -> io::Result<Self> {
        self.0.try_clone().map(Self)
    }

    fn shutdown(&self) -> io::Result<()> {
        self.0.shutdown(std::net::Shutdown::Both)
    }
}

pub struct Listener {
    listener: UnixListener,
    path: PathBuf,
}

/// Listen at `path`. A socket left by a Server that is gone (nothing
/// answers it) is removed first; one that answers means a Server already
/// runs, and this one must not.
pub fn listen(path: &Path) -> io::Result<Listener> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    if path.exists() {
        if UnixStream::connect(path).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                "a Ferrite server already runs for this store",
            ));
        }
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(Listener {
        listener,
        path: path.to_owned(),
    })
}

impl Listener {
    /// The next connection from this user. Anyone else is hung up on.
    pub fn accept(&self) -> io::Result<Stream> {
        loop {
            let (stream, _) = self.listener.accept()?;
            match peer_uid(&stream) {
                Ok(uid) if same_user(uid) => return Ok(Stream(stream)),
                Ok(uid) => crate::log!("server: refused a connection from uid {uid}"),
                Err(error) => crate::log!("server: refused a connection: {error}"),
            }
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn connect(path: &Path) -> io::Result<Stream> {
    UnixStream::connect(path).map(Stream)
}

fn same_user(uid: libc::uid_t) -> bool {
    // SAFETY: getuid has no preconditions and cannot fail.
    uid == unsafe { libc::getuid() }
}

#[cfg(not(target_os = "linux"))]
fn peer_uid(stream: &UnixStream) -> io::Result<libc::uid_t> {
    let mut uid: libc::uid_t = 0;
    let mut gid: libc::gid_t = 0;
    // SAFETY: the descriptor is a live socket and both out-pointers are valid.
    if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(uid)
}

#[cfg(target_os = "linux")]
fn peer_uid(stream: &UnixStream) -> io::Result<libc::uid_t> {
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: the descriptor is a live socket and the buffer is a ucred.
    let answered = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut len,
        )
    };
    if answered != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(credentials.uid)
}

#[cfg(test)]
pub(super) fn peer_is_us(stream: &Stream) -> bool {
    peer_uid(&stream.0).is_ok_and(same_user)
}
