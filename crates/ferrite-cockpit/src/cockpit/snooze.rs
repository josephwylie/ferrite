//! Snooze controls use the Server-owned Thread reminder and the existing menu.
use super::*;
use super::nav_view::{NavEvent, NavView};
use ferrite_core::snooze::{self, Preset};
use gpui::App;

/// The Custom… field (`NavView`): the Thread it snoozes, its line, and why
/// the last ⏎ did not take.
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
        match self.cockpit.snooze_thread(thread, until) {
            Ok(()) => {
                if until.is_some() {
                    self.nav_view.update(cx, |nav, cx| {
                        nav.offer_undo(super::nav_chrome::UndoAct::Snoozed(thread), cx)
                    });
                }
                self.nav_notice.clear();
            }
            Err(error) => self.nav_notice.problem(error.to_string()),
        }
        cx.notify();
    }
}

impl NavView {
    /// Custom… on a Thread's Snooze menu: the field over the list, seeded
    /// with an hour, takes the keyboard.
    pub(super) fn edit_snooze(&mut self, thread: ThreadId, window: &mut Window, cx: &mut Context<Self>) {
        let line = cx.new(ferrite_pane::composer::Composer::new);
        line.update(cx, |line, cx| {
            line.set_role(ferrite_pane::composer::Role::Field(None), cx);
            line.set("1h".into(), cx);
        });
        window.focus(&line.focus_handle(cx), cx);
        self.snooze = Some(Editor {
            thread,
            line,
            error: None,
        });
        cx.notify();
    }

    /// ⏎ (`save`) or esc in the field. A time it reads is asked of the
    /// Cockpit (`NavEvent::Snooze`), and the field stands until the answer
    /// (`snooze_answered`); one it cannot read stays, with why.
    pub(super) fn finish_snooze(&mut self, save: bool, cx: &mut Context<Self>) {
        let Some(editor) = self.snooze.as_mut() else {
            return;
        };
        if !save {
            self.snooze = None;
        } else {
            match snooze::parse(editor.line.read(cx).text(), ferrite_core::clock::system_time()) {
                Ok(until) => cx.emit(NavEvent::Snooze(editor.thread, until)),
                Err(error) => editor.error = Some(error.into()),
            }
        }
        cx.notify();
    }

    /// The Cockpit's answer to `NavEvent::Snooze`: done closes the field, a
    /// refusal stays in it.
    pub(super) fn snooze_answered(&mut self, refused: Option<SharedString>, cx: &mut Context<Self>) {
        match refused {
            None => self.snooze = None,
            Some(error) => {
                if let Some(editor) = self.snooze.as_mut() {
                    editor.error = Some(error);
                }
            }
        }
        cx.notify();
    }

    /// The field goes, unsaved: the nav folding, the keys going to the
    /// list, a row or a notice opened.
    pub(super) fn close_snooze(&mut self, cx: &mut Context<Self>) {
        if self.snooze.take().is_some() {
            cx.notify();
        }
    }

    /// The field's line, while it is open: the focus pump holds the
    /// keyboard on it.
    pub(crate) fn snooze_line(&self) -> Option<&Entity<ferrite_pane::composer::Composer>> {
        self.snooze.as_ref().map(|editor| &editor.line)
    }

    /// The field as the nav draws it over its list, titled with the
    /// Thread's name as the Cockpit has it now.
    pub(super) fn snooze_field(&self, cx: &App) -> Option<Div> {
        let editor = self.snooze.as_ref()?;
        let title = self.cockpit.upgrade()?.read(cx).facts.name(editor.thread);
        Some(field(editor, title))
    }
}

fn field(field: &Editor, title: SharedString) -> Div {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .gap(px(ferrite_ui::theme::SPACE_2))
        .px(px(ferrite_ui::theme::SPACE_2))
        .py(px(ferrite_ui::theme::SPACE_2))
        .child(
            ferrite_ui::components::text_meta()
                .child(format!("Snooze {title}"))
                .truncate(),
        )
        .child(field.line.clone())
        .child(
            ferrite_ui::components::text_meta()
                .whitespace_normal()
                .child("Local date/time: YYYY-MM-DD HH:MM, or a duration: 30m, 3h, 2d"),
        )
        .child(ferrite_ui::components::text_meta().child("↵ snooze · esc cancel"))
        .children(field.error.as_ref().map(|error| nav::notice(error.clone())))
}
