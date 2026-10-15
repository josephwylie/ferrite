//! Finds the Herdr clients attached to a session and what an adapter needs to
//! locate their terminal container.

use std::env;
use std::ffi::{c_int, c_uint, c_void};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::util::command_stdout;

/// A Herdr client process attached to the session.
///
/// Terminals identify a container through variables they inject into the
/// environment, such as kitty's `KITTY_WINDOW_ID` or iTerm2's
/// `ITERM_SESSION_ID`.
#[derive(Debug)]
pub(crate) struct HerdrClient {
    pub(crate) pid: u32,
    pub(crate) environment: Vec<(String, String)>,
}

impl HerdrClient {
    /// A non-empty environment variable of the client process.
    pub(crate) fn env(&self, name: &str) -> Option<&str> {
        self.environment
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
            .filter(|value| !value.is_empty())
    }

    #[cfg(test)]
    pub(crate) fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        Self {
            pid: 0,
            environment: pairs
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
        }
    }
}

/// The clients of the session behind `herdr_socket_path`.
pub(crate) fn herdr_clients(herdr_socket_path: &str) -> Result<Vec<HerdrClient>, String> {
    let client_socket = client_socket_path(
        Path::new(herdr_socket_path),
        env::var_os("HERDR_CLIENT_SOCKET_PATH").map(PathBuf::from),
    )
    .to_string_lossy()
    .into_owned();
    let lsof = command_stdout("lsof", &["-a", "-U", "-c", "herdr", "-F", "pdn"])
        .ok_or("failed to list Herdr client sockets")?;

    Ok(client_pids_from_lsof(&lsof, &client_socket)
        .into_iter()
        .map(|pid| HerdrClient {
            pid,
            environment: process_environment(pid).unwrap_or_default(),
        })
        .collect())
}

/// When the client's terminal last received input, used to prefer the most
/// recently used client.
pub(crate) fn last_input(pid: u32) -> Option<SystemTime> {
    controlling_tty(pid).and_then(|tty| tty_last_input(&tty))
}

/// The socket Herdr clients of this session connect to.
///
/// Herdr honours `HERDR_CLIENT_SOCKET_PATH` and otherwise derives the path
/// from the API socket's stem: `herdr.sock` pairs with `herdr-client.sock`,
/// and a custom `/tmp/demo.sock` with `/tmp/demo-client.sock`.
fn client_socket_path(api_socket: &Path, configured: Option<PathBuf>) -> PathBuf {
    configured
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| {
            let stem = api_socket.file_stem().unwrap_or_default().to_string_lossy();
            api_socket.with_file_name(format!("{stem}-client.sock"))
        })
}

/// Pids of processes connected to `client_socket`, from `lsof -F pdn` output.
///
/// The server accepts client connections on the client socket; a client's
/// end of such a connection is anonymous and only names the server-side socket
/// address as its peer (`n->0x...`). Clients of other sessions connect to a
/// different path and short-lived CLI calls connect to the API socket, so both
/// are excluded.
fn client_pids_from_lsof(output: &str, client_socket: &str) -> Vec<u32> {
    let mut server_ends = Vec::new();
    let mut peers = Vec::new();
    let (mut pid, mut device) = (None, None);
    for line in output.lines() {
        let (field, value) = line.split_at(line.len().min(1));
        match field {
            "p" => pid = value.parse::<u32>().ok(),
            "f" => device = None,
            "d" => device = Some(value),
            "n" => {
                if value == client_socket {
                    server_ends.extend(device);
                } else if let (Some(pid), Some(peer)) = (pid, value.strip_prefix("->")) {
                    peers.push((pid, peer));
                }
            }
            _ => {}
        }
    }

    let mut pids: Vec<u32> = peers
        .into_iter()
        .filter(|(_, peer)| server_ends.contains(peer))
        .map(|(pid, _)| pid)
        .collect();
    pids.sort_unstable();
    pids.dedup();
    pids
}

fn controlling_tty(pid: u32) -> Option<String> {
    let tty = command_stdout("ps", &["-o", "tty=", "-p", &pid.to_string()])?;
    let tty = tty.trim();
    (!tty.is_empty() && !tty.starts_with('?')).then(|| tty.to_string())
}

/// When the terminal was last read, i.e. last received input. Output is
/// mirrored to every client, so the modification time cannot tell them apart.
fn tty_last_input(tty: &str) -> Option<SystemTime> {
    std::fs::metadata(Path::new("/dev").join(tty))
        .and_then(|meta| meta.accessed())
        .ok()
}

extern "C" {
    fn sysctl(
        name: *mut c_int,
        namelen: c_uint,
        oldp: *mut c_void,
        oldlenp: *mut usize,
        newp: *mut c_void,
        newlen: usize,
    ) -> c_int;
}

const CTL_KERN: c_int = 1;
const KERN_ARGMAX: c_int = 8;
const KERN_PROCARGS2: c_int = 49;

