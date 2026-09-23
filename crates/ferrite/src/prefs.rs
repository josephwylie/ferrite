//! Longbridge Settings layout with Ferrite controls and theme tokens.
//! Values and persistence remain owned by the cockpit.
//!
//! **The sheet recipe** (Settings, the Project editor): a `VEIL` over the
//! Cockpit; the sheet `RAISED` with a `HAIRLINE_STRONG` edge, `R_PANE` and
//! the float shadow; a `MODAL_HEAD_H` head (the one `W_LABEL` title, an
//! `esc` keycap, the close button) over a hairline; a scrolling body; a
//! footer pinned under a hairline. Form rows set the label in mono `FS_UI`
//! and its description in prose at `FS_PROSE_SM`, controls are
//! `FORM_CONTROL_H`, and a choice shows its selection in the accent.

use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, App, Axis, Div, SharedString};

use gpui::component::button::Button;
use gpui::component::menu::{DropdownMenu, PopupMenuItem};
use gpui::component::setting::{SettingGroup, SettingItem, SettingPage, Settings};
use gpui::component::{ActiveTheme, Selectable, Sizable};
use gpui_base::{spring, Switch, SwitchThumb, SwitchTrack};
use std::rc::Rc;

use crate::components::{self, MenuItem};
use crate::icons::{self, icon};
use crate::theme::*;

/// The card's width; tall enough sections scroll inside it.
pub const WIDTH: f32 = 820.0;
pub const HEIGHT: f32 = 680.0;
const SIDEBAR_WIDTH: f32 = 172.0;

// ------------------------------------------------------------ the sheet

/// The dim veil over the Cockpit while a sheet is up: a press on it
/// closes the sheet (the cockpit wires that). It covers selectable
/// transcript text, so it owns the neutral cursor outside the sheet too.
pub fn veil() -> Div {
    components::veil().cursor_default()
}

/// A modal sheet, `width` × `height` at most `MODAL_VIEWPORT_FRACTION` of
/// the window: `RAISED`, the strong hairline edge, `R_PANE`, the float
/// shadow, UI type. Its definite height is what lets the body scroll.
pub fn sheet(width: f32, height: f32) -> Div {
    components::text_ui()
        .flex()
        .flex_col()
        .w(px(width))
        .max_w(gpui::relative(MODAL_VIEWPORT_FRACTION))
        .h(px(height))
        .max_h(gpui::relative(MODAL_VIEWPORT_FRACTION))
        .overflow_hidden()
        .rounded(px(R_PANE))
        .bg(rgb(RAISED))
        .border_1()
        .border_color(rgba(HAIRLINE_STRONG))
        .shadow(components::float_shadow())
}

/// A sheet's head: its title (the sheet's one `W_LABEL` line), then the
/// `esc` keycap and the close button, over a hairline.
pub fn sheet_head(title: impl Into<SharedString>, close: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(MODAL_GAP))
        .h(px(MODAL_HEAD_H))
        .pl(px(MODAL_PAD))
        .pr(px(SPACE_2))
        .border_b_1()
        .border_color(rgba(HAIRLINE))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .font_weight(W_LABEL)
                .text_color(rgb(TEXT_STRONG))
                .child(title.into()),
        )
        .child(components::kbd("esc"))
        .child(close)
}

/// A sheet's close button: the close glyph in a 28px square that lifts to
/// `FILL` on the raised sheet.
pub fn sheet_close(id: &'static str, tooltip: &'static str, cx: &App) -> Button {
    components::form_button(id, cx)
        .debug_selector(move || id.into())
        .w(px(ICON_BUTTON))
        .h(px(ICON_BUTTON))
        .p_0()
        .tooltip(tooltip)
        .accessibility_label(tooltip)
        .child(icon(icons::CLOSE, ICON_BUTTON_GLYPH, TEXT_MUTED))
}

/// The pinned footer under a hairline: secondary actions left, the
/// completing action right. It never scrolls with the body.
pub fn sheet_footer(left: impl IntoElement, right: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .gap(px(MODAL_GAP))
        .px(px(MODAL_PAD))
        .py(px(MODAL_GAP))
        .border_t_1()
        .border_color(rgba(HAIRLINE))
        .child(left)
        .child(right)
}

/// A form row's label block: the label in UI `FS_UI` `TEXT`, and its
/// description under it in prose at `FS_PROSE_SM` (prose is never smaller).
pub fn form_text(title: impl Into<SharedString>, detail: Option<SharedString>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(SPACE_0_5))
        .min_w_0()
        .child(components::text_ui().child(title.into()))
        .children(detail.map(|detail| {
            div()
                .font_family(FONT_UI)
                .text_size(px(FS_PROSE_SM))
                .line_height(px(LH_PROSE_SM))
                .text_color(rgb(TEXT_MUTED))
                .child(detail)
        }))
}

