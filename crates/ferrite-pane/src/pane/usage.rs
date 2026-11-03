//! Context and plan usage: the panel's Context section (its reading, its
//! stacked bar, the legend and the lists behind it) and the Composer
//! footer's plan-usage ring with its popover.

use super::*;
use ferrite_core::transcript::RateLimits;
use ferrite_core::{ContextDetails, ContextItem, RateLimitWindow};

/// The footer's plan-usage ring (Claude's): the fullest account window the
/// provider reported, drawn round a `CONTEXT_RING` circle in a footer
/// chip's box. Under the pointer its tooltip reads that window in one line
/// (`Weekly · all models: 17% · Resets Tue 3:30 pm`); a click opens the
/// popover above it (`plan_usage_card`), which the box wears as selected
/// while it is open. None while the provider reports no window: no
/// reading is invented. Context fill is the side panel's.
pub fn plan_usage_ring(
    id: impl Into<gpui::ElementId>,
    limits: RateLimits,
    cx: &gpui::App,
) -> Option<impl IntoElement> {
    let now = ferrite_core::clock::system_time();
    let (name, fullest, span) =
        plan_windows(limits).max_by(|a, b| a.1.used_fraction.total_cmp(&b.1.used_fraction))?;
    let used = fullest.used_fraction.clamp(0., 1.);
    let tip = std::iter::once(format!("{name}: {}", percent(used)))
        .chain(reset_label(fullest.resets_at, span, now).map(|reset| reset.to_string()))
        .collect::<Vec<_>>()
        .join(" \u{b7} ");
    let id = id.into();
    let trigger = toolbar_chip(id.clone(), cx)
        .debug_selector(|| "plan-usage-ring".into())
        .flex_shrink_0()
        .px(px((theme::FOOTER_CHIP_H - theme::CONTEXT_RING) / 2.))
        .tip(tip)
        .child(usage_ring(used));
    Some(
        gpui::component::popover::Popover::new(gpui::ElementId::NamedChild(
            std::sync::Arc::new(id),
            "popover".into(),
        ))
        // The card is its own surface; the kit's would frame it again.
        .appearance(false)
        .anchor(gpui::Anchor::BottomRight)
        // Its foot `USAGE_POP_OFFSET` above the box's top (the kit
        // leaves 4).
        .bottom(px(theme::USAGE_POP_OFFSET))
        .trigger(trigger)
        .content(move |_, _, _| plan_usage_card(limits, now)),
    )
}

/// The ring's reading: the fullest window reported, or None.
pub fn plan_usage_fraction(limits: RateLimits) -> Option<f32> {
    plan_windows(limits)
        .map(|(_, limit, _)| limit.used_fraction.clamp(0., 1.))
        .reduce(f32::max)
}

/// The account windows the provider reported, as the popover names them,
/// with their spans.
fn plan_windows(
    limits: RateLimits,
) -> impl Iterator<Item = (&'static str, RateLimitWindow, Duration)> {
    [
        ("5-hour \u{b7} all models", limits.five_hour, 5 * 3_600),
        ("Weekly \u{b7} all models", limits.weekly, 7 * 86_400),
    ]
    .into_iter()
    .filter_map(|(name, limit, span)| Some((name, limit?, Duration::from_secs(span))))
}

/// A share as the usage surfaces print it: `18%`.
fn percent(fraction: f32) -> String {
    format!("{}%", (fraction.clamp(0., 1.) * 100.).round() as u32)
}

