//! The Snooze field (Custom… on a Thread's Snooze menu): the time it
//! reads is asked of the window, which snoozes the Thread through the
//! Server-owned reminder.
use ferrite_core::snooze;
use ferrite_core::ThreadKey;
use ferrite_pane::composer::Composer;
use gpui::prelude::*;
use gpui::{div, px, Context, Div, Entity, Focusable, SharedString, Window};

use super::{NavEvent, NavInputs, NavView};

/// The Custom… field (`NavView`): the Thread it snoozes, its line, and why
/// the last ⏎ did not take.
pub struct Editor {
    pub thread: ThreadKey,
    pub line: Entity<Composer>,
    pub error: Option<SharedString>,
}

impl NavView {
    /// Custom… on a Thread's Snooze menu: the field over the list, seeded
    /// with an hour, takes the keyboard.
    pub fn edit_snooze(&mut self, thread: ThreadKey, window: &mut Window, cx: &mut Context<Self>) {
        let line = cx.new(Composer::new);
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
    /// window (`NavEvent::Snooze`), and the field stands until the answer
    /// (`snooze_answered`); one it cannot read stays, with why.
    pub fn finish_snooze(&mut self, save: bool, cx: &mut Context<Self>) {
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

    /// The window's answer to `NavEvent::Snooze`: done closes the field, a
    /// refusal stays in it.
    pub fn snooze_answered(&mut self, refused: Option<SharedString>, cx: &mut Context<Self>) {
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
    pub fn close_snooze(&mut self, cx: &mut Context<Self>) {
        if self.snooze.take().is_some() {
            cx.notify();
        }
    }

    /// The field's line, while it is open: the focus pump holds the
    /// keyboard on it.
    pub fn snooze_line(&self) -> Option<&Entity<Composer>> {
        self.snooze.as_ref().map(|editor| &editor.line)
    }

    /// The field as the nav draws it over its list, titled with the
    /// Thread's name as its row has it.
    pub(super) fn snooze_field(&self, inputs: &NavInputs) -> Option<Div> {
        let editor = self.snooze.as_ref()?;
        let title = inputs
            .state
            .open_rows()
            .find(|row| row.key == editor.thread)
            .map(|row| row.name.clone())
            .unwrap_or_default();
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
        .children(field.error.as_ref().map(|error| crate::notice(error.clone())))
}
