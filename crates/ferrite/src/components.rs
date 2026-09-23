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

/// A small verb inside a floating card (Reconnect, Stop, Sign in): a filled
/// face so it reads as pressable at a glance against the card's rows,
/// with one fixed height and label size wherever it appears.
pub fn action_button(id: impl Into<ElementId>, text: impl Into<SharedString>, cx: &App) -> Button {
    button(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .foreground(rgb(theme::TEXT).into())
                .hover(rgb(theme::FILL_HOVER).into())
                .active(rgb(theme::PRESSED).into()),
        )
        .bg(rgb(theme::FILL))
        .h(px(theme::CARD_ACTION_H))
        .px(px(theme::CARD_ACTION_PAD_X))
        .tab_stop(true)
        .child(label(text, theme::TEXT))
}

pub fn label(text: impl Into<SharedString>, ink: u32) -> impl IntoElement {
    div()
        .text_size(px(theme::FS_SM))
        .line_height(gpui::relative(theme::LINE_UI))
        .text_color(rgb(ink))
        .child(text.into())
}

/// Forms use the body size so values and actions read at the same scale as
/// their labels. Dense pane chrome continues to use `label`.
pub fn form_label(text: impl Into<SharedString>, ink: u32) -> impl IntoElement {
    div()
        .text_size(px(theme::FS_MD))
        .line_height(gpui::relative(theme::LINE_UI))
        .text_color(rgb(ink))
        .child(text.into())
}

/// The same menu is opened by a chip or a slash command. The toolkit's
/// Popover places it against its trigger; the list itself is the app's
/// own menu surface, riding the toolkit's `PopupMenu` key bindings.
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
    /// Which corner of the menu meets the trigger: `BottomLeft` for a
    /// control at the left of its row, `BottomRight` at the right, so the
    /// menu opens over its own Pane rather than across the next one.
    pub anchor: gpui::Anchor,
    pub choices: Vec<Choice>,
    pub open: bool,
    pub return_focus: gpui::FocusHandle,
    pub on_open: OpenChanged,
    pub on_pick: Picked,
}

/// What a `ChoiceMenu` keeps between frames: the row under the keyboard or
/// the pointer, the focus the menu's keys ride on, and whether this
/// opening has taken focus yet.
struct ChoiceMenuState {
    selected: Option<usize>,
    focus: gpui::FocusHandle,
    scroll: gpui::ScrollHandle,
    initialized: bool,
}

impl ChoiceMenu {
    /// A row the keyboard can land on: not a heading, not disabled.
    fn clickable(choice: &Choice) -> bool {
        !choice.section && !choice.disabled
    }
}

