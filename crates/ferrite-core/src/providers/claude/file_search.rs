//! One latest-wins native file suggestion request for a Claude Session,
//! and the one a Session makes as it starts to build the CLI's file index.
use std::io;

use serde_json::Value;

use crate::prompt_files::FileSuggestion;
use crate::providers::FileSearchReply;

#[derive(Default)]
pub(super) struct Requests {
    pending: Option<(String, FileSearchReply)>,
    /// The request that builds the index (`warm`), whose answer nobody reads.
    warming: Option<String>,
}

impl Requests {
    pub fn begin(&mut self, id: String, reply: FileSearchReply) {
        if let Some((_, sender)) = self.pending.replace((id, reply)) {
            let _ = sender.send(Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "file search superseded",
            )));
        }
    }

    /// Request `id` only builds the CLI's file index: its answer is dropped.
    pub fn warm(&mut self, id: String) {
        self.warming = Some(id);
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
        if self.warming.as_deref() == Some(id) {
            self.warming = None;
            return true;
        }
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
