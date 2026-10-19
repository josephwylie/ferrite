//! Ferrite: one API request (ADR 0027) on the Server's socket: a JSON line asks, a JSON line answers, as the `ferrite`
//! CLI asks. What the Server refuses comes back in its own words, with the Thread a refusal leaves behind.
use crate::json::*;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

/// The Server at one API socket, asked with one deadline.
pub struct Server { pub socket: PathBuf, pub timeout: Duration }

/// What the Server refused a request with: its code (`api::schema`), its words, and the Thread it left behind
/// (`agent_not_ready`). `no_server` and `timeout` are herdr's own, for a Server that never answered.
#[derive(Debug, Clone, PartialEq)]
pub struct Refused { pub code: String, pub message: String, pub thread: Option<u64> }

impl Refused {
    pub fn new(code: &str, message: impl Into<String>) -> Self { Refused { code: code.into(), message: message.into(), thread: None } }
}

impl Server {
    /// The request's `result`, or why there is none.
    pub fn call(&self, method: &str, params: Value) -> Result<Value, Refused> {
        let not_running = |e: std::io::Error| Refused::new("no_server", format!("Ferrite's Server is not running: nothing answers at {} ({e})", self.socket.display()));
        let mut stream = UnixStream::connect(&self.socket).map_err(not_running)?;
        let timeout = Some(self.timeout.max(Duration::from_millis(1)));
        let _ = stream.set_read_timeout(timeout);
        let _ = stream.set_write_timeout(timeout);
        let line = format!("{}\n", json!({ "id": "herdr", "method": method, "params": params }));
        stream.write_all(line.as_bytes()).map_err(not_running)?;
        let mut answer = String::new();
        match BufReader::new(stream).read_line(&mut answer) {
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => return Err(Refused::new("timeout", "herdr did not answer in time")),
            Err(e) => return Err(not_running(e)),
            Ok(0) => return Err(Refused::new("failed", format!("Ferrite's Server closed the connection before answering `{method}`"))),
            Ok(_) => {}
        }
        let answer: Value = serde_json::from_str(answer.trim()).map_err(|e| Refused::new("failed", format!("Ferrite's Server answered `{method}` with something that is not its API ({e})")))?;
        let error = answer.at("error");
        if error.is_object() {
            return Err(Refused { code: error.t("code"), message: error.t("message"), thread: error.at("thread").as_u64() });
        }
        Ok(answer.at("result").clone())
    }
}
