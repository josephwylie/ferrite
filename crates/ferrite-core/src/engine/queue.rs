//! Acknowledged native queue view and raw-input correlation; no dispatch logic.
use crate::{QueueEvent, QueuedPrompt};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, Write};
use std::path::PathBuf;

/// Where a prompt the operator queued stands on its way into a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Delivery {
    /// Ferrite holds it: no Session can take it yet.
    Holding,
    /// Handed to the provider, which has not acknowledged it yet.
    Sending,
    /// In the provider's queue, for after the running turn.
    Queued,
    /// Folded into the running turn at its next tool boundary (Codex).
    Steering,
    /// A state this build does not know, or a Server too old to say.
    #[serde(other)]
    Unknown,
}

/// One queued prompt, as the Composer shows it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QueueItem {
    /// Stable from the moment it is queued until a turn picks it up: the
    /// client id it is held and handed to the provider under. What cancel
    /// and edit name it by.
    pub id: String,
    pub text: String,
    pub delivery: Delivery,
}

/// A prompt Ferrite holds until its Session can take it, under the
/// client id it will be handed to the provider with.
pub(super) struct Held {
    pub id: String,
    pub text: String,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Metadata {
    pending: HashMap<String, String>,
    consumed: Vec<String>,
}
#[derive(Default)]
pub(super) struct Queue {
    pub items: Vec<QueuedPrompt>,
    metadata: Metadata,
    /// Client ids handed to the provider and not yet acknowledged, oldest
    /// first.
    admitting: Vec<String>,
    path: PathBuf,
    cancel: Option<(String, String, bool, bool)>,
    pub recovered: Vec<(String, bool)>,
    /// Native ids Ferrite let go of itself (an interrupt's resend); their
    /// late cancellation receipts are not the operator's news.
    dropped: HashSet<String>,
    /// Why the saved queue could not be read, until said once.
    unreadable: Option<String>,
}
pub(super) struct Change {
    pub prompt: Option<String>,
    pub historical: bool,
    pub notice: Option<String>,
}
impl Queue {
    /// The queue saved beside a Thread's log. A missing one is an empty
    /// queue; one that cannot be read starts empty too, but says so
    /// (`take_unreadable`) rather than drop what it held without a word.
    pub fn open(path: PathBuf) -> Self {
        let read = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| error.to_string()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Metadata::default()),
            Err(error) => Err(error.to_string()),
        };
        let (metadata, unreadable) = match read {
            Ok(metadata) => (metadata, None),
            Err(error) => (Metadata::default(), Some(error)),
        };
        Self {
            path,
            metadata,
            unreadable,
            ..Self::default()
        }
    }

    /// The Notice an unreadable saved queue owes the operator, once.
    pub fn take_unreadable(&mut self) -> Option<String> {
        self.unreadable.take().map(|error| {
            format!(
                "the queued prompts saved for this Thread could not be read ({error}); \
                 the queue starts empty"
            )
        })
    }
    /// Prompts handed to the provider's queue and not yet seen sent.
    pub fn holds_unsent(&self) -> bool {
        !self.metadata.pending.is_empty()
    }
    pub fn pending(&self) -> bool {
        !self.items.is_empty() || !self.admitting.is_empty()
    }
    pub fn prepare(&mut self, id: &str, text: &str) -> io::Result<()> {
        self.admitting.push(id.into());
        self.metadata.pending.insert(id.into(), text.into());
        self.save()
    }
    pub fn refused(&mut self, id: &str) {
        self.admitting.retain(|admitting| admitting != id);
        self.metadata.pending.remove(id);
        let _ = self.save();
    }
    /// Every prompt not yet picked up, newest first: what Ferrite holds,
    /// then what the provider is taking, then what it has queued.
    pub fn listing(&self, held: &VecDeque<Held>) -> Vec<QueueItem> {
        let held = held.iter().rev().map(|held| QueueItem {
            id: held.id.clone(),
            text: held.text.clone(),
            delivery: Delivery::Holding,
        });
        let sending = self
            .admitting
            .iter()
            .rev()
            .filter(|id| !self.items.iter().any(|item| &item.client_id == *id))
            .filter_map(|id| {
                Some(QueueItem {
                    id: id.clone(),
                    text: self.metadata.pending.get(id)?.clone(),
                    delivery: Delivery::Sending,
                })
            });
        let queued = self.items.iter().rev().map(|item| QueueItem {
            id: stable_id(item).into(),
            text: item.text.clone(),
            delivery: if crate::providers::is_steer_id(&item.id) {
                Delivery::Steering
            } else {
                Delivery::Queued
            },
        });
        held.chain(sending).chain(queued).collect()
    }
    /// Ask to take back the queued prompt `target` names, else the latest.
    /// Answers the provider's id to cancel it by.
    pub fn cancellation(
        &mut self,
        target: Option<&str>,
        restore: bool,
        prepend: bool,
    ) -> Option<String> {
        if self.cancel.is_some() {
            return None;
        }
        let item = match target {
            Some(target) => self.items.iter().find(|item| stable_id(item) == target)?,
            None => self.items.last()?,
        };
        self.cancel = Some((item.id.clone(), item.text.clone(), restore, prepend));
        Some(item.id.clone())
    }
    pub fn cancel_failed(&mut self) {
        self.cancel = None;
    }
    /// Take every mirrored prompt back from the provider, oldest first, so
    /// the caller can resend them as one: an interrupt's "send what I
    /// queued, now". Their client ids are marked consumed, so a late start
    /// notice for one of them cannot insert it a second time; the caller
    /// still owes the provider a cancellation per native id.
    pub fn flush(&mut self) -> Vec<QueuedPrompt> {
        self.cancel = None;
        let items = std::mem::take(&mut self.items);
        for item in &items {
            self.metadata.pending.remove(&item.client_id);
            self.metadata.consumed.push(item.client_id.clone());
            self.dropped.insert(item.id.clone());
        }
        while self.metadata.consumed.len() > 4096 {
            self.metadata.consumed.remove(0);
        }
        let _ = self.save();
        items
    }
    pub fn disconnect(&mut self, durable: bool) -> Option<String> {
        self.cancel = None;
        self.dropped.clear();
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
                self.admitting.retain(|id| *id != item.client_id);
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
                self.admitting.retain(|id| *id != client_id);
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
                if self.dropped.remove(&id) {
                    return change;
                }
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
                self.admitting.retain(|id| *id != client_id);
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
    /// Saved beside and renamed over, behind a barrier: the queue is whole
    /// on disk before the rename can be, without waiting on the drive's
    /// cache for every prompt queued (ADR 0009).
    fn save(&self) -> io::Result<()> {
        let temporary = self.path.with_extension("queue.tmp");
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(&serde_json::to_vec(&self.metadata)?)?;
        crate::store::sync_ordered(&file)?;
        std::fs::rename(temporary, &self.path)
    }
}

