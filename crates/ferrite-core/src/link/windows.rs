//! The named pipe: `\\.\pipe\ferrite-server-<user SID>-<store hash>`,
//! the first instance claimed with `FILE_FLAG_FIRST_PIPE_INSTANCE`, remote
//! clients rejected, and a DACL that grants only this user. A client first
//! checks the pipe's server process runs as itself. Overlapped handles, so
//! one connection's reader and writer threads never serialize each other.

use std::ffi::c_void;
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::Arc;

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, HANDLE,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    EqualSid, GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
    TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED,
    OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId,
    PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES,
    PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};

use super::TryClone;

const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const BUFFER: u32 = 64 * 1024;

struct Handle(HANDLE);

// SAFETY: a pipe handle is usable from any thread; overlapped I/O lets two
// threads use it at once.
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle is ours and closed once.
        unsafe { CloseHandle(self.0) };
    }
}

pub struct Stream(Arc<Handle>);

impl Stream {
    /// One overlapped transfer, waited for.
    fn transfer(&self, read: bool, buf: *mut u8, len: usize) -> io::Result<usize> {
        // SAFETY: a manual-reset event, unnamed, owned by `event` below.
        let event = Handle(unsafe { CreateEventW(null(), 1, 0, null()) });
        if event.0.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: zeroed is a valid OVERLAPPED; it outlives the wait below.
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.hEvent = event.0;
        let len = len.min(u32::MAX as usize) as u32;
        let mut done = 0u32;
        // SAFETY: `buf` is valid for `len` bytes until the wait completes.
        let started = unsafe {
            if read {
                ReadFile(self.0 .0, buf, len, &mut done, &mut overlapped)
            } else {
                WriteFile(self.0 .0, buf, len, &mut done, &mut overlapped)
            }
        };
        // SAFETY: GetLastError has no preconditions.
        if started == 0 && unsafe { GetLastError() } != ERROR_IO_PENDING {
            let error = io::Error::last_os_error();
            return if read && error.raw_os_error() == Some(109) {
                Ok(0) // ERROR_BROKEN_PIPE: the other end closed.
            } else {
                Err(error)
            };
        }
        // SAFETY: the operation above was started on this OVERLAPPED.
        if unsafe { GetOverlappedResult(self.0 .0, &overlapped, &mut done, 1) } == 0 {
            let error = io::Error::last_os_error();
            return if read && error.raw_os_error() == Some(109) {
                Ok(0)
            } else {
                Err(error)
            };
        }
        Ok(done as usize)
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.transfer(true, buf.as_mut_ptr(), buf.len())
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.transfer(false, buf.as_ptr() as *mut u8, buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl TryClone for Stream {
    fn try_clone(&self) -> io::Result<Self> {
        Ok(Self(Arc::clone(&self.0)))
    }

    fn shutdown(&self) -> io::Result<()> {
        // SAFETY: the handle is live; disconnecting a client end fails
        // harmlessly.
        unsafe { DisconnectNamedPipe(self.0 .0) };
        Ok(())
    }
}

pub struct Listener {
    name: Vec<u16>,
    /// The instance the next client will connect to.
    next: std::sync::Mutex<Option<Handle>>,
}

/// Listen on the pipe for the Store in the directory `path` names. A pipe
/// that already exists means a Server already runs.
pub fn listen(path: &Path) -> io::Result<Listener> {
    let name = pipe_name(path)?;
    let first = instance(&name, true)?;
    Ok(Listener {
        name,
        next: std::sync::Mutex::new(Some(first)),
    })
}

impl Listener {
    pub fn accept(&self) -> io::Result<Stream> {
        let pipe = match self.next.lock().unwrap_or_else(|e| e.into_inner()).take() {
            Some(pipe) => pipe,
            None => instance(&self.name, false)?,
        };
        let stream = Stream(Arc::new(pipe));
        // SAFETY: a manual-reset event for the connect below.
        let event = Handle(unsafe { CreateEventW(null(), 1, 0, null()) });
        // SAFETY: zeroed is a valid OVERLAPPED, alive until the wait ends.
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.hEvent = event.0;
        // SAFETY: the pipe was created overlapped; the OVERLAPPED is ours.
        let connected = unsafe { ConnectNamedPipe(stream.0 .0, &mut overlapped) };
        if connected == 0 {
            // SAFETY: GetLastError has no preconditions.
            match unsafe { GetLastError() } {
                ERROR_PIPE_CONNECTED => {}
                ERROR_IO_PENDING => {
                    let mut done = 0;
                    // SAFETY: the connect was started on this OVERLAPPED.
                    if unsafe { GetOverlappedResult(stream.0 .0, &overlapped, &mut done, 1) } == 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                _ => return Err(io::Error::last_os_error()),
            }
        }
        *self.next.lock().unwrap_or_else(|e| e.into_inner()) = Some(instance(&self.name, false)?);
        Ok(stream)
    }
}

/// Connect, and refuse a pipe whose server is not this user's process.
pub fn connect(path: &Path) -> io::Result<Stream> {
    let name = pipe_name(path)?;
    // SAFETY: a NUL-terminated name; the handle returned is ours.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let stream = Stream(Arc::new(Handle(handle)));
    let mut pid = 0u32;
    // SAFETY: the handle is a connected pipe client end.
    if unsafe { GetNamedPipeServerProcessId(handle, &mut pid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the process handle returned is ours and closed by `Handle`.
    let process = Handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) });
    if process.0.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the current-process pseudo-handle needs no closing.
    let ours = user(unsafe { GetCurrentProcess() })?;
    let theirs = user(process.0)?;
    // SAFETY: both buffers hold a TOKEN_USER whose SID points inside them.
    let same = unsafe {
        EqualSid(
            (*(ours.as_ptr() as *const TOKEN_USER)).User.Sid,
            (*(theirs.as_ptr() as *const TOKEN_USER)).User.Sid,
        )
    };
    if same == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the pipe's server runs as another user",
        ));
    }
    Ok(stream)
}

