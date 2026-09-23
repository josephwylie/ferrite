//! The Decision card: approvals and questions, answerable with one key from
//! anywhere. Owned by WP-F, which builds the one card here (slices 11, 12)
//! and renders it inside the Pane's requests overlay.
//!
//! Presentation only — no cockpit state. Every row shows exactly the one
//! key that picks it (`y` `a` `n` on an approval, a digit on a question or
//! a native choice), and a click on the row sends what that key sends —
//! except the standing "always" row, whose click keeps today's
//! `Choose{value}` while `a` sends `AllowAlways` (core does not treat the
//! two alike when the provider forbids a plain Allow).

use ferrite_core::Decision;
use gpui::prelude::*;
use gpui::{div, px, rgb, rgba, AnyElement, App, Div, ElementId, SharedString, Stateful};

use crate::components;
use crate::icons;
use crate::pointer::{Pointer, PointerPressed};
use crate::theme;

/// The keys `PickOption1..4` bind: rows past the fourth are picked by
/// pointer or Tab.
pub const DIGIT_KEYS: usize = 4;

/// What one approval row does when it is picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Allow,
    /// The provider's standing suggestion at this index: `a` sends it as
    /// `AllowAlways`, a click or a digit as `Choose`.
    Always(usize),
    /// A native choice at this index of `Decision::suggestions`.
    Choose(usize),
    Deny,
}

/// One approval row: its verb, its label, the key it shows (a letter, a
/// digit, or none) and whether picking it can do anything.
#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalRow {
    pub verb: Verb,
    pub label: SharedString,
    /// A standing answer's scope, printed after `Always allow` in mono.
    pub scope: Option<SharedString>,
    pub key: Option<SharedString>,
    pub enabled: bool,
}

/// An approval's rows in CLI order: `y Allow`, `a Always allow <scope>`
/// (only while the provider offers a standing answer), the other native
/// choices, `n Deny` last. The same list drives the card, the digit keys
/// and the tests, so what the card shows is what the keys do.
pub fn approval_rows(decision: &Decision) -> Vec<ApprovalRow> {
    let policy = &decision.policy;
    let allows = policy.allow && !policy.interaction_required;
    let mut rows = vec![ApprovalRow {
        verb: Verb::Allow,
        label: "Allow".into(),
        scope: None,
        key: allows.then(|| "y".into()),
        enabled: allows,
    }];
    let standing = decision
        .suggestions
        .iter()
        .position(|choice| choice.standing);
    if let Some(at) = standing {
        let choice = &decision.suggestions[at];
        let scope = standing_scope(&choice.value);
        rows.push(ApprovalRow {
            verb: Verb::Always(at),
            // A rule-scoped answer reads `Always allow Bash(gh issue:*)`;
            // any other standing answer keeps the provider's own words.
            label: if scope.is_some() {
                "Always allow".into()
            } else {
                choice.label.clone().into()
            },
            scope,
            key: allows.then(|| "a".into()),
            enabled: true,
        });
    }
    for (at, choice) in decision.suggestions.iter().enumerate() {
        if Some(at) != standing {
            rows.push(ApprovalRow {
                verb: Verb::Choose(at),
                label: choice.label.clone().into(),
                scope: None,
                key: None,
                enabled: true,
            });
        }
    }
    rows.push(ApprovalRow {
        verb: Verb::Deny,
        label: "Deny".into(),
        scope: None,
        key: policy.deny.then(|| "n".into()),
        enabled: policy.deny,
    });
    // A row without a letter shows its digit, when a digit key exists.
    for (at, row) in rows.iter_mut().enumerate() {
        if row.enabled && row.key.is_none() && at < DIGIT_KEYS {
            row.key = Some(SharedString::from((at + 1).to_string()));
        }
    }
    rows
}