/// The ring's popover (Claude's `Plan usage limits`): a quiet head, then
/// each reported window — its name, when it resets and the share used, over
/// its bar in `usage_ink`.
fn plan_usage_card(limits: RateLimits, now: SystemTime) -> Div {
    let window = |(name, limit, span): (&'static str, RateLimitWindow, Duration)| {
        let used = limit.used_fraction.clamp(0., 1.);
        let reading = percent(used);
        let selector = format!("plan-usage-{}", reading.trim_end_matches('%'));
        div()
            .flex()
            .flex_col()
            .gap(px(theme::USAGE_POP_BAR_GAP))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(theme::SPACE_3))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(INK_STRONG))
                            .child(name),
                    )
                    .children(reset_label(limit.resets_at, span, now).map(|reset| {
                        div()
                            .flex_shrink_0()
                            .text_color(rgb(INK_SUBTLE))
                            .child(reset)
                    }))
                    .child(components::tabular(
                        div()
                            .debug_selector(move || selector.clone())
                            .flex_shrink_0()
                            .text_color(rgb(INK_SUBTLE))
                            .child(reading),
                    )),
            )
            .child(
                div()
                    .w_full()
                    .h(px(theme::USAGE_POP_BAR_H))
                    .rounded_full()
                    .overflow_hidden()
                    .bg(rgb(theme::USAGE_POP_TRACK))
                    .child(div().h_full().w(relative(used)).bg(rgb(usage_ink(used)))),
            )
    };
    components::floating_surface()
        .debug_selector(|| "plan-usage-card".into())
        .flex()
        .flex_col()
        .gap(px(theme::USAGE_POP_GAP))
        .w(px(theme::USAGE_POP_W))
        .px(px(theme::USAGE_POP_PAD_X))
        .py(px(theme::USAGE_POP_PAD_Y))
        .font_family(theme::FONT_UI)
        .text_size(px(theme::FS_META))
        .line_height(px(theme::LH_UI))
        .whitespace_nowrap()
        .child(div().text_color(rgb(INK_SUBTLE)).child("Plan usage limits"))
        .children(plan_windows(limits).map(window))
}

/// A usage ring: a `CONTEXT_RING` circle, its track a hairline, `fraction`
/// drawn round it from the top in `INK_SUBTLE`, `ATTENTION` once the
/// window runs tight (`USAGE_TIGHT`). `fraction` is clamped.
fn usage_ring(fraction: f32) -> Div {
    let fraction = fraction.clamp(0., 1.);
    let ink: gpui::Hsla = rgb(if fraction < theme::USAGE_TIGHT {
        INK_SUBTLE
    } else {
        ATTENTION
    })
    .into();
    let track: gpui::Hsla = theme::paint::LINE.into();
    div().flex_shrink_0().size(px(theme::CONTEXT_RING)).child(
        gpui::canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let stroke = px(theme::CONTEXT_RING_STROKE);
                let r = (bounds.size.width.min(bounds.size.height) - stroke) / 2.;
                let c = bounds.center();
                let at = |turn: f32| {
                    let a = std::f32::consts::TAU * turn - std::f32::consts::FRAC_PI_2;
                    gpui::point(c.x + r * a.cos(), c.y + r * a.sin())
                };
                let arc = |from: f32, to: f32| {
                    let mut path = gpui::PathBuilder::stroke(stroke);
                    path.move_to(at(from));
                    // Two halves at most, so no arc spans a full turn.
                    let mid = (from + to) / 2.;
                    for end in [mid, to] {
                        path.arc_to(gpui::point(r, r), px(0.), false, true, at(end));
                    }
                    path.build().ok()
                };
                if let Some(path) = arc(0., 1.) {
                    window.paint_path(path, track);
                }
                if fraction > 0.005 {
                    if let Some(path) = arc(0., fraction) {
                        window.paint_path(path, ink);
                    }
                }
            },
        )
        .size_full(),
    )
}

/// When a subscription window resets, as the popover reads it: `Resets
/// 3:30 pm` today, `Resets Tue 3:30 pm` later in the week. Providers
/// disagree on the field's units, so only a future value inside the
/// window's own span is read as a Unix instant: a guess is worse than none.
pub(super) fn reset_label(
    resets_at: Option<u64>,
    span: Duration,
    now: SystemTime,
) -> Option<SharedString> {
    let seconds = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let at = resets_at?;
    at.checked_sub(seconds)
        .filter(|remaining| (1..=span.as_secs()).contains(remaining))?;
    let at = UNIX_EPOCH + Duration::from_secs(at);
    Some(format!("Resets {}", ferrite_core::clock::day_label(at, now)).into())
}

