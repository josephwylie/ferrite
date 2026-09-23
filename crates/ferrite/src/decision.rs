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
    pub key: Option<SharedString>,
    pub enabled: bool,
}

/// An approval's rows in CLI order: allow, the standing "always", the other
/// native choices, deny last. The same list drives the card, the digit keys
/// and the tests, so what the card shows is what the keys do.
pub fn approval_rows(decision: &Decision) -> Vec<ApprovalRow> {
    let policy = &decision.policy;
    let allows = policy.allow && !policy.interaction_required;
    let mut rows = vec![ApprovalRow {
        verb: Verb::Allow,
        label: "Allow".into(),
        key: allows.then(|| "y".into()),
        enabled: allows,
    }];
    let standing = decision
        .suggestions
        .iter()
        .position(|choice| choice.standing);
    if let Some(at) = standing {
        rows.push(ApprovalRow {
            verb: Verb::Always(at),
            label: decision.suggestions[at].label.clone().into(),
            key: allows.then(|| "a".into()),
            enabled: true,
        });
    }
    for (at, choice) in decision.suggestions.iter().enumerate() {
        if Some(at) != standing {
            rows.push(ApprovalRow {
                verb: Verb::Choose(at),
                label: choice.label.clone().into(),
                key: None,
                enabled: true,
            });
        }
    }
    rows.push(ApprovalRow {
        verb: Verb::Deny,
        label: "Deny".into(),
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
        ferrite_core::DecisionKind::Approval => "approve",
        ferrite_core::DecisionKind::Questions(_) => "question",
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

/// The card: `RAISED` under the `ATTENTION_WASH` ground, a 1px
/// `ATTENTION_EDGE`, `R_BLOCK`, capped at the reading column. It keeps the
/// `question-island` selector every kind has always answered to, and its
/// own cursor: it floats over selectable prose that paints an I-beam.
pub fn card(serial: u64, children: impl IntoIterator<Item = AnyElement>) -> Stateful<Div> {
    div()
        .id(("question-island", serial as usize))
        .debug_selector(|| "question-island".into())
        .relative()
        .flex()
        .flex_col()
        .w_full()
        .max_w(px(theme::READING_MAX_W))
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .cursor_default()
        .bg(rgb(theme::RAISED))
        .rounded(px(theme::R_BLOCK))
        .child(
            div()
                .flex()
                .flex_col()
                .w_full()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .gap(px(theme::DECISION_GAP))
                .px(px(theme::DECISION_PAD_X))
                .py(px(theme::DECISION_PAD_Y))
                .bg(rgba(theme::ATTENTION_WASH))
                .border_1()
                .border_color(rgba(theme::ATTENTION_EDGE))
                .rounded(px(theme::R_BLOCK))
                .font_family(theme::FONT_UI)
                .text_size(px(theme::FS_UI))
                .line_height(px(theme::LH_UI))
                .text_color(rgb(theme::TEXT))
                .children(children),
        )
}

/// The head line: the drawn `◆`, the kind word (`approve`, `question`) in
/// `W_LABEL` `ATTENTION`, then `· detail` muted, and a status on the right.
pub fn head(
    kind: impl Into<SharedString>,
    detail: Option<SharedString>,
    status: Option<AnyElement>,
) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(theme::SPACE_1_5))
        .min_w_0()
        .h(px(theme::LH_META))
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .child(mark())
        .child(
            div()
                .flex_shrink_0()
                .font_weight(theme::W_LABEL)
                .text_color(rgb(theme::ATTENTION))
                .child(kind.into()),
        )
        .children(detail.map(|detail| {
            div()
                .min_w_0()
                .truncate()
                .text_color(rgb(theme::TEXT_MUTED))
                .child(SharedString::from(format!("· {detail}")))
        }))
        .child(div().flex_1())
        .children(status)
}

/// The Decision mark `◆`, drawn.
pub fn mark() -> impl IntoElement {
    icons::icon(icons::DIAMOND, theme::DECISION_MARK, theme::ATTENTION)
}

/// A head's right-hand status (`waiting`, `answer when ready` …).
pub fn status(text: impl Into<SharedString>) -> Div {
    div()
        .debug_selector(|| "decision-status".into())
        .flex_shrink_0()
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(theme::TEXT_MUTED))
        .child(text.into())
}

/// A question: Geist `FS_PROSE`/`LH_PROSE`, `W_STRONG`, `TEXT_STRONG`.
pub fn question_text(text: impl Into<SharedString>) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_PROSE))
        .line_height(px(theme::LH_PROSE))
        .font_weight(theme::W_STRONG)
        .text_color(rgb(theme::TEXT_STRONG))
        .child(text.into())
}

/// What an approval asks, in prose: Geist `FS_PROSE`, `TEXT`.
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

/// A UI aside (`choose any`, a form field's hint).
pub fn note(text: impl Into<SharedString>) -> Div {
    div()
        .flex_shrink_0()
        .text_size(px(theme::FS_SM))
        .line_height(px(theme::LH_META))
        .text_color(rgb(theme::TEXT_MUTED))
        .child(text.into())
}