/// A standing answer's scope, as Claude Code prints a permission rule:
/// `Bash(gh issue:*)`, several joined by `, `. `None` when the answer is
/// not an allow-rule (a mode switch, a session grant).
fn standing_scope(value: &serde_json::Value) -> Option<SharedString> {
    if value["behavior"].as_str() != Some("allow") {
        return None;
    }
    let rules = value["rules"]
        .as_array()?
        .iter()
        .map(|rule| {
            let tool = rule["toolName"].as_str()?;
            Some(match rule["ruleContent"].as_str() {
                Some(content) => format!("{tool}({content})"),
                None => tool.to_string(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    (!rules.is_empty()).then(|| rules.join(", ").into())
}

/// The verb digit `n` (0-based) picks on an approval, if that row can act.
pub fn digit_verb(decision: &Decision, n: usize) -> Option<Verb> {
    approval_rows(decision)
        .into_iter()
        .nth(n)
        .filter(|row| row.enabled)
        .map(|row| row.verb)
}

/// The head's kind word: what the card asks of the operator.
pub fn kind_word(decision: &Decision) -> &'static str {
    match &decision.kind {
        ferrite_core::DecisionKind::Approval => theme::words::APPROVAL,
        ferrite_core::DecisionKind::Questions(_) => theme::words::QUESTION,
        ferrite_core::DecisionKind::Form { .. } => "input needed",
        ferrite_core::DecisionKind::External { .. } => "finish in browser",
        ferrite_core::DecisionKind::Unsupported { .. } => "can't answer here",
    }
}

/// A provider's `" (Recommended)"` suffix, stripped for display only: the
/// answer goes back by index, so core still sends the original label.
pub fn split_recommended(label: &str) -> (&str, bool) {
    const SUFFIX: &str = " (recommended)";
    let cut = label.len().checked_sub(SUFFIX.len());
    match cut {
        Some(cut) if label.is_char_boundary(cut) && label[cut..].eq_ignore_ascii_case(SUFFIX) => {
            (label[..cut].trim_end(), true)
        }
        _ => (label, false),
    }
}

// ------------------------------------------------------------------ frame

/// The block: `RAISED`, one 1px `COMPOSER_EDGE`, `R_BLOCK`, capped at the
/// reading column. Ochre is only on `◆` and the kind word — no wash, no
/// state edge. `joined` merges it into the live Composer below it: square
/// bottom corners and no bottom edge — the Composer's own top edge is the
/// one full-width seam — so the Decision and the Composer's input line
/// read as one outlined block. Every section keeps its natural height (nothing is
/// clipped); only the command well gives way. It keeps the
/// `question-island` selector every kind has always answered to, and its
/// own cursor: it floats over selectable prose that paints an I-beam.
pub fn card(
    serial: u64,
    joined: bool,
    children: impl IntoIterator<Item = AnyElement>,
) -> Stateful<Div> {
    let block = div()
        .id(("question-island", serial as usize))
        .debug_selector(|| "question-island".into())
        .relative()
        .flex()
        .flex_col()
        .w_full()
        .max_w(px(theme::READING_MAX_W))
        .min_w_0()
        .min_h_0()
        .cursor_default()
        .bg(rgb(theme::RAISED))
        .border_color(rgba(theme::COMPOSER_EDGE));
    let block = if joined {
        block
            .border_t_1()
            .border_l_1()
            .border_r_1()
            .rounded_t(px(theme::R_BLOCK))
    } else {
        block.border_1().rounded(px(theme::R_BLOCK))
    };
    block.child(
        div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .min_h_0()
            .gap(px(theme::DECISION_GAP))
            .px(px(theme::DECISION_PAD_X))
            .py(px(theme::DECISION_PAD_Y))
            .font_family(theme::FONT_UI)
            .text_size(px(theme::FS_UI))
            .line_height(px(theme::LH_UI))
            .text_color(rgb(theme::TEXT))
            .children(children),
    )
}

/// The head line, `◆ approval · Bash`, at `FS_SM`: the drawn `◆` in the
/// glyph box (the Composer `❯`'s column), the kind word in `ATTENTION` at
/// `W_BODY` — the card's only ochre besides the mark — then each detail
/// after its own `TEXT_FAINT` `·` in `TEXT_MUTED`, and a status on the
/// right.
pub fn head(
    kind: impl Into<SharedString>,
    detail: Option<SharedString>,
    status: Option<AnyElement>,
) -> Div {
    let mut line = div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .min_w_0()
        .h(px(theme::LH_META))
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .child(components::gutter(mark(), theme::LH_META))
        .child(
            div()
                .flex_shrink_0()
                .font_weight(theme::W_BODY)
                .text_color(rgb(theme::ATTENTION))
                .child(kind.into()),
        );
    if let Some(detail) = detail {
        for part in detail.split(" \u{b7} ") {
            line = line
                .child(
                    div()
                        .flex_shrink_0()
                        .px(px(theme::SPACE_1_5))
                        .text_color(rgb(theme::TEXT_FAINT))
                        .child("\u{b7}"),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme::TEXT_MUTED))
                        .child(SharedString::from(part.to_string())),
                );
        }
    }
    line.child(div().flex_1()).children(status)
}

/// The Decision mark `◆`, drawn.
pub fn mark() -> impl IntoElement {
    icons::icon(icons::DIAMOND, theme::DECISION_MARK, theme::ATTENTION)
}

/// A head's right-hand status (`answer when ready`, `work continues`).
pub fn status(text: impl Into<SharedString>) -> Div {
    div()
        .debug_selector(|| "decision-status".into())
        .flex_shrink_0()
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(theme::TEXT_MUTED))
        .child(text.into())
}

/// An answer in flight: a still `RUNNING` dot and `sending`, `FS_SM`
/// `TEXT_MUTED`. Nothing on it moves — a Decision is static (rule
/// 2.10.4).
pub fn sending() -> Div {
    div()
        .debug_selector(|| "decision-status".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::SPACE_1_5))
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(theme::TEXT_MUTED))
        .child(components::status_dot(theme::RUNNING))
        .child(theme::words::SENDING)
}