/// A token count as the card reads it (rule 2.11.6): only as precise as it
/// needs to be — `640`, `1.5k`, `64k`, `1.2M`.
pub fn compact_count(count: u64) -> String {
    fn trimmed(value: f64) -> String {
        let text = format!("{value:.1}");
        text.strip_suffix(".0").unwrap_or(&text).to_owned()
    }
    match count {
        0..=999 => count.to_string(),
        1_000..=9_999 => format!("{}k", trimmed(count as f64 / 1_000.)),
        10_000..=999_999 => format!("{}k", (count as f64 / 1_000.).round() as u64),
        _ => format!("{}M", trimmed(count as f64 / 1_000_000.)),
    }
}

/// A context category's ink in the card's stacked bar and in its legend
/// (`CTX_*`: the palette's own quiet hues). The categories a provider
/// usually reports keep one ink each; any other name takes the next from a
/// fixed cycle, so a new category still reads as distinct.
pub(super) fn category_ink(name: &str, index: usize) -> u32 {
    let name = name.to_ascii_lowercase();
    if name.contains("deferred") {
        theme::CTX_DEFERRED
    } else if name.contains("free") {
        theme::CTX_FREE
    } else if name.contains("buffer") || name.contains("compact") {
        theme::CTX_BUFFER
    } else if name.contains("message") {
        theme::CTX_MESSAGES
    } else if name.contains("mcp") {
        theme::CTX_MCP
    } else if name.contains("tool") {
        theme::CTX_TOOLS
    } else if name.contains("skill") {
        theme::CTX_SKILLS
    } else if name.contains("prompt") {
        theme::CTX_PROMPT
    } else if name.contains("memory") {
        theme::CTX_MEMORY
    } else {
        theme::CTX_CYCLE[index % theme::CTX_CYCLE.len()]
    }
}

/// Free space and deferred tools are reported beside the window's
/// contents but take none of it: they are listed, never drawn in the bar.
fn category_in_window(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    !name.contains("free") && !name.contains("deferred")
}

/// What a context bar draws: each category in the window that holds
/// tokens, its ink and its share of `denominator`, in the provider's order.
pub fn context_segments(
    categories: &[ferrite_core::ContextCategory],
    denominator: u64,
) -> Vec<(u32, f32)> {
    if denominator == 0 {
        return Vec::new();
    }
    categories
        .iter()
        .enumerate()
        .filter(|(_, category)| category_in_window(&category.name) && category.tokens > 0)
        .map(|(index, category)| {
            (
                category_ink(&category.name, index),
                category.tokens as f32 / denominator as f32,
            )
        })
        .collect()
}

/// A context meter with no legend to keep: one segment per category in
/// the window, in its `CTX_*` ink and at its share of `window`, or one
/// `CTX_CYCLE[0]` segment when the provider names none; past
/// `CTX_METER_WARN` every segment wears the state's ink (`ctx_meter_ink`).
/// The Context section's bar uses it only when the provider names no
/// categories; with them it keeps each category's ink to match its legend
/// (`context_usage`).
pub fn context_meter_segments(
    total: u64,
    window: u64,
    details: Option<&ferrite_core::ContextDetails>,
) -> Vec<(u32, f32)> {
    let fraction = total as f32 / window as f32;
    let mut segments = details
        .map(|details| context_segments(&details.categories, window))
        .unwrap_or_default();
    if segments.is_empty() {
        segments.push((theme::CTX_CYCLE[0], fraction));
    }
    if fraction >= theme::CTX_METER_WARN {
        let ink = ctx_meter_ink(fraction);
        for segment in &mut segments {
            segment.0 = ink;
        }
    }
    segments
}

