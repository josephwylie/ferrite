//! Native controls over the board's existing actions and setting jobs.
use std::rc::Rc;

use gpui::component::{Disableable, Selectable};
use gpui::prelude::*;
use gpui::{div, px, rgb, AnyElement, Context, SharedString, Window};
use sw_board::board::keys::{entry, Press, K};
use sw_board::board::state::Mode;
use sw_board::board::update::actions::running;
use sw_board::board::update::{edit, Event};
use sw_board::json::*;

use super::BoardView;
use crate::components::{self, Tip};
use crate::theme::{
    BAND2, CH, CHROME, CONTROL_H, ICON_CHEVRON, LINE, ON_PRIMARY, PLANE, ROW, TEXT, TEXT_DIM,
    TEXT_STRONG, W_STRONG,
};

pub(super) const HEIGHT: f32 = ROW * 3.;

#[derive(Default)]
pub(super) struct Controls {
    pub open: Option<&'static str>,
}

impl BoardView {
    pub(super) fn project_scope_button(&self, only: bool, cx: &mut Context<Self>) -> AnyElement {
        let selected = self
            .board
            .as_ref()
            .is_some_and(|b| b.state.project_root.is_some())
            == only;
        let label = if only { "This project" } else { "All" };
        let id = if only {
            "board-project-only"
        } else {
            "board-project-all"
        };
        components::form_button_on(id, rgb(if selected { BAND2 } else { PLANE }).into(), cx)
            .tab_stop(false)
            .h(px(CONTROL_H))
            .px(px(CH))
            .selected(selected)
            .accessibility_label(label)
            .child(
                components::text_ui()
                    .text_color(rgb(if selected { TEXT_STRONG } else { TEXT_DIM }))
                    .child(label),
            )
            .on_click(cx.listener(move |view, _, _, cx| {
                let root = view.cwd().map(str::to_owned);
                if let Some(state) = view.state() {
                    state.project_root = if only { root } else { None };
                    sw_board::board::update::list::relist(state);
                }
                view.follow_selection();
                cx.notify();
            }))
            .into_any_element()
    }

    /// Use the same dispatch as a key, including drafts and confirmations.
    fn press_action(&mut self, key: K, window: &mut Window, cx: &mut Context<Self>) {
        let key = entry(key);
        let press = match key.raw.first().and_then(|s| s.chars().next()) {
            Some(ch) => Press::char(ch),
            None => Press::chord(key.keys[0]),
        };
        self.feed(Event::Key(press), cx);
        self.focus.focus(window, cx);
    }

    fn action_button(
        &self,
        id: &'static str,
        label: &'static str,
        key: K,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let working = self
            .board
            .as_ref()
            .and_then(|b| b.state.current())
            .is_some_and(running);
        let primary = matches!(id, "board-start" | "board-confirm" | "board-send")
            || id == "board-open" && working;
        let button = if primary {
            components::primary_button(id, disabled, cx)
        } else {
            components::form_button_on(id, rgb(CHROME).into(), cx).disabled(disabled)
        };
        button
            .tab_stop(false)
            .debug_selector(move || id.into())
            .h(px(CONTROL_H))
            .px(px(CH))
            .disabled(disabled)
            .accessibility_label(label)
            .child(
                components::text_ui()
                    .text_color(rgb(if disabled {
                        TEXT_DIM
                    } else if primary {
                        ON_PRIMARY
                    } else {
                        TEXT
                    }))
                    .child(label),
            )
            .on_click(cx.listener(move |view, _, window, cx| view.press_action(key, window, cx)))
            .into_any_element()
    }

