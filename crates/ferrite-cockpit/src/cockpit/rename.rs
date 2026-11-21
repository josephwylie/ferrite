//! The inline title rename: the one-line field that stands in for a
//! Thread's title, in its nav row or the view head, while it is renamed.
//! It gets no view (ADR 0041, What does not get a view): every press that
//! opens or closes it is the root's (a menu's Rename, F2 and a row's second
//! click through `NavEvent::Rename`, the view head's double-click, ⏎ and
//! esc, a press elsewhere, a popup going up), and saving is the store's.

use super::*;
use ferrite_pane::composer::Composer;
use gpui::App;

/// What an inline rename is aimed at: a Thread's own display name, at
/// one of the sites that edit it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RenameTarget {
    /// A Thread, renamed from its nav row.
    Thread(ThreadId),
    /// A Thread, renamed from the view head (`view_head`): the same title,
    /// a different editor site, so the two never draw one editor twice.
    ViewTitle(ThreadId),
}

/// The rename in flight, if any: what is being renamed, and the one-line
/// Composer standing in for its title. At most one — the title cell *is*
/// the editor, so two at once would need two rows to be the row you are
/// looking at.
#[derive(Default)]
pub(super) struct Rename {
    open: Option<(RenameTarget, Entity<Composer>)>,
}

impl Rename {
    /// Open the field on `target`, seeded with `title` as it stands — a
    /// rename starts from what is there, not from an empty line.
    pub(super) fn open(&mut self, target: RenameTarget, title: String, cx: &mut App) {
        let editor = cx.new(Composer::new);
        editor.update(cx, |editor, cx| {
            editor.set_role(ferrite_pane::composer::Role::Field(Some("NavRename")), cx);
            editor.set(title, cx)
        });
        self.open = Some((target, editor));
    }

    /// The field and what it renames, while it is open.
    pub(super) fn shown(&self) -> Option<(RenameTarget, &Entity<Composer>)> {
        self.open.as_ref().map(|(target, editor)| (*target, editor))
    }

    pub(super) fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Close the field unsaved.
    pub(super) fn close(&mut self) {
        self.open = None;
    }

    /// Close the field, with the Thread and the title it asks for: none
    /// for a blank line, which is not a rename to an empty title but a
    /// rename the operator abandoned.
    pub(super) fn take(&mut self, cx: &mut App) -> Option<(ThreadId, String)> {
        let (target, editor) = self.open.take()?;
        let title = editor.update(cx, |line, cx| line.take(cx));
        let (RenameTarget::Thread(thread) | RenameTarget::ViewTitle(thread)) = target;
        (!title.trim().is_empty()).then_some((thread, title))
    }
}

/// A rename editor with its keys beside it while it edits: `↵ save · esc
/// cancel` in mono `INK_SUBTLE`, the `·` in structure ink. The keys live
/// here, where they act, rather than on the menu row that opened it.
pub(super) fn rename_field(editor: Entity<Composer>, hint: bool) -> Div {
    let text = "↵ save · esc cancel";
    let at = text.find(" · ").unwrap_or(0);
    let hint_text = ferrite_ui::components::text_meta()
        .debug_selector(|| "rename-hint".into())
        .flex_shrink_0()
        .font_family(ferrite_ui::theme::FONT_CODE)
        .child(gpui::StyledText::new(text).with_highlights(vec![(
            at..at + " · ".len(),
            gpui::HighlightStyle {
                color: Some(rgb(ferrite_ui::theme::TEXT_FAINT).into()),
                ..Default::default()
            },
        )]));
    div()
        .flex()
        .items_center()
        .gap(px(ferrite_ui::theme::SPACE_2))
        .min_w_0()
        .flex_1()
        .child(
            div()
                .debug_selector(|| "rename-field".into())
                .min_w_0()
                .flex_1()
                .child(editor),
        )
        .children(hint.then_some(hint_text))
}