/// The panel's Context section under its head (SPEC §5.6, Claude's): how
/// full the window is (`27% used`, `109.9k / 400k`) over a bar stacked by
/// category, then the legend, always open — each category in the window
/// with its ink, its count and its share; the deferred ones apart, counted
/// but taking no share; and the lists behind the categories that have
/// them (`MCP tools`, `Skills`, `Memory files`), each folding open to its
/// items. Counts are reported values, never estimates. The turn's usage
/// closes it when the provider reports it.
pub fn context_usage(
    usage: ferrite_core::transcript::Usage,
    details: Option<&ContextDetails>,
    usage_details: Option<&ferrite_core::UsageDetails>,
) -> impl IntoElement {
    let maximum = usage.context_window.filter(|limit| *limit > 0);
    let current = usage.total_tokens;
    let categories = details.map_or(&[][..], |details| details.categories.as_slice());
    // Shares are of the whole window when the provider reports one; of
    // what the categories add up to when it does not.
    let denominator = maximum.unwrap_or_else(|| {
        categories
            .iter()
            .filter(|category| category_in_window(&category.name))
            .map(|category| category.tokens)
            .sum()
    });
    let share_of = |tokens: u64| -> f32 {
        if denominator == 0 {
            0.
        } else {
            tokens as f32 / denominator as f32
        }
    };

    // The reading: the share used at the left, in its state's ink past
    // the warning (rule 6: colour on the word, the bar keeping its
    // categories' inks); the counts behind it at the right.
    let reading = match maximum {
        Some(maximum) => {
            let fraction = current as f32 / maximum as f32;
            let label = format!("{} used", percent(fraction));
            let selector = format!(
                "context-usage-context-{}",
                percent(fraction).trim_end_matches('%')
            );
            let ink = if fraction >= theme::CTX_METER_WARN {
                ctx_meter_ink(fraction)
            } else {
                INK_STRONG
            };
            div()
                .debug_selector(move || selector.clone())
                .flex_shrink_0()
                .text_color(rgb(ink))
                .child(label)
        }
        None => div()
            .flex_shrink_0()
            .text_color(rgb(INK_STRONG))
            .child(format!("{} tokens", compact_count(current))),
    };
    let counts = div()
        .debug_selector(move || match maximum {
            Some(maximum) => format!("context-usage-maximum-{maximum}"),
            None => "context-usage-maximum-unknown".into(),
        })
        .flex_shrink_0()
        .text_color(rgb(INK_SUBTLE))
        .children(
            maximum
                .map(|maximum| format!("{} / {}", compact_count(current), compact_count(maximum))),
        );
    let head = div()
        .debug_selector(move || format!("context-usage-current-{current}"))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(theme::SPACE_3))
        .h(px(LH_UI))
        .child(reading)
        .child(counts);

    // The bar: each category in its own ink, as the legend under it shows
    // it, at its share of the window. A provider that names no categories
    // gets one segment (`context_meter_segments`), warning ink and all.
    let by_category = context_segments(categories, denominator);
    let in_window = match maximum {
        Some(maximum) if by_category.is_empty() => context_meter_segments(current, maximum, None),
        _ => by_category,
    };
    let bar = (!in_window.is_empty()).then(|| {
        div()
            .flex()
            .w_full()
            .mt(px(theme::SPACE_1))
            .h(px(theme::USAGE_CARD_BAR_H))
            .rounded_full()
            .overflow_hidden()
            .bg(theme::paint::LINE)
            .children(in_window.into_iter().map(|(ink, share)| {
                div()
                    .flex_shrink_0()
                    .h_full()
                    .w(relative(share.clamp(0., 1.)))
                    .bg(rgb(ink))
            }))
    });

    let group = || {
        div()
            .flex()
            .flex_col()
            .mt(px(theme::USAGE_LEGEND_GROUP_GAP))
    };
    let mut held = group();
    let mut deferred = group();
    let mut any_deferred = false;
    for (index, category) in categories.iter().enumerate() {
        let tokens = category.tokens;
        let row = |share: String| {
            legend_row(
                format!("context-category-{index}-{tokens}"),
                category_ink(&category.name, index),
                sentence_case(&category.name),
                tokens,
                share,
            )
        };
        if category.name.to_ascii_lowercase().contains("deferred") {
            // Loaded on demand: counted, but no share of the window.
            any_deferred = true;
            deferred = deferred.child(row("\u{2014}".into()));
        } else {
            held = held.child(row(format!("{:.1}%", share_of(tokens) * 100.)));
        }
    }
    let lists = details.map(|details| {
        group().children(
            [
                (
                    "mcp-tools",
                    "MCP tools",
                    &details.mcp_tools,
                    ListItem::McpTool,
                ),
                ("skills", "Skills", &details.skills, ListItem::Plain),
                (
                    "memory-files",
                    "Memory files",
                    &details.memory_files,
                    ListItem::Path,
                ),
            ]
            .into_iter()
            .filter(|(_, _, items, _)| !items.is_empty())
            .map(|(key, label, items, kind)| ContextList {
                key,
                label,
                items: items.iter().map(|item| kind.row(item)).collect(),
            }),
        )
    });

    let mut card = div()
        .id("context-usage-body")
        .flex()
        .flex_col()
        .w_full()
        .px(px(theme::PANEL_ROW_ICON_X))
        .text_size(px(theme::FS_META))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(INK_STRONG))
        .child(head)
        .children(bar)
        .child(held)
        .when(any_deferred, |card| card.child(deferred))
        .children(lists);

    // The turn's usage: a quiet key at the left, the reported value
    // right-aligned in tabular digits, under the scope's heading.
    let row = |key: String, value: String| {
        div()
            .flex()
            .flex_shrink_0()
            .justify_between()
            .gap(px(theme::SPACE_3))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(INK_SUBTLE))
                    .child(key),
            )
            .child(components::tabular(div().flex_shrink_0().child(value)))
    };
    if let Some(details) = usage_details {
        let scope = match details.scope {
            ferrite_core::UsageScope::Message => ("message", "This message"),
            ferrite_core::UsageScope::Turn => ("turn", "This turn"),
            ferrite_core::UsageScope::Session => ("session", "This session"),
        };
        let mut section = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .mt(px(theme::USAGE_CARD_GAP))
            .gap(px(theme::SPACE_0_5))
            .child(
                ferrite_ui::menu::section(scope.1, None, None)
                    .debug_selector(move || format!("usage-scope-{}", scope.0)),
            );
        for (key, label, count) in [
            ("input", "Input", details.input_tokens),
            ("cached-input", "Cached input", details.cached_input_tokens),
            ("output", "Output", details.output_tokens),
            (
                "reasoning-output",
                "Reasoning output",
                details.reasoning_output_tokens,
            ),
        ] {
            section = section.child(
                row(label.into(), compact_count(count))
                    .debug_selector(move || format!("usage-{key}-{count}")),
            );
        }
        card = card.child(section);
    }
    // Counts and percentages tick while the panel is open.
    components::tabular(card)
}