/// A question: Geist `FS_PROSE`/`LH_PROSE`, `W_STRONG`, `TEXT_STRONG`, on
/// the prose measure.
pub fn question_text(text: impl Into<SharedString>) -> Div {
    div()
        .w_full()
        .min_w_0()
        .max_w(px(theme::PROSE_MEASURE))
        .flex_shrink_0()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_PROSE))
        .line_height(px(theme::LH_PROSE))
        .font_weight(theme::W_STRONG)
        .text_color(rgb(theme::TEXT_STRONG))
        .child(text.into())
}

/// What an approval asks, in prose: Geist `FS_PROSE`, `TEXT`. The first
/// section to go when the Pane is short.
pub fn prose(text: impl Into<SharedString>) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_PROSE))
        .line_height(px(theme::LH_PROSE))
        .text_color(rgb(theme::TEXT))
        .child(text.into())
}

/// Whether an approval's prose says only what its command well already
/// shows: the description, trimmed, equals or sits inside the command's
/// first line. Such prose is dropped — the subject is printed once — and a
/// description that gives a reason stays.
pub fn prose_repeats_command(description: &str, command: Option<&str>) -> bool {
    let description = description.trim();
    let Some(first) = command.and_then(|command| command.lines().next()) else {
        return false;
    };
    !description.is_empty() && first.trim().contains(description)
}

/// A UI aside (`choose any`, a form field's hint).
pub fn note(text: impl Into<SharedString>) -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(theme::TEXT_MUTED))
        .child(text.into())
}

/// The command well: the exact input, mono `TEXT_STRONG`, on `RAISED_2`
/// (one step up from the card, never darker than the Pane) at
/// `R_CONTROL`. A shell command reads `$ gh issue close 212`: the `$ ` is
/// `TEXT_FAINT` and outside the selectable text, so a copy gives the
/// command alone. The one part of a card that gives way when the Pane is
/// short: it keeps one line (`LH_CODE` + its padding) and scrolls the rest,
/// so the rows below it stay whole.
pub fn well(prompt: bool, child: impl IntoElement) -> Div {
    div()
        .flex()
        .items_start()
        .w_full()
        .min_w_0()
        .flex_shrink_1()
        .min_h(px(theme::DECISION_WELL_MIN_H))
        .overflow_hidden()
        .bg(rgb(theme::RAISED_2))
        .rounded(px(theme::R_CONTROL))
        .px(px(theme::DECISION_WELL_PAD_X))
        .py(px(theme::DECISION_WELL_PAD_Y))
        .font_family(theme::FONT_CODE)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_CODE))
        .text_color(rgb(theme::TEXT_STRONG))
        .when(prompt, |well| {
            well.child(
                div()
                    .debug_selector(|| "approval-prompt".into())
                    .flex_shrink_0()
                    .text_color(rgb(theme::TEXT_FAINT))
                    .child("$\u{a0}"),
            )
        })
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .max_h_full()
                .child(child),
        )
}

// ------------------------------------------------------------------- rows

/// What one option row shows.
pub struct Row {
    /// The key that picks it; `None` leaves its keycap slot empty.
    pub key: Option<SharedString>,
    pub label: SharedString,
    /// A standing answer's scope (`Bash(gh issue:*)`), mono `TEXT_MUTED`
    /// after the label.
    pub scope: Option<SharedString>,
    pub description: Option<SharedString>,
    pub recommended: bool,
    pub selected: bool,
    pub enabled: bool,
    /// A declining verb (Deny): its title in `TEXT`, never red and never
    /// brighter than the verbs that act.
    pub quiet: bool,
    /// The row ↵ would choose: a trailing mono `↵`.
    pub enter: bool,
}

