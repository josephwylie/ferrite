//! A very small HTTP/1.1 server and client on the standard library, for the few local servers the board runs (the
//! evidence pages, the web Board's front door). One thread per connection; 127.0.0.1 only, always.
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

pub struct Request { pub method: String, pub target: String, pub path: String, pub query: HashMap<String, String>, pub headers: Vec<(String, String)>, pub body: Vec<u8>, pub head: Vec<u8> }
impl Request {
    pub fn header(&self, name: &str) -> Option<&str> { self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()) }
}
pub struct Response { pub status: u16, pub headers: Vec<(String, String)>, pub body: Vec<u8> }
impl Response {
    pub fn new(status: u16, kind: &str, body: impl Into<Vec<u8>>) -> Self { Response { status, headers: vec![("content-type".into(), kind.into())], body: body.into() } }
    pub fn text(status: u16, body: &str) -> Self { Self::new(status, "text/plain; charset=utf-8", body) }
    pub fn json(v: &serde_json::Value) -> Self { Self::new(200, "application/json", serde_json::to_vec(v).unwrap_or_default()) }
    pub fn with(mut self, k: &str, v: &str) -> Self { self.headers.push((k.into(), v.into())); self }
    pub fn write(&self, s: &mut impl Write) -> std::io::Result<()> {
        let mut head = format!("HTTP/1.1 {} {}\r\n", self.status, reason(self.status));
        for (k, v) in &self.headers { head.push_str(&format!("{k}: {v}\r\n")); }
        head.push_str(&format!("content-length: {}\r\nconnection: close\r\n\r\n", self.body.len()));
        s.write_all(head.as_bytes())?;
        s.write_all(&self.body)?;
        s.flush()
    }
}
fn reason(code: u16) -> &'static str { match code { 200 => "OK", 204 => "No Content", 302 => "Found", 400 => "Bad Request", 403 => "Forbidden", 404 => "Not Found", 405 => "Method Not Allowed", 409 => "Conflict", 502 => "Bad Gateway", _ => "Error" } }

/// Read one request from a connection: its head, and a body of at most `max` bytes.
pub fn read_request(stream: &mut TcpStream, max: usize) -> Option<Request> {
    let mut r = BufReader::new(stream.try_clone().ok()?);
    let mut head = Vec::new();
    let mut line = String::new();
    r.read_line(&mut line).ok()?;
    head.extend_from_slice(line.as_bytes());
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next()?.to_string(), parts.next()?.to_string());
    let mut headers = vec![];
    loop {
        let mut l = String::new();
        if r.read_line(&mut l).ok()? == 0 { break; }
        head.extend_from_slice(l.as_bytes());
        let t = l.trim_end();
        if t.is_empty() { break; }
        if let Some((k, v)) = t.split_once(':') { headers.push((k.trim().to_string(), v.trim().to_string())); }
        if head.len() > 64 * 1024 { return None; }
    }
    let len: usize = headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-length")).and_then(|(_, v)| v.parse().ok()).unwrap_or(0);
    if len > max { return None; }
    let mut body = vec![0; len];
    r.read_exact(&mut body).ok()?;
    let (path, q) = target.split_once('?').unwrap_or((target.as_str(), ""));
    let query = q.split('&').filter(|p| !p.is_empty()).map(|p| { let (k, v) = p.split_once('=').unwrap_or((p, "")); (crate::util::url_decode(k), crate::util::url_decode(&v.replace('+', " "))) }).collect();
    Some(Request { method, path: crate::util::url_decode(path), target, query, headers, body, head })
}
/// Serve on a listener until the process ends: each connection on a thread of its own. The handler may answer, or
/// take the connection itself (a stream of events, a passed-on upgrade) and answer None.
pub fn serve(listener: TcpListener, handler: impl Fn(Request, &mut TcpStream) -> Option<Response> + Send + Sync + 'static) {
    let h = std::sync::Arc::new(handler);
    for conn in listener.incoming() {
        let Ok(mut s) = conn else { continue };
        let h = h.clone();
        std::thread::spawn(move || {
            let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(30)));
            let Some(req) = read_request(&mut s, 4 << 20) else { return };
            if let Some(resp) = h(req, &mut s) { let _ = resp.write(&mut s); }
        });
    }
}
/// Listen on 127.0.0.1: on `port` when it is free (tried for a few seconds: the process this one takes over from may
/// still hold it), else on any free port.
pub fn listen(port: u16) -> std::io::Result<TcpListener> {
    if port != 0 { for _ in 0..12 { if let Ok(l) = TcpListener::bind(("127.0.0.1", port)) { return Ok(l); } std::thread::sleep(std::time::Duration::from_millis(250)); } }
    TcpListener::bind(("127.0.0.1", 0))
}
/// A plain GET or POST to a local server: (status, body). For the front door's checks and tests.
pub fn ask(port: u16, method: &str, path: &str, body: &[u8], timeout_ms: u64) -> Option<(u16, Vec<u8>)> {
    let mut s = TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), std::time::Duration::from_millis(timeout_ms)).ok()?;
    let _ = s.set_read_timeout(Some(std::time::Duration::from_millis(timeout_ms)));
    write!(s, "{method} {path} HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len()).ok()?;
    s.write_all(body).ok()?;
    let mut out = vec![];
    s.read_to_end(&mut out).ok()?;
    let at = out.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&out[..at]).to_string();
    let code = head.split_whitespace().nth(1)?.parse().ok()?;
    Some((code, out[at + 4..].to_vec()))
}
