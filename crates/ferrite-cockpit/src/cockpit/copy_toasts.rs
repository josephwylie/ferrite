//! What the window says of a copy (`Copies`) away from its card: a toast
//! that follows one under way and can be closed without stopping it, a
//! toast for how it ended, and as it ends the chime and, with the window
//! in the background, the system's notification (Settings › Notifications).
//! Each reads the copy as it is drawn; none keeps a copy's state.

use super::copies::words::{bytes, count, took};
use super::copies::{CopyKey, Landed, Phase};
use super::*;
use gpui::component::button::Button;
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
    /// Stopped, or found its destination taken: nothing was added.
    Failed,
}

#[derive(Default)]
pub(super) struct CopyToasts {
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
}

impl CockpitView {
    /// Hear the copies, and do what each that moved asks of the window.
    pub(super) fn poll_copies(&mut self, cx: &mut Context<Self>) {
        let landed = self.copies.update(cx, |copies, cx| copies.poll(cx));
        for landed in landed {
            self.copy_landed(landed, cx);
        }
    }

    fn copy_landed(&mut self, landed: Landed, cx: &mut Context<Self>) {
        match landed {
            Landed::Sent(key) => self.place_copy(&key, cx),
            Landed::Ended(key) => self.tell_copy(&key, cx),
        }
    }

    /// The copy of `key` again (`Copies::again`): one whose files all
    /// landed is added at once.
    pub(super) fn copy_again(&mut self, key: &CopyKey, cx: &mut Context<Self>) {
        if let Some(landed) = self.copies.update(cx, |copies, cx| copies.again(key, cx)) {
            self.copy_landed(landed, cx);
        }
    }

    /// The copy of `key` ended: the chime, and with the window in the
    /// background, the system's notification, card or no card.
    pub(super) fn tell_copy(&mut self, key: &CopyKey, cx: &mut Context<Self>) {
        let Some(copy) = self.copies.read(cx).get(key) else {
            return;
        };
        let done = matches!(copy.phase, Phase::Done(_));
        let detail = match &copy.phase {
            Phase::Done(summary) => match summary.skipped.entries {
                0 => format!("{} in {}", bytes(summary.bytes), took(summary.took)),
                entries => not_copied(entries),
            },
            _ => copy.why().unwrap_or_default(),
        };
        let title = format!("Copy of {} to {}", copy.order.placing.title, copy.order.placing.machine_name);
        if self.prefs.settings.sounds {
            self.chime(if done { Sound::Done } else { Sound::NeedsYou });
        }
        let window_active = self.cockpit.read(cx).window_active();
        if self.prefs.settings.system_notifications && crate::system_notifications::wanted(true, window_active) {
            crate::system_notifications::post(
                crate::system_notifications::Posting {
                    tag: key.tag(),
                    needs_you: !done,
                    word: if done { "done" } else { "failed" },
                    title,
                    detail,
                    agent: None,
                    cwd: None,
                },
                cx,
            );
            self.copy_toasts.posted.insert(key.clone());
        }
    }

    /// A click on a copy's system notification: its card.
    pub(super) fn open_copy_notification(&mut self, tag: &str, cx: &mut Context<Self>) -> bool {
        let key = self
            .copies
            .read(cx)
            .iter()
            .map(|(key, _)| key)
            .find(|key| key.tag() == tag)
            .cloned();
        match key {
            Some(key) => {
                self.show_copy(&key, cx);
                true
            }
            None => false,
        }
    }

    /// Each copy's toast as it stands: none for the copy on the card up,
    /// one that follows it while it runs unless the operator closed that,
    /// and one for how it ended until the operator has seen it. A system
    /// notification is taken back once the window, active, shows what it
    /// told.
    pub(super) fn present_copies(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let carded = self.placement_sheet(cx).map(|sheet| sheet.read(cx).key());
        let window_active = self.cockpit.read(cx).window_active();
        let copies = self.copies.read(cx);
        let toasts = &mut self.copy_toasts;
        let running: HashSet<&CopyKey> = copies
            .iter()
            .filter(|(key, copy)| copy.phase.live() && carded.as_ref() != Some(*key))
            .map(|(key, _)| key)
            .collect();
        toasts.closed.retain(|key| running.contains(key));
        let wanted: HashMap<CopyKey, Kind> = copies
            .iter()
            .filter(|(key, _)| carded.as_ref() != Some(*key))
            .filter_map(|(key, copy)| {
                let kind = match &copy.phase {
                    phase if phase.live() => (!toasts.closed.contains(key)).then_some(Kind::Progress)?,
                    Phase::Done(_) => Kind::Done,
                    Phase::Failed { .. } | Phase::Occupied { .. } => Kind::Failed,
                    // Cancelled on its card: the operator saw it stop.
                    _ => return None,
                };
                Some((key.clone(), kind))
            })
            .collect();
        let seen: Vec<CopyKey> = toasts
            .posted
            .iter()
            .filter(|key| match copies.get(key) {
                Some(copy) if !copy.phase.live() => {
                    let shown = carded.as_ref() == Some(*key)
                        || matches!(wanted.get(*key), Some(Kind::Done | Kind::Failed));
                    window_active && shown
                }
                // Gone, or under way again.
                _ => true,
            })
            .cloned()
            .collect();
        let gone: Vec<CopyKey> = toasts
            .up
            .keys()
            .filter(|key| !wanted.contains_key(*key))
            .cloned()
            .collect();
        for key in gone {
            toasts.up.remove(&key);
            window.remove_notification1::<CopyToast>(key.tag(), cx);
        }
        for key in seen {
            toasts.posted.remove(&key);
            crate::system_notifications::withdraw(&key.tag(), cx);
        }
        let fresh: Vec<(CopyKey, Kind)> = wanted
            .into_iter()
            .filter(|(key, kind)| toasts.up.get(key).map(|(up, _)| up) != Some(kind))
            .collect();
        for (key, kind) in fresh {
            self.copy_toasts.pushed += 1;
            let pushed = self.copy_toasts.pushed;
            self.copy_toasts.up.insert(key.clone(), (kind, pushed));
            if let Some(toast) = self.copy_toast(&key, kind, pushed, cx) {
                window.push_notification(toast, cx);
            }
        }
    }

