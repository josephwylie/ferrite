//! What a model is called, and which ones a provider offers when its own
//! CLI has not said.
//!
//! Claude's CLI announces its menu at the handshake — values, display
//! names, one-line descriptions — and that announcement always wins. Codex
//! announces nothing, and a draft Pane has no Session to ask; both read the
//! fallback catalog here. The display grooming turns any raw id a Session's
//! Init names (`claude-fable-5-1`, `gpt-5.4-mini`) into the name a person
//! would say (`Fable 5.1`, `GPT-5.4 Mini`), so no API spelling ever reaches
//! a chip.

use crate::store::Provider;
use crate::ModelInfo;

/// The models a provider offers when no Session of it has announced a
/// list. Values are the spellings the CLI accepts today; the display is
/// what the picker shows.
pub fn fallback(provider: Provider) -> Vec<ModelInfo> {
    let rows: &[(&str, &str, &str)] = match provider {
        Provider::Claude => &[
            ("default", "Default", "The CLI's own default model"),
            ("fable", "Fable", "Most capable, for the hardest and longest tasks"),
            ("opus", "Opus", "Best for everyday, complex tasks"),
            ("sonnet", "Sonnet", "Efficient for routine tasks"),
            ("haiku", "Haiku", "Fastest, for quick answers"),
        ],
        Provider::Codex => &[
            ("gpt-5.6-sol", "GPT-5.6 Sol", "Most capable, deepest reasoning"),
            ("gpt-5.6", "GPT-5.6", "Frontier coding model"),
            ("gpt-5.5", "GPT-5.5", "Strong and fast for everyday work"),
            ("gpt-5.4", "GPT-5.4", "Previous generation"),
            ("gpt-5.4-mini", "GPT-5.4 Mini", "Small and quick"),
        ],
    };
    rows.iter()
        .map(|(value, display, detail)| ModelInfo {
            value: (*value).into(),
            display: (*display).into(),
            detail: (*detail).into(),
            resolved: None,
        })
        .collect()
}

/// The rows a picker shows for `provider`: what its Sessions announced
/// when any did, else the fallback catalog.
pub fn catalog(provider: Provider, announced: &[ModelInfo]) -> Vec<ModelInfo> {
    if announced.is_empty() {
        fallback(provider)
    } else {
        announced.to_vec()
    }
}

/// The name to show for `model` — a chosen value or an Init's full id:
/// the announced row's own display when one matches, else groomed from
/// the id.
pub fn label(model: &str, announced: &[ModelInfo]) -> String {
    announced
        .iter()
        .find(|row| row.is(model))
        .map(|row| row.display.clone())
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
            ("claude-sonnet-4-5", "Sonnet 4.5"),
            ("claude-haiku-4-5-20251001", "Haiku 4.5"),
            ("opus[1m]", "Opus (1M)"),
            ("sonnet", "Sonnet"),
            ("default", "Default"),
            ("gpt-5.6-sol", "GPT-5.6 Sol"),
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
    /// id finds its row through `resolved`.
    #[test]
    fn a_label_prefers_what_the_provider_announced() {
        let announced = vec![ModelInfo {
            value: "opus[1m]".into(),
            display: "Opus (1M context)".into(),
            detail: "Opus 5 with 1M context".into(),
            resolved: Some("claude-opus-5[1m]".into()),
        }];
        assert_eq!(label("opus[1m]", &announced), "Opus (1M context)");
        assert_eq!(label("claude-opus-5[1m]", &announced), "Opus (1M context)");
        assert_eq!(label("claude-fable-5-1", &announced), "Fable 5.1");
        assert_eq!(label("claude-fable-5-1", &[]), "Fable 5.1");
    }

    #[test]
    fn every_provider_has_a_fallback_and_an_announcement_replaces_it() {
        for provider in [Provider::Claude, Provider::Codex] {
            let rows = fallback(provider);
            assert!(rows.len() >= 3, "{provider:?}");
            assert!(rows.iter().all(|row| !row.value.is_empty() && !row.display.is_empty()));
            assert_eq!(catalog(provider, &[]), rows);
        }
        let announced = vec![ModelInfo::bare("sonnet")];
        assert_eq!(catalog(Provider::Claude, &announced), announced);
        assert_eq!(ModelInfo::bare("claude-fable-5-1").display, "Fable 5.1");
    }
}