/// One option row on `gpui_base::Button` (tab stop, Enter/Space, focus
/// ring). A title-only row is `MENU_ROW_H` (`LH_UI` + `DECISION_ROW_PAD_Y`
/// above and below); a description adds `LH_PROSE_SM` per line. The keycap
/// (`KBD_H`) is centred on the glyph column the head's `◆` and the
/// Composer's `❯` share — the row hangs `DECISION_ROW_PAD_X` left of the
/// card's content so its hover ground reaches around the key — and the
/// title starts on the text column. Title `W_BODY` `TEXT_STRONG`
/// (`TEXT_MUTED` disabled), description `W_BODY` `TEXT_2`, `recommended`
/// `FS_SM` `TEXT_MUTED` at the right. Selected is `FILL` plus the check;
/// hover `RAISED_2`; keyboard focus the one `FOCUS_RING` recipe.
pub fn option_row(id: impl Into<ElementId>, row: Row) -> gpui_base::Button {
    let id = id.into();
    let key = crate::pointer::hover_key(&id);
    let ink = match (row.enabled, row.quiet) {
        (false, _) => theme::TEXT_MUTED,
        (true, true) => theme::TEXT,
        (true, false) => theme::TEXT_STRONG,
    };
    let accessibility = SharedString::from(format!(
        "{}{}",
        row.label,
        if row.selected { ", selected" } else { "" }
    ));
    let trailing_line = || {
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(theme::LH_UI))
    };
    gpui_base::Button::new(id)
        .tab_stop(row.enabled)
        .disabled(!row.enabled)
        .accessibility_label(accessibility)
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .justify_start()
        .items_start()
        .gap(px(theme::DECISION_ROW_INNER_GAP))
        .ml(px(-theme::DECISION_ROW_PAD_X))
        .mr(px(-theme::DECISION_ROW_PAD_X))
        .px(px(theme::DECISION_ROW_PAD_X))
        .py(px(theme::DECISION_ROW_PAD_Y))
        .rounded(px(theme::R_CONTROL))
        .font_family(theme::FONT_UI)
        .font_weight(theme::W_BODY)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(ink))
        // A selected option takes its FILL at once (a keyboard change);
        // only the pointer half of the ladder blends.
        .map(|button| match (row.enabled, row.selected) {
            (false, true) => button.bg(rgb(theme::FILL)).cursor_default(),
            (false, false) => button.cursor_default(),
            (true, true) => button.hover_carried(key).press_raised(),
            (true, false) => button.hover_raised(key).press_raised(),
        })
        .focus_visible(components::control_focus)
        .child(
            // The glyph box: the keycap is wider than the box and centred
            // on it, so it overhangs the box evenly on both sides.
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .w(px(theme::GLYPH_BOX))
                .h(px(theme::LH_UI))
                .children(row.key.filter(|_| row.enabled).map(components::kbd)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_baseline()
                        .gap_x(px(theme::SPACE_1_5))
                        .min_w_0()
                        .child(row.label)
                        .children(row.scope.map(|scope| {
                            div()
                                .min_w_0()
                                .font_family(theme::FONT_CODE)
                                .text_color(rgb(theme::TEXT_MUTED))
                                .child(scope)
                        })),
                )
                .children(row.description.filter(|text| !text.is_empty()).map(|text| {
                    div()
                        .font_family(theme::FONT_UI)
                        .font_weight(theme::W_BODY)
                        .text_size(px(theme::FS_PROSE_SM))
                        .line_height(px(theme::LH_PROSE_SM))
                        .text_color(rgb(theme::TEXT_2))
                        .child(text)
                })),
        )
        .when(row.recommended, |button| {
            button.child(
                trailing_line()
                    .text_size(px(theme::FS_SM))
                    .text_color(rgb(theme::TEXT_MUTED))
                    .child("recommended"),
            )
        })
        .when(row.enter, |button| {
            button.child(
                trailing_line()
                    .font_family(theme::FONT_CODE)
                    .text_size(px(theme::FS_SM))
                    .text_color(rgb(theme::TEXT_MUTED))
                    .child("\u{21b5}"),
            )
        })
        .when(row.selected, |button| {
            button.child(trailing_line().child(icons::icon(
                icons::CHECK,
                theme::DECISION_CHECK,
                theme::ACCENT,
            )))
        })
}

