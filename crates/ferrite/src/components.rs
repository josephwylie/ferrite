//! Longbridge controls in Ferrite's visual language. The toolkit owns the
//! control mechanics; the existing theme remains the only token source.

use gpui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui::component::{FocusableExt, Sizable};
use gpui::prelude::*;
use gpui::{div, point, px, rgb, App, BoxShadow, ElementId, SharedString, StyleRefinement};

use crate::theme;

/// A compact, neutral button. Supply content with its own typography so
/// upstream control sizes and hover foregrounds cannot recolour the label.
pub fn button(id: impl Into<ElementId>) -> Button {
    Button::new(id)
        .ghost()
        .xsmall()
        .tab_stop(false)
        .focus_ring(false)
        .border_0()
        .focus_visible(control_focus)
        .rounded(px(theme::R_CONTROL))
        .font_family(theme::FONT_UI)
        .cursor_pointer()
}

/// The inset outline survives hover's border/background refinements and
/// stays inside clipped forms without taking any layout space.
pub fn control_focus(style: StyleRefinement) -> StyleRefinement {
    focus_outline(style, theme::TEXT_2)
}

fn focus_outline(style: StyleRefinement, ink: u32) -> StyleRefinement {
    style.shadow(vec![BoxShadow {
        inset: true,
        color: rgb(ink).into(),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(2.),
    }])
}

/// Form actions need an opaque hover face on the modal's raised ground.
/// Use the toolkit's variant API: its renderer owns hover/press handlers.
pub fn form_button(id: impl Into<ElementId>, cx: &App) -> Button {
    button(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(rgb(theme::TEXT).into())
                .hover(rgb(theme::FILL).into())
                .active(rgb(theme::FILL_HOVER).into()),
        )
        .tab_stop(true)
}

/// A neutral completing action, with a separate disabled presentation.
pub fn primary_button(id: impl Into<ElementId>, disabled: bool, cx: &App) -> Button {
    use gpui::component::Disableable;
    form_button(id, cx)
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(rgb(theme::GROUND).into())
                .hover(rgb(theme::TEXT_STRONG).into())
                .active(rgb(theme::TEXT_2).into()),
        )
        .bg(rgb(if disabled { theme::FILL } else { theme::TEXT }))
        .focus_visible(|style| focus_outline(style, theme::GROUND))
        .disabled(disabled)
        .when(disabled, |button| button.cursor_default())
}

pub fn label(text: impl Into<SharedString>, ink: u32) -> impl IntoElement {
    div()
        .text_size(px(theme::FS_SM))
        .line_height(gpui::px(theme::LH_META))
        .text_color(rgb(ink))
        .child(text.into())
}

/// Forms use the body size so values and actions read at the same scale as
/// their labels. Dense pane chrome continues to use `label`.
pub fn form_label(text: impl Into<SharedString>, ink: u32) -> impl IntoElement {
    div()
        .text_size(px(theme::FS_UI))
        .line_height(gpui::px(theme::LH_UI))
        .text_color(rgb(ink))
        .child(text.into())
}