    pub(super) fn controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = self.board.as_ref().map(|b| &b.state);
        let task = state.and_then(|s| s.current());
        let asking = state.is_some_and(|s| s.ask.is_some());
        let draft = state.is_some_and(|s| !s.value().is_empty());
        let modal =
            state.is_some_and(|s| s.menu.is_some() || s.editing.is_some() || s.mode == Mode::Find);
        let disabled = task.is_none() || draft || modal || asking;
        let working = task.is_some_and(running);
        let actions = if asking {
            div()
                .flex()
                .child(self.action_button("board-confirm", "Confirm", K::SayYes, false, cx))
                .child(self.action_button("board-cancel", "Cancel", K::SayNo, false, cx))
        } else if draft || modal {
            div()
                .flex()
                .child(self.action_button(
                    "board-send",
                    if state.is_some_and(|s| s.mode == Mode::Find) {
                        "Apply filter"
                    } else if modal {
                        "Choose"
                    } else {
                        "Send"
                    },
                    K::Enter,
                    false,
                    cx,
                ))
                .child(self.action_button("board-back", "Back", K::Back, false, cx))
        } else {
            div()
                .flex()
                .when(!working, |actions| {
                    actions.child(self.action_button(
                        "board-start",
                        "Start agent",
                        K::Delegate,
                        disabled || task.is_some_and(|r| r.b("archivedAt")),
                        cx,
                    ))
                })
                .child(self.action_button("board-open", "Open agent", K::Enter, disabled, cx))
                .child(self.action_button("board-details", "Details", K::Panel, disabled, cx))
                .child(self.more_menu(disabled, cx))
        };
        let selected = match (state, task) {
            (Some(state), Some(task)) => div()
                .flex()
                .items_center()
                .gap(px(CH))
                .min_w_0()
                .flex_1()
                .child(components::text_meta().flex_shrink_0().child(
                    sw_board::store::keys::say_id(task.s("id"), &state.data.keys),
                ))
                .child(
                    components::text_ui()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(TEXT_STRONG))
                        .font_weight(W_STRONG)
                        .child(task.t("title")),
                ),
            _ => div()
                .flex_1()
                .child(components::text_meta().child("Select a task")),
        };
        components::text_ui()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .h(px(HEIGHT))
            .text_color(rgb(TEXT))
            .px(px(CH * 1.5))
            .border_b_1()
            .border_color(rgb(LINE))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(CH * 2.))
                    .h(px(HEIGHT / 2.))
                    .child(selected)
                    .child(actions.items_center().gap(px(CH / 2.)).flex_shrink_0()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(HEIGHT / 2.))
                    .gap(px(CH))
                    .child(self.role_picker("worker.agent", "Developer", disabled, cx))
                    .child(self.role_picker("reviewer.agent", "Reviewer", disabled, cx))
                    .child(div().flex_1())
                    .child(self.action_button(
                        "board-talk",
                        if state.is_some_and(|s| s.reply) {
                            "Coordinator"
                        } else {
                            "Talk about task"
                        },
                        K::Talk,
                        disabled,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn more_menu(&self, disabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let state = self.board.as_ref().map(|b| &b.state);
        let task = state.and_then(|s| s.current());
        let options = vec![
            (
                if task.is_some_and(|r| r.s("stage") == "failed") {
                    "Retry"
                } else {
                    "Approve"
                },
                K::Yes,
            ),
            ("Send back", K::SendBack),
            (
                if task.is_some_and(running) {
                    "Stop agent"
                } else {
                    "Remove task"
                },
                K::Stop,
            ),
            (
                if task.is_some_and(|r| r.b("archivedAt")) {
                    "Restore task"
                } else {
                    "Archive task"
                },
                K::Archive,
            ),
            ("Task settings", K::Settings),
            ("Find", K::Find),
            (
                if state.is_some_and(|s| s.archive_view) {
                    "Hide archive"
                } else {
                    "Show archive"
                },
                K::ArchiveView,
            ),
            ("Group by project", K::ByRepo),
            ("Show all done", K::AllDone),
        ];
        let choices = options
            .iter()
            .map(|(label, key)| components::Choice {
                label: (*label).into(),
                disabled: disabled
                    && matches!(
                        key,
                        K::Yes | K::SendBack | K::Stop | K::Archive | K::Settings
                    ),
                ..Default::default()
            })
            .collect();
        let weak = cx.entity().downgrade();
        let picking = weak.clone();
        components::ChoiceMenu {
            id: "board-more".into(),
            anchor: gpui::Anchor::TopLeft,
            trigger: components::form_button_on("board-more", rgb(CHROME).into(), cx)
                .tab_stop(false)
                .h(px(CONTROL_H))
                .px(px(CH))
                .child(components::text_ui().child("More"))
                .child(crate::icons::icon(
                    crate::icons::CHEVRON_DOWN,
                    ICON_CHEVRON,
                    TEXT_DIM,
                )),
            choices,
            open: self.controls.open == Some("more"),
            return_focus: self.focus.clone(),
            on_open: Rc::new(move |open, _, cx| {
                let _ = weak.update(cx, |view, cx| {
                    view.controls.open = open.then_some("more");
                    cx.notify();
                });
            }),
            on_pick: Rc::new(move |at, window, cx| {
                if let Some((_, key)) = options.get(at) {
                    let _ = picking.update(cx, |view, cx| view.press_action(*key, window, cx));
                }
            }),
        }
        .into_any_element()
    }

    fn role_picker(
        &self,
        name: &'static str,
        label: &'static str,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let task = self.board.as_ref().and_then(|b| b.state.current());
        let setting = task.and_then(|r| {
            r.at("snap")
                .arr("settings")
                .iter()
                .find(|r| r.s("name") == name)
        });
        let value = setting.map(|s| s.t("value")).unwrap_or_default();
        let own = setting.is_some_and(|s| s.b("own"));
        let mut values: Vec<Option<String>> =
            vec![None, Some("claude".into()), Some("codex".into())];
        if name == "reviewer.agent" {
            values.push(Some("none".into()));
        }
        if !value.is_empty() && !values.iter().any(|v| v.as_deref() == Some(&value)) {
            values.push(Some(value.clone()));
        }
        let choices = values
            .iter()
            .map(|v| components::Choice {
                label: match v.as_deref() {
                    None => "Use default".into(),
                    Some("claude") => "Claude".into(),
                    Some("codex") => "Codex".into(),
                    Some("none") => "None".into(),
                    Some(v) => v.to_string().into(),
                },
                checked: if !own {
                    v.is_none()
                } else {
                    v.as_deref() == Some(&value)
                },
                ..Default::default()
            })
            .collect();
        let shown = match value.as_str() {
            "claude" => "Claude",
            "codex" => "Codex",
            "none" => "None",
            "" => "—",
            v => v,
        };
        let title: SharedString = format!("{label}: {shown}").into();
        let task_id = task.map(|r| r.t("id")).unwrap_or_default();
        let archived = task.is_some_and(|r| r.b("archivedAt"));
        let weak = cx.entity().downgrade();
        let picking = weak.clone();
        components::ChoiceMenu {
            id: name.into(),
            anchor: gpui::Anchor::TopLeft,
            trigger: components::form_button(name, cx)
                .tab_stop(false)
                .debug_selector(move || name.into())
                .h(px(CONTROL_H))
                .px(px(CH / 2.))
                .max_w(px(29. * CH))
                .gap(px(CH / 2.))
                .disabled(disabled || setting.is_none() || archived)
                .accessibility_label(label)
                .tip(format!("{title} · applies to this task’s next agent run"))
                .child(components::text_meta().child(label))
                .child(
                    components::text_ui()
                        .truncate()
                        .text_color(rgb(if disabled { TEXT_DIM } else { TEXT }))
                        .child(shown.to_owned()),
                )
                .child(crate::icons::icon(
                    crate::icons::CHEVRON_DOWN,
                    ICON_CHEVRON,
                    TEXT_DIM,
                )),
            choices,
            open: self.controls.open == Some(name),
            return_focus: self.focus.clone(),
            on_open: Rc::new(move |open, _, cx| {
                let _ = weak.update(cx, |view, cx| {
                    view.controls.open = open.then_some(name);
                    cx.notify();
                });
            }),
            on_pick: Rc::new(move |at, _, cx| {
                let Some(value) = values.get(at).cloned() else {
                    return;
                };
                let _ = picking.update(cx, |view, cx| {
                    let Some(state) = view.state() else { return };
                    if state.sel != task_id {
                        return;
                    }
                    let mut effects = vec![];
                    edit::save(state, name, value, &mut effects);
                    for effect in effects {
                        view.effect(effect, cx);
                    }
                    cx.notify();
                });
            }),
        }
        .into_any_element()
    }
}
