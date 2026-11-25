//! What the window says of a copy away from its card: a toast that
//! follows one under way and can be closed without stopping it, and a
//! toast for how it ended. Each reads the copy as it is drawn; none keeps
//! a copy's state. A toast for a copy done goes by itself unless it left
//! something as it was; one for a copy that stopped stays until it is
//! closed. A copy is remembered while it leaves the operator something to
//! do or see: the operator closing its toast lets it go, and so does its
//! toast going by itself once its draft moved there. Otherwise one that
//! went by itself leaves the copy, its system notification and its card
//! (the chip's menu lists it) as they were. What a toast's buttons ask of
//! the window is a `CopyEvent`.

use super::words::{bytes, count, took};
use super::*;
use ferrite_ui::components;
use gpui::component::notification::{Notification, NotificationProgress, NotificationType};
use gpui::component::WindowExt as _;
use std::collections::{HashMap, HashSet};

/// A copy's toast among the window's notifications.
struct CopyToast;

/// What a copy's toast says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Progress,
    Done,
    /// Stopped, or found its destination taken: nothing was added. Said
    /// again once its Machine is `back`.
    Failed { back: bool },
}

/// What a copy's toast asks of the window: its card, the copy again, or
/// a chat on the Project it made.
pub(crate) enum CopyEvent {
    Show(CopyKey),
    Again(CopyKey),
    Chat(CopyKey, ProjectKey),
}

impl EventEmitter<CopyEvent> for Copies {}

#[derive(Default)]
pub(super) struct Toasts {
    /// The toast up for each copy, and which one it is: the kit tells of
    /// every close, its own replacing too, and only the one up counts.
    up: HashMap<CopyKey, (Kind, u64)>,
    pushed: u64,
    /// Copies whose progress toast the operator closed: none comes back
    /// until the copy ends, or its card is shown again.
    closed: HashSet<CopyKey>,
    /// Copies whose end the system's notification told, until the
    /// operator has seen it in the window.
    posted: HashSet<CopyKey>,
    /// Copies done whose toast went by itself: none comes back while the
    /// copy stays as it is.
    lapsed: HashSet<CopyKey>,
}

impl Copies {
    /// The system's notification told how the copy of `key` ended: it is
    /// taken back once the window shows it (`present`).
    pub(crate) fn posted(&mut self, key: &CopyKey) {
        self.toasts.posted.insert(key.clone());
    }

    /// Whether the toast of how the copy of `key` ended went by itself:
    /// none offers what its card does any more.
    pub(crate) fn lapsed(&self, key: &CopyKey) -> bool {
        self.toasts.lapsed.contains(key)
    }

    /// The copy a system notification tagged `tag` told of.
    pub(crate) fn tagged(&self, tag: &str) -> Option<CopyKey> {
        self.iter().map(|(key, _)| key).find(|key| key.tag() == tag).cloned()
    }