/// The command well: the exact input on `GROUND`, mono, `TEXT_STRONG`. The
/// one part of a card that gives way when the Pane is short: it keeps one
/// line and scrolls the rest, so the rows below it stay in reach.
pub fn well(child: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .flex_shrink_1()
        .min_h(px(theme::LH_CODE + 2. * theme::DECISION_WELL_PAD_Y))
        .overflow_hidden()
        .bg(rgb(theme::GROUND))
        .rounded(px(theme::R_CONTROL))
        .px(px(theme::DECISION_WELL_PAD_X))
        .py(px(theme::DECISION_WELL_PAD_Y))
        .font_family(theme::FONT_CODE)
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_CODE))
        .text_color(rgb(theme::TEXT_STRONG))
        .child(child)
}

// ------------------------------------------------------------------- rows

/// What one option row shows.
pub struct Row {
    /// The key that picks it; `None` draws an empty key column.
    pub key: Option<SharedString>,
    pub label: SharedString,
    pub description: Option<SharedString>,
    pub recommended: bool,
    pub selected: bool,
    pub enabled: bool,
    /// Question options read as prose (Geist); approval verbs are mono.
    pub prose: bool,
}

/// One option row on `gpui_base::Button` (tab stop, Enter/Space, focus
/// ring): keycap, label over description, and a trailing `recommended` tag
/// and selected check. Hover `FILL`; selected `FILL` + an `ACCENT` check;
/// keyboard focus the one `FOCUS_RING` recipe; disabled `TEXT_MUTED` ink
/// with no key, no hover.
pub fn option_row(id: impl Into<ElementId>, row: Row) -> gpui_base::Button {
    let ink = match (row.enabled, row.selected) {
        (false, _) => theme::TEXT_MUTED,
        (true, true) => theme::TEXT_STRONG,
        (true, false) => theme::TEXT,
    };
    let label = if row.prose {
        div()
            .font_family(theme::FONT_UI)
            .text_size(px(theme::FS_PROSE_SM))
            .font_weight(theme::W_LABEL)
    } else {
        div()
            .font_family(theme::FONT_UI)
            .text_size(px(theme::FS_UI))
    };
    let accessibility = SharedString::from(format!(
        "{}{}",
        row.label,
        if row.selected { ", selected" } else { "" }
    ));
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
        .px(px(theme::DECISION_ROW_PAD_X))
        .py(px(theme::DECISION_ROW_PAD_Y))
        .rounded(px(theme::R_CONTROL))
        .line_height(px(theme::LH_PROSE_SM))
        .text_color(rgb(ink))
        .when(row.selected, |button| button.bg(rgb(theme::FILL)))
        .map(|button| match (row.enabled, row.selected) {
            (false, _) => button.cursor_default(),
            (true, true) => button.hover_carried().press_raised(),
            (true, false) => button.hover_raised().press_raised(),
        })
        .focus_visible(components::control_focus)
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .min_w(px(theme::KBD_H))
                .h(px(theme::LH_PROSE_SM))
                .children(row.key.filter(|_| row.enabled).map(components::kbd)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(label.line_height(px(theme::LH_PROSE_SM)).child(row.label))
                .children(row.description.filter(|text| !text.is_empty()).map(|text| {
                    div()
                        .font_family(theme::FONT_UI)
                        .text_size(px(theme::FS_PROSE_SM))
                        .line_height(px(theme::LH_PROSE_SM))
                        .text_color(rgb(theme::TEXT_2))
                        .child(text)
                })),
        )
        .when(row.recommended, |button| {
            button.child(
                div()
                    .flex_shrink_0()
                    .font_family(theme::FONT_UI)
                    .text_size(px(theme::FS_SM))
                    .line_height(px(theme::LH_PROSE_SM))
                    .text_color(rgb(theme::ACCENT))
                    .child("recommended"),
            )
        })
        .when(row.selected, |button| {
            button.child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .h(px(theme::LH_PROSE_SM))
                    .child(icons::icon(
                        icons::CHECK,
                        theme::DECISION_CHECK,
                        theme::ACCENT,
                    )),
            )
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

/// The answer row: key hints on the left, actions on the right.
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

/// The completing action (`Send`, `Complete`): the steel primary.
pub fn send_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    disabled: bool,
    cx: &App,
) -> gpui::component::button::Button {
    components::primary_button(id, disabled, cx)
        .h(px(theme::CONTROL_H))
        .px(px(theme::CONTROL_PAD_X))
        .child(
            components::text_ui()
                .font_weight(theme::W_LABEL)
                .text_color(rgb(if disabled {
                    theme::TEXT_MUTED
                } else {
                    theme::ON_ACCENT
                }))
                .child(label.into()),
        )
}

/// The declining action (`Skip`, `Cancel`): a ghost.
pub fn skip_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &App,
) -> gpui::component::button::Button {
    components::ghost_button(id, label, cx).tab_stop(true)
}

// -------------------------------------------------------------------- L2

/// An L2 keycap pair — `y allow` — pressable, on the cell's `PANE` ground:
/// the key's cap, then its verb, muted.
pub fn key_action(id: &'static str, key: &'static str, verb: &'static str) -> Stateful<Div> {
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
        .hover_row()
        .press_row()
        .child(components::kbd(key))
        .child(div().pr(px(theme::SPACE_1)).child(verb))
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
