//! What a model is called, which ones a provider offers when its own CLI
//! has not said, and which efforts each takes.
//!
//! Claude's CLI announces its menu at the handshake — values, resolved
//! ids, one-line descriptions, effort levels — and Codex answers a
//! `model/list` both at app startup (without a Thread) and when a Session
//! starts; either announcement always wins. The last usable menu is saved
//! per provider across launches. Until a fresh announcement, pickers use
//! that saved menu, or the bundled fallback here if none was saved. The
//! display grooming turns any raw id a Session's Init names
//! (`claude-fable-5-1`, `gpt-5.4-mini`) into the name a person would say
//! (`Fable 5.1`, `GPT-5.4 Mini`), so no API spelling ever reaches a chip.

use crate::store::Provider;
use crate::ModelInfo;

mod cache;
pub(crate) use cache::ModelCache;

/// The effort ladder every Claude model takes, as the CLI announces it.
const CLAUDE_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];
/// The Codex ladders, as `model/list` announced them (0.156.0).
const CODEX_EFFORTS_ULTRA: &[&str] = &["low", "medium", "high", "xhigh", "max", "ultra"];
const CODEX_EFFORTS_MAX: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// One fallback row: value, resolved id (the full id an Init names, where
/// an alias has one), display, detail, effort ladder, default effort.
type FallbackRow = (
    &'static str,
    Option<&'static str>,
    &'static str,
    &'static str,
    &'static [&'static str],
    Option<&'static str>,
);

/// The models a provider offers when its adapter has no live or saved
/// list. Values are the spellings the CLI accepts today; the display is
/// what the picker shows; the detail is the picker's one lowercase line
/// (the prototype's `#picker`, untruncated at 66 cells); the efforts are
/// what each took when last probed. The first row is the provider's own
/// default.
pub fn fallback(provider: Provider) -> Vec<ModelInfo> {
    let rows: &[FallbackRow] = match provider {
        Provider::Claude => &[
            (
                "default",
                None,
                "default",
                "the CLI's own default",
                CLAUDE_EFFORTS,
                None,
            ),
            (
                "fable",
                Some("claude-fable-5-2"),
                "Fable 5.2",
                "most capable, for the hardest work",
                CLAUDE_EFFORTS,
                None,
            ),
            (
                "opus[1m]",
                Some("claude-opus-5-5[1m]"),
                "Opus 5.5 (1M)",
                "1M context, for long sessions",
                CLAUDE_EFFORTS,
                None,
            ),
            (
                "sonnet",
                Some("claude-sonnet-5-5"),
                "Sonnet 5.5",
                "efficient for routine tasks",
                CLAUDE_EFFORTS,
                None,
            ),
            (
                "haiku",
                Some("claude-haiku-5-5"),
                "Haiku 5.5",
                "fastest, for quick answers",
                CLAUDE_EFFORTS,
                None,
            ),
        ],
        Provider::Codex => &[
            (
                "gpt-6-astra",
                None,
                "GPT-6 Astra",
                "frontier, for the hardest tasks",
                CODEX_EFFORTS_ULTRA,
                Some("medium"),
            ),
            (
                "gpt-6.1-sol",
                None,
                "GPT-6.1 Sol",
                "workhorse for coding",
                CODEX_EFFORTS_ULTRA,
                Some("low"),
            ),
            (
                "gpt-6-luna",
                None,
                "GPT-6 Luna",
                "fast and affordable",
                CODEX_EFFORTS_MAX,
                Some("medium"),
            ),
        ],
    };
    rows.iter()
        .map(
            |(value, resolved, display, detail, efforts, default_effort)| ModelInfo {
                value: (*value).into(),
                display: (*display).into(),
                detail: (*detail).into(),
                resolved: resolved.map(str::to_string),
                efforts: efforts.iter().map(|effort| (*effort).to_string()).collect(),
                default_effort: default_effort.map(str::to_string),
            },
        )
        .collect()
}

/// The models `provider` offers: what its adapter announced when any
/// did, else the fallback catalog. Every row, superseded ones included,
/// so a Thread pinned to an older model still finds its name and ladder.
pub fn catalog(provider: Provider, announced: &[ModelInfo]) -> Vec<ModelInfo> {
    if announced.is_empty() {
        fallback(provider)
    } else {
        announced.to_vec()
    }
}

