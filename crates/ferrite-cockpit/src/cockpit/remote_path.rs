//! A directory on a machine typed into a sheet's field (ADR 0026), where no
//! folder dialog can browse: what is typed completes against the directory
//! it names there, listed by the machine's Server (`list_dir`). The Project
//! card's path field and the setup sheet's destination are each one.

use super::*;
use ferrite_core::folder::Listing;
use gpui::App;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

pub(super) struct RemotePath {
    pub(super) field: Entity<Composer>,
    /// The listing the completions come from, of the directory as typed:
    /// empty when it could not be read, which is not asked again.
    listed: Option<(String, Listing)>,
    /// A listing asked for and not answered yet, of the directory as typed.
    asking: Option<(String, Receiver<std::io::Result<Listing>>)>,
}

/// How many completions the field lists under it.
const COMPLETIONS: usize = 8;

/// A typed path's directory, to list, and the start of the name in it, to
/// complete: `~/src/fe` is `~/src/` and `fe`.
fn split_typed(typed: &str) -> (&str, &str) {
    match typed.rfind('/') {
        Some(at) => typed.split_at(at + 1),
        None => ("", typed),
    }
}

impl RemotePath {
    /// A field holding `start`. Its owner hears each edit as the field's
    /// `Edited`.
    pub(super) fn new<V: 'static>(start: String, cx: &mut Context<V>) -> Self {
        let field = cx.new(Composer::new);
        field.update(cx, |field, cx| {
            field.set_role(ferrite_pane::composer::Role::Field(None), cx);
            field.set(start, cx);
        });
        Self {
            field,
            listed: None,
            asking: None,
        }
    }

    pub(super) fn typed<'a>(&self, cx: &'a App) -> &'a str {
        self.field.read(cx).text()
    }

    /// Ask `machine` for the directory the field names, unless its listing
    /// is in hand or on its way.
    pub(super) fn list(
        &mut self,
        model: &Entity<Cockpit>,
        machine: ferrite_core::MachineId,
        cx: &mut App,
    ) {
        let typed = self.typed(cx).to_string();
        let (directory, _) = split_typed(&typed);
        let known = |held: Option<&String>| held.is_some_and(|held| held == directory);
        if known(self.listed.as_ref().map(|(at, _)| at))
            || known(self.asking.as_ref().map(|(at, _)| at))
        {
            return;
        }
        self.asking = if directory.is_empty() {
            None
        } else {
            model
                .update(cx, |cockpit, _| {
                    cockpit.list_dir(machine, std::path::Path::new(directory))
                })
                .ok()
                .map(|answer| (directory.to_string(), answer))
        };
    }

    /// Take the machine's answer to the field's listing, once it lands; true
    /// when one did. A directory that cannot be read lists nothing; one
    /// whose answer the lost connection took is asked again at the next
    /// keystroke.
    pub(super) fn poll(&mut self) -> bool {
        let Some((directory, answer)) = &self.asking else {
            return false;
        };
        let listing = match answer.try_recv() {
            Err(TryRecvError::Empty) => return false,
            Ok(Ok(listing)) => Some(listing),
            Ok(Err(_)) => Some(Listing {
                path: directory.into(),
                entries: Vec::new(),
            }),
            Err(TryRecvError::Disconnected) => None,
        };
        if let Some(listing) = listing {
            self.listed = Some((directory.clone(), listing));
        }
        self.asking = None;
        true
    }

    /// The directories in the listing whose names start as the field's
    /// last name does; a dot directory only once a dot is typed.
    pub(super) fn completions(&self, cx: &App) -> Vec<String> {
        let (directory, start) = split_typed(self.typed(cx));
        let Some((_, listing)) = self
            .listed
            .as_ref()
            .filter(|(listed, _)| listed == directory)
        else {
            return Vec::new();
        };
        listing
            .entries
            .iter()
            .filter(|entry| {
                entry.directory
                    && entry.name.starts_with(start)
                    && (start.starts_with('.') || !entry.name.starts_with('.'))
            })
            .take(COMPLETIONS)
            .map(|entry| entry.name.clone())
            .collect()
    }

    /// Complete the field's last name to `name`, a directory: the field
    /// then lists what is in it.
    pub(super) fn complete(&self, name: &str, cx: &mut App) {
        let typed = self.typed(cx).to_string();
        let (directory, _) = split_typed(&typed);
        let completed = format!("{directory}{name}/");
        self.field.update(cx, |field, cx| field.set(completed, cx));
    }

    /// Tab in the field: complete it to the first directory that fits.
    /// False when the field does not hold the keyboard.
    pub(super) fn tab(&self, window: &Window, cx: &mut App) -> bool {
        if !self.focused(window, cx) {
            return false;
        }
        if let Some(first) = self.completions(cx).into_iter().next() {
            self.complete(&first, cx);
        }
        true
    }

    pub(super) fn focused(&self, window: &Window, cx: &App) -> bool {
        self.field.read(cx).focus_handle(cx).is_focused(window)
    }

    /// `path` as the machine's Server spelled it out in the listing in hand
    /// — of it, or of the directory it is in — so `~` reads as the
    /// machine's home and one directory typed two ways is one. As typed
    /// otherwise: the Server spells `~` out when it registers.
    pub(super) fn spelled_out(&self, path: &str) -> PathBuf {
        let (directory, leaf) = split_typed(path);
        match &self.listed {
            Some((listed, listing)) if listed.trim_end_matches('/') == path => listing.path.clone(),
            Some((listed, listing)) if listed == directory => listing.path.join(leaf),
            _ => path.into(),
        }
    }

    /// The field, labelled `label`, and under it the directories that
    /// complete it, each a press away (`complete`, the owner's way to
    /// reach this field).
    pub(super) fn element<V: 'static>(
        &self,
        label: SharedString,
        window: &Window,
        complete: fn(&mut V, &str, &mut Context<V>),
        cx: &mut Context<V>,
    ) -> Div {
        let focused = self.focused(window, cx);
        // Each its own flex row, so the control hugs its label.
        let completions = self
            .completions(cx)
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                div().flex().child(
                    crate::project_editor::path_completion(index, &name, cx).on_click(cx.listener(
                        move |view: &mut V, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            complete(view, &name, cx);
                        },
                    )),
                )
            });
        div()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(crate::project_editor::path_field(
                label,
                self.field.clone(),
                focused,
            ))
            .child(crate::project_editor::directory_list(completions))
    }
}