/// The same menu is opened by a chip or a slash command. PopupMenu owns
/// keyboard navigation, checked rows, scrolling and dismissal.
#[derive(Clone)]
pub struct Choice {
    pub label: SharedString,
    pub icon: Option<(&'static str, u32)>,
    pub checked: bool,
    pub disabled: bool,
    pub section: bool,
}

type OpenChanged = std::rc::Rc<dyn Fn(bool, &mut gpui::Window, &mut gpui::App)>;
type Picked = std::rc::Rc<dyn Fn(usize, &mut gpui::Window, &mut gpui::App)>;

#[derive(IntoElement)]
pub struct ChoiceMenu {
    pub id: SharedString,
    pub trigger: Button,
    pub choices: Vec<Choice>,
    pub open: bool,
    pub return_focus: gpui::FocusHandle,
    pub on_open: OpenChanged,
    pub on_pick: Picked,
}

#[derive(Default)]
struct ChoiceMenuState {
    menu: Option<gpui::Entity<gpui::component::menu::PopupMenu>>,
    steps: usize,
    initialized: bool,
}

impl gpui::RenderOnce for ChoiceMenu {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        use gpui::component::{
            menu::{PopupMenu, PopupMenuItem},
            popover::Popover,
        };
        use gpui::Focusable as _;
        let retained =
            window.use_keyed_state(self.id.clone(), cx, |_, _| ChoiceMenuState::default());
        if !self.open {
            retained.update(cx, |state, _| {
                state.menu = None;
                state.initialized = false;
            });
        } else if retained.read(cx).menu.is_none() {
            let checked = self
                .choices
                .iter()
                .filter(|choice| !choice.section && !choice.disabled)
                .position(|choice| choice.checked)
                .unwrap_or(0);
            let steps = checked + 1;
            let pick = self.on_pick.clone();
            let menu = PopupMenu::build(window, cx, move |mut menu, _, _| {
                menu = menu
                    .action_context(self.return_focus)
                    .check_side(gpui::component::Side::Right)
                    .min_w(px(240.))
                    .max_w(px(320.))
                    .max_h(px(420.))
                    .scrollable(true);
                for (index, choice) in self.choices.into_iter().enumerate() {
                    if choice.section {
                        continue;
                    }
                    let picked = pick.clone();
                    let item = PopupMenuItem::new(choice.label)
                        .when_some(choice.icon, |item, (path, color)| {
                            item.icon(
                                gpui::component::Icon::empty()
                                    .path(path)
                                    .text_color(rgb(color)),
                            )
                        })
                        .checked(choice.checked)
                        .disabled(choice.disabled)
                        .on_click(move |_, window, cx| picked(index, window, cx));
                    menu = menu.item(item);
                }
                menu
            });
            let on_open = self.on_open.clone();
            window
                .subscribe(&menu, cx, move |_, _: &gpui::DismissEvent, window, cx| {
                    on_open(false, window, cx);
                })
                .detach();
            retained.update(cx, |state, _| {
                state.menu = Some(menu);
                state.steps = steps;
            });
        }
        let menu = retained.read(cx).menu.clone();
        let on_open = self.on_open;
        let mut popover = Popover::new(SharedString::from(format!("choice:{}", self.id)))
            .appearance(false)
            .overlay_closable(false)
            .anchor(gpui::Anchor::BottomLeft)
            .trigger(self.trigger)
            .open(self.open)
            .on_open_change(move |open, window, cx| on_open(*open, window, cx));
        if let Some(menu) = menu {
            popover = popover
                .track_focus(&menu.focus_handle(cx))
                .content(move |_, _, _| {
                    use gpui::base::ElementExt as _;
                    let retained = retained.clone();
                    let menu = menu.clone();
                    div().child(menu.clone()).on_prepaint(move |_, window, cx| {
                        let steps = retained.update(cx, |state, _| {
                            if state.initialized {
                                return None;
                            }
                            state.initialized = true;
                            Some(state.steps)
                        });
                        if let Some(steps) = steps {
                            menu.focus_handle(cx).focus(window, cx);
                            for _ in 0..steps {
                                window
                                    .dispatch_action(Box::new(gpui::base::actions::SelectDown), cx);
                            }
                        }
                    })
                });
        }
        popover
    }
}

/// The scrollbar. gpui paints none of its own, so the toolkit's draws it:
/// an 8px thumb in a 16px gutter that lightens under the pointer, drags,
/// and fades out two seconds after the scroll stops — and nothing at all
/// when the content fits, because an always-on track would be a line, and
/// Soft draws no lines. The colours are `theme::init_components`' own
/// `scrollbar_thumb` tokens, so this stays in Ferrite's palette.
///
/// [`gpui::base::ScrollbarMode::Hover`] is the mode, not the toolkit's default
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
pub fn scrollbar(
    id: impl Into<ElementId>,
    scroll: &(impl gpui::base::ScrollbarHandle + Clone),
) -> gpui::Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .child(
            crate::scrollbar::Scrollbar::vertical(scroll)
                .id(id)
                .scrollbar_show(gpui::component::scroll::ScrollbarMode::Hover),
        )
}
