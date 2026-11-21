//! The Workflows editor's own state, in a view of its own (ADR 0041):
//! each text field's Composer, made once for as long as the Factory is
//! open, and the field that holds the keyboard. The Factory holds it.
//!
//! It has no render of its own: the root draws the surface and has it
//! draw each field (`input`). Which fields the surface shows the root
//! decides from the draft (`ensure_fields`); what is typed it says as a
//! `WorkflowsEvent`, since every edit is a Server ask.

use super::*;

/// What the editor's fields ask of the window.
pub(crate) enum WorkflowsEvent {
    /// A field was typed in: what it holds goes to the draft.
    Edited(Field),
    /// A press on a field: it holds the keyboard now.
    Focused,
}

/// A Factory's Workflows editor.
pub(crate) struct WorkflowsView {
    /// Each text field's Composer, made once.
    fields: HashMap<Field, Entity<Composer>>,
    /// The field that holds the keyboard; none, the outline.
    pub(crate) focused: Option<Field>,
}

impl EventEmitter<WorkflowsEvent> for WorkflowsView {}

impl WorkflowsView {
    fn new() -> Self {
        Self {
            fields: HashMap::new(),
            focused: None,
        }
    }

    /// `field`'s Composer, filled with `text` when it is first made; one
    /// already made keeps what was typed in it.
    pub(super) fn ensure(&mut self, field: Field, text: &str, cx: &mut Context<Self>) {
        if self.fields.contains_key(&field) {
            return;
        }
        let composer = cx.new(Composer::new);
        composer.update(cx, |composer, cx| {
            composer.set_role(Role::Field(None), cx);
            if !text.is_empty() {
                composer.insert(text, cx);
            }
        });
        let key = field.clone();
        cx.subscribe(&composer, move |_, _, _: &Edited, cx| {
            cx.emit(WorkflowsEvent::Edited(key.clone()))
        })
        .detach();
        self.fields.insert(field, composer);
    }

    pub(crate) fn field(&self, field: &Field) -> Option<&Entity<Composer>> {
        self.fields.get(field)
    }

    /// What holds the keyboard: the chosen field, if it is made.
    pub(super) fn focus(&self, cx: &gpui::App) -> Option<FocusHandle> {
        let composer = self.fields.get(self.focused.as_ref()?)?;
        Some(composer.read(cx).focus_handle(cx))
    }

    /// One field: its label over its Composer, ringed while it has the
    /// keyboard. A press on it gives it the keyboard.
    pub(super) fn input(
        &self,
        field: &Field,
        label: &'static str,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let composer = self.fields.get(field)?.clone();
        let focused = composer.read(cx).focus_handle(cx).is_focused(window);
        let chosen = field.clone();
        Some(field_input(
            field,
            label,
            composer,
            focused,
            cx.listener(move |editor, _: &MouseDownEvent, _, cx| {
                editor.focused = Some(chosen.clone());
                cx.emit(WorkflowsEvent::Focused);
            }),
        ))
    }
}

impl CockpitView {
    /// A Factory's Workflows editor, heard from its making.
    pub(in crate::cockpit) fn new_workflows(&mut self, cx: &mut Context<Self>) -> Entity<WorkflowsView> {
        let editor = cx.new(|_| WorkflowsView::new());
        cx.subscribe(&editor, Self::on_workflows_event).detach();
        editor
    }

    /// A field typed in goes to the draft; one pressed has the focus pump
    /// hand it the keyboard.
    fn on_workflows_event(
        &mut self,
        _: Entity<WorkflowsView>,
        event: &WorkflowsEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            WorkflowsEvent::Edited(field) => self.field_edited(field, cx),
            WorkflowsEvent::Focused => cx.notify(),
        }
    }
}
