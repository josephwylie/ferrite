//! The one place a sheet is up (ADR 0042). `Sheets` holds the sheet that
//! is up, or the one fading out after a close, and draws it: the veil, the
//! press outside the card, the entrance and the exit. Showing a sheet
//! replaces the one up, at once; a sheet's view exists only while it is
//! drawn. What a sheet changes in the window, and every close, is the
//! Cockpit's (`show_sheet`, `hide_sheet`).

use super::background_dialog_view::BackgroundDialogView;
use super::direct_view::DirectView;
use super::new_task_view::NewTaskView;
use super::project_editor_view::ProjectEditorView;
use super::settings_view::SettingsView;
use super::*;
use ferrite_ui::motion;
use ferrite_ui::sheet::Drawn;
use gpui::{App, Bounds};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

/// A sheet the host can hold. The set is closed: one variant per sheet.
#[derive(Clone)]
pub(crate) enum Sheet {
    Settings(Entity<SettingsView>),
    ProjectEditor(Entity<ProjectEditorView>),
    BackgroundDialog(Entity<BackgroundDialogView>),
    NewTask(Entity<NewTaskView>),
    /// A Task's Rework or Direct this task sheet.
    Direct(Entity<DirectView>),
}

impl Sheet {
    pub(super) fn focus(&self, cx: &App) -> FocusHandle {
        match self {
            Sheet::Settings(view) => view.read(cx).focus.clone(),
            Sheet::ProjectEditor(view) => view.read(cx).focus.clone(),
            Sheet::BackgroundDialog(view) => view.read(cx).focus.clone(),
            Sheet::NewTask(view) => view.read(cx).focus(cx),
            Sheet::Direct(view) => view.read(cx).focus(cx),
        }
    }

