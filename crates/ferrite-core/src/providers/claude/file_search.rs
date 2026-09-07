//! One latest-wins native file suggestion request for a Claude Session.
use std::io;
use std::sync::mpsc::SyncSender;

use serde_json::Value;

use crate::providers::FileSuggestion;

#[derive(Default)]
pub(super) struct Requests {
    pending: Option<(String, SyncSender<io::Result<Vec<FileSuggestion>>>)>,
}

impl Requests {
    pub fn begin(&mut self, id: String, reply: SyncSender<io::Result<Vec<FileSuggestion>>>) {
        if let Some((_, sender)) = self.pending.replace((id, reply)) {
            let _ = sender.send(Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "file search superseded",
            )));
        }
    }

    pub fn discard(&mut self, id: &str) {
        if self.pending.as_ref().map(|(pending, _)| pending.as_str()) == Some(id) {
            self.pending = None;
        }
    }

    /// Returns true only for a reply belonging to this module, so its control
    /// envelope cannot reach ordinary event decoding.
    pub fn observe(&mut self, frame: &Value) -> bool {
        if frame["type"] != "control_response" {
            return false;
        }
        let Some(id) = frame["response"]["request_id"].as_str() else {
            return false;
        };
        if self.pending.as_ref().map(|(pending, _)| pending.as_str()) != Some(id) {
            return false;
        }
        let (_, sender) = self.pending.take().expect("matched above");
        let response = &frame["response"];
        let result = if response["subtype"] == "success" {
            parse(&response["response"])
        } else {
            Err(io::Error::other(
                response["error"]
                    .as_str()
                    .unwrap_or("Claude rejected file suggestions"),
            ))
        };
        let _ = sender.send(result);
        true
    }

    pub fn disconnect(&mut self) {
        if let Some((_, sender)) = self.pending.take() {
            let _ = sender.send(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Claude closed before file suggestions arrived",
            )));
        }
    }
}

fn parse(response: &Value) -> io::Result<Vec<FileSuggestion>> {
    response["suggestions"]
        .as_array()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Claude file suggestions missing",
            )
        })?
        .iter()
        .map(|suggestion| {
            let path = suggestion["path"].as_str().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Claude file suggestion path missing",
                )
            })?;
            Ok(FileSuggestion {
                path: path.to_owned(),
                is_directory: path.ends_with('/'),
                matched: Vec::new(),
            })
        })
        .collect()
}
