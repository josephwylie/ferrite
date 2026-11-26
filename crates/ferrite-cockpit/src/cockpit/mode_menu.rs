//! The permission menu (T3's `ComposerPermissionsSection`): the Session's
//! permission modes as one segmented radio row, the mode under the pointer
//! named and explained under it. The footer's mode chip opens it over
//! itself as a popup; the model picker's footer opens it inside the picker
//! (`ModelPicker::mode`), over its permissions button.
//! A pick sends the native `SetPermissionMode` (`request_mode`) and shuts
//! it; ⇧⇥ still cycles the modes without it. It gets no view (ADR 0041,
//! What does not get a view): the slot holds all it keeps (`Popup::Mode`),
//! and it is drawn from what the root reads of the Session
//! (`ModeSegments`).

use super::*;
use ferrite_ui::theme::*;

/// The open menu: the Session it is on and which mode it previews.
pub(super) struct ModeMenu {
    pub(super) thread: ThreadKey,
    pub(super) generation: u64,
    /// The mode under the pointer; else the menu names the mode in force.
    pub(super) preview: Option<String>,
}

impl CockpitView {
    /// Open the permission menu on `thread`'s Session, or shut it.
    pub(super) fn toggle_mode_menu(&mut self, thread: ThreadKey, generation: u64, cx: &mut Context<Self>) {
        if self
            .mode_menu()
            .is_some_and(|open| (open.thread, open.generation) == (thread, generation))
        {
            self.close_popup(cx);
        } else {
            self.show_popup(
                Popup::Mode(ModeMenu {
                    thread,
                    generation,
                    preview: None,
                }),
                cx,
            );
        }
    }

    /// The menu over its chip, from the chip's left.
    pub(super) fn mode_menu_element(
        &self,
        menu: &ModeMenu,
        face: popups::Face,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let trigger = self.hang_from(&ferrite_pane::cell::mode_chip_trigger(menu.thread), window)?;
        let surface = self.mode_menu_surface(menu, cx)?;
        Some(
            anchored()
                .anchor(gpui::Anchor::BottomLeft)
                .position(gpui::point(trigger.left(), trigger.top() - px(FLOAT_OFFSET)))
                .snap_to_window_with_margin(px(GRID_PAD))
                .child(self.popup_face(face, "mode-menu-in", surface, ferrite_ui::motion::Opens::Up, cx))
                .into_any_element(),
        )
    }

    /// The menu the model picker's footer opened, over its permissions
    /// button from that button's right (T3's `align="end"`). A press
    /// outside it shuts it alone; one on it is the picker's own.
    pub(super) fn picker_mode_element(
        &self,
        menu: &ModeMenu,
        face: popups::Face,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let trigger = self.hang_from(model_picker::PICKER_PERMISSIONS, window)?;
        let surface = self.mode_menu_surface(menu, cx)?;
        Some(
            anchored()
                .anchor(gpui::Anchor::BottomRight)
                .position(gpui::point(trigger.right(), trigger.top() - px(FLOAT_OFFSET)))
                .snap_to_window_with_margin(px(GRID_PAD))
                .child(ferrite_ui::components::on_bounds(
                    div().child(self.popup_face(
                        face,
                        popups::PICKER_MODE,
                        surface,
                        ferrite_ui::motion::Opens::Up,
                        cx,
                    )),
                    self.record_trigger(popups::PICKER_MODE.into()),
                ))
                .into_any_element(),
        )
    }

    /// The menu for `menu`'s Session, none once another one runs.
    fn mode_menu_surface(&self, menu: &ModeMenu, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        Some(ModeSegments::of(menu, &self.cockpit.read(cx).thread(menu.thread)?)?.render(cx))
    }
}

/// The menu as it reads: the Session's modes in Claude's order (`Auto`
/// first), the mode in force and the mode it names, the one under the
/// pointer or else the one in force.
struct ModeSegments {
    thread: ThreadKey,
    generation: u64,
    modes: Vec<ferrite_core::PermissionModeChoice>,
    current: String,
    shown: String,
}

impl ModeSegments {
    fn of(menu: &ModeMenu, open: &ThreadView) -> Option<Self> {
        if open.generation() != menu.generation {
            return None;
        }
        let mut modes = open.permission_modes();
        modes.sort_by_key(|choice| choice.value != "auto");
        let current = open.permission_mode().unwrap_or("default").to_string();
        Some(Self {
            thread: menu.thread,
            generation: menu.generation,
            shown: menu.preview.clone().unwrap_or_else(|| current.clone()),
            modes,
            current,
        })
    }

    fn render(self, cx: &mut Context<CockpitView>) -> Stateful<Div> {
        let Self {
            thread,
            generation,
            modes,
            current,
            shown,
        } = self;
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
                        let Some(preview) = view.mode_preview_mut() else {
                            return;
                        };
                        if *over || preview.as_deref() == Some(hovered.as_str()) {
                            *preview = over.then(|| hovered.clone());
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            view.dismiss_popup(cx);
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
            .child(
                div()
                    .text_size(px(FS_BADGE))
                    .font_weight(W_LABEL)
                    .text_color(rgb(INK_SUBTLE))
                    .child("Permissions"),
            )
            .child(track)
            .child(preview);
        menu
    }
}

impl CockpitView {
    /// A pick of permission mode `mode`. A mode that stops asking
    /// (`theme::mode_stops_asking`) is confirmed first, as Codex confirms
    /// Full access; any other goes at once.
    pub(super) fn request_mode(
        &mut self,
        thread: ThreadKey,
        generation: u64,
        mode: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let set = ferrite_core::SessionControl::SetPermissionMode { mode: mode.clone() };
        let standing = self
            .cockpit
            .read(cx)
            .thread(thread)
            .is_some_and(|open| open.permission_mode() == Some(mode.as_str()));
        if standing || !ferrite_ui::theme::mode_stops_asking(&mode) {
            self.run_session_control(thread, generation, set, cx);
            return;
        }
        let answer = window.prompt(
            gpui::PromptLevel::Warning,
            "Stop asking before changes?",
            Some("The agent will run commands and edit files without asking you first. Only do this in a folder you trust."),
            &["Turn on", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |view, cx| {
            if answer.await == Ok(0) {
                let _ = view.update(cx, |view, cx| {
                    view.run_session_control(thread, generation, set, cx);
                    cx.notify();
                });
            }
        })
        .detach();
    }
}
