//! The lightbox (Claude's): an image opened anywhere in a Pane — a sent
//! image, a Composer chip, a file link, a ⌘-clicked path — shown over the
//! whole window on a dim (`theme::LIGHTBOX_*`), never in the Panel. The
//! Pane's preview holds it (`Preview::lightbox`, opened by `Preview::open`)
//! and its image viewer is the stage: fit and actual size, zoom, pinch and
//! pan (`image_viewer.rs`).
//!
//! The picture grows in from the spot it was pressed (`Lightbox::from`,
//! `motion::grow_from`). Its bar reads the file's name, with zoom (a menu:
//! Zoom to fit, 25–200%), save a copy (a save panel,
//! then the file copied there), open in its app and × at the right. With
//! more than one image (a prompt's), ← → doors stand at its sides and the
//! arrow keys step too, and `1 of N` says where it is. Esc, ⌘W, the × or
//! a press on the dim around the image closes it and hands the keyboard
//! back to the Pane. The dim darkens in over `FADE_QUICK`; under reduced
//! motion it is simply there.

use super::*;
use ferrite_pane::preview::Lightbox;
use ferrite_ui::icons::{self, icon};
use ferrite_ui::menu;
use ferrite_ui::theme::{self, paint};

gpui::actions!(lightbox, [Close]);

/// Over every float and the toasts: nothing else answers while it is up.
const LIGHTBOX_PRIORITY: usize = crate::float::FLOAT_PRIORITY + 1;

impl CockpitView {
    /// The focused Pane's lightbox, while it has one open. Focus moving to
    /// another Pane (⌘}) takes the keyboard there, the lightbox kept for
    /// coming back.
    pub(super) fn lightbox(&self) -> Option<(usize, Lightbox)> {
        let index = self.focused();
        Some((index, self.panes.get(index)?.preview.lightbox()?))
    }

    /// Close Pane `index`'s lightbox: the keyboard is the Pane's again.
    pub(super) fn close_lightbox(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(pane) = self.panes.get(index) {
            pane.preview.close_lightbox();
            self.focus_pane(index);
            cx.notify();
        }
    }

    /// ← → in the lightbox: the next or the previous of the images it was
    /// opened among.
    fn step_lightbox(&mut self, index: usize, by: isize, cx: &mut Context<Self>) {
        if self
            .panes
            .get(index)
            .is_some_and(|pane| pane.preview.step_image(by, cx))
        {
            cx.notify();
        }
    }