/// One legend row (`USAGE_LEGEND_ROW_H`): the category's square in the
/// name column's gutter, its name, its count and its share, the share
/// right-aligned in `USAGE_SHARE_W` so the column lines up.
fn legend_row(selector: String, ink: u32, name: String, tokens: u64, share: String) -> Div {
    div()
        .debug_selector(move || selector.clone())
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(theme::USAGE_LEGEND_ROW_H))
        .child(
            div()
                .flex_shrink_0()
                .w(px(theme::USAGE_LEGEND_TEXT_X))
                .child(
                    div()
                        .size(px(theme::USAGE_SWATCH))
                        .rounded(px(theme::R_TIGHT))
                        .bg(rgb(ink)),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(rgb(INK_SUBTLE))
                .child(name),
        )
        .child(
            div()
                .flex_shrink_0()
                .pl(px(theme::SPACE_2))
                .text_color(rgb(INK_SUBTLE))
                .child(compact_count(tokens)),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .flex_shrink_0()
                .w(px(theme::USAGE_SHARE_W))
                .text_color(rgb(INK_STRONG))
                .child(share),
        )
}

/// How a list's items read.
#[derive(Clone, Copy)]
enum ListItem {
    /// As reported: a skill's name.
    Plain,
    /// `mcp__github__get_issue` as `github · get_issue`.
    McpTool,
    /// A memory file's path, the home spelled `~`.
    Path,
}

impl ListItem {
    fn row(self, item: &ContextItem) -> (String, u64) {
        let name = match self {
            ListItem::Plain => item.name.clone(),
            ListItem::McpTool => mcp_tool_name(&item.name),
            ListItem::Path => components::home_relative(std::path::Path::new(&item.name)),
        };
        (name, item.tokens)
    }
}

