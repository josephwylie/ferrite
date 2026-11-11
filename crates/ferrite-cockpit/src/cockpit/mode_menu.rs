//! The permission menu (T3's `ComposerPermissionsSection`): the Session's
//! permission modes as one segmented radio row, the mode under the pointer
//! named and explained under it. The footer's mode chip opens it over
//! itself; the model picker's footer opens it over its permissions button.
//! A pick sends the native `SetPermissionMode` (`request_mode`) and shuts
//! it; ⇧⇥ still cycles the modes without it.

use super::*;
use ferrite_ui::theme::*;

/// Where the open menu hangs, and which mode it previews.
#[derive(Default)]
pub(super) struct ModeMenu {
    /// Opened from the model picker's footer rather than the mode chip.
    pub(super) from_picker: bool,
    /// The mode under the pointer; else the menu names the mode in force.
    pub(super) preview: Option<String>,
}

/// The key a Thread's mode chip records its bounds under.
pub(super) fn chip_trigger(thread: ThreadId) -> SharedString {
    format!("mode-chip-{}", thread.get()).into()
}

impl CockpitView {
    /// Open the permission menu on `thread`'s Session, or shut it.
    pub(super) fn toggle_mode_menu(
        &mut self,
        thread: ThreadId,
        generation: u64,
        from_picker: bool,
        cx: &mut Context<Self>,
    ) {
        if self.mode_picker == Some((thread, generation)) {
            self.mode_picker = None;
        } else {
            if !from_picker {
                self.popover = None;
            }
            self.context_menu = None;
            self.session_controls = None;
            self.mode_picker = Some((thread, generation));
            self.mode_menu = ModeMenu {
                from_picker,
                preview: None,
            };
        }
        cx.notify();
    }

    pub(super) fn mode_menu_element(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (thread, generation) = self.mode_picker?;
        let open = self.cockpit.thread(thread)?;
        if open.generation() != generation {
            return None;
        }
        let key = if self.mode_menu.from_picker {
            SharedString::from(model_picker::PICKER_PERMISSIONS)
        } else {
            chip_trigger(thread)
        };
        let Some(trigger) = self.float_triggers.borrow().get(key.as_ref()).copied() else {
            ferrite_ui::motion::request_frame(window);
            return None;
        };
        let mut modes = open.permission_modes();
        // Claude's order: `Auto` first.
        modes.sort_by_key(|choice| choice.value != "auto");
        let current = open.permission_mode().unwrap_or("default").to_string();
        let shown = self.mode_menu.preview.clone().unwrap_or_else(|| current.clone());
        let last = modes.len().saturating_sub(1);
        let mut track = div()
            .debug_selector(|| "mode-segments".into())
            .flex()
            .rounded(px(SEGMENT_TRACK_R))
            .bg(rgba(SEGMENT_TRACK))
            .p(px(SEGMENT_TRACK_PAD));
        for (at, choice) in modes.iter().enumerate() {
            let value = choice.value.clone();
            let hovered = value.clone();
            let checked = value == current;
            let selector = format!("mode-segment-{at}");
            track = track.child(
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .justify_center()
                    .h(px(SEGMENT_H))
                    .px(px(SPACE_2))
                    .when(at == 0, |segment| {
                        segment.rounded_tl(px(R_CONTROL)).rounded_bl(px(R_CONTROL))
                    })
                    .when(at == last, |segment| {
                        segment.rounded_tr(px(R_CONTROL)).rounded_br(px(R_CONTROL))
                    })
                    .text_size(px(FS_BADGE))
                    .text_color(rgb(if checked { INK_STRONG } else { INK_SUBTLE }))
                    .when(checked, |segment| segment.bg(rgba(SEGMENT_CHECKED)))
                    .cursor_pointer()
                    .hover(|style| style.text_color(rgb(INK_STRONG)))
                    .on_hover(cx.listener(move |view, over: &bool, _, cx| {
                        let preview = over.then(|| hovered.clone());
                        if *over || view.mode_menu.preview.as_deref() == Some(hovered.as_str()) {
                            view.mode_menu.preview = preview;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            view.mode_picker = None;
                            view.request_mode(thread, generation, value.clone(), window, cx);
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(pane::permission_mode_label(&choice.value, &modes)),
                    ),
            );
        }
        let preview = div()
            .debug_selector(|| "mode-preview".into())
            .flex()
            .flex_col()
            .gap(px(SPACE_0_5))
            .min_h(px(MODE_PREVIEW_MIN_H))
            .child(
                div()
                    .text_size(px(FS_UI))
                    .text_color(rgb(if shown == current { INK_STRONG } else { INK_SUBTLE }))
                    .child(pane::permission_mode_label(&shown, &modes)),
            )
            .children(ferrite_ui::theme::mode_note(&shown).map(|note| {
                div()
                    .text_size(px(FS_BADGE))
                    .text_color(rgb(INK_SUBTLE))
                    .child(note)
            }));
        let menu = ferrite_ui::menu::float()
            .id("mode-menu")
            .debug_selector(|| "mode-menu".into())
            .w(px(MODE_MENU_W))
            .flex()
            .flex_col()
            .gap(px(SPACE_1_5))
            .px(px(SPACE_2))
            .py(px(SPACE_1_5))
            .font_family(FONT_UI)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .text_size(px(FS_BADGE))
                    .font_weight(W_LABEL)
                    .text_color(rgb(INK_SUBTLE))
                    .child("Permissions"),
            )
            .child(track)
            .child(preview);
        // Over its chip from the chip's left; over the picker's button,
        // from that button's right (T3's `align="end"`).
        let (anchor, x) = if self.mode_menu.from_picker {
            (gpui::Anchor::BottomRight, trigger.right())
        } else {
            (gpui::Anchor::BottomLeft, trigger.left())
        };
        Some(
            deferred(
                anchored()
                    .anchor(anchor)
                    .position(gpui::point(x, trigger.top() - px(FLOAT_OFFSET)))
                    .snap_to_window_with_margin(px(GRID_PAD))
                    .child(ferrite_ui::motion::menu_in(
                        "mode-menu-in",
                        menu,
                        ferrite_ui::motion::Opens::Up,
                    )),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }
}
