//! Context and account usage: the Context tab, its bars and ink, and the
//! Composer footer's plan-usage ring and its card.

use super::*;

/// The footer's plan-usage ring (Claude's): the fullest account window
/// the provider reported, drawn round a `CONTEXT_RING` circle, its card
/// (`plan_usage_card`) on hover. None while the provider reports no
/// window: no reading is invented. Context fill is the side panel's.
pub fn plan_usage_ring(
    id: impl Into<gpui::ElementId>,
    limits: ferrite_core::transcript::RateLimits,
) -> Option<Stateful<Div>> {
    let fraction = plan_usage_fraction(limits)?;
    Some(
        div()
            .id(id)
            .debug_selector(|| "plan-usage-ring".into())
            .flex()
            .items_center()
            .child(usage_ring(fraction))
            .tooltip(move |window, cx| {
                let now = ferrite_core::clock::system_time();
                gpui::component::tooltip::Tooltip::element(move |_, _| plan_usage_card(limits, now))
                    .font_family(theme::FONT_UI)
                    .text_size(px(theme::FS_UI))
                    .line_height(px(theme::LH_UI))
                    .px(px(theme::TOOLTIP_PAD_X))
                    .py(px(theme::TOOLTIP_PAD_Y))
                    .w(px(theme::USAGE_TIP_W))
                    .rounded(px(theme::R_FLOAT))
                    .border_color(theme::paint::FLOAT_EDGE)
                    .shadow(components::float_shadow())
                    .build(window, cx)
            }),
    )
}

/// The ring's reading: the fullest window reported, or None.
pub fn plan_usage_fraction(limits: ferrite_core::transcript::RateLimits) -> Option<f32> {
    [limits.five_hour, limits.weekly]
        .into_iter()
        .flatten()
        .map(|limit| limit.used_fraction.clamp(0., 1.))
        .reduce(f32::max)
}

/// The ring's card (Claude's `Plan usage limits`): a quiet head, then each
/// reported window — its name, when it resets, the share used in
/// `usage_ink` — over its bar.
fn plan_usage_card(limits: ferrite_core::transcript::RateLimits, now: SystemTime) -> Div {
    let window = |label: &'static str, limit: ferrite_core::RateLimitWindow, span: Duration| {
        let used = limit.used_fraction.clamp(0., 1.);
        let percent = (used * 100.).round() as u32;
        let reset = reset_label(limit.resets_at, span, now).map(|label| {
            let mut chars = label.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        });
        div()
            .flex()
            .flex_col()
            .gap(px(theme::USAGE_CARD_ROW_GAP))
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
                            .text_color(rgb(INK))
                            .child(label),
                    )
                    .children(reset.map(|reset| {
                        div()
                            .flex_shrink_0()
                            .text_color(rgb(INK_SUBTLE))
                            .child(reset)
                    }))
                    .child(components::tabular(
                        div()
                            .debug_selector(move || format!("plan-usage-{percent}"))
                            .flex_shrink_0()
                            .text_color(rgb(usage_ink(used)))
                            .child(format!("{percent}%")),
                    )),
            )
            .child(usage_bar(used))
    };
    div()
        .flex()
        .flex_col()
        .gap(px(theme::USAGE_CARD_GAP))
        .child(
            div()
                .text_size(px(theme::FS_META))
                .text_color(rgb(INK_SUBTLE))
                .child("Plan usage limits"),
        )
        .children(
            limits
                .five_hour
                .map(|limit| window("5-hour", limit, Duration::from_secs(5 * 3_600))),
        )
        .children(
            limits
                .weekly
                .map(|limit| window("Weekly", limit, Duration::from_secs(7 * 86_400))),
        )
}