/// The rows a model picker shows: `rows` without any a newer version of
/// the same line replaces (`gpt-6-sol` once `gpt-6.1-sol` is listed,
/// `claude-fable-5-1` beside `fable` → `claude-fable-5-2`). A row is its
/// resolved id's line — family, suffix words and context tag — so `Sol`
/// never replaces `Luna` and `(1M)` never replaces the plain window. The
/// provider's default (the first row) and the `chosen` model always stay.
pub fn menu(rows: Vec<ModelInfo>, chosen: Option<&str>) -> Vec<ModelInfo> {
    let lineages: Vec<_> = rows
        .iter()
        .map(|row| Lineage::of(row.resolved.as_deref().unwrap_or(&row.value)))
        .collect();
    let superseded = |at: usize| {
        let Some(own) = &lineages[at] else {
            return false;
        };
        lineages
            .iter()
            .flatten()
            .any(|other| other.line == own.line && other.version > own.version)
    };
    rows.into_iter()
        .enumerate()
        .filter(|(at, row)| *at == 0 || chosen.is_some_and(|c| row.is(c)) || !superseded(*at))
        .map(|(_, row)| row)
        .collect()
}

/// A model id split into the line it belongs to and its version on that
/// line: `gpt-6.1-sol` is line (`gpt`, `sol`) at 6.1; `claude-opus-5-5[1m]`
/// is line (`opus`, `[1m]`) at 5.5. `None` for an id with no version
/// (`opus`, `default`), which no other row can replace.
struct Lineage {
    line: (String, Vec<String>, &'static str),
    version: Vec<u32>,
}

impl Lineage {
    fn of(id: &str) -> Option<Self> {
        let parts = Parts::of(id)?;
        let version = parts
            .version
            .iter()
            .flat_map(|token| token.split('.'))
            .map(|number| number.parse().ok())
            .collect::<Option<Vec<u32>>>()?;
        if version.is_empty() {
            return None;
        }
        Some(Self {
            line: (
                parts.family.to_ascii_lowercase(),
                parts
                    .words
                    .iter()
                    .map(|word| word.to_ascii_lowercase())
                    .collect(),
                parts.context,
            ),
            version,
        })
    }
}

/// The effort levels the effort picker offers for a Thread on `provider`
/// whose chosen model is `model` (`None` is the provider's default): the
/// chosen row's own ladder — the default row's for `None` — else, for a
/// model no row knows, everything any row of the provider takes, so an
/// unknown id still gets a menu. Empty means the model takes none.
pub fn efforts_for(
    provider: Provider,
    model: Option<&str>,
    announced: &[ModelInfo],
) -> Vec<String> {
    let rows = catalog(provider, announced);
    let row = match model {
        Some(model) => rows.iter().find(|row| row.is(model)),
        None => rows.first(),
    };
    if let Some(row) = row {
        return row.efforts.clone();
    }
    let mut union: Vec<String> = Vec::new();
    for effort in rows.iter().flat_map(|row| row.efforts.iter()) {
        if !union.contains(effort) {
            union.push(effort.clone());
        }
    }
    union
}

/// The effort the provider's own CLI runs `model` at when nobody chose one
/// (`None` is the provider's default model): what the model picker's
/// effort ladder shows inverse while no effort resolves. Not the catalog's
/// `default_effort`, which the status line reads (rule 2.6.5): a Claude
/// Thread with no choice says `opus 5.5`, and its picker still says which
/// level the CLI is using. `None` for a model that takes no effort.
pub fn cli_default_effort(provider: Provider, model: Option<&str>) -> Option<&'static str> {
    let ladder = efforts_for(provider, model, &[]);
    if ladder.is_empty() {
        return None;
    }
    let level = match provider {
        // Claude Code runs its models at `high` unless told otherwise.
        Provider::Claude => "high",
        // The Codex CLI's `model_reasoning_effort` default.
        Provider::Codex => "medium",
    };
    ladder.iter().any(|step| step == level).then_some(level)
}