/// One form row: its label block beside the control (a switch), or over it
/// (a choice, a chooser, a field).
fn form_row(
    title: &'static str,
    detail: SharedString,
    axis: Axis,
    control: impl IntoElement,
) -> Div {
    let text = form_text(title, (!detail.is_empty()).then_some(detail));
    match axis {
        Axis::Horizontal => div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(SPACE_4))
            .child(text.flex_1().max_w(gpui::relative(FORM_TEXT_FRACTION)))
            .child(div().flex_shrink_0().child(control)),
        Axis::Vertical => div()
            .flex()
            .flex_col()
            .gap(px(SPACE_2))
            .child(text)
            .child(control),
    }
}

/// A form row as a Settings item. Ferrite sets all of its type; the kit
/// only lays items out and searches them, so the title, the description and
/// every option label ride as keywords.
fn form_item(
    title: &'static str,
    detail: SharedString,
    keywords: Vec<SharedString>,
    axis: Axis,
    control: impl Fn(&mut gpui::Window, &mut App) -> gpui::AnyElement + 'static,
) -> SettingItem {
    let words: Vec<SharedString> = [SharedString::from(title), detail.clone()]
        .into_iter()
        .chain(keywords)
        .collect();
    SettingItem::render(move |_, window, cx| {
        form_row(title, detail.clone(), axis, control(window, cx))
    })
    .keywords(words)
}

// ------------------------------------------------------------ Settings

/// Categories are native Settings pages, so navigation changes pages without
/// relying on estimated positions in a virtualized list. Search spans them all.
pub fn body(pages: Vec<SettingPage>) -> Div {
    // The sidebar paints its own background, so it must own this corner too:
    // GPUI's overflow mask alone does not clip descendants to rounded corners.
    let sidebar = gpui::StyleRefinement::default()
        .bg(rgb(RAISED))
        .border_r_1()
        .border_color(rgba(HAIRLINE))
        .rounded_bl(px(R_PANE - 1.));
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

/// A page, its header in the UI voice over the one rule weight.
pub fn page(title: &'static str, groups: Vec<SettingGroup>) -> SettingPage {
    let header = gpui::StyleRefinement::default()
        .px(px(MODAL_PAD))
        .py(px(SPACE_3))
        .border_color(rgba(HAIRLINE))
        .text_size(px(FS_UI))
        .text_color(rgb(TEXT_STRONG));
    SettingPage::new(title)
        .resettable(false)
        .header_style(&header)
        .groups(groups)
}

/// A group of items. Its title is a section header: UI `FS_SM` in the
/// kit's muted ink, sentence case (the items set their own type).
pub fn group(title: Option<&'static str>) -> SettingGroup {
    let group = SettingGroup::new()
        .font_family(FONT_UI)
        .text_size(px(FS_SM));
    match title {
        Some(title) => group.title(title),
        None => group,
    }
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
    form_item(
        title,
        detail.into(),
        keywords,
        Axis::Vertical,
        move |_, cx| {
            let tray = div()
                .flex()
                .flex_wrap()
                .max_w(gpui::relative(1.))
                .gap(px(SPACE_0_5))
                .p(px(FORM_CHOICE_PAD))
                .rounded(px(R_CONTROL))
                .border_1()
                .border_color(rgba(HAIRLINE_STRONG))
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
            div().flex().child(tray).into_any_element()
        },
    )
}

/// A longer option list exposes its current value first; the menu retains
/// every available value, and Settings search also indexes the hidden labels.
/// The menu's rows are the one menu row, the standing value checked in the
/// accent.
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
    form_item(
        title,
        detail.into(),
        keywords,
        Axis::Vertical,
        move |_, cx| {
            let options = options.clone();
            let change = change.clone();
            components::form_button(id, cx)
                .debug_selector(move || id.into())
                .accessibility_label(format!("{title}: {selected}"))
                .h(px(FORM_CONTROL_H))
                .w_full()
                .max_w(px(FORM_FIELD_W))
                .px(px(FORM_FIELD_PAD_X))
                .bg(rgb(PANE))
                .border_1()
                .border_color(rgba(HAIRLINE_STRONG))
                .dropdown_caret(true)
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(components::form_label(selected.clone(), TEXT_STRONG)),
                )
                .dropdown_menu(move |menu, _, _| {
                    options.iter().fold(
                        menu.min_w(px(CHOICE_MENU_MIN_W))
                            .max_w(px(CHOICE_MENU_MAX_W))
                            .max_h(px(MENU_MAX_H))
                            .scrollable(true),
                        |menu, (label, selected, value)| {
                            let value = value.clone();
                            let change = change.clone();
                            let item = MenuItem::new(label.clone()).checked(*selected);
                            menu.item(
                                PopupMenuItem::element(move |_, _| {
                                    components::kit_row(components::menu_row_content(
                                        &item, false, false,
                                    ))
                                })
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    change(value.clone(), cx);
                                }),
                            )
                        },
                    )
                })
                .into_any_element()
        },
    )
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