    /// The card's entrance, by sheet: a sheet replacing another plays its
    /// own, over the veil that stays.
    fn entrance(&self) -> &'static str {
        match self {
            Sheet::Settings(_) => "settings-card-in",
            Sheet::ProjectEditor(_) => "project-editor-card-in",
            Sheet::BackgroundDialog(_) => "background-dialog-in",
            Sheet::NewTask(_) => "new-task-card-in",
            Sheet::Direct(_) => "direct-card-in",
        }
    }

    fn draw(&self, window: &mut Window, cx: &mut App) -> Option<Drawn> {
        match self {
            Sheet::Settings(view) => view.update(cx, |view, cx| view.card(window, cx)),
            Sheet::ProjectEditor(view) => view.update(cx, |view, cx| view.card(window, cx)),
            Sheet::BackgroundDialog(view) => view.update(cx, |view, cx| view.card(window, cx)),
            Sheet::NewTask(view) => view.update(cx, |view, cx| view.card(window, cx)),
            Sheet::Direct(view) => view.update(cx, |view, cx| view.card(window, cx)),
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
    /// What held the keyboard before the sheet up took it.
    back: Option<FocusHandle>,
    /// Where the card up was last drawn.
    card: Rc<Cell<Option<Bounds<Pixels>>>>,
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

    /// Where the card up was last drawn; `None` while none is up.
    pub(super) fn card(&self) -> Option<Bounds<Pixels>> {
        self.up().and(self.card.get())
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

    pub(super) fn background_dialog(&self) -> Option<&Entity<BackgroundDialogView>> {
        match self.up() {
            Some(Sheet::BackgroundDialog(view)) => Some(view),
            _ => None,
        }
    }

    pub(super) fn new_task(&self) -> Option<&Entity<NewTaskView>> {
        match self.up() {
            Some(Sheet::NewTask(view)) => Some(view),
            _ => None,
        }
    }

    pub(super) fn direct(&self) -> Option<&Entity<DirectView>> {
        match self.up() {
            Some(Sheet::Direct(view)) => Some(view),
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

impl Sheets {
    /// The card's child that notes where the card is drawn, and draws the
    /// window again when that moved: the toasts hang from it.
    fn probe(&self) -> impl IntoElement {
        let card = self.card.clone();
        let noted = move |bounds: Bounds<Pixels>, window: &mut Window, _: &mut App| {
            if card.replace(Some(bounds)) != Some(bounds) {
                window.on_next_frame(|window, _| window.refresh());
            }
        };
        gpui::canvas(noted, |_, _, _, _| {}).absolute().top_0().left_0().size_full()
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
                    .child(motion::dialog_in(sheet.entrance(), card.child(self.probe()))),
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

    /// Put `sheet` up in place of whatever was. The popup up goes too.
    pub(super) fn show_sheet(&mut self, sheet: Sheet, cx: &mut Context<Self>) {
        if let Some(replaced) = self.sheets.update(cx, |sheets, cx| sheets.show(sheet, cx)) {
            self.sheet_gone(replaced, cx);
        }
        self.close_popup(cx);
        cx.notify();
    }

    /// Close the sheet up, if one is; whether one was. Every close comes
    /// here: Escape, the veil, a sheet's own close button, a sheet done.
    /// The popup up goes with it, as does a menu the sheet opened.
    pub(super) fn hide_sheet(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(gone) = self.sheets.update(cx, |sheets, cx| sheets.hide(cx)) else {
            return false;
        };
        self.sheet_gone(gone, cx);
        self.close_popup(cx);
        cx.notify();
        true
    }

    /// The keyboard a sheet took comes back as it closes, to what held it
    /// as the sheet came up, if the last frame drew that; else the focus
    /// rule decides. Tended once a frame, before that rule hands the sheet
    /// its focus.
    pub(super) fn tend_sheet_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let up = self.sheet_up(cx);
        let focused = window.focused(cx);
        let back = self.sheets.update(cx, |sheets, _| {
            if up {
                if sheets.back.is_none() {
                    sheets.back = focused;
                }
                None
            } else {
                sheets.back.take()
            }
        });
        let Some(back) = back else {
            return;
        };
        window.focus(&back, cx);
        if !self.focus.contains_focused(window, cx) {
            window.focus(&self.focus, cx);
        }
    }

    /// What goes with a sheet, closed or replaced.
    fn sheet_gone(&mut self, gone: Sheet, cx: &mut App) {
        match gone {
            // At the close, not when the fade drops the view: the editor
            // holds the window's keystrokes and an attempt in flight.
            Sheet::Settings(sheet) => {
                let connections = sheet.read(cx).connections.clone();
                connections.update(cx, |connections, _| connections.close_editor());
            }
            Sheet::NewTask(sheet) => self.new_task_gone(&sheet, cx),
            Sheet::Direct(sheet) => self.direct_gone(&sheet, cx),
            Sheet::ProjectEditor(_) | Sheet::BackgroundDialog(_) => {}
        }
    }

    /// The sheet up closes once what it shows is gone, rather than hold
    /// the keyboard with no card drawn. Run each pump, so a change from
    /// the Server or another window counts as much as one made here.
    pub(super) fn close_sheet_with_nothing_to_draw(&mut self, cx: &mut Context<Self>) {
        let gone = match self.sheets.read(cx).up() {
            None | Some(Sheet::Settings(_)) => false,
            Some(Sheet::ProjectEditor(card)) => card
                .read(cx)
                .target
                .is_some_and(|project| self.cockpit.read(cx).project(project).is_none()),
            Some(Sheet::BackgroundDialog(dialog)) => {
                self.cockpit.read(cx).thread(dialog.read(cx).thread).is_none()
            }
            Some(Sheet::NewTask(sheet)) => {
                self.factory_project() != Some(sheet.read(cx).project)
            }
            Some(Sheet::Direct(sheet)) => {
                let sheet = sheet.read(cx);
                self.direct_inputs(sheet.project, &sheet.task, cx).is_none()
            }
        };
        if gone {
            self.hide_sheet(cx);
        }
    }

    pub(super) fn on_sheets_event(&mut self, _: Entity<Sheets>, _: &Dismissed, cx: &mut Context<Self>) {
        self.hide_sheet(cx);
    }
}
