//! Longbridge Settings layout with Ferrite controls and theme tokens.
//! Values and persistence remain owned by the cockpit.

use gpui::prelude::*;
use gpui::{div, point, px, rgb, rgba, App, Axis, BoxShadow, Div, FontWeight, SharedString};

use gpui::component::button::Button;
use gpui::component::setting::{SettingField, SettingGroup, SettingItem, SettingPage, Settings};
use gpui::component::{switch::Switch, Sizable};
use std::rc::Rc;

use crate::components;
use crate::icons::{self, icon};
use crate::theme::{
    FILL, FONT_MONO, FONT_UI, FS_LG, FS_MD, FS_MONO, ICON_BUTTON, ICON_BUTTON_GLYPH, MENU,
    MENU_PAD, RAISED, R_CHIP, R_MENU, SHADOW_FAR, SHADOW_FAR_BLUR, SHADOW_FAR_SPREAD, SHADOW_FAR_Y,
    SHADOW_NEAR, SHADOW_NEAR_BLUR, SHADOW_NEAR_Y, TEXT, TEXT_2, TEXT_MUTED, TEXT_STRONG,
};

/// The card's width; tall enough sections scroll inside it.
pub const WIDTH: f32 = 820.0;
const HEAD_H: f32 = 44.0;
const PAD: f32 = 16.0;
const ROW_GAP: f32 = 8.0;
const CHIP_H: f32 = 22.0;

/// The dim veil over the Cockpit while the panel is up: a press on it
/// closes the panel (the cockpit wires that).
pub fn veil() -> Div {
    div()
        .absolute()
        .inset_0()
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
        .max_w(gpui::relative(0.94))
        .h(px(680.))
        .max_h(relative_h())
        .overflow_hidden()
        .text_size(px(FS_MD))
        .text_color(rgb(TEXT))
        .rounded(px(R_MENU))
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

fn relative_h() -> gpui::DefiniteLength {
    gpui::relative(0.86)
}

/// The head: the title, the escape hint, the close button (wired by the
/// caller).
pub fn head(close: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(HEAD_H))
        .pl(px(PAD))
        .pr(px(MENU_PAD + 4.))
        .gap(px(ROW_GAP))
        .child(
            div()
                .text_size(px(FS_LG))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT_STRONG))
                .child("Settings"),
        )
        .child(div().flex_1())
        .child(
            div()
                .text_size(px(FS_MONO))
                .font_family(FONT_MONO)
                .text_color(rgb(TEXT_MUTED))
                .child("esc close"),
        )
        .child(close)
}

/// The 28px close button.
pub fn close_button() -> Button {
    components::button("settings-close")
        .debug_selector(|| "settings-close".into())
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .tooltip("Close Settings")
        .child(icon(icons::CLOSE, ICON_BUTTON_GLYPH, TEXT_MUTED))
}

/// Categories are native Settings pages, so navigation changes pages without
/// relying on estimated positions in a virtualized list. Search spans them all.
pub fn body(groups: Vec<SettingGroup>) -> Div {
    let mut sidebar = gpui::StyleRefinement::default();
    sidebar.background = Some(rgb(MENU).into());
    let settings = Settings::new("ferrite-settings")
        .small()
        .sidebar_width(px(172.))
        .sidebar_style(&sidebar);
    let settings = groups
        .into_iter()
        .zip(["New Threads", "Permissions", "Behaviour", "About"])
        .fold(settings, |settings, (group, title)| {
            settings.page(
                SettingPage::new(title)
                    .resettable(false)
                    .groups(vec![group]),
            )
        });
    div().flex_1().min_h_0().child(settings)
}

pub fn choices<T: Clone + 'static>(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    options: Vec<(SharedString, bool, T)>,
    change: impl Fn(T, &mut App) + 'static,
) -> SettingItem {
    let change = Rc::new(change);
    SettingItem::new(
        title,
        SettingField::render(move |_, _, _| {
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.))
                .children(
                    options
                        .iter()
                        .enumerate()
                        .map(|(at, (label, selected, value))| {
                            let value = value.clone();
                            let change = change.clone();
                            chip((id, at), label.clone(), *selected).on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                change(value.clone(), cx);
                            })
                        }),
                )
        }),
    )
    .description(detail.into())
    .layout(Axis::Vertical)
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
        SettingField::render(move |_, _, _| {
            let change = change.clone();
            div().id(id).debug_selector(move || id.into()).child(
                Switch::new(id)
                    .small()
                    .checked(checked)
                    .on_click(move |value, _, cx| {
                        cx.stop_propagation();
                        change(*value, cx);
                    }),
            )
        }),
    )
    .description(detail.into())
}

/// One choice chip: the selected one carries the fill and the strong ink.
pub fn chip(id: (&'static str, usize), label: SharedString, selected: bool) -> Button {
    components::button(id)
        .tab_stop(true)
        .debug_selector(move || format!("{}-{}", id.0, id.1))
        .h(px(CHIP_H))
        .px(px(9.))
        .rounded(px(R_CHIP))
        .bg(rgb(if selected { FILL } else { RAISED }))
        .child(components::label(
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
                .text_size(px(FS_MONO))
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
    fn the_selected_chip_carries_the_fill_and_the_rest_the_raised_ground() {
        let mut on = chip(("chip", 0), "Claude".into(), true);
        assert_eq!(on.style().background, Some(rgb(FILL).into()));
        let mut off = chip(("chip", 1), "Codex".into(), false);
        assert_eq!(off.style().background, Some(rgb(RAISED).into()));
    }
}
