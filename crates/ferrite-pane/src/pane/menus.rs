//! The Composer menus' rows: the `/` and `@` popover and the band pickers.

use super::*;

/// One row of the `/` or `@` popover, ready to draw: what a pick inserts,
/// what the row shows, and where the fuzzy filter matched. Prepared by the
/// cockpit when the menu changes — never per frame.
pub struct MenuRow {
    /// What lands in the line on ↵ — a command name, or a file's relative
    /// path.
    pub insert: SharedString,
    /// The row's leading text: `/name`, or the file's name.
    pub name: SharedString,
    /// Matched byte ranges inside `name`, painted `TEXT_STRONG` at `W_STRONG`
    /// (`components::match_highlights`; one face, so nothing reflows).
    pub matched: Vec<std::ops::Range<usize>>,
    /// The dimmer text after it: a command's description, or the file's
    /// directory. Empty draws nothing.
    pub detail: SharedString,
    /// Whether `detail` is Ferrite's description (cut at its end) or
    /// machine text such as a path (cut at its head so the useful tail
    /// survives).
    pub prose_detail: bool,
    /// A row kept visible but dead (#25's locked provider door): muted ink,
    /// no match highlights, and its pick does nothing but dismiss.
    pub inert: bool,
}

/// The Composer menus' popover: the float, as wide as its slot (at most
/// `SLASH_MENU_W`), capped at `MENU_MAX_H` (its row list scrolls past
/// that).
pub fn menu_popover() -> Div {
    ferrite_ui::menu::float()
        .w_full()
        .max_h(px(theme::MENU_MAX_H))
}

/// A `/` or `@` row in the float grammar: the cursor row's fill, the name with
/// its matches in `TEXT_STRONG`, the detail muted in its own column
/// (`label_w` aligns the slash commands' descriptions), `↵` on the cursor
/// row.
pub fn menu_row(
    id: impl Into<gpui::ElementId>,
    row: &MenuRow,
    cursor: bool,
    label_w: Option<f32>,
) -> Stateful<Div> {
    // A description or a path cut at the popover's width keeps its whole
    // text one hover away.
    let detail = (!row.detail.is_empty()).then(|| row.detail.clone());
    ferrite_ui::menu::item_row(id, &menu_item(row, cursor, label_w), cursor, false)
        .when_some(detail, |row, detail| {
            row.tooltip(ferrite_ui::menu::tooltip(detail))
        })
}

/// A `MenuRow` as the shared menu row's content.
pub fn menu_item(row: &MenuRow, cursor: bool, label_w: Option<f32>) -> components::MenuItem {
    let mut item = components::MenuItem::new(row.name.clone())
        .matched(row.matched.clone())
        .disabled(row.inert);
    if let Some(width) = label_w {
        item = item.label_w(width);
    }
    if !row.detail.is_empty() {
        let detail = if row.prose_detail {
            row.detail.clone()
        } else {
            head_truncated(&row.detail, theme::MENU_PATH_TAIL)
        };
        item = item.detail(detail).mono(!row.prose_detail);
    }
    // No ↵ on an inert row: enter only dismisses there, and the key would
    // advertise an offer the row does not make.
    if cursor && !row.inert {
        item = item.shortcut("↵");
    }
    item
}

/// A path cut at its head to about its last `tail` characters, behind
/// `…/`: gpui truncates only at the end, where a path keeps what matters.
fn head_truncated(path: &SharedString, tail: usize) -> SharedString {
    let count = path.chars().count();
    if count <= tail {
        return path.clone();
    }
    let rest: String = path.chars().skip(count - tail).collect();
    let rest = rest.split_once('/').map_or(rest.as_str(), |(_, rest)| rest);
    format!("…/{rest}").into()
}


/// The ✓-row the pickers share — the band popovers (#29) — so "what this
/// Pane is on right now" can never be spelled two ways: the strong check on
/// the standing choice, the muted tag ("checked out", "worktree · dir")
/// after the label.
pub fn picker_row(
    id: impl Into<gpui::ElementId>,
    label: SharedString,
    detail: SharedString,
    cursor: bool,
    active: bool,
    inert: bool,
) -> Stateful<Div> {
    let mut item = components::MenuItem::new(label)
        .checked(active)
        .disabled(inert);
    if !detail.is_empty() {
        item = item.detail(detail);
    }
    ferrite_ui::menu::item_row(id, &item, cursor, false)
}

/// A picker's section row: the Provider's logomark in its brand colour (the
/// one place brand colour is allowed) and its name, lowercase, with an
/// optional note after a `·` (`codex · handover`). Non-interactive — the
/// arrows skip it.
pub fn picker_section(provider: Provider, note: SharedString) -> Div {
    let (mark, ink, title) = match provider {
        Provider::Codex => (icons::CODEX, theme::PROVIDER_CODEX, "codex"),
        Provider::Claude => (icons::CLAUDE, theme::PROVIDER_CLAUDE, "claude"),
    };
    ferrite_ui::menu::section(title, Some((mark, ink)), (!note.is_empty()).then_some(note))
}

/// The popover's key-hint footer, each menu supplying its own verbs.
pub fn popover_footer(hints: &[(&str, &str)]) -> Div {
    ferrite_ui::menu::footer(hints)
}