/// A model's one-line description in the picker's voice: lowercase-first
/// (`Most capable for…` → `most capable for…`), an acronym or a figure
/// left as written (`GPT-6…`, `1M context…`).
pub fn groom_detail(detail: &str) -> String {
    let detail = detail.trim();
    let mut chars = detail.chars();
    let (Some(first), second) = (chars.next(), chars.next()) else {
        return detail.to_string();
    };
    if first.is_uppercase() && second.is_none_or(|second| second.is_lowercase()) {
        first.to_lowercase().chain(detail.chars().skip(1)).collect()
    } else {
        detail.to_string()
    }
}

/// The name to show for `model` — a chosen value or an Init's full id:
/// the announced row's own display when one matches, else the fallback
/// catalog's for a value it lists (`fable` → `Fable 5.2`), else groomed
/// from the id.
pub fn label(model: &str, announced: &[ModelInfo]) -> String {
    announced
        .iter()
        .find(|row| row.is(model))
        .map(|row| row.display.clone())
        .or_else(|| {
            [Provider::Claude, Provider::Codex]
                .into_iter()
                .flat_map(fallback)
                .find(|row| row.is(model))
                .map(|row| row.display)
        })
        .unwrap_or_else(|| display_name(model))
}

/// The model as the Composer's footer, and the model menu, name it
/// (Claude's `Opus 5.5`): `model`, else the provider's default row, the
/// model it resolves to where the provider says (`Default · Opus 5 (1M)`
/// → `Opus 5`), never the provider's own name and never the bare word
/// `Default`: a default nobody has resolved yet names the model the CLI
/// runs then (`cli_default_model`). The context tag (`(1M)`) is neither's.
pub fn footer_name(provider: Provider, model: Option<&str>, announced: &[ModelInfo]) -> String {
    let name = match model {
        Some(model) => label(model, announced),
        None => catalog(provider, announced)
            .first()
            .map(|row| row.display.clone())
            .unwrap_or_default(),
    };
    let name = name.rsplit(" \u{b7} ").next().unwrap_or(&name);
    let name = name.split(" (").next().unwrap_or(name).trim();
    if name.eq_ignore_ascii_case("default") {
        return footer_name(provider, Some(cli_default_model(provider)), &[]);
    }
    capitalize(name)
}

/// The model a provider's CLI runs when nobody chose one and it has not
/// said which: Claude Code's Opus with its 1M context, Codex's first row.
/// Never `default` itself, so `footer_name` resolves in one step.
pub fn cli_default_model(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "opus[1m]",
        Provider::Codex => "gpt-6-astra",
    }
}

/// A person's name for a raw model id or alias: `claude-fable-5-1` →
/// `Fable 5.1`, `claude-haiku-4-5-20251001` → `Haiku 4.5`,
/// `claude-opus-5[1m]` → `Opus 5 (1M)`, `gpt-5.6-sol` → `GPT-5.6 Sol`,
/// `gpt-5.4-mini` → `GPT-5.4 Mini`, `sonnet` → `Sonnet`, `default` →
/// `Default`. Unknown shapes come back capitalized, never blank.
pub fn display_name(model: &str) -> String {
    let model = model.trim();
    let Some(parts) = Parts::of(model) else {
        return model.to_string();
    };
    let family = match parts.family {
        "gpt" => "GPT".to_string(),
        other => capitalize(other),
    };
    let mut name = family.clone();
    if !parts.version.is_empty() {
        name.push(if family == "GPT" { '-' } else { ' ' });
        name.push_str(&parts.version.join("."));
    }
    for word in parts.words {
        name.push(' ');
        name.push_str(&capitalize(word));
    }
    if !parts.context.is_empty() {
        name.push_str(" (1M)");
    }
    name
}

/// A raw model id or alias in pieces: `claude-opus-5-5-20251101[1m]` is
/// family `opus`, version `5`, `5`, no words, context `[1m]`;
/// `gpt-5.3-codex-spark` is family `gpt`, version `5.3`, words `codex`,
/// `spark`. Version digits come before any word; an eight-digit date
/// stamp is dropped. `None` for a blank id.
struct Parts<'a> {
    family: &'a str,
    version: Vec<&'a str>,
    words: Vec<&'a str>,
    /// The context-window tag a Claude alias or id ends with, else empty.
    context: &'static str,
}

