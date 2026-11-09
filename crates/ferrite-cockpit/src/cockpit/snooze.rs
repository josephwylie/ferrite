//! Snooze controls use the Server-owned Thread reminder and the existing menu.
use super::*;
use ferrite_core::snooze::{self, Preset};

pub(super) struct Editor {
    pub thread: ThreadId,
    pub line: Entity<ferrite_pane::composer::Composer>,
    pub error: Option<SharedString>,
}

impl CockpitView {
    pub(super) fn snooze_rows(&self) -> Vec<(menu::Item, MenuVerb)> {
        let mut rows: Vec<_> = Preset::available(ferrite_core::clock::system_time())
            .into_iter()
            .map(|(preset, _)| (menu::Item::new(preset.label()), MenuVerb::Snooze(preset)))
            .collect();
        rows.push((menu::Item::new("Custom…"), MenuVerb::SnoozeCustom));
        rows
    }

    pub(super) fn snooze_thread(
        &mut self,
        thread: ThreadId,
        until: Option<std::time::SystemTime>,
        cx: &mut Context<Self>,
    ) {
        self.group_error = self
            .cockpit
            .snooze_thread(thread, until)
            .err()
            .map(|error| error.to_string().into());
        cx.notify();
    }

    pub(super) fn edit_snooze(
        &mut self,
        thread: ThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_nav_view(nav::NavView::Inbox, None, cx);
        let line = cx.new(ferrite_pane::composer::Composer::new);
        line.update(cx, |line, cx| {
            line.set_role(ferrite_pane::composer::Role::Field(None), cx);
            line.set("1h".into(), cx);
        });
        window.focus(&line.focus_handle(cx), cx);
        self.snooze_editor = Some(Editor {
            thread,
            line,
            error: None,
        });
        cx.notify();
    }

    pub(super) fn finish_snooze(&mut self, save: bool, cx: &mut Context<Self>) {
        let Some(editor) = self.snooze_editor.take() else {
            return;
        };
        if save {
            match snooze::parse(
                editor.line.read(cx).text(),
                ferrite_core::clock::system_time(),
            ) {
                Ok(until) => match self.cockpit.snooze_thread(editor.thread, Some(until)) {
                    Ok(()) => self.group_error = None,
                    Err(error) => {
                        self.snooze_editor = Some(Editor {
                            error: Some(error.to_string().into()),
                            ..editor
                        })
                    }
                },
                Err(error) => {
                    self.snooze_editor = Some(Editor {
                        error: Some(error.into()),
                        ..editor
                    })
                }
            }
        }
        cx.notify();
    }

    pub(super) fn snooze_field(&self) -> Option<Div> {
        let editor = self.snooze_editor.as_ref()?;
        Some(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .gap(px(ferrite_ui::theme::SPACE_2))
                .px(px(ferrite_ui::theme::SPACE_2))
                .py(px(ferrite_ui::theme::SPACE_2))
                .child(
                    ferrite_ui::components::text_meta()
                        .child(format!("Snooze {}", self.facts.name(editor.thread)))
                        .truncate(),
                )
                .child(editor.line.clone())
                .child(
                    ferrite_ui::components::text_meta()
                        .whitespace_normal()
                        .child("Local date/time: YYYY-MM-DD HH:MM, or a duration: 30m, 3h, 2d"),
                )
                .child(ferrite_ui::components::text_meta().child("↵ snooze · esc cancel"))
                .children(
                    editor
                        .error
                        .as_ref()
                        .map(|error| nav::notice(error.clone())),
                ),
        )
    }
}