    /// Each copy's toast as it stands: none for the copy on the card up
    /// (`carded`), one that follows it while it runs unless the operator
    /// closed that, and one for how it ended until the operator has seen
    /// it, said again once its Machine is back. A system notification is
    /// taken back once the window, active, shows what it told.
    pub(crate) fn present(
        &mut self,
        carded: Option<CopyKey>,
        window_active: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let running: HashSet<CopyKey> = self
            .iter()
            .filter(|(key, copy)| copy.phase.live() && carded.as_ref() != Some(*key))
            .map(|(key, _)| key.clone())
            .collect();
        self.toasts.closed.retain(|key| running.contains(key));
        let done: HashSet<CopyKey> = self
            .iter()
            .filter(|(_, copy)| matches!(copy.phase, Phase::Done(_)))
            .map(|(key, _)| key.clone())
            .collect();
        self.toasts.lapsed.retain(|key| done.contains(key));
        let wanted: HashMap<CopyKey, Kind> = self
            .iter()
            .filter(|(key, copy)| carded.as_ref() != Some(*key) && !copy.seen())
            .filter(|(key, _)| !self.toasts.lapsed.contains(*key))
            .filter_map(|(key, copy)| {
                let kind = match &copy.phase {
                    phase if phase.live() => (!self.toasts.closed.contains(key)).then_some(Kind::Progress)?,
                    Phase::Done(_) => Kind::Done,
                    Phase::Failed { when_back, .. } => Kind::Failed {
                        back: *when_back && self.back(key, cx),
                    },
                    Phase::Occupied { .. } => Kind::Failed { back: false },
                    // Cancelled on its card: the operator saw it stop.
                    _ => return None,
                };
                Some((key.clone(), kind))
            })
            .collect();
        let seen: Vec<CopyKey> = self
            .toasts
            .posted
            .iter()
            .filter(|key| match self.get(key) {
                Some(copy) if !copy.phase.live() => {
                    let shown = carded.as_ref() == Some(*key)
                        || matches!(wanted.get(*key), Some(Kind::Done | Kind::Failed { .. }));
                    copy.seen() || (window_active && shown)
                }
                // Gone, or under way again.
                _ => true,
            })
            .cloned()
            .collect();
        let gone: Vec<CopyKey> = self
            .toasts
            .up
            .keys()
            .filter(|key| !wanted.contains_key(*key))
            .cloned()
            .collect();
        for key in gone {
            self.toasts.up.remove(&key);
            window.remove_notification1::<CopyToast>(key.tag(), cx);
        }
        for key in seen {
            self.toasts.posted.remove(&key);
            crate::system_notifications::withdraw(&key.tag(), cx);
        }
        let fresh: Vec<(CopyKey, Kind)> = wanted
            .into_iter()
            .filter(|(key, kind)| self.toasts.up.get(key).map(|(up, _)| up) != Some(kind))
            .collect();
        for (key, kind) in fresh {
            self.toasts.pushed += 1;
            let pushed = self.toasts.pushed;
            self.toasts.up.insert(key.clone(), (kind, pushed));
            if let Some(toast) = self.toast(&key, kind, pushed, cx) {
                window.push_notification(toast, cx);
            }
        }
    }

    /// The toast for the copy of `key`, the `pushed`th.
    fn toast(&self, key: &CopyKey, kind: Kind, pushed: u64, cx: &mut Context<Self>) -> Option<Notification> {
        let copy = self.get(key)?;
        let (title, machine) = (&copy.order.placing.title, &copy.order.placing.machine_name);
        let copies = cx.weak_entity();
        let (dismissed, gone) = ((copies.clone(), key.clone()), (copies.clone(), key.clone()));
        let toast = Notification::new()
            .id1::<CopyToast>(key.tag())
            .on_dismiss(move |_, cx| {
                let (copies, key) = &dismissed;
                copies
                    .update(cx, |copies, cx| copies.toast_dismissed(key, pushed, cx))
                    .ok();
            })
            .on_close(move |_, cx| {
                let (copies, key) = &gone;
                copies
                    .update(cx, |copies, cx| copies.toast_gone(key, pushed, cx))
                    .ok();
            });
        // A button that asks `event` of the window.
        let asks = |copies: &gpui::WeakEntity<Self>, event: CopyEvent, cx: &mut App| {
            copies.update(cx, |_, cx| cx.emit(event)).ok();
        };
        let shown = (copies.clone(), key.clone());
        let show = move |_: &mut Notification, _: &mut Window, cx: &mut Context<Notification>| {
            let (copies, key) = shown.clone();
            components::secondary_button("copy-toast-show", None, "Show", cx)
                .debug_selector(|| "copy-toast-show".into())
                .on_click(move |_, _, cx| asks(&copies, CopyEvent::Show(key.clone()), cx))
        };
        Some(match kind {
            Kind::Progress => {
                let key = key.clone();
                toast
                    .title(format!("Copying {title} to {machine}"))
                    .autohide(false)
                    .progress(move |_, cx| {
                        let progress = copies.upgrade().and_then(|copies| {
                            let copy = copies.read(cx).get(&key)?;
                            Some(progress(&copy.phase, &copy.order.dest, &copy.order.placing.machine_name))
                        });
                        progress.unwrap_or(NotificationProgress {
                            value: None,
                            caption: SharedString::default(),
                        })
                    })
                    .action(show)
            }
            Kind::Done => {
                let Phase::Done(summary) = &copy.phase else {
                    return None;
                };
                let toast = toast
                    .with_type(NotificationType::Success)
                    .title(format!("Copied {title} to {machine}"));
                // What it left is on the card: the toast says how many, and
                // stays until it is closed.
                let left = summary.skipped.entries;
                let toast = match left {
                    0 => toast.message(format!("{} in {}", bytes(summary.bytes), took(summary.took))),
                    entries => toast.message(not_copied(entries)).action(show),
                };
                let toast = if summary.moved {
                    toast
                } else {
                    let (project, key) = (summary.project, key.clone());
                    toast.action(move |_, _, cx| {
                        let (copies, key) = (copies.clone(), key.clone());
                        ferrite_ui::form::primary_button("copy-toast-chat", "Start a chat there", false, cx)
                            .debug_selector(|| "copy-toast-chat".into())
                            .on_click(move |_, _, cx| asks(&copies, CopyEvent::Chat(key.clone(), project), cx))
                    })
                };
                // A press on it does not keep a toast that says all went
                // well: it goes as the kit's toasts go, its copy kept
                // (`toast_gone`).
                toast.autohide(left == 0)
            }
            Kind::Failed { back } => {
                let standing = copy.standing(back);
                let failed = standing.as_ref().is_none_or(|(_, failed)| *failed);
                let toast = toast
                    .with_type(if failed { NotificationType::Warning } else { NotificationType::Info })
                    .title(format!("Copy of {title} to {machine} stopped"))
                    .message(standing.map(|(said, _)| said).unwrap_or_default());
                // A copy whose Machine was removed has nothing to show or
                // do: only its close.
                let Some(verb) = copy.phase.again().map(|again| again.verb()) else {
                    return Some(toast);
                };
                let key = key.clone();
                toast.action(show).action(move |_, _, cx| {
                    let (copies, key) = (copies.clone(), key.clone());
                    components::secondary_button("copy-toast-again", None, verb, cx)
                        .debug_selector(|| "copy-toast-again".into())
                        .on_click(move |_, _, cx| asks(&copies, CopyEvent::Again(key.clone()), cx))
                })
            }
        })
    }