    /// The lightbox over the window, while one is open.
    pub(super) fn lightbox_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (index, lightbox) = self.lightbox()?;
        let count = lightbox.gallery.len();
        let path = lightbox.path().to_path_buf();
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let zoom_open = lightbox.zoom_menu;
        let stops = lightbox.viewer.read(cx).zoom_target();
        let bar = div()
            .debug_selector(|| "lightbox-bar".into())
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(theme::LIGHTBOX_BAR_H))
            .flex()
            .items_center()
            .gap(px(theme::SPACE_1))
            .px(px(theme::SPACE_4))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(theme::FS_UI))
                    .font_weight(theme::W_LABEL)
                    .text_color(rgb(theme::INK_STRONG))
                    .child(name.clone()),
            )
            .child(
                door(
                    "lightbox-zoom",
                    icons::ZOOM_IN,
                    "Zoom \u{b7} click the image",
                )
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    if let Some(pane) = view.panes.get(index) {
                        pane.preview.set_zoom_menu(!zoom_open);
                    }
                    cx.notify();
                })),
            )
            .child({
                let path = path.clone();
                door("lightbox-save", icons::DOWNLOAD, "Save a copy\u{2026}").on_click(
                    move |_, window, cx| {
                        cx.stop_propagation();
                        save_copy(path.clone(), &name, window, cx);
                    },
                )
            })
            .child({
                let path = path.clone();
                door("lightbox-original", icons::EXTERNAL, "Open in its app").on_click(
                    move |_, window, cx| {
                        cx.stop_propagation();
                        ferrite_pane::preview::open_original(&path, window, cx);
                    },
                )
            })
            .child(
                door("lightbox-close", icons::CLOSE, "Close").on_click(cx.listener(
                    move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        view.close_lightbox(index, cx);
                    },
                )),
            );
        let stepper = |key: &'static str, glyph: &'static str, by: isize, live: bool| {
            let side = (theme::LIGHTBOX_SIDE - theme::LIGHTBOX_ARROW) / 2.;
            let arrow = div()
                .id(key)
                .debug_selector(move || key.into())
                .absolute()
                .top(gpui::relative(0.5))
                .mt(px(-theme::LIGHTBOX_ARROW / 2.))
                .size(px(theme::LIGHTBOX_ARROW))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(paint::FLOAT)
                .border_1()
                .border_color(paint::FLOAT_EDGE)
                .occlude()
                .child(icon(
                    glyph,
                    theme::ICON_BUTTON_GLYPH,
                    if live {
                        theme::INK_STRONG
                    } else {
                        theme::INK_SUBTLE
                    },
                ))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    view.step_lightbox(index, by, cx);
                }));
            if by < 0 {
                arrow.left(px(side))
            } else {
                arrow.right(px(side))
            }
        };
        let many = count > 1;
        let sides = if many { theme::LIGHTBOX_SIDE } else { 0. };
        let foot = if many { theme::LIGHTBOX_FOOT_H } else { 0. };
        let zoom_menu = zoom_open.then(|| {
            let level = |at: usize, label: String, to: Option<f32>| {
                let checked = match (stops, to) {
                    (None, None) => true,
                    (Some(now), Some(to)) => (now - to).abs() < 0.001,
                    _ => false,
                };
                let item = menu::Item::new(label).checked(checked);
                let viewer = lightbox.viewer.clone();
                menu::item_row(("lightbox-zoom-level", at), &item, false, false)
                    .debug_selector(move || format!("lightbox-zoom-level-{at}"))
                    .on_click(
                    cx.listener(move |view, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        viewer.update(cx, |viewer, cx| viewer.zoom_to(to, cx));
                        if let Some(pane) = view.panes.get(index) {
                            pane.preview.set_zoom_menu(false);
                        }
                        cx.notify();
                    }),
                )
            };
            menu::float()
                .debug_selector(|| "lightbox-zoom-menu".into())
                .absolute()
                .top(px(theme::LIGHTBOX_BAR_H))
                .right(px(theme::SPACE_4))
                .w(px(theme::EFFORT_MENU_W))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(level(0, "Zoom to fit".into(), None))
                .children([0.25, 0.5, 1.0, 1.5, 2.0].into_iter().enumerate().map(|(at, scale)| {
                    level(at + 1, format!("{}%", (scale * 100.0) as u32), Some(scale))
                }))
        });
        let from = lightbox.from;
        let stage = div()
            .debug_selector(|| "lightbox-stage".into())
            .absolute()
            .top(px(theme::LIGHTBOX_BAR_H))
            .bottom(px(foot))
            .left(px(sides))
            .right(px(sides))
            .flex()
            // A press on the picture is the viewer's (zoom, pan): only the
            // dim around it closes.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(lightbox.viewer.clone());
        let stage = ferrite_ui::motion::grow_from("lightbox-grow", stage, from);
        let lightbox_root = div()
            .id("lightbox")
            .debug_selector(|| "lightbox".into())
            .key_context(ferrite_ui::keymap::LIGHTBOX)
            .absolute()
            .inset_0()
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    view.close_lightbox(index, cx);
                }),
            )
            .on_action(cx.listener(move |view, _: &Close, _, cx| {
                view.close_lightbox(index, cx);
            }))
            .on_action(
                cx.listener(move |view, _: &ferrite_pane::image_viewer::Next, _, cx| {
                    view.step_lightbox(index, 1, cx)
                }),
            )
            .on_action(cx.listener(
                move |view, _: &ferrite_pane::image_viewer::Previous, _, cx| {
                    view.step_lightbox(index, -1, cx)
                },
            ))
            .child(stage)
            .child(bar)
            .children(zoom_menu)
            .when(many, |root| {
                root.child(stepper(
                    "lightbox-previous",
                    icons::BACK,
                    -1,
                    lightbox.at > 0,
                ))
                .child(stepper(
                    "lightbox-next",
                    icons::FORWARD,
                    1,
                    lightbox.at + 1 < count,
                ))
                .child(
                    div()
                        .debug_selector(|| "lightbox-count".into())
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .h(px(theme::LIGHTBOX_FOOT_H))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(theme::FS_META))
                        .text_color(rgb(theme::INK_SUBTLE))
                        .child(count_text(lightbox.at, count)),
                )
            });
        Some(
            deferred(ferrite_ui::motion::dim_in(
                "lightbox-dim",
                lightbox_root,
                theme::LIGHTBOX_VEIL,
            ))
            .with_priority(LIGHTBOX_PRIORITY)
            .into_any_element(),
        )
    }
}

/// Where the lightbox is among its images: `2 of 5`.
fn count_text(at: usize, count: usize) -> SharedString {
    format!("{} of {count}", at + 1).into()
}

/// One of the bar's doors: an `ICON_BUTTON` square in `INK_STRONG` on the
/// dim, lit under the pointer. A press on it never reaches the dim.
fn door(key: &'static str, glyph: &'static str, tip: &'static str) -> Stateful<Div> {
    crate::nav::icon_button(
        key,
        glyph,
        theme::ICON_BUTTON_GLYPH,
        theme::ICON_BUTTON,
        theme::R_CONTROL,
    )
    .occlude()
    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    .tooltip(ferrite_ui::menu::tooltip(tip))
}

/// Save a copy of `path` where the operator chooses (the system's save
/// panel, in Downloads, under its own name). A copy that fails says so.
fn save_copy(path: std::path::PathBuf, name: &str, window: &mut Window, cx: &mut gpui::App) {
    use gpui::component::{notification::Notification, WindowExt};
    let directory =
        dirs_downloads().unwrap_or_else(|| path.parent().unwrap_or(path.as_path()).to_path_buf());
    let chosen = cx.prompt_for_new_path(&directory, Some(name));
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(to))) = chosen.await else {
            return;
        };
        if let Err(error) = std::fs::copy(&path, &to) {
            let _ = handle.update(cx, |_, window, cx| {
                window.push_notification(
                    Notification::error(format!("Could not save {}: {error}", to.display())),
                    cx,
                );
            });
        }
    })
    .detach();
}

/// The operator's Downloads folder, where a save panel starts.
fn dirs_downloads() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|home| home.join("Downloads"))
        .filter(|downloads| downloads.is_dir())
}

#[cfg(test)]
mod tests {
    use super::count_text;

    #[test]
    fn the_count_says_where_it_is_among_them() {
        assert_eq!(count_text(0, 3), "1 of 3");
        assert_eq!(count_text(2, 3), "3 of 3");
    }
}