/// Reads another process's environment with `sysctl(KERN_PROCARGS2)`.
///
/// `ps eww` prints the same data, but joined by spaces, which makes values
/// containing spaces (such as a socket path) ambiguous.
fn process_environment(pid: u32) -> Option<Vec<(String, String)>> {
    let mut argmax: c_int = 0;
    let mut size = std::mem::size_of::<c_int>();
    let mut mib = [CTL_KERN, KERN_ARGMAX];
    // SAFETY: `argmax` is a valid c_int buffer of `size` bytes.
    let status = unsafe {
        sysctl(
            mib.as_mut_ptr(),
            2,
            (&mut argmax as *mut c_int).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 || argmax <= 0 {
        return None;
    }

    let mut buffer = vec![0u8; argmax as usize];
    let mut size = buffer.len();
    let mut mib = [CTL_KERN, KERN_PROCARGS2, c_int::try_from(pid).ok()?];
    // SAFETY: `buffer` is writable for `size` bytes, and sysctl updates `size`
    // to the number of bytes written.
    let status = unsafe {
        sysctl(
            mib.as_mut_ptr(),
            3,
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return None;
    }
    buffer.truncate(size);
    environment_from_procargs(&buffer)
}

/// Parses a `KERN_PROCARGS2` buffer: a native-endian `argc`, the executable
/// path, NUL padding, `argc` arguments, then the environment until an empty
/// string or the end of the buffer.
fn environment_from_procargs(buffer: &[u8]) -> Option<Vec<(String, String)>> {
    let argc = i32::from_ne_bytes(buffer.get(..4)?.try_into().ok()?);
    let mut strings = buffer[4..].split(|byte| *byte == 0);
    strings.next()?; // executable path
    let mut strings = strings.skip_while(|s| s.is_empty());
    for _ in 0..argc {
        strings.next()?;
    }

    Some(
        strings
            .take_while(|s| !s.is_empty())
            .filter_map(|entry| {
                let entry = String::from_utf8_lossy(entry);
                let (key, value) = entry.split_once('=')?;
                Some((key.to_string(), value.to_string()))
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIENT_SOCKET: &str = "/Users/me/.config/herdr/herdr-client.sock";

    #[test]
    fn derives_the_client_socket_from_the_api_socket() {
        assert_eq!(
            client_socket_path(Path::new("/Users/me/.config/herdr/herdr.sock"), None),
            PathBuf::from(CLIENT_SOCKET)
        );
        assert_eq!(
            client_socket_path(Path::new("/tmp/demo.sock"), None),
            PathBuf::from("/tmp/demo-client.sock")
        );
    }

    #[test]
    fn prefers_a_configured_client_socket() {
        assert_eq!(
            client_socket_path(
                Path::new("/tmp/demo.sock"),
                Some(PathBuf::from("/run/custom-client.sock"))
            ),
            PathBuf::from("/run/custom-client.sock")
        );
        assert_eq!(
            client_socket_path(Path::new("/tmp/demo.sock"), Some(PathBuf::new())),
            PathBuf::from("/tmp/demo-client.sock")
        );
    }

    #[test]
    fn ignores_clients_of_another_session_in_the_same_directory() {
        let lsof = "\
p100
f4
d0xa1
n->0xdemo
p200
f4
d0xa2
n->0xherdr
p300
f7
d0xdemo
n/tmp/demo-client.sock
f8
d0xherdr
n/tmp/herdr-client.sock
";
        assert_eq!(
            client_pids_from_lsof(lsof, "/tmp/demo-client.sock"),
            vec![100]
        );
    }

    #[test]
    fn finds_clients_connected_to_the_session_client_socket() {
        let lsof = "\
p100
f4
d0xa1
n->0xs1
f8
d0xa2
n->0xa3
p200
f4
d0xs0
n/Users/me/.config/herdr/herdr.sock
f18
d0xlisten
n/Users/me/.config/herdr/herdr-client.sock
f19
d0xs1
n/Users/me/.config/herdr/herdr-client.sock
f21
d0xs2
n/Users/me/.config/herdr/herdr-client.sock
p300
f5
d0xb1
n->0xs2
p400
f3
d0xc1
n->0xs0
p500
f3
d0xd1
n->0xother
";
        // 400 is a CLI call on the API socket; 500 belongs to another session.
        assert_eq!(client_pids_from_lsof(lsof, CLIENT_SOCKET), vec![100, 300]);
    }

    #[test]
    fn finds_no_clients_without_the_session_client_socket() {
        let lsof = "p100\nf4\nd0xa1\nn->0xs1\n";
        assert!(client_pids_from_lsof(lsof, CLIENT_SOCKET).is_empty());
    }

    #[test]
    fn reads_non_empty_client_environment_variables() {
        let client = HerdrClient::from_pairs(&[("SET", "value"), ("EMPTY", "")]);
        assert_eq!(client.env("SET"), Some("value"));
        assert_eq!(client.env("EMPTY"), None);
        assert_eq!(client.env("MISSING"), None);
    }

    #[test]
    fn parses_environment_from_procargs() {
        let mut buffer = 2i32.to_ne_bytes().to_vec();
        buffer.extend_from_slice(b"/usr/bin/herdr\0\0\0\0herdr\0--session\0");
        buffer.extend_from_slice(b"KITTY_WINDOW_ID=1\0KITTY_LISTEN_ON=unix:/tmp/a b\0\0junk\0");
        assert_eq!(
            environment_from_procargs(&buffer),
            Some(vec![
                ("KITTY_WINDOW_ID".to_string(), "1".to_string()),
                ("KITTY_LISTEN_ON".to_string(), "unix:/tmp/a b".to_string()),
            ])
        );
    }

    #[test]
    fn rejects_truncated_procargs() {
        assert_eq!(environment_from_procargs(&[1, 0]), None);
        let mut buffer = 3i32.to_ne_bytes().to_vec();
        buffer.extend_from_slice(b"/usr/bin/herdr\0herdr\0");
        assert_eq!(environment_from_procargs(&buffer), None);
    }

    #[test]
    fn reads_own_environment() {
        let environment = process_environment(std::process::id()).expect("own environment");
        assert!(environment.iter().any(|(key, _)| key == "PATH"));
    }
}