/// One `USAGE_CARD_BAR_H` bar, full width, on the meter's track in
/// `usage_ink`.
fn usage_bar(fraction: f32) -> Div {
    let used = fraction.clamp(0., 1.);
    div()
        .w_full()
        .h(px(theme::USAGE_CARD_BAR_H))
        .bg(theme::paint::LINE)
        .child(div().h_full().w(relative(used)).bg(rgb(usage_ink(used))))
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

/// A subscription window's plausible Unix reset instant in compact, useful
/// units. Providers disagree on the field's units, so only a future value
/// inside the window's own maximum span is safe to present as a countdown.
pub(super) fn reset_label(
    resets_at: Option<u64>,
    span: Duration,
    now: SystemTime,
) -> Option<SharedString> {
    let now = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    // A reset the provider did not report, one already past, or one
    // outside the window's span reads nothing: a guess is worse than none.
    let remaining = resets_at?
        .checked_sub(now)
        .filter(|remaining| (1..=span.as_secs()).contains(remaining))?;
    let label = match remaining {
        0..=59 => "resets in <1m".into(),
        60..=3_599 => format!("resets in {}m", remaining / 60),
        3_600..=86_399 => {
            let hours = remaining / 3_600;
            let minutes = remaining % 3_600 / 60;
            if minutes == 0 {
                format!("resets in {hours}h")
            } else {
                format!("resets in {hours}h {minutes}m")
            }
        }
        86_400.. => {
            let days = remaining / 86_400;
            let hours = remaining % 86_400 / 3_600;
            if hours == 0 {
                format!("resets in {days}d")
            } else {
                format!("resets in {days}d {hours}h")
            }
        }
    };
    Some(SharedString::from(label))
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

/// A turn's cost as the card reads it: cents, or `<$0.01` below one.
pub(crate) fn cost_label(cost: f64) -> String {
    if cost < 0.005 {
        "<$0.01".into()
    } else {
        format!("${cost:.2}")
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

/// What the Panel foot's context meter draws: one segment per category in
/// the window, in its `CTX_*` ink and at its share of `window`, or one
/// `CTX_CYCLE[0]` segment when the provider names none; past
/// `CTX_METER_WARN` every segment wears the state's ink (`ctx_meter_ink`),
/// the foot having no legend to keep. The Context tab draws the same
/// segments but keeps each category's ink to match its legend
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

/// The panel's Context tab (SPEC §5.6): the context window and the
/// account windows, in that order, each a labelled bar over the reading
/// behind it. Counts are reported values, never estimates. An account
/// window the provider has not reported is left out; when it reports
/// neither, one line says so (`Limits not reported by this provider`) in
/// place of two empty bars. Each block holds the rows' inset
/// (the panel's `PANEL_ROW_ICON_X`).
///
/// The context bar is stacked by category when the provider says what
/// fills the window, and its heading is a row that opens the legend: every
/// category with its ink, its count and its share of the window. The tab
/// grows to `max_h` (the panel's room) and scrolls past it.
#[allow(clippy::too_many_arguments)]
pub fn context_usage(
    usage: ferrite_core::transcript::Usage,
    limits: ferrite_core::transcript::RateLimits,
    details: Option<&ferrite_core::ContextDetails>,
    usage_details: Option<&ferrite_core::UsageDetails>,
    last_cost: Option<f64>,
    expanded: bool,
    max_h: f32,
    on_toggle: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let maximum = usage.context_window.filter(|limit| *limit > 0);
    let now = ferrite_core::clock::system_time();
    let bar = usage_bar;
    // A window's heading: its name at the left, what it reads at the
    // right — the one line that answers the question at a glance. The
    // name is a row's label, not a title: `W_BODY`.
    let heading = |label: &'static str, value: AnyElement| {
        div()
            .flex()
            .items_baseline()
            .justify_between()
            .gap(px(theme::SPACE_3))
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(theme::W_BODY)
                    .text_color(rgb(INK))
                    .child(label),
            )
            .child(value)
    };
    let percent_value = |key: &'static str, fraction: f32| {
        let percent = (fraction.clamp(0., 1.) * 100.).round() as u32;
        div()
            .id(key)
            .debug_selector(move || format!("context-usage-{key}-{percent}"))
            .flex_shrink_0()
            .child(SharedString::from(format!("{percent}%")))
    };
    // The context window's percentage wears its state past the warning,
    // the bar keeping its categories' inks.
    let context_percent = |fraction: f32| {
        let reading = percent_value("context", fraction);
        if fraction >= theme::CTX_METER_WARN {
            reading.text_color(rgb(ctx_meter_ink(fraction)))
        } else {
            reading
        }
    };
    let reset_value = |key: &'static str, resets_at: Option<u64>, span: Duration| {
        reset_label(resets_at, span, now).map(|label| {
            div()
                .id(SharedString::from(format!("reset-{key}")))
                .text_size(px(theme::FS_UI))
                .debug_selector(move || format!("context-usage-{key}-reset-reported"))
                .text_color(rgb(INK_SUBTLE))
                .child(label)
                .into_any_element()
        })
    };
    let block = || {
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(theme::USAGE_CARD_ROW_GAP))
            .px(px(theme::PANEL_ROW_ICON_X))
    };
    let window =
        |label: &'static str, key: &'static str, fraction: f32, detail: Option<AnyElement>| {
            block()
                .child(heading(
                    label,
                    percent_value(key, fraction).into_any_element(),
                ))
                .child(bar(fraction))
                .children(detail)
        };
    let categories = details.map_or(&[][..], |details| details.categories.as_slice());
    let expandable = details.is_some_and(|details| {
        !details.categories.is_empty()
            || details.usable_window.is_some()
            || details.auto_compact_threshold.is_some()
    });
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
    // The counts behind the context bar, in the card's quietest ink: the
    // bar says how full, this says of what — `64k / 200k tokens`. When
    // there is a legend behind it, a chevron after the counts says so.
    let current = usage.total_tokens;
    let counts = components::tabular(
        div()
            .id("context-usage-counts")
            .flex()
            .items_center()
            .gap(px(theme::SPACE_1))
            .text_size(px(theme::FS_UI))
            .text_color(rgb(INK_SUBTLE))
            .debug_selector(move || match maximum {
                Some(maximum) => format!("context-usage-maximum-{maximum}"),
                None => "context-usage-maximum-unknown".into(),
            })
            .child(
                div()
                    .debug_selector(move || format!("context-usage-current-{current}"))
                    .child(SharedString::from(match maximum {
                        Some(maximum) => format!(
                            "{} / {} tokens",
                            compact_count(current),
                            compact_count(maximum)
                        ),
                        None => format!("{} tokens", compact_count(current)),
                    })),
            ),
    );
    let context_fraction = maximum.map(|maximum| current as f32 / maximum as f32);
    // The bar: each category in its own ink, as the legend under it shows
    // it, at its share of the window (or, with no window, of what the
    // categories add up to). A provider that names no categories gets the
    // Panel foot's one segment (`context_meter_segments`), warning ink and
    // all; with categories, past `CTX_METER_WARN` the state rides on the
    // heading's percentage instead (rule 6: colour on the word).
    let by_category = context_segments(categories, denominator);
    let in_window = match maximum {
        Some(maximum) if by_category.is_empty() => context_meter_segments(current, maximum, None),
        _ => by_category,
    };
    let context_bar = if !in_window.is_empty() {
        Some(
            div()
                .flex()
                .w_full()
                .h(px(theme::USAGE_CARD_BAR_H))
                .overflow_hidden()
                .bg(theme::paint::LINE)
                .children(in_window.into_iter().map(|(ink, share)| {
                    div()
                        .flex_shrink_0()
                        .h_full()
                        .w(relative(share.clamp(0., 1.)))
                        .bg(rgb(ink))
                }))
                .into_any_element(),
        )
    } else {
        // Nothing to draw: the count alone, no empty bar.
        None
    };
    // The context heading is the legend's door when there is one: the
    // whole line is the hit row, lifting to the hover face like a menu row,
    // with the chevron after the reading. It reads as every Panel section's
    // head does (`panel::panel_section_head`: 30 high, 12/500 `INK_SUBTLE`,
    // its reading at the right), so the sections read alike.
    let context_heading = div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(theme::SPACE_3))
        .h(px(theme::PANEL_SECTION_H))
        .text_size(px(theme::FS_META))
        .text_color(rgb(INK_SUBTLE))
        .child(
            div()
                .flex_shrink_0()
                .font_weight(theme::W_LABEL)
                .child("Context"),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(theme::SPACE_1))
                .children(context_fraction.map(context_percent))
                // One chevron that turns a quarter as the legend opens, over
                // `motion::CHEVRON` — the disclosures' turn.
                .when(expandable, |reading| {
                    reading.child(ferrite_ui::motion::settled(
                        "context-legend-turn",
                        expanded,
                        ferrite_ui::motion::CHEVRON,
                        |turn| {
                            icon(icons::CHEVRON_RIGHT, theme::ICON_CHEVRON, INK_SUBTLE)
                                .with_transformation(gpui::Transformation::rotate(gpui::radians(
                                    std::f32::consts::FRAC_PI_2 * turn,
                                )))
                        },
                    ))
                })
                .into_any_element(),
        );
    let context_heading = if expandable {
        let id = gpui::ElementId::from("context-window-toggle");
        let key = ferrite_ui::pointer::hover_key(&id);
        div()
            .id(id)
            .debug_selector(|| "context-window-toggle".into())
            .mx(px(-theme::PANEL_ROW_ICON_X))
            .px(px(theme::PANEL_ROW_ICON_X))
            .cursor_pointer()
            .hover_raised(key)
            .press_raised()
            .on_click(on_toggle)
            .child(context_heading)
            .into_any_element()
    } else {
        context_heading.into_any_element()
    };
    let mut context = block()
        .child(context_heading)
        .children(context_bar)
        .child(counts);
    // The legend: an ink square, the name, the count muted and the share
    // at the right edge, in tabular figures so the column lines up.
    let legend = |selector: String,
                  swatch: Option<u32>,
                  label: SharedString,
                  count: String,
                  share: SharedString| {
        components::tabular(
            div()
                .id(SharedString::from(selector.clone()))
                .debug_selector(move || selector.clone())
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(theme::SPACE_2))
                .h(px(theme::USAGE_LEGEND_ROW_H))
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(theme::USAGE_SWATCH))
                        .rounded(px(theme::R_TIGHT))
                        .when_some(swatch, |square, ink| square.bg(rgb(ink))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(INK))
                        .child(label),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(rgb(INK_SUBTLE))
                        .child(count),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .flex_shrink_0()
                        .w(px(theme::USAGE_SHARE_W))
                        .text_color(rgb(INK))
                        .child(share),
                ),
        )
    };
    if let Some(details) = details.filter(|_| expanded && expandable) {
        let mut rows = div().flex().flex_col().pt(px(theme::SPACE_1));
        for (index, category) in details.categories.iter().enumerate() {
            let tokens = category.tokens;
            // Deferred tools are loaded on demand: listed, but no share.
            let share = if category.name.to_ascii_lowercase().contains("deferred") {
                "\u{2014}".into()
            } else {
                format!("{:.1}%", share_of(tokens) * 100.)
            };
            rows = rows.child(legend(
                format!("context-category-{index}-{tokens}"),
                Some(category_ink(&category.name, index)),
                sentence_case(&category.name).into(),
                compact_count(tokens),
                share.into(),
            ));
        }
        if let Some(usable) = details.usable_window {
            rows = rows.child(legend(
                format!("context-usable-{usable}"),
                None,
                "Usable window".into(),
                compact_count(usable),
                "".into(),
            ));
        }
        if let Some(threshold) = details.auto_compact_threshold {
            rows = rows.child(legend(
                format!("context-compaction-{threshold}"),
                None,
                "Auto-compact at".into(),
                compact_count(threshold),
                match details.is_auto_compact_enabled {
                    Some(false) => "off".into(),
                    _ => "".into(),
                },
            ));
        }
        // The legend's rows fade in as it opens (`ROW_IN`, opacity only);
        // closing is instant, like every exit.
        context = context.child(ferrite_ui::motion::fade_in(
            "context-legend-in",
            rows,
            ferrite_ui::motion::ROW_IN,
            1.0,
        ));
    }
    let mut card = div()
        .id("context-usage-body")
        .flex()
        .flex_col()
        .w_full()
        .gap(px(theme::USAGE_CARD_GAP))
        .text_size(px(theme::FS_UI))
        .line_height(px(theme::LH_UI))
        .text_color(rgb(INK))
        .child(context);
    if limits.five_hour.is_none() && limits.weekly.is_none() {
        // On the blocks' inset, not a float's: the card's left edge.
        card = card.child(
            block()
                .id("context-usage-limits-unknown")
                .debug_selector(|| "context-usage-limits-unknown".into())
                .whitespace_normal()
                .text_color(rgb(INK_SUBTLE))
                .child("Limits not reported by this provider"),
        );
    }
    if let Some(limit) = limits.five_hour {
        card = card.child(window(
            "5-hour limit",
            "five-hour",
            limit.used_fraction,
            reset_value("five-hour", limit.resets_at, Duration::from_secs(5 * 3_600)),
        ));
    }
    if let Some(limit) = limits.weekly {
        card = card.child(window(
            "Weekly limit",
            "weekly",
            limit.used_fraction,
            reset_value("weekly", limit.resets_at, Duration::from_secs(7 * 86_400)),
        ));
    }
    // Everything below the windows is a terminal readout: a quiet key at
    // the left, the reported value right-aligned in tabular digits, and a
    // section head where the scope changes.
    let row = |key: String, value: String| {
        div()
            .flex()
            .flex_shrink_0()
            .justify_between()
            .gap(px(theme::SPACE_3))
            .px(px(theme::PANEL_ROW_ICON_X))
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
        if let Some(cost) = last_cost {
            section = section.child(
                row("Last turn".into(), cost_label(cost))
                    .debug_selector(move || format!("usage-cost-{cost}")),
            );
        }
        card = card.child(section);
    } else if let Some(cost) = last_cost {
        card = card.child(
            row("Last turn".into(), cost_label(cost))
                .debug_selector(move || format!("usage-cost-{cost}")),
        );
    }
    // Counts, percentages and the cost tick while the card is open. Every
    // block holds its height (`flex_shrink_0`): once the open legend
    // outgrows the room the card scrolls rather than squeezing its rows.
    components::tabular(card)
        .max_h(px(max_h))
        .overflow_y_scrollbar()
}

/// A usage reading's ink: neutral `INK` until the window runs tight
/// (`USAGE_TIGHT`), then `ATTENTION`. There is no `BLOCKED` step: a full
/// window stops nothing until the provider says so. Colour is state; a
/// context half full is not one.
pub fn usage_ink(fraction: f32) -> u32 {
    if fraction >= theme::USAGE_TIGHT {
        ATTENTION
    } else {
        INK
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
