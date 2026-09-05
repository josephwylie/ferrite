//! What a model is called, which ones a provider offers when its own CLI
//! has not said, and which efforts each takes.
//!
//! Claude's CLI announces its menu at the handshake — values, resolved
//! ids, one-line descriptions, effort levels — and Codex answers a
//! `model/list` both at app startup (without a Thread) and when a Session
//! starts; either announcement always wins. Until discovery or a Session
//! announces a menu, pickers read the fallback catalog here. The
//! display grooming turns any raw id a Session's Init names
//! (`claude-fable-5-1`, `gpt-5.4-mini`) into the name a person would say
//! (`Fable 5.1`, `GPT-5.4 Mini`), so no API spelling ever reaches a chip.

use crate::store::Provider;
use crate::ModelInfo;

/// The effort ladder every Claude model but Haiku takes, as the CLI
/// announces it.
const CLAUDE_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];
/// The Codex ladders, as `model/list` announced them (0.144.4).
const CODEX_EFFORTS_ULTRA: &[&str] = &["low", "medium", "high", "xhigh", "max", "ultra"];
const CODEX_EFFORTS_MAX: &[&str] = &["low", "medium", "high", "xhigh", "max"];
const CODEX_EFFORTS_XHIGH: &[&str] = &["low", "medium", "high", "xhigh"];

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

