//! Acknowledged native queue view and raw-input correlation; no dispatch logic.
use crate::{QueueEvent, QueuedPrompt};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Metadata {
    pending: HashMap<String, String>,
    consumed: Vec<String>,
}
#[derive(Default)]
pub(super) struct Queue {
    pub items: Vec<QueuedPrompt>,
    metadata: Metadata,
    admitting: HashSet<String>,
    path: PathBuf,
    cancel: Option<(String, String, bool, bool)>,
    pub recovered: Vec<(String, bool)>,
}
pub(super) struct Change {
    pub prompt: Option<String>,
    pub historical: bool,
    pub notice: Option<String>,
}
impl Queue {
    pub fn open(path: PathBuf) -> Self {
        let metadata = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            metadata,
            ..Self::default()
        }
    }
    pub fn pending(&self) -> bool {
        !self.items.is_empty() || !self.admitting.is_empty()
    }
    pub fn prepare(&mut self, id: &str, text: &str) -> io::Result<()> {
        self.admitting.insert(id.into());
        self.metadata.pending.insert(id.into(), text.into());
        self.save()
    }
    pub fn refused(&mut self, id: &str) {
        self.admitting.remove(id);
        self.metadata.pending.remove(id);
        let _ = self.save();
    }
    pub fn cancellation(&mut self, restore: bool, prepend: bool) -> Option<String> {
        if self.cancel.is_some() {
            return None;
        }
        let item = self.items.last()?;
        self.cancel = Some((item.id.clone(), item.text.clone(), restore, prepend));
        Some(item.id.clone())
    }
    pub fn cancel_failed(&mut self) {
        self.cancel = None;
    }
    pub fn disconnect(&mut self, durable: bool) -> Option<String> {
        self.cancel = None;
        self.admitting.clear();
        self.items.clear();
        if durable || self.metadata.pending.is_empty() {
            return None;
        }
        let texts = self
            .metadata
            .pending
            .values()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n");
        self.metadata.pending.clear();
        let _ = self.save();
        Some(format!("Claude Session ended with pending input; delivery is uncertain and was not retried:\n{texts}"))
    }
    fn raw(&self, item: &mut QueuedPrompt) {
        if let Some(text) = self.metadata.pending.get(&item.client_id) {
            item.text = text.clone();
        }
    }
    pub fn observe(&mut self, event: QueueEvent) -> Change {
        let mut change = Change {
            prompt: None,
            historical: false,
            notice: None,
        };
        match event {
            QueueEvent::Snapshot(mut items) => {
                items.retain(|item| !self.metadata.consumed.contains(&item.client_id));
                for item in &mut items {
                    self.raw(item);
                }
                self.items = items;
            }
            QueueEvent::Accepted(mut item) => {
                self.admitting.remove(&item.client_id);
                if !self.metadata.consumed.contains(&item.client_id) {
                    self.raw(&mut item);
                    if let Some(old) = self.items.iter_mut().find(|old| old.id == item.id) {
                        *old = item;
                    } else {
                        self.items.push(item);
                    }
                }
            }
            QueueEvent::Started {
                client_id,
                text,
                historical,
            } => {
                if historical && !self.metadata.pending.contains_key(&client_id) {
                    return change;
                }
                change.historical = historical;
                self.admitting.remove(&client_id);
                if !self.metadata.consumed.contains(&client_id) {
                    change.prompt = self
                        .metadata
                        .pending
                        .remove(&client_id)
                        .or_else(|| {
                            self.items
                                .iter()
                                .find(|item| item.client_id == client_id)
                                .map(|item| item.text.clone())
                        })
                        .or(text);
                    self.metadata.consumed.push(client_id.clone());
                    if self.metadata.consumed.len() > 4096 {
                        self.metadata.consumed.remove(0);
                    }
                }
                self.items.retain(|item| item.client_id != client_id);
            }
            QueueEvent::Removed { id } => {
                // Cancellation lifecycle can precede its control response.
                if let Some(item) = self.items.iter().find(|item| item.id == id) {
                    self.metadata.pending.remove(&item.client_id);
                }
                self.items.retain(|item| item.id != id);
            }
            QueueEvent::Cancelled {
                id,
                cancelled,
                error,
            } => {
                if let Some((pending, text, restore, prepend)) = self.cancel.take() {
                    if pending == id && cancelled {
                        if restore {
                            self.recovered.push((text, prepend));
                        }
                        if let Some(item) = self.items.iter().find(|item| item.id == id) {
                            self.metadata.pending.remove(&item.client_id);
                        }
                        self.items.retain(|item| item.id != id);
                    } else if pending != id {
                        self.cancel = Some((pending, text, restore, prepend));
                    }
                }
                change.notice = error.or_else(|| (!cancelled).then(|| "Queued prompt already started or could not be cancelled; it was not restored.".into()));
            }
            QueueEvent::Failed { client_id, error } => {
                self.admitting.remove(&client_id);
                let text = self.metadata.pending.remove(&client_id).unwrap_or_default();
                self.items.retain(|item| item.client_id != client_id);
                change.notice = Some(format!("native queue failed: {error}\n{text}"));
            }
        }
        if let Err(error) = self.save() {
            change.notice = Some(format!("queue metadata could not be saved: {error}"));
        }
        change
    }
    fn save(&self) -> io::Result<()> {
        let temporary = self.path.with_extension("queue.tmp");
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(&serde_json::to_vec(&self.metadata)?)?;
        file.sync_data()?;
        std::fs::rename(temporary, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_race_never_restores_started_input_and_snapshot_owns_pending() {
        let path =
            std::env::temp_dir().join(format!("ferrite-native-mirror-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut queue = Queue::open(path.clone());
        assert!(queue
            .observe(QueueEvent::Started {
                historical: true,
                client_id: "external-history".into(),
                text: Some("already imported".into())
            })
            .prompt
            .is_none());
        queue.prepare("client", "raw\n@/tmp/image.png").unwrap();
        queue.observe(QueueEvent::Accepted(QueuedPrompt {
            id: "native".into(),
            client_id: "client".into(),
            text: "wire".into(),
        }));
        assert_eq!(queue.cancellation(true, false).as_deref(), Some("native"));
        queue.observe(QueueEvent::Snapshot(Vec::new()));
        assert!(
            !queue.pending(),
            "raw metadata is not another execution queue"
        );
        let started = queue.observe(QueueEvent::Started {
            historical: false,
            client_id: "client".into(),
            text: Some("wire".into()),
        });
        assert_eq!(started.prompt.as_deref(), Some("raw\n@/tmp/image.png"));
        queue.observe(QueueEvent::Cancelled {
            id: "native".into(),
            cancelled: false,
            error: None,
        });
        assert!(queue.recovered.is_empty());
        queue.prepare("crash-pending", "raw recovered").unwrap();
        let mut reopened = Queue::open(path.clone());
        let recovered = reopened.observe(QueueEvent::Started {
            historical: true,
            client_id: "crash-pending".into(),
            text: Some("wire".into()),
        });
        assert_eq!(recovered.prompt.as_deref(), Some("raw recovered"));
        assert!(recovered.historical);
        assert!(reopened
            .observe(QueueEvent::Started {
                historical: false,
                client_id: "client".into(),
                text: Some("wire".into())
            })
            .prompt
            .is_none());
        let _ = std::fs::remove_file(path);
    }
}