fn instance(name: &[u16], first: bool) -> io::Result<Handle> {
    let sid = user_sid()?;
    let sddl: Vec<u16> = format!("D:P(A;;GA;;;{sid})\0").encode_utf16().collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: a NUL-terminated SDDL string; the descriptor is freed below.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let open = PIPE_ACCESS_DUPLEX
        | FILE_FLAG_OVERLAPPED
        | if first {
            FILE_FLAG_FIRST_PIPE_INSTANCE
        } else {
            0
        };
    // SAFETY: a NUL-terminated name and valid security attributes.
    let pipe = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            open,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            BUFFER,
            BUFFER,
            0,
            &attributes,
        )
    };
    // SAFETY: allocated by the conversion above, freed once.
    unsafe { LocalFree(descriptor as *mut c_void) };
    if pipe == INVALID_HANDLE_VALUE {
        let error = io::Error::last_os_error();
        // ERROR_ACCESS_DENIED on the first instance: another Server has it.
        return Err(if first && error.raw_os_error() == Some(5) {
            io::Error::new(
                io::ErrorKind::AddrInUse,
                "a Ferrite server already runs for this store",
            )
        } else {
            error
        });
    }
    Ok(Handle(pipe))
}

/// One pipe per user and Store: the SID, and a hash of the Store's path.
/// The hash is FNV-1a, which no toolchain changes: an app and a Server
/// from different builds must name the same pipe.
fn pipe_name(path: &Path) -> io::Result<Vec<u16>> {
    let hash = path
        .as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    let name = format!(r"\\.\pipe\ferrite-server-{}-{hash:016x}", user_sid()?);
    Ok(std::ffi::OsStr::new(&name)
        .encode_wide()
        .chain(Some(0))
        .collect())
}

/// A process's access token, for reading who it runs as.
fn token(process: HANDLE) -> io::Result<Handle> {
    let mut token: HANDLE = null_mut();
    // SAFETY: `process` is live with query rights; the token is ours.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Handle(token))
}

/// Who a process runs as: its TOKEN_USER, in an 8-byte aligned buffer.
fn user(process: HANDLE) -> io::Result<Vec<u64>> {
    let token = token(process)?;
    let mut len = 0u32;
    // SAFETY: a size query; the call fails with the size it needs.
    unsafe { GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut len) };
    let mut buffer = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: the buffer is at least `len` bytes and 8-byte aligned.
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            len,
            &mut len,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(buffer)
}

fn user_sid() -> io::Result<String> {
    // SAFETY: the current-process pseudo-handle needs no closing.
    let buffer = user(unsafe { GetCurrentProcess() })?;
    let mut text: *mut u16 = null_mut();
    // SAFETY: the SID points inside `buffer`; `text` is freed below.
    if unsafe {
        ConvertSidToStringSidW(
            (*(buffer.as_ptr() as *const TOKEN_USER)).User.Sid,
            &mut text,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: a NUL-terminated string the conversion allocated.
    let sid = unsafe {
        let len = (0..).take_while(|&i| *text.add(i) != 0).count();
        String::from_utf16_lossy(std::slice::from_raw_parts(text, len))
    };
    // SAFETY: allocated by ConvertSidToStringSidW, freed once.
    unsafe { LocalFree(text as *mut c_void) };
    Ok(sid)
}
