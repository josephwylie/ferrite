//! Longbridge controls in Ferrite's visual language. The toolkit owns the
//! control mechanics; the existing theme remains the only token source.

use gpui::prelude::*;
use gpui::{div, px, rgb, ElementId, SharedString};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::Sizable;

use crate::theme;

/// A compact, neutral button. Supply content with its own typography so
/// upstream control sizes and hover foregrounds cannot recolour the label.
pub fn button(id: impl Into<ElementId>) -> Button {
    Button::new(id)
        .ghost()
        .xsmall()
        .tab_stop(false)
        .border_0()
        .rounded(px(theme::R_CONTROL))
        .font_family(theme::FONT_UI)
        .cursor_pointer()
}

pub fn label(text: impl Into<SharedString>, ink: u32) -> impl IntoElement {
    div()
        .text_size(px(theme::FS_SM))
        .line_height(gpui::relative(theme::LINE_UI))
        .text_color(rgb(ink))
        .child(text.into())
}

/// GPUI Component 0.5.1 scrolls to a group's estimated bottom. Measure this
/// small Settings list up front so its sidebar reaches offscreen groups on
/// the first click. This keyed-state name belongs to the pinned toolkit's
/// single SettingPage (page.rs); keep the navigation regression on upgrades.
#[derive(IntoElement)]
pub struct MeasuredSettings {
    pub settings: gpui_component::setting::Settings,
    pub groups: usize,
}

impl gpui::RenderOnce for MeasuredSettings {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        window.use_keyed_state(SharedString::from("list-state:0"), cx, |_, _| {
            gpui::ListState::new(self.groups, gpui::ListAlignment::Top, px(100.)).measure_all()
        });
        self.settings.render(window, cx)
    }
}

/// The scrollbar. gpui paints none of its own, so the toolkit's draws it:
/// an 8px thumb in a 16px gutter that lightens under the pointer, drags,
/// and fades out two seconds after the scroll stops — and nothing at all
/// when the content fits, because an always-on track would be a line, and
/// Soft draws no lines. The colours are `theme::init_components`' own
/// `scrollbar_thumb` tokens, so this stays in Ferrite's palette.
///
/// [`ScrollbarShow::Hover`] is the mode, not the toolkit's default
/// `Scrolling`: under `Scrolling` the bar answers the pointer *only* while
/// it happens to be visible, so once it has faded the gutter is dead and
/// the thumb can never be grabbed — the wheel is the only way to move.
/// Hover keeps the same fade, and brings the thumb back when the pointer
/// enters the gutter, which is the only moment anyone wants to grab it.
///
/// Hang it as a *sibling* of the scrolling element inside a shared
/// `relative()` parent, never as a child, or it scrolls away with the
/// content. The `id` must be unique per scroll area: the toolkit keys the
/// bar's hover, drag and fade state off it, and one helper here means the
/// caller location cannot do that keying for us.
pub fn scrollbar(id: impl Into<ElementId>, scroll: &gpui::ScrollHandle) -> gpui::Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .child(
            crate::scrollbar::Scrollbar::vertical(scroll)
                .id(id)
                .scrollbar_show(gpui_component::scroll::ScrollbarShow::Hover),
        )
}