/// A switch beside its label. Custom rather than the kit's (whose tokens do
/// not reach it): the track `ACCENT_STRONG` when on and `RAISED_2` when off,
/// so both read on the raised sheet; the thumb `TEXT_STRONG`, sprung.
pub fn toggle(
    id: &'static str,
    title: &'static str,
    detail: impl Into<SharedString>,
    checked: bool,
    change: impl Fn(bool, &mut App) + 'static,
) -> SettingItem {
    let change = Rc::new(change);
    form_item(
        title,
        detail.into(),
        Vec::new(),
        Axis::Horizontal,
        move |window, cx| {
            let change = change.clone();
            let thumb_x = spring(
                (id, "thumb"),
                px(if checked { SWITCH_TRAVEL } else { 0. }),
                cx.theme().motion_tokens().spring_move,
                window,
                cx,
            );
            div()
                .id(id)
                .debug_selector(move || id.into())
                .child(
                    Switch::new(id)
                        .checked(checked)
                        .accessibility_label(title)
                        .p(px(SPACE_1))
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
                                .w(px(SWITCH_W))
                                .h(px(SWITCH_H))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .border(px(SWITCH_INSET))
                                .border_color(rgba(TRANSPARENT))
                                .bg(rgb(switch_track(checked)))
                                .child(
                                    SwitchThumb::new(checked)
                                        .rounded_full()
                                        .size(px(SWITCH_THUMB))
                                        .left(thumb_x)
                                        .bg(rgb(TEXT_STRONG)),
                                ),
                        ),
                )
                .into_any_element()
        },
    )
}

/// The switch track: steel when on, one step above the sheet when off.
fn switch_track(checked: bool) -> u32 {
    if checked {
        ACCENT_STRONG
    } else {
        RAISED_2
    }
}

/// One option of a segmented choice, inside the tray. Its 1px edge is always
/// in layout, so selection never moves a neighbour: selected wears the
/// accent wash and edge with `TEXT_STRONG`, the rest `TEXT_2` on the tray.
pub fn chip(id: (&'static str, usize), label: SharedString, selected: bool, cx: &App) -> Button {
    let (ink, ground, edge) = components::choice_inks(selected);
    components::form_button(id, cx)
        .selected(selected)
        .toggled(selected)
        .debug_selector(move || format!("{}-{}", id.0, id.1))
        .h(px(FORM_CONTROL_H - 2. * (FORM_CHOICE_PAD + 1.)))
        .px(px(FORM_CHIP_PAD_X))
        .rounded(px(R_CHIP))
        .border_1()
        .border_color(rgba(if selected { edge } else { TRANSPARENT }))
        .when_some(ground, |chip, ground| chip.bg(rgba(ground)))
        .child(components::form_label(label, ink))
}

/// A read-only fact: its key in a muted column, its value UI `TEXT_2`
/// wrapping anywhere, so a full path stays readable. Searchable by both.
pub fn fact(title: &'static str, value: SharedString) -> SettingItem {
    let words = [SharedString::from(title), value.clone()];
    SettingItem::render(move |_, _, _| {
        div()
            .flex()
            .items_start()
            .gap(px(SPACE_3))
            .child(
                components::text_ui()
                    .flex_shrink_0()
                    .w(px(FACT_KEY_W))
                    .text_color(rgb(TEXT_MUTED))
                    .child(title),
            )
            .child(
                components::text_ui()
                    .id(title)
                    .debug_selector(move || format!("settings-fact-{title}"))
                    .flex_1()
                    .min_w_0()
                    .text_color(rgb(TEXT_2))
                    .child(value.clone()),
            )
    })
    .keywords(words)
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

    #[test]
    fn the_sheet_is_raised_edged_and_rounded_over_the_veil() {
        let mut drawn = sheet(WIDTH, HEIGHT);
        let style = drawn.style();
        assert_eq!(style.background, Some(rgb(RAISED).into()));
        assert_eq!(style.border_color, Some(rgba(HAIRLINE_STRONG).into()));
        assert_eq!(style.box_shadow, Some(components::float_shadow()));
        assert_eq!(
            style.corner_radii.top_left,
            Some(px(R_PANE).into()),
            "a sheet is a Pane-sized surface"
        );
        assert_eq!(veil().style().background, Some(rgba(VEIL).into()));
    }

    #[test]
    fn the_switch_is_steel_when_on_and_reads_on_the_sheet_when_off() {
        assert_eq!(switch_track(true), ACCENT_STRONG);
        assert_eq!(switch_track(false), RAISED_2);
        assert_ne!(switch_track(false), RAISED, "off must not vanish");
    }

    #[gpui::test]
    fn a_selected_choice_wears_the_accent_and_every_choice_keeps_its_edge(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            crate::theme::init_components(cx);
            let mut on = chip(("c", 0), "Claude".into(), true, cx);
            let mut off = chip(("c", 1), "Codex".into(), false, cx);
            assert_eq!(on.style().background, Some(rgba(ACCENT_WASH).into()));
            assert_eq!(on.style().border_color, Some(rgba(ACCENT_EDGE).into()));
            assert_eq!(off.style().background, None);
            assert_eq!(off.style().border_color, Some(rgba(TRANSPARENT).into()));
            for chip in [&mut on, &mut off] {
                let widths = chip.style().border_widths.clone();
                assert_eq!(widths.left, Some(px(1.).into()));
            }
        });
    }
}
