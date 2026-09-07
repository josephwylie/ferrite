//! One latest-wins native fuzzy-file request for a Codex Session.
use std::io;
use std::sync::mpsc::SyncSender;

use serde_json::Value;

use crate::providers::FileSuggestion;

#[derive(Default)]
pub(super) struct Requests {
    token: String,
    pending: Option<(u64, SyncSender<io::Result<Vec<FileSuggestion>>>)>,
}

impl Requests {
    pub fn configure(&mut self, thread: &str) {
        self.token = format!("ferrite:file-search:{thread}");
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn begin(&mut self, id: u64, reply: SyncSender<io::Result<Vec<FileSuggestion>>>) {
        if let Some((_, sender)) = self.pending.replace((id, reply)) {
            let _ = sender.send(Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "file search superseded",
            )));
        }
    }

    pub fn discard(&mut self, id: u64) {
        if self.pending.as_ref().map(|(pending, _)| *pending) == Some(id) {
            self.pending = None;
        }
    }

    /// Returns true only for a reply belonging to this module, before ordinary
    /// request, catalog, and activity decoding sees it.
    pub fn observe(&mut self, frame: &Value) -> bool {
        if frame.get("method").is_some()
            || (frame.get("result").is_none() && frame.get("error").is_none())
        {
            return false;
        }
        let Some(id) = frame["id"].as_u64() else {
            return false;
        };
        if self.pending.as_ref().map(|(pending, _)| *pending) != Some(id) {
            return false;
        }
        let (_, sender) = self.pending.take().expect("matched above");
        let result = frame
            .get("error")
            .map(native_error)
            .unwrap_or_else(|| parse(&frame["result"]));
        let _ = sender.send(result);
        true
    }

    pub fn disconnect(&mut self) {
        if let Some((_, sender)) = self.pending.take() {
            let _ = sender.send(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Codex closed before file search arrived",
            )));
        }
    }
}

fn native_error(error: &Value) -> io::Result<Vec<FileSuggestion>> {
    Err(io::Error::other(
        error["message"]
            .as_str()
            .unwrap_or("Codex rejected file search"),
    ))
}

fn parse(result: &Value) -> io::Result<Vec<FileSuggestion>> {
    result["files"]
        .as_array()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Codex file search missing files",
            )
        })?
        .iter()
        .map(|file| {
            let _root = file["root"].as_str().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "Codex file search root missing")
            })?;
            let path = file["path"].as_str().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "Codex file search path missing")
            })?;
            let match_type = file["match_type"].as_str().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Codex file search match type missing",
                )
            })?;
            let _file_name = file["file_name"].as_str().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Codex file search file name missing",
                )
            })?;
            if !file["score"].is_number() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Codex file search score missing",
                ));
            }
            let matched = match file.get("indices") {
                None | Some(Value::Null) => Vec::new(),
                Some(Value::Array(indices)) => indices
                    .iter()
                    .map(|index| {
                        index
                            .as_u64()
                            .and_then(|index| usize::try_from(index).ok())
                            .ok_or_else(|| {
                                io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    "Codex file search index invalid",
                                )
                            })
                    })
                    .collect::<io::Result<Vec<_>>>()?,
                Some(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Codex file search indices invalid",
                    ));
                }
            };
            Ok(FileSuggestion {
                path: path.to_owned(),
                is_directory: match_type == "directory",
                matched,
            })
        })
        .collect()
}
