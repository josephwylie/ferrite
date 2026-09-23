//! Longbridge Settings layout with Ferrite controls and theme tokens.
//! Values and persistence remain owned by the cockpit.

use gpui::prelude::*;
use gpui::{div, point, px, rgb, rgba, App, Axis, BoxShadow, Div, SharedString};

use gpui::component::button::Button;
use gpui::component::menu::{DropdownMenu, PopupMenuItem};
use gpui::component::setting::{SettingField, SettingGroup, SettingItem, SettingPage, Settings};
use gpui::component::{ActiveTheme, Selectable, Sizable};
use gpui_base::{spring, Switch, SwitchThumb, SwitchTrack};
use std::rc::Rc;

use crate::components;
use crate::icons::{self, icon};
use crate::theme::{
    FILL, FILL_HOVER, FONT_MONO, FONT_UI, FORM_CHOICE_PAD, FORM_CONTROL_H, FORM_FIELD_W, FS_SM,
    FS_UI, ICON_BUTTON, ICON_BUTTON_GLYPH, MENU, MODAL_HEAD_H, MODAL_PAD, MODAL_VIEWPORT_FRACTION,
    PANE, R_BLOCK, R_CHIP, R_CONTROL, SHADOW_FAR, SHADOW_FAR_BLUR, SHADOW_FAR_SPREAD, SHADOW_FAR_Y,
    SHADOW_NEAR, SHADOW_NEAR_BLUR, SHADOW_NEAR_Y, TEXT, TEXT_2, TEXT_MUTED, TEXT_STRONG, W_LABEL,
};

/// The card's width; tall enough sections scroll inside it.
pub const WIDTH: f32 = 820.0;
const ROW_GAP: f32 = 8.0;
const SIDEBAR_WIDTH: f32 = 172.0;

/// The dim veil over the Cockpit while the panel is up: a press on it
/// closes the panel (the cockpit wires that).
pub fn veil() -> Div {
    div()
        .absolute()
        .inset_0()
        // The modal veil covers the cockpit, including selectable transcript
        // text, and therefore owns the neutral cursor outside the card too.
        .cursor_default()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(0x0000008c))
}

/// The card: menu ground, the float shadow, no border.
pub fn card() -> Div {
    div()
        .flex()
        .flex_col()
        .w(px(WIDTH))
        .max_w(gpui::relative(MODAL_VIEWPORT_FRACTION))
        .h(px(680.))
        .max_h(gpui::relative(MODAL_VIEWPORT_FRACTION))
        .overflow_hidden()
        .text_size(px(FS_UI))
        .text_color(rgb(TEXT))
        .rounded(px(R_BLOCK))
        .bg(rgb(MENU))
        .font_family(FONT_UI)
        .shadow(vec![
            BoxShadow {
                inset: false,
                color: rgba(SHADOW_FAR).into(),
                offset: point(px(0.), px(SHADOW_FAR_Y)),
                blur_radius: px(SHADOW_FAR_BLUR),
                spread_radius: px(SHADOW_FAR_SPREAD),
            },
            BoxShadow {
                inset: false,
                color: rgba(SHADOW_NEAR).into(),
                offset: point(px(0.), px(SHADOW_NEAR_Y)),
                blur_radius: px(SHADOW_NEAR_BLUR),
                spread_radius: px(0.),
            },
        ])
}

/// The head: the title, the escape hint, the close button (wired by the
/// caller).
pub fn head(close: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(MODAL_HEAD_H))
        .pl(px(MODAL_PAD))
        .pr(px(MODAL_PAD))
        .gap(px(ROW_GAP))
        .child(
            div()
                .text_size(px(FS_UI))
                .font_weight(W_LABEL)
                .text_color(rgb(TEXT_STRONG))
                .child("Settings"),
        )
        .child(div().flex_1())
        .child(
            div()
                .text_size(px(FS_SM))
                .font_family(FONT_MONO)
                .text_color(rgb(TEXT_MUTED))
                .child("esc close"),
        )
        .child(close)
}

/// The 28px close button.
pub fn close_button(cx: &App) -> Button {
    components::form_button("settings-close", cx)
        .debug_selector(|| "settings-close".into())
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .tooltip("Close Settings")
        .child(icon(icons::CLOSE, ICON_BUTTON_GLYPH, TEXT_MUTED))
}