    /// The toast for the copy of `key`, the `pushed`th.
    fn copy_toast(&self, key: &CopyKey, kind: Kind, pushed: u64, cx: &mut Context<Self>) -> Option<Notification> {
        let copy = self.copies.read(cx).get(key)?;
        let (title, machine) = (&copy.order.placing.title, &copy.order.placing.machine_name);
        let view = cx.weak_entity();
        let closed = (view.clone(), key.clone());
        let toast = Notification::new()
            .id1::<CopyToast>(key.tag())
            .on_close(move |_, cx| {
                let (view, key) = &closed;
                view.update(cx, |view, cx| view.copy_toast_closed(key, pushed, cx))
                    .ok();
            });
        let shown = (view.clone(), key.clone());
        let show = move |_: &mut Notification, _: &mut Window, _: &mut Context<Notification>| {
            let (view, key) = shown.clone();
            Button::new("copy-toast-show")
                .debug_selector(|| "copy-toast-show".into())
                .label("Show")
                .on_click(move |_, _, cx| {
                    view.update(cx, |view, cx| view.show_copy(&key, cx)).ok();
                })
        };
        Some(match kind {
            Kind::Progress => {
                let copies = self.copies.downgrade();
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
                // What it left is on the card: the toast says how many.
                let toast = match summary.skipped.entries {
                    0 => toast.message(format!("{} in {}", bytes(summary.bytes), took(summary.took))),
                    entries => toast.message(not_copied(entries)).action(show),
                };
                if summary.moved {
                    toast
                } else {
                    let (project, key) = (summary.project, key.clone());
                    toast.action(move |_, _, _| {
                        let (view, key) = (view.clone(), key.clone());
                        Button::new("copy-toast-chat")
                            .debug_selector(|| "copy-toast-chat".into())
                            .label("Start a chat there")
                            .on_click(move |_, _, cx| {
                                view.update(cx, |view, cx| {
                                    view.open_draft_in_project(project, cx);
                                    view.copies.update(cx, |copies, cx| copies.forget(&key, cx));
                                })
                                .ok();
                            })
                    })
                }
            }
            Kind::Failed => {
                let again = match &copy.phase {
                    Phase::Failed { again, .. } => Some(again.verb()),
                    _ => None,
                };
                let toast = toast
                    .with_type(NotificationType::Warning)
                    .title(format!("Copy of {title} to {machine} stopped"))
                    .message(copy.why().unwrap_or_default())
                    .action(show);
                match again {
                    Some(verb) => {
                        let key = key.clone();
                        toast.action(move |_, _, _| {
                            let (view, key) = (view.clone(), key.clone());
                            Button::new("copy-toast-again")
                                .debug_selector(|| "copy-toast-again".into())
                                .label(verb)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |view, cx| view.copy_again(&key, cx)).ok();
                                })
                        })
                    }
                    None => toast,
                }
            }
        })
    }

    /// The operator closed the `pushed`th toast of the copy of `key`: the
    /// copy runs on unfollowed, or how it ended has been seen.
    fn copy_toast_closed(&mut self, key: &CopyKey, pushed: u64, cx: &mut Context<Self>) {
        let Some((kind, up)) = self.copy_toasts.up.get(key).copied() else {
            return;
        };
        if up != pushed {
            return;
        }
        self.copy_toasts.up.remove(key);
        match kind {
            Kind::Progress => {
                self.copy_toasts.closed.insert(key.clone());
            }
            Kind::Done | Kind::Failed => {
                self.copies.update(cx, |copies, cx| copies.forget(key, cx));
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
fn not_copied(entries: usize) -> String {
    match entries {
        1 => "1 item not copied".into(),
        entries => format!("{} items not copied", count(entries)),
    }
}