    /// The `pushed`th toast of the copy of `key` went by itself, a copy
    /// done's: none comes back. A copy whose draft moved there and that
    /// left nothing as it was leaves the operator nothing to do or see, so
    /// it goes with its toast, any system notification of it left to the
    /// system; one that offers a chat there, or lists what it left, stays
    /// to be seen, on its card.
    fn toast_gone(&mut self, key: &CopyKey, pushed: u64, cx: &mut Context<Self>) {
        if !self.toasts.up.get(key).is_some_and(|up| *up == (Kind::Done, pushed)) {
            return;
        }
        self.toasts.up.remove(key);
        let done_with = |phase: &Phase| matches!(phase, Phase::Done(summary) if summary.moved && summary.skipped.entries == 0);
        if self.get(key).is_some_and(|copy| done_with(&copy.phase)) {
            self.toasts.posted.remove(key);
            self.forget(key, cx);
        } else {
            self.toasts.lapsed.insert(key.clone());
        }
        cx.notify();
    }

    /// The operator closed the `pushed`th toast of the copy of `key`: the
    /// copy runs on unfollowed, or how it ended has been seen.
    fn toast_dismissed(&mut self, key: &CopyKey, pushed: u64, cx: &mut Context<Self>) {
        let Some((kind, up)) = self.toasts.up.get(key).copied() else {
            return;
        };
        if up != pushed {
            return;
        }
        self.toasts.up.remove(key);
        match kind {
            Kind::Progress => {
                self.toasts.closed.insert(key.clone());
            }
            Kind::Done | Kind::Failed { .. } => {
                self.forget(key, cx);
            }
        }
        cx.notify();
    }
}

/// A copy under way as its toast says it: the bar, and how far in words.
fn progress(phase: &Phase, dest: &str, machine: &str) -> NotificationProgress {
    let caption = match phase {
        &Phase::Listing { files: 0 } => "Listing files\u{2026}".to_string(),
        &Phase::Listing { files } => format!("Listing files\u{2026} {}", count(files)),
        Phase::Finishing { .. } => {
            let dest = dest.trim_end_matches(['/', '\\']);
            format!("Adding {} on {machine}", dest.rsplit(['/', '\\']).next().unwrap_or(dest))
        }
        Phase::Copying { reading, .. } => {
            let done = (phase.done().unwrap_or_default() * 100.).floor();
            match reading.left {
                Some(left) => format!("{done}% \u{b7} {left} left"),
                None => format!("{done}%"),
            }
        }
        _ => String::new(),
    };
    NotificationProgress {
        value: phase.done().map(|done| done * 100.),
        caption: caption.into(),
    }
}

/// How many entries a copy left as they were: `2 items not copied`.
pub(crate) fn not_copied(entries: usize) -> String {
    match entries {
        1 => "1 item not copied".into(),
        entries => format!("{} items not copied", count(entries)),
    }
}