/// Categories are native Settings pages, so navigation changes pages without
/// relying on estimated positions in a virtualized list. Search spans them all.
pub fn body(pages: Vec<SettingPage>) -> Div {
    // The sidebar paints its own background, so it must own this corner too:
    // GPUI's overflow mask alone does not clip descendants to rounded corners.
    let sidebar = gpui::StyleRefinement::default()
        .bg(rgb(MENU))
        .rounded_bl(px(R_BLOCK));
    let settings = Settings::new("ferrite-settings")
        .small()
        .sidebar_width(px(SIDEBAR_WIDTH))
        .sidebar_size_range(px(160.)..px(216.))
        .sidebar_style(&sidebar);
    let settings = pages
        .into_iter()
        .fold(settings, |settings, page| settings.page(page));
    div().flex_1().min_h_0().child(settings)
}

pub fn page(title: &'static str, groups: Vec<SettingGroup>) -> SettingPage {
    SettingPage::new(title).resettable(false).groups(groups)
}

pub fn choices<T: Clone + 'static>(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    options: Vec<(SharedString, bool, T)>,
    change: impl Fn(T, &mut App) + 'static,
) -> SettingItem {
    // Long ladders are values to choose, not a second row of navigation.
    // The same selector as models keeps effort and permissions compact even
    // when the Settings sidebar leaves a narrow content column.
    if options.len() > 3 {
        return chooser(id, title, detail, options, change);
    }
    let change = Rc::new(change);
    let keywords: Vec<_> = options.iter().map(|(label, _, _)| label.clone()).collect();
    SettingItem::new(
        title,
        SettingField::render(move |_, _, cx| {
            let tray = div()
                .flex()
                .flex_wrap()
                .max_w(gpui::relative(1.))
                .gap(px(2.))
                .p(px(FORM_CHOICE_PAD))
                .rounded(px(R_CONTROL))
                .border_1()
                .border_color(rgb(FILL))
                .bg(rgb(PANE))
                .children(
                    options
                        .iter()
                        .enumerate()
                        .map(|(at, (label, selected, value))| {
                            let value = value.clone();
                            let change = change.clone();
                            chip((id, at), label.clone(), *selected, cx).on_click(
                                move |_, _, cx| {
                                    cx.stop_propagation();
                                    change(value.clone(), cx);
                                },
                            )
                        }),
                );
            div().flex().child(tray)
        }),
    )
    .description(detail.into())
    .keywords(keywords)
    .layout(Axis::Vertical)
}

/// A longer option list exposes its current value first; the menu retains
/// every available value, and Settings search also indexes the hidden labels.
pub fn chooser<T: Clone + 'static>(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    options: Vec<(SharedString, bool, T)>,
    change: impl Fn(T, &mut App) + 'static,
) -> SettingItem {
    let change = Rc::new(change);
    let keywords: Vec<_> = options.iter().map(|(label, _, _)| label.clone()).collect();
    let selected = options
        .iter()
        .find(|(_, selected, _)| *selected)
        .map(|(label, _, _)| label.clone())
        .unwrap_or_else(|| "Choose an option".into());
    SettingItem::new(
        title,
        SettingField::render(move |_, _, cx| {
            let options = options.clone();
            let change = change.clone();
            components::form_button(id, cx)
                .debug_selector(move || id.into())
                .accessibility_label(format!("{title}: {selected}"))
                .h(px(FORM_CONTROL_H))
                .w_full()
                .max_w(px(FORM_FIELD_W))
                .px(px(10.))
                .bg(rgb(PANE))
                .border_1()
                .border_color(rgb(FILL))
                .dropdown_caret(true)
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(components::form_label(selected.clone(), TEXT_STRONG)),
                )
                .dropdown_menu(move |menu, _, _| {
                    options.iter().fold(
                        menu.min_w(px(240.))
                            .max_w(px(360.))
                            .max_h(px(320.))
                            .scrollable(true),
                        |menu, (label, selected, value)| {
                            let value = value.clone();
                            let change = change.clone();
                            menu.item(
                                PopupMenuItem::new(label.clone())
                                    .checked(*selected)
                                    .on_click(move |_, _, cx| {
                                        cx.stop_propagation();
                                        change(value.clone(), cx);
                                    }),
                            )
                        },
                    )
                })
        }),
    )
    .description(detail.into())
    .keywords(keywords)
    .layout(Axis::Vertical)
}

/// Keep the effective selected value represented even when it is an alias or
/// absent from the current catalog. No choice is silently made for the user.
pub fn model_options(
    catalog: Vec<ferrite_core::ModelInfo>,
    chosen: Option<&str>,
) -> Vec<(SharedString, bool, Option<String>)> {
    let chosen = chosen.filter(|value| *value != "default");
    let mut represented = chosen.is_none();
    let mut options = vec![("CLI default".into(), represented, None)];
    for model in catalog.into_iter().filter(|model| model.value != "default") {
        let selected = !represented && chosen.is_some_and(|chosen| model.is(chosen));
        represented |= selected;
        options.push((model.display.into(), selected, Some(model.value)));
    }
    if !represented {
        let chosen = chosen.expect("the default always has a row");
        options.push((chosen.to_string().into(), true, Some(chosen.to_string())));
    }
    options
}