impl<'a> Parts<'a> {
    fn of(model: &'a str) -> Option<Self> {
        let model = model.trim();
        let (id, context) = match model.strip_suffix("[1m]") {
            Some(id) => (id, "[1m]"),
            None => (model, ""),
        };
        let id = id
            .strip_prefix("claude-")
            .or_else(|| id.strip_prefix("codex-"))
            .unwrap_or(id);
        let mut tokens = id.split('-').filter(|token| !token.is_empty());
        let family = tokens.next()?;
        let mut version = Vec::new();
        let mut words = Vec::new();
        for token in tokens {
            let numeric = token.chars().all(|c| c.is_ascii_digit() || c == '.');
            if numeric && token.len() >= 8 {
                continue;
            }
            if numeric && words.is_empty() {
                version.push(token);
            } else {
                words.push(token);
            }
        }
        Some(Self {
            family,
            version,
            words,
            context,
        })
    }
}

/// A provider's one-line description with the model's own name struck
/// off its front: the Claude CLI writes `Fable 5.1 · Most capable…` and
/// `Opus 5 with 1M context · Best for…`, and a row that already shows
/// `Fable 5.1` beside it would say the name twice. The leading segment
/// goes when it is the name, or starts with the name's family-and-version
/// (`Opus 5` for `Opus 5 (1M)`).
pub fn detail_without_name(description: &str, name: &str) -> String {
    let Some((lead, rest)) = description.split_once(" · ") else {
        return description.trim().to_string();
    };
    let bare = name.split(" (").next().unwrap_or(name).trim();
    let duplicated =
        !bare.is_empty() && (lead.trim() == name.trim() || lead.trim().starts_with(bare));
    if duplicated {
        rest.trim().to_string()
    } else {
        description.trim().to_string()
    }
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The footer says the model, never the provider, and leaves the
    /// context tag to the menu.
    #[test]
    fn the_footer_names_the_model_without_its_context_tag() {
        let announced = [ModelInfo {
            display: "Default \u{b7} Opus 5 (1M)".into(),
            ..ModelInfo::bare("default")
        }];
        assert_eq!(
            footer_name(Provider::Claude, Some("claude-opus-5-5[1m]"), &[]),
            "Opus 5.5"
        );
        assert_eq!(
            footer_name(Provider::Claude, Some("opus[1m]"), &[]),
            "Opus 5.5"
        );
        assert_eq!(footer_name(Provider::Claude, None, &announced), "Opus 5");
        assert_eq!(
            footer_name(Provider::Claude, None, &[]),
            "Opus 5.5",
            "an unresolved default names the model the CLI runs, never `Default`"
        );
        assert_eq!(
            footer_name(Provider::Claude, Some("default"), &[]),
            "Opus 5.5"
        );
        assert_eq!(footer_name(Provider::Codex, None, &[]), "GPT-6 Astra");
    }

    /// The menus the CLIs announced on 2026-11-10, trimmed to what
    /// nothing newer replaces; a pinned older model keeps its row.
    #[test]
    fn a_menu_drops_models_a_newer_version_replaces() {
        let row = |value: &str, resolved: Option<&str>| ModelInfo {
            resolved: resolved.map(str::to_string),
            ..ModelInfo::bare(value)
        };
        let values = |rows: Vec<ModelInfo>| -> Vec<String> {
            rows.into_iter().map(|row| row.value).collect()
        };
        let codex: Vec<ModelInfo> = [
            "gpt-6.1-sol",
            "gpt-6-astra",
            "gpt-6.1-terra",
            "gpt-6-sol",
            "gpt-6-luna",
            "gpt-5.6-sol",
            "gpt-5.6-terra",
            "gpt-5.6-luna",
            "gpt-5.3-codex-spark",
        ]
        .into_iter()
        .map(ModelInfo::bare)
        .collect();
        assert_eq!(
            values(menu(codex.clone(), None)),
            [
                "gpt-6.1-sol",
                "gpt-6-astra",
                "gpt-6.1-terra",
                "gpt-6-luna",
                "gpt-5.3-codex-spark"
            ]
        );
        assert!(values(menu(codex, Some("gpt-5.6-luna"))).contains(&"gpt-5.6-luna".to_string()));

        let claude = vec![
            row("default", Some("claude-opus-5-5")),
            row("opus", Some("claude-opus-5-5")),
            row("opus[1m]", Some("claude-opus-5-5[1m]")),
            row("fable", Some("claude-fable-5-2")),
            row("haiku", Some("claude-haiku-5-5")),
            row("claude-fable-5-1", None),
            row("claude-haiku-4-5-20251001", None),
            row("claude-opus-5", None),
            row("claude-opus-4-8", None),
            row("claude-sonnet-4-6", None),
        ];
        assert_eq!(
            values(menu(claude, None)),
            [
                "default",
                "opus",
                "opus[1m]",
                "fable",
                "haiku",
                "claude-sonnet-4-6"
            ]
        );
    }

    #[test]
    fn raw_ids_become_names_a_person_would_say() {
        for (id, name) in [
            ("claude-fable-5-1", "Fable 5.1"),
            ("claude-fable-5", "Fable 5"),
            ("claude-opus-5", "Opus 5"),
            ("claude-opus-5[1m]", "Opus 5 (1M)"),
            ("claude-opus-5-5", "Opus 5.5"),
            ("claude-opus-5-5[1m]", "Opus 5.5 (1M)"),
            ("claude-sonnet-5", "Sonnet 5"),
            ("claude-sonnet-4-5", "Sonnet 4.5"),
            ("claude-haiku-4-5-20251001", "Haiku 4.5"),
            ("opus[1m]", "Opus (1M)"),
            ("sonnet", "Sonnet"),
            ("default", "Default"),
            ("gpt-6-sol", "GPT-6 Sol"),
            ("gpt-6-luna", "GPT-6 Luna"),
            ("gpt-5.6-sol", "GPT-5.6 Sol"),
            ("gpt-5.6-terra", "GPT-5.6 Terra"),
            ("gpt-5.6", "GPT-5.6"),
            ("gpt-5.4-mini", "GPT-5.4 Mini"),
            ("gpt-5.3-codex", "GPT-5.3 Codex"),
            ("codex-gpt-5.4-mini", "GPT-5.4 Mini"),
            ("o3", "O3"),
            ("", ""),
        ] {
            assert_eq!(display_name(id), name, "{id}");
        }
    }

    /// The announced row's own display wins over grooming; an Init's full
    /// id finds its row through `resolved`; a fallback value is named the
    /// way the fallback names it.
    #[test]
    fn a_label_prefers_what_the_provider_announced() {
        let announced = vec![ModelInfo {
            value: "opus[1m]".into(),
            display: "Opus (1M context)".into(),
            detail: "Opus 5 with 1M context".into(),
            resolved: Some("claude-opus-5[1m]".into()),
            efforts: Vec::new(),
            default_effort: None,
        }];
        assert_eq!(label("opus[1m]", &announced), "Opus (1M context)");
        assert_eq!(label("claude-opus-5[1m]", &announced), "Opus (1M context)");
        assert_eq!(label("claude-fable-5-1", &announced), "Fable 5.1");
        assert_eq!(label("claude-fable-5-1", &[]), "Fable 5.1");
    }

    /// The names the fallback catalog carries are today's, and a raw Init
    /// id or a bare alias comes out the same way the catalog spells it.
    #[test]
    fn the_fallback_names_are_versioned_and_labels_agree_with_them() {
        for (value, display) in [
            ("default", "default"),
            ("fable", "Fable 5.2"),
            ("opus[1m]", "Opus 5.5 (1M)"),
            ("sonnet", "Sonnet 5.5"),
            ("haiku", "Haiku 5.5"),
            ("gpt-6-astra", "GPT-6 Astra"),
            ("gpt-6.1-sol", "GPT-6.1 Sol"),
            ("gpt-6-luna", "GPT-6 Luna"),
        ] {
            let provider = if value.starts_with("gpt") {
                Provider::Codex
            } else {
                Provider::Claude
            };
            let row = fallback(provider)
                .into_iter()
                .find(|row| row.value == value)
                .unwrap_or_else(|| panic!("{value} is in the fallback"));
            assert_eq!(row.display, display, "{value}");
            assert_eq!(label(value, &[]), display, "{value}");
        }
        for (id, display) in [
            ("claude-fable-5-2", "Fable 5.2"),
            ("claude-fable-5-1", "Fable 5.1"),
            ("claude-opus-5-5[1m]", "Opus 5.5 (1M)"),
            ("claude-opus-5-5", "Opus 5.5"),
            ("claude-sonnet-5", "Sonnet 5"),
            ("claude-sonnet-5-5", "Sonnet 5.5"),
            ("claude-haiku-4-5-20251001", "Haiku 4.5"),
        ] {
            assert_eq!(label(id, &[]), display, "{id}");
        }
    }

    /// The picker's fallback rows read exactly as the prototype's model
    /// picker: lowercase-first details, each untruncated at 66 cells beside
    /// its name.
    #[test]
    fn the_fallback_rows_read_as_the_prototype_picker() {
        let rows = |provider| -> Vec<(String, String)> {
            fallback(provider)
                .into_iter()
                .map(|row| (row.display, row.detail))
                .collect()
        };
        let pair = |display: &str, detail: &str| (display.to_string(), detail.to_string());
        assert_eq!(
            rows(Provider::Claude),
            vec![
                pair("default", "the CLI's own default"),
                pair("Fable 5.2", "most capable, for the hardest work"),
                pair("Opus 5.5 (1M)", "1M context, for long sessions"),
                pair("Sonnet 5.5", "efficient for routine tasks"),
                pair("Haiku 5.5", "fastest, for quick answers"),
            ]
        );
        assert_eq!(
            rows(Provider::Codex),
            vec![
                pair("GPT-6 Astra", "frontier, for the hardest tasks"),
                pair("GPT-6.1 Sol", "workhorse for coding"),
                pair("GPT-6 Luna", "fast and affordable"),
            ]
        );
        // The gutter, the name, two cells, the detail, the check: 66 cells.
        for (display, detail) in rows(Provider::Claude)
            .into_iter()
            .chain(rows(Provider::Codex))
        {
            let cells = 1 + 2 + display.chars().count() + 2 + detail.chars().count() + 2 + 1;
            assert!(cells <= 66, "{display} · {detail} needs {cells} cells");
        }
    }

    /// The CLI's own effort for a model with a ladder (what the picker
    /// inverts with nothing chosen), none for one without; announced
    /// descriptions read lowercase-first.
    #[test]
    fn the_cli_default_effort_and_groomed_details() {
        assert_eq!(cli_default_effort(Provider::Claude, None), Some("high"));
        assert_eq!(
            cli_default_effort(Provider::Claude, Some("opus[1m]")),
            Some("high")
        );
        assert_eq!(
            cli_default_effort(Provider::Claude, Some("haiku")),
            Some("high")
        );
        assert_eq!(
            cli_default_effort(Provider::Codex, Some("gpt-6-luna")),
            Some("medium")
        );
        for provider in [Provider::Claude, Provider::Codex] {
            for row in fallback(provider) {
                assert_eq!(
                    cli_default_effort(provider, Some(row.value.as_str())).is_some(),
                    !row.efforts.is_empty(),
                    "{}",
                    row.value
                );
            }
        }
        assert_eq!(
            groom_detail("Most capable for your hardest tasks"),
            "most capable for your hardest tasks"
        );
        assert_eq!(groom_detail("GPT-6 for coding"), "GPT-6 for coding");
        assert_eq!(groom_detail("1M context"), "1M context");
        assert_eq!(groom_detail(""), "");
    }

    #[test]
    fn every_provider_has_a_fallback_and_an_announcement_replaces_it() {
        for provider in [Provider::Claude, Provider::Codex] {
            let rows = fallback(provider);
            assert!(rows.len() >= 3, "{provider:?}");
            assert!(rows
                .iter()
                .all(|row| !row.value.is_empty() && !row.display.is_empty()));
            assert_eq!(catalog(provider, &[]), rows);
        }
        let announced = vec![ModelInfo::bare("sonnet")];
        assert_eq!(catalog(Provider::Claude, &announced), announced);
        assert_eq!(ModelInfo::bare("claude-fable-5-1").display, "Fable 5.1");
        assert!(ModelInfo::bare("sonnet").efforts.is_empty());
    }

    /// The fallback ladders are the probed ones: every Claude model takes
    /// five levels; Codex's Astra and Sol add `ultra`, Luna stops at `max`.
    #[test]
    fn the_fallback_carries_each_models_effort_ladder() {
        let claude = fallback(Provider::Claude);
        for row in &claude {
            assert_eq!(row.efforts, CLAUDE_EFFORTS, "{}", row.value);
            assert_eq!(row.default_effort, None);
        }
        let codex = fallback(Provider::Codex);
        let ladder = |value: &str| {
            codex
                .iter()
                .find(|row| row.value == value)
                .unwrap()
                .efforts
                .clone()
        };
        assert_eq!(ladder("gpt-6-astra"), CODEX_EFFORTS_ULTRA);
        assert_eq!(ladder("gpt-6.1-sol"), CODEX_EFFORTS_ULTRA);
        assert_eq!(ladder("gpt-6-luna"), CODEX_EFFORTS_MAX);
        assert_eq!(codex[0].default_effort.as_deref(), Some("medium"));
    }

    /// The effort menu follows the chosen model: its own ladder, the
    /// default row's for no choice, none for a model that takes none, and
    /// the union for an id no row knows.
    #[test]
    fn efforts_follow_the_chosen_model() {
        assert_eq!(
            efforts_for(Provider::Claude, None, &[]),
            CLAUDE_EFFORTS,
            "the default row's ladder"
        );
        assert_eq!(
            efforts_for(Provider::Claude, Some("sonnet"), &[]),
            CLAUDE_EFFORTS
        );
        assert_eq!(
            efforts_for(Provider::Claude, Some("haiku"), &[]),
            CLAUDE_EFFORTS
        );
        assert_eq!(
            efforts_for(Provider::Codex, Some("gpt-6-luna"), &[]),
            CODEX_EFFORTS_MAX
        );
        assert_eq!(
            efforts_for(Provider::Codex, Some("gpt-9-unheard-of"), &[]),
            CODEX_EFFORTS_ULTRA,
            "an unknown id gets everything any row takes"
        );
        // An announcement wins over the fallback, and a resolved id finds
        // its announced row.
        let announced = vec![
            ModelInfo {
                value: "sonnet".into(),
                display: "Sonnet 5".into(),
                detail: String::new(),
                resolved: Some("claude-sonnet-5".into()),
                efforts: vec!["low".into(), "high".into()],
                default_effort: None,
            },
            ModelInfo::bare("haiku"),
        ];
        assert_eq!(
            efforts_for(Provider::Claude, Some("claude-sonnet-5"), &announced),
            ["low", "high"]
        );
        assert_eq!(
            efforts_for(Provider::Claude, None, &announced),
            ["low", "high"],
            "the first announced row is the default"
        );
        assert!(efforts_for(Provider::Claude, Some("haiku"), &announced).is_empty());
    }

    /// A description that opens with the model's own name loses that
    /// segment; one that does not is kept whole.
    #[test]
    fn a_detail_drops_the_name_the_row_already_shows() {
        assert_eq!(
            detail_without_name(
                "Fable 5.1 · Most capable for your hardest tasks",
                "Fable 5.1"
            ),
            "Most capable for your hardest tasks"
        );
        assert_eq!(
            detail_without_name(
                "Opus 5 with 1M context · Best for everyday, complex tasks",
                "Opus 5 (1M)"
            ),
            "Best for everyday, complex tasks"
        );
        assert_eq!(
            detail_without_name("Haiku 4.5 · Fastest for quick answers", "Haiku 4.5"),
            "Fastest for quick answers"
        );
        assert_eq!(
            detail_without_name("Sonnet 5 · Efficient for routine tasks", "Opus 5"),
            "Sonnet 5 · Efficient for routine tasks"
        );
        assert_eq!(
            detail_without_name("Reliable agentic workhorse.", "GPT-5.6 Sol"),
            "Reliable agentic workhorse."
        );
        assert_eq!(detail_without_name("", "Fable 5.1"), "");
    }
}
