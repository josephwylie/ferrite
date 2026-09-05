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