pub fn toggle(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    checked: bool,
    change: impl Fn(bool, &mut App) + 'static,
) -> SettingItem {
    let change = Rc::new(change);
    SettingItem::new(
        title,
        SettingField::render(move |_, window, cx| {
            let change = change.clone();
            let thumb_x = spring(
                (id, "thumb"),
                px(if checked { 12. } else { 0. }),
                cx.theme().motion_tokens().spring_move,
                window,
                cx,
            );
            div().id(id).debug_selector(move || id.into()).child(
                Switch::new(id)
                    .checked(checked)
                    .accessibility_label(title)
                    .p(px(4.))
                    .rounded(px(R_CONTROL))
                    .cursor_pointer()
                    .focus_visible(components::control_focus)
                    .hover(|style| style.bg(rgb(FILL)))
                    .on_change(move |value, _, _, cx| {
                        cx.stop_propagation();
                        change(value, cx);
                    })
                    .child(
                        SwitchTrack::new((gpui::ElementId::from(id), "track"))
                            .checked(checked)
                            .w(px(28.))
                            .h(px(16.))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .border(px(2.))
                            .border_color(rgba(crate::theme::TRANSPARENT))
                            .bg(rgb(if checked { FILL_HOVER } else { PANE }))
                            .child(
                                SwitchThumb::new(checked)
                                    .rounded_full()
                                    .size(px(12.))
                                    .left(thumb_x)
                                    .bg(rgb(TEXT_STRONG)),
                            ),
                    ),
            )
        }),
    )
    .description(detail.into())
}

/// The stable check slot keeps neighboring choices still when selection moves.
pub fn chip(id: (&'static str, usize), label: SharedString, selected: bool, cx: &App) -> Button {
    components::form_button(id, cx)
        .selected(selected)
        .toggled(selected)
        .debug_selector(move || format!("{}-{}", id.0, id.1))
        .h(px(FORM_CONTROL_H - 2. * (FORM_CHOICE_PAD + 1.)))
        .px(px(9.))
        .rounded(px(R_CHIP))
        .bg(rgb(if selected { FILL } else { PANE }))
        .when(selected, |button| {
            button.hover(|style| style.bg(rgb(FILL_HOVER)))
        })
        .child(
            div()
                .flex_shrink_0()
                .w(px(12.))
                .h(px(12.))
                .when(selected, |slot| {
                    slot.child(icon(icons::CHECK, 12., TEXT_STRONG))
                }),
        )
        .child(components::form_label(
            label,
            if selected { TEXT_STRONG } else { TEXT_2 },
        ))
}

/// Read-only values remain searchable and wrap so full paths are readable.
pub fn fact(title: &'static str, value: SharedString) -> SettingItem {
    SettingItem::new(
        title,
        SettingField::render(move |_, _, _| {
            div()
                .id(title)
                .debug_selector(move || format!("settings-fact-{title}"))
                .font_family(FONT_MONO)
                .text_size(px(FS_SM))
                .text_color(rgb(TEXT_2))
                .child(value.clone())
        }),
    )
    .layout(Axis::Vertical)
}

/// The nav chrome's gear: the door to this panel.
pub fn gear_button() -> Button {
    components::button("settings-gear")
        .debug_selector(|| "settings-gear".into())
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .tooltip("Settings")
        .child(icon(icons::GEAR, ICON_BUTTON_GLYPH, TEXT_MUTED))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_choices_keep_default_alias_and_custom_values_selected() {
        use ferrite_core::{providers::models::fallback, store::Provider};
        for provider in [Provider::Claude, Provider::Codex] {
            let options = model_options(fallback(provider), None);
            assert_eq!(options.iter().filter(|(_, on, _)| *on).count(), 1);
            assert_eq!(options[0], ("CLI default".into(), true, None));
        }
        let options = model_options(fallback(Provider::Claude), Some("claude-sonnet-5"));
        let selected: Vec<_> = options.iter().filter(|(_, on, _)| *on).collect();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].2.as_deref(), Some("sonnet"));
        let options = model_options(fallback(Provider::Codex), Some("private-model"));
        assert_eq!(
            options.last().unwrap(),
            &("private-model".into(), true, Some("private-model".into()))
        );
        assert_eq!(options.iter().filter(|(_, on, _)| *on).count(), 1);
    }
}
