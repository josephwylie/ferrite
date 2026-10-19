//! The one setup step: the web UI's shared link. The web UI does not share itself; its Phone setup shows the command
//! (`tailscale serve --bg --https=<port> http://127.0.0.1:<its port>`) and recognises its link by where that entry
//! points. So the Board shows at that same link once the entry points at the front door instead, which passes
//! everything on to the web UI. After a yes (`link`) such entries point at the front door; `unlink` points them back.
//! What was changed is written down (extras.json), and only an entry that still points at the front door is put back.
//! Private only: an entry open to the internet (Funnel) is never touched, and nothing here ever starts one.
use crate::extras::{extras, note};
use crate::json::*;

#[derive(Clone, Debug)]
pub struct Entry { pub host: String, pub https: u16, pub to: u16, pub funnel: bool, pub url: String }
fn tailscale(a: &[&str]) -> Option<String> {
    let o = std::process::Command::new("tailscale").args(a).stdin(std::process::Stdio::null()).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}
pub fn address_of(port: u16) -> String { format!("http://127.0.0.1:{port}") }
fn target_of(t: &str) -> Option<u16> {
    let rest = t.strip_prefix("http://").or_else(|| t.strip_prefix("https://"))?;
    let (hostport, path) = rest.split_once('/').map(|(a, b)| (a, b)).unwrap_or((rest, ""));
    if !path.is_empty() { return None; }
    let (host, port) = hostport.rsplit_once(':').unwrap_or((hostport, if t.starts_with("https") { "443" } else { "80" }));
    (["127.0.0.1", "localhost"].contains(&host)).then(|| port.parse().ok()).flatten()
}
/// Every HTTPS entry of `tailscale serve` whose root goes to a port on this machine. None: Tailscale did not say.
pub fn read() -> Option<Vec<Entry>> {
    let status: Value = serde_json::from_str(&tailscale(&["serve", "status", "--json"])?).ok()?;
    let mut out = vec![];
    for (hp, site) in status.at("Web").as_object().cloned().unwrap_or_default() {
        let Some(at) = hp.rfind(':') else { continue };
        let (host, https) = (hp[..at].to_string(), hp[at + 1..].parse::<u16>().unwrap_or(0));
        if !status.at("TCP").at(&https.to_string()).b("HTTPS") { continue; }
        let Some(to) = target_of(site.at("Handlers").at("/").s("Proxy")) else { continue };
        out.push(Entry { url: if https == 443 { format!("https://{host}") } else { format!("https://{host}:{https}") }, funnel: status.at("AllowFunnel").b(&hp), host, https, to });
    }
    Some(out)
}
fn point(https: u16, port: u16) { tailscale(&["serve", "--bg", &format!("--https={https}"), &address_of(port)]); }
fn recorded() -> Vec<Value> { extras().arr("webUiLinks").to_vec() }
pub struct Sorted { pub ours: Vec<Entry>, pub direct: Vec<Entry>, pub open: Vec<Entry> }
fn sort(all: Vec<Entry>, web: u16, door: Option<u16>) -> Sorted {
    let ours: Vec<Entry> = all.iter().filter(|e| Some(e.to) == door).cloned().collect();
    let straight: Vec<Entry> = all.iter().filter(|e| Some(e.to) != door && e.to == web).cloned().collect();
    Sorted { ours, direct: straight.iter().filter(|e| !e.funnel).cloned().collect(), open: straight.into_iter().filter(|e| e.funnel).collect() }
}
pub fn look(web: u16, door: Option<u16>) -> Sorted { sort(read().unwrap_or_default(), web, door) }
/// Points each of the web UI's private links at the front door, and writes down where the web UI is. The caller has
/// the yes. Returns the links it now goes through.
pub fn link(web: u16, door: u16) -> Vec<String> {
    let Some(all) = read() else { return vec![] };
    let s = sort(all, web, Some(door));
    let rec = |https: u16, host: &str| json!({ "https": https, "host": host, "was": web, "now": address_of(door) });
    let mut kept: Vec<Value> = recorded().into_iter().filter(|r| s.ours.iter().any(|e| e.https as i64 == r.i("https"))).map(|r| rec(r.i("https") as u16, r.s("host"))).collect();
    for e in &s.direct { point(e.https, door); if !kept.iter().any(|r| r.i("https") == e.https as i64) { kept.push(rec(e.https, &e.host)); } }
    note(json!({ "webUi": "yes", "webUiLinks": kept }));
    s.ours.iter().chain(s.direct.iter()).map(|e| e.url.clone()).collect()
}
/// The front door moved: each link that still points at where it was goes with it.
pub fn follow(door: u16, was: Option<u16>, web: Option<u16>) {
    let mine = recorded();
    if mine.is_empty() { return; }
    let Some(all) = read() else { return };
    let mut next = vec![];
    for r in mine {
        let e = all.iter().find(|x| x.https as i64 == r.i("https"));
        let old = if r.b("now") { r.t("now") } else { was.map(address_of).unwrap_or_default() };
        let moves = e.map(|e| e.to != door && !old.is_empty() && address_of(e.to) == old).unwrap_or(false);
        if moves { point(r.i("https") as u16, door); }
        let mut n = r.as_object().cloned().unwrap_or_default();
        if let Some(w) = web { n.put("was", w); }
        if moves || e.map(|e| e.to == door).unwrap_or(false) { n.put("now", address_of(door)); }
        next.push(Value::Object(n));
    }
    note(json!({ "webUiLinks": next }));
}
/// Puts every link back at the web UI, where it still points at the front door. Returns how many.
pub fn unlink(door: Option<u16>, web: Option<u16>) -> Result<usize, String> {
    let mine = recorded();
    if mine.is_empty() { return Ok(0); }
    let all = read().ok_or("Tailscale did not answer, so the web UI's link was left as it is. Try again: herdr-projects web-ui unlink")?;
    let mut n = 0;
    for r in mine { if let Some(e) = all.iter().find(|x| x.https as i64 == r.i("https")) { if Some(e.to) == door { point(r.i("https") as u16, web.unwrap_or(r.i("was") as u16)); n += 1; } } }
    note(json!({ "webUiLinks": [] }));
    Ok(n)
}
