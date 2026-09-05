//! One image preview per Pane. The kit owns dialog focus and dismissal;
//! this module supplies the owning Pane's bounds instead of the window's.

use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};

use gpui::component::{
    button::{Button, ButtonVariants},
    dialog::{DialogContent, DialogHeader, DialogTitle},
    IconName, Sizable, Theme, ThemeStyled,
};
use gpui::{
    canvas, div, prelude::*, relative, rems, App, Bounds, Div, FocusHandle, IntoElement, Pixels,
    Window,
};

#[derive(Default)]
struct State {
    image: Option<(PathBuf, String)>,
    return_focus: Option<FocusHandle>,
}

#[derive(Clone)]
pub struct Preview {
    state: Rc<RefCell<State>>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    focus: FocusHandle,
}

impl Preview {
    pub fn new(cx: &mut App) -> Self {
        Self {
            state: Rc::new(RefCell::new(State::default())),
            bounds: Rc::new(Cell::new(Bounds::default())),
            focus: cx.focus_handle(),
        }
    }

    pub fn focus_target(&self) -> Option<FocusHandle> {
        self.state
            .borrow()
            .image
            .as_ref()
            .map(|_| self.focus.clone())
    }

    pub fn open(&self, path: PathBuf, title: String, window: &mut Window, cx: &mut App) {
        let mut state = self.state.borrow_mut();
        if state.image.is_none() {
            state.return_focus = window.focused(cx);
        }
        // Repeated activation replaces one slot; it can never stack dialogs.
        state.image = Some((path, title));
        drop(state);
        self.focus.focus(window, cx);
        window.refresh();
    }

    fn close(&self, window: &mut Window, cx: &mut App) {
        let mut state = self.state.borrow_mut();
        state.image = None;
        let focus = state.return_focus.take();
        drop(state);
        if let Some(focus) = focus {
            focus.focus(window, cx);
        }
        window.refresh();
    }

    pub fn mount(&self, pane: Div) -> Div {
        let preview = self.clone();
        pane.child(
            canvas(
                move |bounds, window, cx| {
                    if preview.bounds.replace(bounds) != bounds
                        && preview.state.borrow().image.is_some()
                    {
                        // refresh() is ignored during prepaint. Schedule the
                        // new pane geometry after this frame completes.
                        window.defer(cx, |window, _| window.refresh());
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        )
        .child(PreviewLayer(self.clone()))
    }
}

#[derive(IntoElement)]
struct PreviewLayer(Preview);

impl RenderOnce for PreviewLayer {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let preview = self.0;
        let image = preview.state.borrow().image.clone();
        let Some((path, title)) = image else {
            return div().absolute().into_any_element();
        };
        let bounds = preview.bounds.get();
        let close_button = preview.clone();
        let close_dialog = preview.clone();
        let content = div()
            .debug_selector(|| "attachment-preview-content".into())
            .w(relative(0.9))
            .h(relative(0.85))
            .max_w(rems(48.))
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .child(
                DialogContent::new()
                    .size_full()
                    .popover_style(cx)
                    .p_3()
                    .gap_2()
                    .child(
                        DialogHeader::new()
                            .flex_row()
                            .items_center()
                            .flex_shrink_0()
                            .child(
                                DialogTitle::new()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(title),
                            )
                            .child(
                                Button::new("close-attachment-preview")
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::Close)
                                    .accessibility_label("Close image preview")
                                    .tooltip("Close image preview")
                                    .on_click(move |_, window, cx| {
                                        cx.stop_propagation();
                                        close_button.close(window, cx);
                                    }),
                            ),
                    )
                    .child(
                        div().relative().flex_1().min_h_0().w_full().child(
                            gpui::img(path)
                                .absolute()
                                .inset_0()
                                .size_full()
                                .object_fit(gpui::ObjectFit::Contain),
                        ),
                    ),
            );
        gpui::base::Dialog::new(cx)
            .focus_handle(preview.focus.clone())
            .left(bounds.origin.x)
            .top(bounds.origin.y)
            .w(bounds.size.width)
            .h(bounds.size.height)
            .backdrop(div().size_full().bg(Theme::global(cx).overlay))
            .popup(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(content),
            )
            .request_close(move |_, window, cx| close_dialog.close(window, cx))
            .into_any_element()
    }
}