// ---------------------------------------------------------------- footer

/// The one error line: the drawn `✗` and `lead` in `BLOCKED` (only that
/// phrase carries the hue), then `· detail` in `TEXT_2`.
pub fn error_line(lead: &'static str, detail: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_start()
        .gap(px(theme::SPACE_1_5))
        .min_w_0()
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .h(px(theme::LH_META))
                .child(icons::icon(
                    icons::CLOSE,
                    theme::DECISION_MARK,
                    theme::BLOCKED,
                )),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(rgb(theme::BLOCKED))
                .child(lead),
        )
        .child(
            div()
                .min_w_0()
                .text_color(rgb(theme::TEXT_2))
                .child(SharedString::from(format!("· {}", detail.into()))),
        )
}

/// The answer row: key hints on the left, actions on the right. It keeps
/// its height whatever the Pane's.
pub fn footer(hints: &[(&str, &str)], actions: impl IntoIterator<Item = AnyElement>) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::SPACE_2))
        .min_w_0()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .children((!hints.is_empty()).then(|| components::key_hints(hints))),
        )
        .children(actions)
}

/// The completing action (`Send`, `Complete`): the steel primary, its label
/// at `W_BODY`. `enter` adds the key that sends — a mono `FS_SM` `↵` at
/// `ON_ACCENT` 70% — so the key lives on the button, not in the hints.
pub fn send_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    disabled: bool,
    enter: bool,
    cx: &App,
) -> gpui::component::button::Button {
    let ink = if disabled {
        theme::TEXT_MUTED
    } else {
        theme::ON_ACCENT
    };
    components::primary_button(id, disabled, cx)
        .h(px(theme::CONTROL_H))
        .px(px(theme::CONTROL_PAD_X))
        .child(
            components::text_ui()
                .flex()
                .items_center()
                .gap(px(theme::SPACE_1_5))
                .text_color(rgb(ink))
                .child(label.into())
                .when(enter, |label| {
                    label.child(
                        div()
                            .font_family(theme::FONT_CODE)
                            .text_size(px(theme::FS_SM))
                            .map(|key| {
                                if disabled {
                                    key.text_color(rgb(theme::TEXT_MUTED))
                                } else {
                                    key.text_color(rgba(theme::SEND_KEY_INK))
                                }
                            })
                            .child("\u{21b5}"),
                    )
                }),
        )
}

/// The declining action (`Skip`, `Cancel`): a ghost with no key.
pub fn skip_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &App,
) -> gpui::component::button::Button {
    components::ghost_button(id, label, cx).tab_stop(true)
}

// -------------------------------------------------------------------- L2

/// An L2 keycap pair — `y allow` — pressable, on the cell's `PANE` ground:
/// the key's cap, then its verb, muted. A narrow cell drops the verb and
/// keeps the key (`verb` false): pairs go whole, never cut.
pub fn key_action(
    id: &'static str,
    key: &'static str,
    verb: &'static str,
    with_verb: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::DECISION_KEY_GAP))
        .rounded(px(theme::R_CHIP))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(theme::TEXT_MUTED))
        .hover_row(id)
        .press_row()
        .child(components::kbd(key))
        .when(with_verb, |pair| {
            pair.child(div().pr(px(theme::SPACE_1)).child(verb))
        })
}