/// An MCP tool's full name as the list reads it: its server, then the tool
/// (`github · get_issue`); a name that is not `mcp__server__tool` reads
/// whole.
fn mcp_tool_name(name: &str) -> String {
    name.strip_prefix("mcp__")
        .and_then(|rest| rest.split_once("__"))
        .map_or_else(
            || name.to_owned(),
            |(server, tool)| format!("{server} \u{b7} {tool}"),
        )
}

/// A list behind a category (`MCP tools 197`): a row that folds open to
/// its items, its chevron in the name column's gutter and its count at the
/// right. Under the pointer its words blend up to `INK_STRONG`. The fold is this
/// element's own, kept while the panel shows.
#[derive(IntoElement)]
struct ContextList {
    key: &'static str,
    label: &'static str,
    items: Vec<(String, u64)>,
}

impl RenderOnce for ContextList {
    fn render(self, window: &mut gpui::Window, cx: &mut gpui::App) -> impl IntoElement {
        let id = SharedString::from(format!("context-list-{}", self.key));
        let open = window.use_keyed_state(id.clone(), cx, |_, _| false);
        let unfolded = *open.read(cx);
        let hover = ferrite_ui::pointer::hover_key(&gpui::ElementId::from(id.clone()));
        let count = self.items.len();
        let head = div()
            .id(id.clone())
            .debug_selector({
                let id = id.clone();
                move || id.to_string()
            })
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(theme::USAGE_LEGEND_ROW_H))
            .cursor_pointer()
            .text_color(ferrite_ui::motion::hover_blend(
                &hover,
                rgb(INK_SUBTLE).into(),
                rgb(INK_STRONG).into(),
            ))
            .on_hover(ferrite_ui::motion::hover_listener(hover))
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .w(px(theme::USAGE_LEGEND_TEXT_X))
                    .child(components::fold_chevron(
                        unfolded,
                        theme::ICON_CHEVRON,
                        INK_SUBTLE,
                    )),
            )
            .child(div().flex_1().min_w_0().truncate().child(self.label))
            .child(
                div()
                    .flex_shrink_0()
                    .pl(px(theme::SPACE_2))
                    .child(count.to_string()),
            )
            .on_click(move |_, _, cx| {
                open.update(cx, |open, cx| {
                    *open = !*open;
                    cx.notify();
                })
            });
        let key = self.key;
        div().flex().flex_col().child(head).when(unfolded, |list| {
            list.children(
                self.items
                    .into_iter()
                    .enumerate()
                    .map(|(index, (name, tokens))| {
                        div()
                            .debug_selector(move || format!("context-list-{key}-{index}"))
                            .flex()
                            .flex_shrink_0()
                            .items_center()
                            .h(px(theme::USAGE_LEGEND_ROW_H))
                            .pl(px(theme::USAGE_LEGEND_TEXT_X))
                            .text_color(rgb(INK_SUBTLE))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis_middle()
                                    .child(name),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .pl(px(theme::SPACE_2))
                                    .child(compact_count(tokens)),
                            )
                    }),
            )
        })
    }
}

/// A usage reading's ink: neutral `INK_STRONG` until the window runs tight
/// (`USAGE_TIGHT`), then `ATTENTION`. There is no `BLOCKED` step: a full
/// window stops nothing until the provider says so. Colour is state; a
/// context half full is not one.
pub fn usage_ink(fraction: f32) -> u32 {
    if fraction >= theme::USAGE_TIGHT {
        ATTENTION
    } else {
        INK_STRONG
    }
}

/// The ctx meter's fill: `RUNNING` while the window has room, `ATTENTION`
/// from `CTX_METER_WARN`, `BLOCKED` from `CTX_METER_FULL` — the prototype's
/// green, yellow and red by fill.
pub fn ctx_meter_ink(fraction: f32) -> u32 {
    if fraction >= theme::CTX_METER_FULL {
        BLOCKED
    } else if fraction >= theme::CTX_METER_WARN {
        ATTENTION
    } else {
        RUNNING
    }
}
