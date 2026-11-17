//! The one place a sheet is up (ADR 0042). `Sheets` holds the sheet that
//! is up, or the one fading out after a close, and draws it: the veil, the
//! press outside the card, the entrance and the exit. Showing a sheet
//! replaces the one up, at once; a sheet's view exists only while it is
//! drawn. What a sheet changes in the window, and every close, is the
//! Cockpit's (`show_sheet`, `hide_sheet`).

use super::project_editor_view::ProjectEditorView;
use super::settings_view::SettingsView;
use super::*;
use ferrite_ui::motion;
use ferrite_ui::sheet::Drawn;
use gpui::App;
use std::time::Instant;

/// A sheet the host can hold. The set is closed: one variant per sheet.
#[derive(Clone)]
pub(crate) enum Sheet {
    Settings(Entity<SettingsView>),
    ProjectEditor(Entity<ProjectEditorView>),
}

impl Sheet {
    pub(super) fn focus(&self, cx: &App) -> FocusHandle {
        match self {
            Sheet::Settings(view) => view.read(cx).focus.clone(),
            Sheet::ProjectEditor(view) => view.read(cx).focus.clone(),
        }
    }

    /// The card's entrance, by sheet: a sheet replacing another plays its
    /// own, over the veil that stays.
    fn entrance(&self) -> &'static str {
        match self {
            Sheet::Settings(_) => "settings-card-in",
            Sheet::ProjectEditor(_) => "project-editor-card-in",
        }
    }

    fn draw(&self, window: &mut Window, cx: &mut App) -> Option<Drawn> {
        match self {
            Sheet::Settings(view) => view.update(cx, |view, cx| view.card(window, cx)),
            Sheet::ProjectEditor(view) => view.update(cx, |view, cx| view.card(window, cx)),
        }
    }
}

enum Shown {
    Up(Sheet),
    /// Closed to nothing at this instant, drawn while `DIALOG_OUT` runs.
    Leaving(Sheet, Instant),
}

#[derive(Default)]
pub(crate) struct Sheets {
    shown: Option<Shown>,
}

/// The operator pressed the veil, outside the card.
pub(crate) struct Dismissed;

impl EventEmitter<Dismissed> for Sheets {}

impl Sheets {
    pub(super) fn up(&self) -> Option<&Sheet> {
        match &self.shown {
            Some(Shown::Up(sheet)) => Some(sheet),
            _ => None,
        }
    }

    pub(super) fn settings(&self) -> Option<&Entity<SettingsView>> {
        match self.up() {
            Some(Sheet::Settings(view)) => Some(view),
            _ => None,
        }
    }

    pub(super) fn project_editor(&self) -> Option<&Entity<ProjectEditorView>> {
        match self.up() {
            Some(Sheet::ProjectEditor(view)) => Some(view),
            _ => None,
        }
    }

    /// Put `sheet` up. The one up before goes at once, with no exit, and
    /// is returned; one still fading out goes too.
    pub(super) fn show(&mut self, sheet: Sheet, cx: &mut Context<Self>) -> Option<Sheet> {
        let replaced = match self.shown.replace(Shown::Up(sheet)) {
            Some(Shown::Up(sheet)) => Some(sheet),
            _ => None,
        };
        cx.notify();
        replaced
    }

    /// Close the sheet up, if one is, and return it. It fades out over
    /// `DIALOG_OUT`, unless motion is reduced.
    pub(super) fn hide(&mut self, cx: &mut Context<Self>) -> Option<Sheet> {
        let Some(Shown::Up(sheet)) = self.shown.take() else {
            return None;
        };
        if !motion::reduced_motion(cx) {
            self.shown = Some(Shown::Leaving(sheet.clone(), cx.background_executor().now()));
        }
        cx.notify();
        Some(sheet)
    }
}

impl Render for Sheets {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ferrite_ui::loops_overlay::begin(window, cx.entity_id());
        let now = cx.background_executor().now();
        if let Some(Shown::Leaving(_, left)) = &self.shown {
            if now.saturating_duration_since(*left) >= motion::DIALOG_OUT.duration() {
                self.shown = None;
            }
        }
        let (sheet, left) = match &self.shown {
            Some(Shown::Up(sheet)) => (sheet.clone(), None),
            Some(Shown::Leaving(sheet, left)) => (sheet.clone(), Some(*left)),
            None => return div().into_any_element(),
        };
        let Some(Drawn { card, above }) = sheet.draw(window, cx) else {
            return div().into_any_element();
        };
        let face = match left {
            Some(left) => motion::veil_out(
                left,
                motion::VEIL,
                ferrite_ui::sheet::leaving_veil().child(motion::dialog_out(left, card)),
            )
            .into_any_element(),
            None => motion::veil_in(
                "sheet-veil",
                motion::VEIL,
                ferrite_ui::sheet::veil()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            cx.emit(Dismissed);
                        }),
                    )
                    .child(motion::dialog_in(sheet.entrance(), card)),
            )
            .into_any_element(),
        };
        div()
            .absolute()
            .inset_0()
            .child(deferred(face).with_priority(3))
            .children(above.map(|above| deferred(above).with_priority(4)))
            .into_any_element()
    }
}

impl CockpitView {
    /// A sheet is up.
    pub(crate) fn sheet_up(&self, cx: &App) -> bool {
        self.sheets.read(cx).up().is_some()
    }

    /// Put `sheet` up in place of whatever was.
    pub(super) fn show_sheet(&mut self, sheet: Sheet, cx: &mut Context<Self>) {
        if let Some(replaced) = self.sheets.update(cx, |sheets, cx| sheets.show(sheet, cx)) {
            self.sheet_gone(replaced);
        }
        cx.notify();
    }

    /// Close the sheet up, if one is; whether one was. Every close comes
    /// here: Escape, the veil, a sheet's own close button, a sheet done.
    pub(super) fn hide_sheet(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(gone) = self.sheets.update(cx, |sheets, cx| sheets.hide(cx)) else {
            return false;
        };
        self.sheet_gone(gone);
        cx.notify();
        true
    }

    /// What goes with a sheet, closed or replaced.
    fn sheet_gone(&mut self, gone: Sheet) {
        match gone {
            Sheet::Settings(_) => self.connections.editor = None,
            Sheet::ProjectEditor(_) => {}
        }
    }

    pub(super) fn on_sheets_event(&mut self, _: Entity<Sheets>, _: &Dismissed, cx: &mut Context<Self>) {
        self.hide_sheet(cx);
    }
}