/// The L2 keycaps' cluster.
pub fn key_actions() -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::DECISION_KEYS_GAP))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::DecisionChoice;

    fn approval(suggestions: Vec<DecisionChoice>) -> Decision {
        Decision {
            delivery: Default::default(),
            kind: Default::default(),
            policy: Default::default(),
            id: "perm".into(),
            tool_use_id: "toolu".into(),
            tool_name: "Bash".into(),
            description: "ls".into(),
            input: serde_json::Value::Null,
            suggestions,
        }
    }

    fn choice(label: &str, standing: bool) -> DecisionChoice {
        DecisionChoice {
            label: label.into(),
            value: serde_json::json!({ "label": label }),
            standing,
        }
    }

    fn keys(decision: &Decision) -> Vec<(Verb, Option<String>)> {
        approval_rows(decision)
            .into_iter()
            .map(|row| (row.verb, row.key.map(|key| key.to_string())))
            .collect()
    }

    #[test]
    fn approval_rows_follow_cli_order_and_show_the_key_that_picks_them() {
        assert_eq!(
            keys(&approval(vec![])),
            [
                (Verb::Allow, Some("y".into())),
                (Verb::Deny, Some("n".into()))
            ]
        );
        // The standing answer takes `a` and sits second wherever the
        // provider listed it; other choices show their digit; deny is last.
        let offered = approval(vec![
            choice("Block example.com", false),
            choice("Always", true),
        ]);
        assert_eq!(
            keys(&offered),
            [
                (Verb::Allow, Some("y".into())),
                (Verb::Always(1), Some("a".into())),
                (Verb::Choose(0), Some("3".into())),
                (Verb::Deny, Some("n".into())),
            ]
        );
        assert_eq!(digit_verb(&offered, 2), Some(Verb::Choose(0)));
        assert_eq!(digit_verb(&offered, 0), Some(Verb::Allow));
        assert_eq!(digit_verb(&offered, 9), None);
    }

    #[test]
    fn a_forbidden_allow_shows_no_key_and_its_standing_row_falls_back_to_a_digit() {
        let mut decision = approval(vec![choice("Allow this session", true)]);
        decision.policy.allow = false;
        let rows = approval_rows(&decision);
        assert!(!rows[0].enabled && rows[0].key.is_none());
        // `a` would be refused, but the click (and so its digit) still
        // chooses the native value.
        assert_eq!(rows[1].key.as_ref().map(|k| k.as_ref()), Some("2"));
        assert_eq!(digit_verb(&decision, 0), None);
        assert_eq!(digit_verb(&decision, 1), Some(Verb::Always(0)));
        decision.policy.deny = false;
        let rows = approval_rows(&decision);
        assert!(!rows.last().unwrap().enabled && rows.last().unwrap().key.is_none());
    }

    /// A rule-scoped standing answer reads `Always allow` with its scope
    /// verbatim; any other standing answer keeps the provider's words.
    #[test]
    fn a_standing_rule_reads_always_allow_with_its_scope() {
        let mut scoped = choice("Allow Bash(gh issue:*) in local settings", true);
        scoped.value = serde_json::json!({
            "type": "addRules",
            "behavior": "allow",
            "destination": "localSettings",
            "rules": [{ "toolName": "Bash", "ruleContent": "gh issue:*" }],
        });
        let rows = approval_rows(&approval(vec![scoped]));
        let labels: Vec<_> = rows
            .iter()
            .map(|row| {
                (
                    row.label.to_string(),
                    row.scope.as_ref().map(|scope| scope.to_string()),
                )
            })
            .collect();
        assert_eq!(
            labels,
            [
                ("Allow".to_string(), None),
                (
                    "Always allow".to_string(),
                    Some("Bash(gh issue:*)".to_string())
                ),
                ("Deny".to_string(), None),
            ]
        );
        let session = approval(vec![choice("Allow for this session", true)]);
        let rows = approval_rows(&session);
        assert_eq!(rows[1].label.as_ref(), "Allow for this session");
        assert_eq!(rows[1].scope, None);
    }

    /// The head's kind words are the lexicon's.
    #[test]
    fn kind_words_come_from_the_lexicon() {
        let mut decision = approval(vec![]);
        assert_eq!(kind_word(&decision), theme::words::APPROVAL);
        decision.kind = ferrite_core::DecisionKind::Questions(Vec::new());
        assert_eq!(kind_word(&decision), theme::words::QUESTION);
    }

    /// The subject is printed once: prose that only restates the command
    /// goes, and a reason stays.
    #[test]
    fn prose_that_repeats_the_command_is_dropped() {
        let command = Some("gh issue close 212\n--comment done");
        assert!(prose_repeats_command("gh issue close 212", command));
        assert!(prose_repeats_command("  issue close 212 ", command));
        assert!(!prose_repeats_command(
            "Close the stale issue after the fix landed",
            command
        ));
        assert!(!prose_repeats_command("gh issue close 212", None));
        assert!(!prose_repeats_command("", command));
    }

    #[test]
    fn recommended_is_stripped_for_display_only() {
        assert_eq!(
            split_recommended("Keep the draft (Recommended)"),
            ("Keep the draft", true)
        );
        assert_eq!(split_recommended("keep (recommended)"), ("keep", true));
        assert_eq!(split_recommended("Rewrite"), ("Rewrite", false));
        assert_eq!(split_recommended("é"), ("é", false));
    }
}
