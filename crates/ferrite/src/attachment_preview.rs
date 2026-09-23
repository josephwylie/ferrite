//! One image preview per Pane. The kit owns dialog focus and dismissal;
//! this module supplies the owning Pane's bounds instead of the window's.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use gpui::component::{
    dialog::{DialogContent, DialogHeader, DialogTitle},
    Theme,
};
use gpui::{
    canvas, div, prelude::*, px, relative, rgb, rgba, App, Bounds, Div, FocusHandle, IntoElement,
    Pixels, Window,
};

#[derive(Default)]
struct State {
    image: Option<(PathBuf, String)>,
    return_focus: Option<FocusHandle>,
}

#[derive(Clone)]
// Native Markdown callbacks require Send + Sync captures; the Pane still owns this slot.
pub struct Preview {
    state: Arc<Mutex<State>>,
    bounds: Arc<Mutex<Bounds<Pixels>>>,
    focus: FocusHandle,
}

impl Preview {
    pub fn new(cx: &mut App) -> Self {
        Self {
            state: Arc::new(Mutex::new(State::default())),
            bounds: Arc::new(Mutex::new(Bounds::default())),
            focus: cx.focus_handle(),
        }
    }

    pub fn focus_target(&self) -> Option<FocusHandle> {
        self.state
            .lock()
            .unwrap()
            .image
            .as_ref()
            .map(|_| self.focus.clone())
    }

    pub fn open(&self, path: PathBuf, title: String, window: &mut Window, cx: &mut App) {
        let mut state = self.state.lock().unwrap();
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
        let mut state = self.state.lock().unwrap();
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
                    if {
                        let mut prior = preview.bounds.lock().unwrap();
                        let changed = *prior != bounds;
                        *prior = bounds;
                        changed
                    } && preview.state.lock().unwrap().image.is_some()
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

/// The OS opens the original file in its image viewer, where it can be
/// inspected at full resolution. Canonicalize before URL encoding so relative
/// paths, symlinks and Windows drive paths all use the same file-link route.
fn open_original(path: &Path, window: &mut Window, cx: &mut App) {
    use gpui::component::{notification::Notification, WindowExt};
    match std::fs::canonicalize(path) {
        Ok(path) => crate::file_links::FileLink {
            path,
            location: None,
        }
        .open(window, cx),
        Err(error) => window.push_notification(
            Notification::error(format!("Could not open {}: {error}", path.display())),
            cx,
        ),
    }
}

#[derive(IntoElement)]
struct PreviewLayer(Preview);

impl RenderOnce for PreviewLayer {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let preview = self.0;
        let image = preview.state.lock().unwrap().image.clone();
        let Some((path, title)) = image else {
            return div().absolute().into_any_element();
        };
        let bounds = *preview.bounds.lock().unwrap();
        let close_button = preview.clone();
        let close_dialog = preview.clone();
        let original = path.clone();
        // A floating sheet on Ferrite's own tokens: the raised ground, the
        // strong hairline, the block radius and the float shadow; a mono
        // title, the sheet's ghost button for Open Original, and the image
        // in a recessed well.
        let content = div()
            .debug_selector(|| "attachment-preview-content".into())
            .w(relative(0.9))
            .h(relative(0.85))
            .max_w(px(crate::theme::PREVIEW_MAX_W))
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .child(
                DialogContent::new()
                    .size_full()
                    .bg(rgb(crate::theme::RAISED))
                    .border_1()
                    .border_color(rgba(crate::theme::HAIRLINE_STRONG))
                    .rounded(px(crate::theme::R_BLOCK))
                    .shadow(crate::components::float_shadow())
                    .p(px(crate::theme::SPACE_3))
                    .gap(px(crate::theme::SPACE_2))
                    .child(
                        DialogHeader::new()
                            .flex_row()
                            .items_center()
                            .gap(px(crate::theme::SPACE_2))
                            .flex_shrink_0()
                            .child(
                                DialogTitle::new()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .font_family(crate::theme::FONT_MONO)
                                    .text_size(px(crate::theme::FS_UI))
                                    .font_weight(crate::theme::W_LABEL)
                                    .text_color(rgb(crate::theme::TEXT_STRONG))
                                    .child(title),
                            )
                            .child(
                                crate::components::form_button("open-original-attachment", cx)
                                    .flex_shrink_0()
                                    .h(px(crate::theme::CONTROL_H))
                                    .px(px(crate::theme::CONTROL_PAD_X))
                                    .child(crate::components::form_label(
                                        "Open Original",
                                        crate::theme::TEXT_2,
                                    ))
                                    .accessibility_label("Open original image in the default app")
                                    .tooltip("Open full-size image in the default app")
                                    .debug_selector(|| "open-original-attachment".into())
                                    .on_click(move |_, window, cx| {
                                        cx.stop_propagation();
                                        open_original(&original, window, cx);
                                    }),
                            )
                            .child(
                                crate::components::icon_button(
                                    "close-attachment-preview",
                                    crate::icons::CLOSE,
                                    "Close image preview",
                                    cx,
                                )
                                .flex_shrink_0()
                                .on_click(move |_, window, cx| {
                                    cx.stop_propagation();
                                    close_button.close(window, cx);
                                }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .p(px(crate::theme::SPACE_2))
                            .rounded(px(crate::theme::R_CHIP))
                            .bg(rgb(crate::theme::GROUND))
                            .overflow_hidden()
                            .child(
                                div().relative().flex_1().min_h_0().min_w_0().child(
                                    gpui::img(path)
                                        .absolute()
                                        .inset_0()
                                        .size_full()
                                        .object_fit(gpui::ObjectFit::Contain),
                                ),
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