/// The models a provider offers when its adapter has not announced a
/// list. Values are the spellings the CLI accepts today; the display is
/// what the picker shows; the efforts are what each took when last
/// probed. The first row is the provider's own default.
pub fn fallback(provider: Provider) -> Vec<ModelInfo> {
    let rows: &[FallbackRow] = match provider {
        Provider::Claude => &[
            (
                "default",
                None,
                "Default",
                "The CLI's own default model",
                CLAUDE_EFFORTS,
                None,
            ),
            (
                "fable",
                Some("claude-fable-5-1"),
                "Fable 5.1",
                "Most capable, for the hardest and longest tasks",
                CLAUDE_EFFORTS,
                None,
            ),
            (
                "opus[1m]",
                Some("claude-opus-5[1m]"),
                "Opus 5 (1M)",
                "Opus 5 with 1M context, best for everyday, complex tasks",
                CLAUDE_EFFORTS,
                None,
            ),
            (
                "opus",
                Some("claude-opus-5"),
                "Opus 5",
                "Best for everyday, complex tasks",
                CLAUDE_EFFORTS,
                None,
            ),
            (
                "sonnet",
                Some("claude-sonnet-5"),
                "Sonnet 5",
                "Efficient for routine tasks",
                CLAUDE_EFFORTS,
                None,
            ),
            (
                "haiku",
                Some("claude-haiku-4-5-20251001"),
                "Haiku 4.5",
                "Fastest, for quick answers",
                &[],
                None,
            ),
        ],
        Provider::Codex => &[
            (
                "gpt-5.6-sol",
                None,
                "GPT-5.6 Sol",
                "Reliable agentic workhorse for everyday tasks",
                CODEX_EFFORTS_ULTRA,
                Some("low"),
            ),
            (
                "gpt-5.6-terra",
                None,
                "GPT-5.6 Terra",
                "Balanced agentic coding model for everyday work",
                CODEX_EFFORTS_ULTRA,
                Some("medium"),
            ),
            (
                "gpt-5.6-luna",
                None,
                "GPT-5.6 Luna",
                "Fast and affordable agentic coding model",
                CODEX_EFFORTS_MAX,
                Some("medium"),
            ),
            (
                "gpt-5.5",
                None,
                "GPT-5.5",
                "Proven previous-generation model for coding and general work",
                CODEX_EFFORTS_XHIGH,
                Some("medium"),
            ),
            (
                "gpt-5.4",
                None,
                "GPT-5.4",
                "Strong model for everyday coding",
                CODEX_EFFORTS_XHIGH,
                Some("medium"),
            ),
            (
                "gpt-5.4-mini",
                None,
                "GPT-5.4 Mini",
                "Small, fast, and cost-efficient for simpler coding tasks",
                CODEX_EFFORTS_XHIGH,
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

/// The rows a picker shows for `provider`: what its adapter announced
/// when any did, else the fallback catalog.
pub fn catalog(provider: Provider, announced: &[ModelInfo]) -> Vec<ModelInfo> {
    if announced.is_empty() {
        fallback(provider)
    } else {
        announced.to_vec()
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

/// The name to show for `model` — a chosen value or an Init's full id:
/// the announced row's own display when one matches, else the fallback
/// catalog's for a value it lists (`fable` → `Fable 5.1`), else groomed
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

/// A person's name for a raw model id or alias: `claude-fable-5-1` →
/// `Fable 5.1`, `claude-haiku-4-5-20251001` → `Haiku 4.5`,
/// `claude-opus-5[1m]` → `Opus 5 (1M)`, `gpt-5.6-sol` → `GPT-5.6 Sol`,
/// `gpt-5.4-mini` → `GPT-5.4 Mini`, `sonnet` → `Sonnet`, `default` →
/// `Default`. Unknown shapes come back capitalized, never blank.
pub fn display_name(model: &str) -> String {
    let model = model.trim();
    if model.is_empty() {
        return String::new();
    }
    // A context-window tag rides the end of a Claude alias or id.
    let (id, context) = match model.strip_suffix("[1m]") {
        Some(id) => (id, " (1M)"),
        None => (model, ""),
    };
    let id = id
        .strip_prefix("claude-")
        .or_else(|| id.strip_prefix("codex-"))
        .unwrap_or(id);
    let mut tokens = id.split('-').filter(|token| !token.is_empty());
    let Some(family) = tokens.next() else {
        return model.to_string();
    };
    let family = match family {
        "gpt" => "GPT".to_string(),
        other => capitalize(other),
    };
    // Version digits gather into one dotted number; an eight-digit date
    // stamp is dropped; anything else is a suffix word.
    let mut version: Vec<&str> = Vec::new();
    let mut words: Vec<String> = Vec::new();
    for token in tokens {
        let numeric = token.chars().all(|c| c.is_ascii_digit() || c == '.');
        if numeric && token.len() >= 8 {
            continue;
        }
        if numeric && words.is_empty() {
            version.push(token);
        } else {
            words.push(capitalize(token));
        }
    }
    let mut name = family.clone();
    if !version.is_empty() {
        let joined = version.join(".");
        if family == "GPT" {
            name.push('-');
        } else {
            name.push(' ');
        }
        name.push_str(&joined);
    }
    for word in words {
        name.push(' ');
        name.push_str(&word);
    }
    name.push_str(context);
    name
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

    #[test]
    fn raw_ids_become_names_a_person_would_say() {
        for (id, name) in [
            ("claude-fable-5-1", "Fable 5.1"),
            ("claude-fable-5", "Fable 5"),
            ("claude-opus-5", "Opus 5"),
            ("claude-opus-5[1m]", "Opus 5 (1M)"),
            ("claude-sonnet-5", "Sonnet 5"),
            ("claude-sonnet-4-5", "Sonnet 4.5"),
            ("claude-haiku-4-5-20251001", "Haiku 4.5"),
            ("opus[1m]", "Opus (1M)"),
            ("sonnet", "Sonnet"),
            ("default", "Default"),
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
            ("default", "Default"),
            ("fable", "Fable 5.1"),
            ("opus[1m]", "Opus 5 (1M)"),
            ("opus", "Opus 5"),
            ("sonnet", "Sonnet 5"),
            ("haiku", "Haiku 4.5"),
            ("gpt-5.6-sol", "GPT-5.6 Sol"),
            ("gpt-5.6-terra", "GPT-5.6 Terra"),
            ("gpt-5.6-luna", "GPT-5.6 Luna"),
            ("gpt-5.5", "GPT-5.5"),
            ("gpt-5.4", "GPT-5.4"),
            ("gpt-5.4-mini", "GPT-5.4 Mini"),
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
            ("claude-fable-5-1", "Fable 5.1"),
            ("claude-opus-5[1m]", "Opus 5 (1M)"),
            ("claude-sonnet-5", "Sonnet 5"),
            ("claude-haiku-4-5-20251001", "Haiku 4.5"),
        ] {
            assert_eq!(label(id, &[]), display, "{id}");
        }
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

    /// The fallback ladders are the probed ones: every Claude model but
    /// Haiku takes five levels; Codex's Sol and Terra add `ultra`, Luna
    /// stops at `max`, the 5.5 and 5.4 line at `xhigh`.
    #[test]
    fn the_fallback_carries_each_models_effort_ladder() {
        let claude = fallback(Provider::Claude);
        for row in &claude {
            if row.value == "haiku" {
                assert!(row.efforts.is_empty(), "haiku takes no effort");
            } else {
                assert_eq!(row.efforts, CLAUDE_EFFORTS, "{}", row.value);
            }
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
        assert_eq!(ladder("gpt-5.6-sol"), CODEX_EFFORTS_ULTRA);
        assert_eq!(ladder("gpt-5.6-terra"), CODEX_EFFORTS_ULTRA);
        assert_eq!(ladder("gpt-5.6-luna"), CODEX_EFFORTS_MAX);
        assert_eq!(ladder("gpt-5.5"), CODEX_EFFORTS_XHIGH);
        assert_eq!(ladder("gpt-5.4"), CODEX_EFFORTS_XHIGH);
        assert_eq!(ladder("gpt-5.4-mini"), CODEX_EFFORTS_XHIGH);
        assert_eq!(codex[0].default_effort.as_deref(), Some("low"));
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
        assert!(efforts_for(Provider::Claude, Some("haiku"), &[]).is_empty());
        assert!(efforts_for(Provider::Claude, Some("claude-haiku-4-5-20251001"), &[]).is_empty());
        assert_eq!(
            efforts_for(Provider::Codex, Some("gpt-5.4"), &[]),
            CODEX_EFFORTS_XHIGH
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
