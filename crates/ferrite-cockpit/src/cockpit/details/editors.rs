//! Installed editor choices for the context card. Discovery runs off the
//! UI thread; choosing a menu row saves a preference, and the primary
//! button opens that editor. File-manager opens stay with FileLink.

use super::*;
use gpui::component::{notification::Notification, WindowExt};
use std::path::PathBuf;

mod catalog;
mod discovery;
use catalog::{Definition, DEFINITIONS};
use discovery::{discover, launch};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::cockpit) struct EditorId(usize);

impl EditorId {
    const FILE_MANAGER: Self = Self(20);

    fn definition(self) -> &'static Definition {
        &DEFINITIONS[self.0]
    }
    pub(super) fn id(self) -> &'static str {
        self.definition().id
    }
    pub(super) fn label(self) -> &'static str {
        if self == Self::FILE_MANAGER {
            if cfg!(target_os = "macos") {
                "Finder"
            } else if cfg!(windows) {
                "File Explorer"
            } else {
                "Files"
            }
        } else {
            self.definition().label
        }
    }
    fn jetbrains(self) -> bool {
        (8..20).contains(&self.0)
    }

    pub(in crate::cockpit) fn image(self, size: f32) -> AnyElement {
        let asset = if self == Self::FILE_MANAGER {
            if cfg!(windows) {
                ferrite_ui::icons::EDITOR_FILE_EXPLORER
            } else if cfg!(target_os = "macos") {
                ferrite_ui::icons::EDITOR_FINDER
            } else {
                return ferrite_ui::icons::icon(ferrite_ui::icons::FOLDER, size, INK_SUBTLE)
                    .into_any_element();
            }
        } else {
            self.definition().asset
        };
        gpui::img(asset)
            .w(px(size))
            .h(px(size))
            .flex_shrink_0()
            .into_any_element()
    }
}

#[derive(Clone)]
struct InstalledEditor {
    id: EditorId,
    program: Option<PathBuf>,
    base_args: &'static [&'static str],
}

pub(in crate::cockpit) struct Editors {
    available: Vec<InstalledEditor>,
    discovered: bool,
    discovering: bool,
    failures: Vec<String>,
}

impl Default for Editors {
    fn default() -> Self {
        Self {
            available: if cfg!(target_os = "macos") || cfg!(windows) {
                vec![InstalledEditor {
                    id: EditorId::FILE_MANAGER,
                    program: None,
                    base_args: &[],
                }]
            } else {
                Vec::new()
            },
            discovered: false,
            discovering: false,
            failures: Vec::new(),
        }
    }
}

impl Editors {
    pub(super) fn preferred(&self, saved: Option<&str>) -> Option<EditorId> {
        self.available
            .iter()
            .find(|editor| Some(editor.id.id()) == saved)
            .or_else(|| self.available.first())
            .map(|editor| editor.id)
    }

    #[cfg(test)]
    pub(in crate::cockpit) fn with_ids(ids: &[&str]) -> Self {
        Self {
            available: DEFINITIONS
                .iter()
                .enumerate()
                .filter(|(_, definition)| ids.contains(&definition.id))
                .map(|(index, _)| InstalledEditor {
                    id: EditorId(index),
                    program: None,
                    base_args: &[],
                })
                .collect(),
            discovered: true,
            discovering: false,
            failures: Vec::new(),
        }
    }
}

impl CockpitView {
    pub(super) fn tend_details_editors(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.on_machine() && !self.details_editors.discovered {
            self.refresh_details_editors(cx);
        }
        for error in self.details_editors.failures.drain(..) {
            window.push_notification(Notification::error(error), cx);
        }
    }

    pub(super) fn refresh_details_editors(&mut self, cx: &mut Context<Self>) {
        if self.details_editors.discovering || self.on_machine() {
            return;
        }
        self.details_editors.discovering = true;
        cx.spawn(async move |view, cx| {
            let available = cx.background_spawn(async { discover() }).await;
            view.update(cx, |view, cx| {
                view.details_editors.available = available;
                view.details_editors.discovered = true;
                view.details_editors.discovering = false;
                if view
                    .context_menu
                    .as_ref()
                    .is_some_and(|menu| matches!(menu.target, MenuTarget::DetailsEditor(_)))
                {
                    let rows = view.details_editor_rows();
                    if let Some(menu) = view.context_menu.as_mut() {
                        menu.rows = rows;
                        if matches!(menu.cursor, super::super::context_menu::MenuCursor::Row(index)
                            if index >= menu.rows.len())
                        {
                            menu.cursor = super::super::context_menu::MenuCursor::Idle;
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(in crate::cockpit) fn details_editor_rows(&self) -> Vec<Option<(menu::Item, MenuVerb)>> {
        if self.details_editors.available.is_empty() {
            return vec![Some((
                menu::Item::new("No installed editors found").disabled(true),
                MenuVerb::DetailsEditor(EditorId::FILE_MANAGER),
            ))];
        }
        self.details_editors
            .available
            .iter()
            .map(|editor| {
                Some((
                    menu::Item::new(editor.id.label()),
                    MenuVerb::DetailsEditor(editor.id),
                ))
            })
            .collect()
    }

    pub(in crate::cockpit) fn prefer_details_editor(
        &mut self,
        editor: EditorId,
        cx: &mut Context<Self>,
    ) {
        if self
            .details_editors
            .available
            .iter()
            .any(|available| available.id == editor)
        {
            self.change_settings(
                |settings| settings.preferred_editor = Some(editor.id().to_owned()),
                cx,
            );
        }
    }

    pub(super) fn open_details_editor(
        &mut self,
        thread: ThreadId,
        editor: EditorId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.on_machine() {
            return;
        }
        let Some(path) = self.thread_path(thread) else {
            return;
        };
        let Some(installed) = self
            .details_editors
            .available
            .iter()
            .find(|available| available.id == editor)
            .cloned()
        else {
            window.push_notification(
                Notification::error("The chosen editor is no longer available."),
                cx,
            );
            self.refresh_details_editors(cx);
            return;
        };
        self.prefer_details_editor(editor, cx);
        if editor == EditorId::FILE_MANAGER {
            ferrite_pane::preview::open_original(&path, window, cx);
            return;
        }
        cx.spawn(async move |view, cx| {
            let outcome = cx
                .background_spawn(async move { launch(&installed, &path) })
                .await;
            if let Err(error) = outcome {
                view.update(cx, |view, cx| {
                    view.details_editors
                        .failures
                        .push(format!("Could not open {}: {error}", editor.label()));
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preference_follows_availability_and_contract_order() {
        let editors = Editors::with_ids(&["file-manager", "vscode", "cursor"]);
        assert_eq!(editors.preferred(None).unwrap().id(), "cursor");
        assert_eq!(editors.preferred(Some("vscode")).unwrap().id(), "vscode");
        assert_eq!(
            editors.preferred(Some("removed-editor")).unwrap().id(),
            "cursor"
        );
        assert_eq!(Editors::with_ids(&[]).preferred(Some("cursor")), None);
    }
}