/// A provider-queued prompt's stable id: the client id it was handed over
/// under, or the provider's own for one Ferrite never handed over.
fn stable_id(item: &QueuedPrompt) -> &str {
    if item.client_id.is_empty() {
        &item.id
    } else {
        &item.client_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One id from held to queued; each stage says where the prompt stands.
    #[test]
    fn listing_names_each_prompt_by_one_id_and_its_delivery() {
        let path =
            std::env::temp_dir().join(format!("ferrite-queue-listing-{}.json", std::process::id()));
        let mut queue = Queue::open(path.clone());
        let held = VecDeque::from([Held {
            id: "held".into(),
            text: "later".into(),
        }]);
        queue.prepare("sending", "soon").unwrap();
        queue.prepare("steered", "now").unwrap();
        queue.observe(QueueEvent::Accepted(QueuedPrompt {
            id: "steer:steered".into(),
            client_id: "steered".into(),
            text: "now".into(),
        }));
        let delivery = |queue: &Queue| {
            queue
                .listing(&held)
                .into_iter()
                .map(|item| (item.id, item.delivery))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            delivery(&queue),
            [
                ("held".into(), Delivery::Holding),
                ("sending".into(), Delivery::Sending),
                ("steered".into(), Delivery::Steering),
            ]
        );
        queue.observe(QueueEvent::Accepted(QueuedPrompt {
            id: "native".into(),
            client_id: "sending".into(),
            text: "soon".into(),
        }));
        assert_eq!(
            delivery(&queue),
            [
                ("held".into(), Delivery::Holding),
                ("sending".into(), Delivery::Queued),
                ("steered".into(), Delivery::Steering),
            ],
            "acknowledged, it keeps its id"
        );
        assert_eq!(
            queue.cancellation(Some("steered"), true, false).as_deref(),
            Some("steer:steered"),
            "cancel takes the prompt it names, not the latest"
        );
        let _ = std::fs::remove_file(path);
    }
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
        assert_eq!(
            queue.cancellation(None, true, false).as_deref(),
            Some("native")
        );
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
