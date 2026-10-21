//! The herdr web UI plugin (devswha.herdr-web-ui): whether it is installed in this herdr, and the address its server
//! listens on. Read from herdr and from the web UI's own files, never written: its settings (`PORT`, `HOST` in the
//! `env` or `.env` of its config folder) and the port a start of it settled on (`plugin-port`). Checked on
//! herdr-web-ui 0.4.8.
use crate::herdr::Herdr;
use crate::json::*;
use std::sync::Mutex;

pub const WEB_UI: &str = "devswha.herdr-web-ui";
pub const DEFAULT_PORT: u16 = 7317;
#[derive(Clone, Debug, PartialEq, Default)]
pub struct WebUi { pub host: String, pub port: u16, pub socket: String, pub version: String, pub named: bool }
fn env_files(dir: &str) -> std::collections::HashMap<String, String> {
    let mut vars = std::collections::HashMap::new();
    for name in ["env", ".env"] {
        let Ok(text) = std::fs::read_to_string(std::path::Path::new(dir).join(name)) else { continue };
        for line in text.lines() {
            let t = line.trim();
            let Some(eq) = t.find('=') else { continue };
            if t.starts_with('#') || eq == 0 { continue; }
            vars.insert(t[..eq].trim().to_string(), t[eq + 1..].trim().trim_matches(|c| c == '"' || c == '\'').to_string());
        }
    }
    vars
}
/// Where the web UI listens and which herdr it shows, from its plugin's config folder.
pub fn address_from(config_dir: &str) -> WebUi {
    let vars = if config_dir.is_empty() { Default::default() } else { env_files(config_dir) };
    let host = vars.get("HOST").cloned().unwrap_or_else(|| "127.0.0.1".into());
    // Its herdr, as the web UI's own start works it out: the startup socket herdr hands every plugin wins.
    let socket = vars.get("HERDR_SOCKET_PATH").cloned().or_else(|| std::env::var("HERDR_SOCKET_PATH").ok()).unwrap_or_else(|| vars.get("HERDR_SOCKET").cloned().or_else(|| std::env::var("HERDR_SOCKET").ok()).unwrap_or_default());
    if let Some(p) = vars.get("PORT").and_then(|p| p.parse::<u16>().ok()).filter(|p| *p > 0) { return WebUi { host, port: p, socket, ..Default::default() }; }
    let state = vars.get("HERDR_WEB_STATE_DIR").cloned().or_else(|| std::env::var("HERDR_WEB_STATE_DIR").ok()).unwrap_or_else(|| std::env::var("XDG_CONFIG_HOME").map(std::path::PathBuf::from).unwrap_or_else(|_| crate::store::files::user_home().join(".config")).join("herdr-web-ui").display().to_string());
    let saved = std::fs::read_to_string(std::path::Path::new(&state).join("plugin-port")).ok().and_then(|s| s.trim().parse::<u16>().ok()).filter(|p| *p > 0);
    WebUi { host, port: saved.unwrap_or(DEFAULT_PORT), socket, ..Default::default() }
}
/// A web UI named by hand (`HERDR_PROJECTS_WEBUI=17431`, or `host:port`): one herdr does not list. `off`: none.
fn named(v: &str) -> Option<Option<WebUi>> {
    let v = v.trim();
    if v.is_empty() { return None; }
    if v.eq_ignore_ascii_case("off") { return Some(None); }
    let (host, port) = match v.rfind(':') { Some(at) => (&v[..at], &v[at + 1..]), None => ("127.0.0.1", v) };
    Some(port.parse::<u16>().ok().filter(|p| *p > 0).map(|port| WebUi { host: host.into(), port, named: true, ..Default::default() }))
}
static KEPT: Mutex<(i64, Option<WebUi>)> = Mutex::new((0, None));
/// The web UI's address, or None when there is none to stand in front of. Asking herdr starts a program, so the answer
/// is kept for a few seconds.
pub fn web_ui(fresh: bool) -> Option<WebUi> {
    let now = crate::clock::now();
    { let k = KEPT.lock().unwrap(); if !fresh && k.0 > 0 && now - k.0 < 15000 { return k.1.clone(); } }
    let v = look();
    *KEPT.lock().unwrap() = (now, v.clone());
    v
}
fn look() -> Option<WebUi> {
    // Off by the setting, or taken out with `remove-extras` (until `web-ui link` asks for it again).
    if crate::store::settings::lookup::setting("board.webUi", None) == json!("none") || crate::extras::extras().s("webUi") == "removed" { return None; }
    if let Some(by_hand) = named(&std::env::var("HERDR_PROJECTS_WEBUI").unwrap_or_default()) { return by_hand; }
    let h = Herdr::new(None);
    let p = h.plugins().into_iter().find(|p| p.s("plugin_id") == WEB_UI && p.at("enabled") != &json!(false))?;
    let mut w = address_from(&h.plugin_config_dir(WEB_UI));
    w.version = p.t("version");
    Some(w)
}
