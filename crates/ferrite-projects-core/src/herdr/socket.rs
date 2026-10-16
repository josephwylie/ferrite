//! herdr's socket, spoken to directly, for the two things its command line has not got (herdr 0.9.3): closing its
//! popup (`popup.close`), and being told the moment another tab shows (`events.subscribe`). One JSON line asks, one
//! answers; a subscription keeps its connection and gets a line per event.
use super::cli::{current_socket, fake, HRes};
use crate::json::*;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

fn request(method: &str, params: &Value) -> String { format!("{}\n", json!({ "id": "herdr-projects", "method": method, "params": params })) }
/// One request: its result, or what herdr refused it with (`code`: "popup_not_open").
pub fn ask(method: &str, params: &Value, socket: Option<&str>) -> HRes<Value> {
    if let Some(f) = fake() { return f.ask(method, params); }
    super::ferrite::ask(method, params, socket) // Ferrite: herdr's socket requests go to Ferrite's Server
}
/// herdr's popup, whoever opened it. False when none was open.
pub fn popup_close(socket: Option<&str>) -> HRes<bool> {
    match ask("popup.close", &json!({}), socket) { Ok(_) => Ok(true), Err(e) if e.code == "popup_not_open" => Ok(false), Err(e) => Err(e) }
}
/// Told each time herdr shows another tab: `on(tab id)` on a thread of its own. `started`: called once herdr has taken
/// the subscription. A connection that fails or ends is left at that.
pub fn on_tab_shown(socket: Option<String>, on: impl Fn(String) + Send + 'static, started: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        let path = socket.unwrap_or_else(current_socket);
        let Ok(mut s) = UnixStream::connect(&path) else { return };
        if s.write_all(request("events.subscribe", &json!({ "subscriptions": [{ "type": "tab.focused" }] })).as_bytes()).is_err() { return; }
        let mut started = Some(started);
        for line in BufReader::new(s).lines() {
            let Ok(line) = line else { return };
            let Ok(j) = serde_json::from_str::<Value>(&line) else { continue };
            if j.s("event") == "tab_focused" && j.at("data").b("tab_id") { on(j.at("data").t("tab_id")); }
            else if j.at("result").s("type") == "subscription_started" { if let Some(f) = started.take() { f(); } }
        }
    });
}