impl gpui::RenderOnce for ChoiceMenu {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        use gpui::base::actions::{Cancel, Confirm, SelectDown, SelectUp};
        use gpui::component::popover::Popover;
        let retained = window.use_keyed_state(self.id.clone(), cx, |_, cx| ChoiceMenuState {
            selected: None,
            focus: cx.focus_handle(),
            scroll: gpui::ScrollHandle::new(),
            initialized: false,
        });
        let choices = std::rc::Rc::new(self.choices);
        if !self.open {
            retained.update(cx, |state, _| {
                state.selected = None;
                state.initialized = false;
            });
        } else if retained.read(cx).selected.is_none() {
            // Open on the current choice, else the first live row.
            let selected = choices
                .iter()
                .position(|choice| Self::clickable(choice) && choice.checked)
                .or_else(|| choices.iter().position(Self::clickable));
            retained.update(cx, |state, _| state.selected = selected);
        }
        let focus = retained.read(cx).focus.clone();
        let on_open = self.on_open;
        let on_pick = self.on_pick;
        let return_focus = self.return_focus;
        // Closing hands the keyboard back to where it came from, unless a
        // pick has already moved it somewhere else on purpose.
        let close = {
            let on_open = on_open.clone();
            let focus = focus.clone();
            std::rc::Rc::new(move |window: &mut gpui::Window, cx: &mut gpui::App| {
                if focus.contains_focused(window, cx) || window.focused(cx).is_none() {
                    window.focus(&return_focus, cx);
                }
                on_open(false, window, cx);
            })
        };
        let step = {
            let retained = retained.clone();
            let choices = choices.clone();
            move |forward: bool, cx: &mut gpui::App| {
                retained.update(cx, |state, cx| {
                    let live: Vec<usize> = choices
                        .iter()
                        .enumerate()
                        .filter(|(_, choice)| Self::clickable(choice))
                        .map(|(index, _)| index)
                        .collect();
                    if live.is_empty() {
                        return;
                    }
                    // Wraps at either end, as the toolkit's menu did.
                    let next = match state
                        .selected
                        .and_then(|at| live.iter().position(|i| *i == at))
                    {
                        Some(at) if forward => live[(at + 1) % live.len()],
                        Some(at) => live[(at + live.len() - 1) % live.len()],
                        None if forward => live[0],
                        None => live[live.len() - 1],
                    };
                    state.selected = Some(next);
                    state.scroll.scroll_to_item(next);
                    cx.notify();
                });
            }
        };
        let step = std::rc::Rc::new(step);
        let content =
            {
                let retained = retained.clone();
                move |_: &mut gpui::component::popover::PopoverState,
                  _: &mut gpui::Window,
                  cx: &mut gpui::Context<gpui::component::popover::PopoverState>| {
                let (selected, scroll, initialized) = {
                    let state = retained.read(cx);
                    (state.selected, state.scroll.clone(), state.initialized)
                };
                let mut list = crate::menu::shell()
                    .id("choice-menu")
                    .key_context("PopupMenu")
                    .track_focus(&focus)
                    .w_auto()
                    .min_w(px(240.))
                    .max_w(px(320.))
                    .max_h(px(420.))
                    .overflow_y_scroll()
                    .track_scroll(&scroll)
                    .on_action({
                        let step = step.clone();
                        move |_: &SelectDown, _, cx| {
                            cx.stop_propagation();
                            step(true, cx)
                        }
                    })
                    .on_action({
                        let step = step.clone();
                        move |_: &SelectUp, _, cx| {
                            cx.stop_propagation();
                            step(false, cx)
                        }
                    })
                    .on_action({
                        let retained = retained.clone();
                        let on_pick = on_pick.clone();
                        let close = close.clone();
                        move |_: &Confirm, window, cx| {
                            cx.stop_propagation();
                            if let Some(index) = retained.read(cx).selected {
                                on_pick(index, window, cx);
                            }
                            close(window, cx);
                        }
                    })
                    .on_action({
                        let close = close.clone();
                        move |_: &Cancel, window, cx| {
                            cx.stop_propagation();
                            close(window, cx);
                        }
                    })
                    .on_mouse_down_out({
                        let close = close.clone();
                        move |_, window, cx| close(window, cx)
                    });
                for (index, choice) in choices.iter().enumerate() {
                    if choice.section {
                        list = list.child(crate::menu::heading_after(
                            choice.icon.map(|(path, color)| {
                                crate::icons::icon(path, theme::PROVIDER_MARK_SM, color)
                            }),
                            choice.label.clone(),
                        ));
                        continue;
                    }
                    let live = Self::clickable(choice);
                    let ink = if choice.disabled {
                        theme::TEXT_MUTED
                    } else if choice.checked {
                        theme::TEXT_STRONG
                    } else {
                        theme::TEXT_2
                    };
                    // The app's menu row: 30px, the current choice strong
                    // with its check hard right, the keyboard's row on the
                    // hover face.
                    let row = div()
                        .id(("choice-row", index))
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(theme::ROW_ICON_GAP + 3.))
                        .h(px(theme::MENU_ROW_H))
                        .px(px(theme::ROW_PAD_X))
                        .rounded(px(theme::R_CONTROL))
                        .text_color(rgb(ink))
                        .when(choice.checked, |row| row.font_weight(gpui::FontWeight::MEDIUM))
                        .when(live && selected == Some(index), |row| {
                            row.bg(rgb(theme::HOVER))
                        })
                        .when_some(choice.icon, |row, (path, color)| {
                            row.child(crate::icons::icon(path, theme::PROVIDER_MARK_SM, color))
                        })
                        .child(div().flex_1().min_w_0().truncate().child(choice.label.clone()))
                        .when(choice.checked, |row| {
                            row.child(crate::icons::icon(
                                crate::icons::CHECK,
                                theme::ICON_CHEVRON_LG,
                                theme::TEXT,
                            ))
                        });
                    list = list.child(if live {
                        let retained = retained.clone();
                        let on_pick = on_pick.clone();
                        let close = close.clone();
                        row.cursor_pointer()
                            .on_mouse_move(move |_, _, cx| {
                                retained.update(cx, |state, cx| {
                                    if state.selected != Some(index) {
                                        state.selected = Some(index);
                                        cx.notify();
                                    }
                                });
                            })
                            .on_click(move |_, window, cx| {
                                on_pick(index, window, cx);
                                close(window, cx);
                            })
                    } else {
                        row
                    });
                }
                // Take the keyboard once per opening, on the first frame
                // the list is painted.
                if !initialized {
                    use gpui::base::ElementExt as _;
                    let retained = retained.clone();
                    let focus = focus.clone();
                    list = list.on_prepaint(move |_, window, cx| {
                        let first = retained.update(cx, |state, _| {
                            !std::mem::replace(&mut state.initialized, true)
                        });
                        if first {
                            focus.focus(window, cx);
                        }
                    });
                }
                list
            }
            };
        Popover::new(SharedString::from(format!("choice:{}", self.id)))
            .appearance(false)
            .overlay_closable(false)
            .anchor(self.anchor)
            .trigger(self.trigger)
            .open(self.open)
            .on_open_change(move |open, window, cx| on_open(*open, window, cx))
            .track_focus(&retained.read(cx).focus.clone())
            .when(self.open, |popover| popover.content(content))
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
