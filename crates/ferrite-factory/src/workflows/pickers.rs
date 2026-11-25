//! The Workflows editor's own pickers: a Step's Provider, model and
//! Effort, the templates a new Workflow starts from, and how a model is
//! named. The rows that read the window's state are the window's.

use ferrite_core::factory_board::{Step, Template};
use ferrite_core::provider::Provider;

use crate::picker::Pick;

/// A Provider's model choices as a picker offers them: its default first,
/// named as the catalog names it (`Default · Opus 5.5`), then the rest.
/// The value of the default is empty: no model is pinned.
pub fn model_choices(catalog: &[ferrite_core::ModelInfo]) -> Vec<(String, String)> {
    let default = catalog
        .iter()
        .find(|model| model.value == DEFAULT_MODEL)
        .map_or_else(|| "Default".to_string(), |model| model.display.clone());
    std::iter::once((default, String::new()))
        .chain(
            catalog
                .iter()
                .filter(|model| model.value != DEFAULT_MODEL)
                .map(|model| (model.display.clone(), model.value.clone())),
        )
        .collect()
}

/// A pinned model as its picker row names it (`Haiku 5.5`), else as pinned.
pub fn model_name(catalog: &[ferrite_core::ModelInfo], value: &str) -> String {
    catalog
        .iter()
        .find(|model| model.value == value)
        .map_or_else(|| value.to_string(), |model| model.display.clone())
}

/// The catalog's own row for the Provider's default model.
const DEFAULT_MODEL: &str = "default";

/// Whether `chosen` is the model a picker's `value` row stands for.
pub fn model_chosen(chosen: Option<&str>, value: &str) -> bool {
    match chosen {
        None | Some(DEFAULT_MODEL) => value.is_empty(),
        Some(chosen) => chosen == value,
    }
}

/// The templates a new Workflow starts from.
pub fn template_picks() -> Vec<(ferrite_ui::menu::Item, Pick)> {
    Template::ALL
        .into_iter()
        .map(|template| {
            (
                ferrite_ui::menu::Item::new(template.label()),
                Pick::Create(template),
            )
        })
        .collect()
}

/// A Step's Provider choices.
pub fn provider_picks(step: &Step) -> Vec<(ferrite_ui::menu::Item, Pick)> {
    Provider::ALL
        .into_iter()
        .map(|provider| {
            (
                ferrite_ui::menu::Item::new(provider.to_string())
                    .checked(step.agent.provider == provider),
                Pick::Provider(step.id.clone(), provider),
            )
        })
        .collect()
}

/// A Step's model choices, from its Provider's `models`.
pub fn model_picks(
    step: &Step,
    models: &[ferrite_core::ModelInfo],
) -> Vec<(ferrite_ui::menu::Item, Pick)> {
    model_choices(models)
        .into_iter()
        .map(|(label, value)| {
            let chosen = model_chosen(step.agent.model.as_deref(), &value);
            (
                ferrite_ui::menu::Item::new(label).checked(chosen),
                Pick::Model(step.id.clone(), value),
            )
        })
        .collect()
}

/// A Step's Effort choices: its model's, else any of its Provider's
/// `models`.
pub fn effort_picks(
    step: &Step,
    models: &[ferrite_core::ModelInfo],
) -> Vec<(ferrite_ui::menu::Item, Pick)> {
    let mut efforts: Vec<String> = match &step.agent.model {
        Some(value) => models
            .iter()
            .find(|model| &model.value == value)
            .map(|model| model.efforts.clone())
            .unwrap_or_default(),
        None => Vec::new(),
    };
    if efforts.is_empty() {
        for model in models {
            for effort in &model.efforts {
                if !efforts.contains(effort) {
                    efforts.push(effort.clone());
                }
            }
        }
    }
    let mut picks = vec![(
        ferrite_ui::menu::Item::new("Default").checked(step.agent.effort.is_none()),
        Pick::Effort(step.id.clone(), String::new()),
    )];
    for effort in efforts {
        let chosen = step.agent.effort.as_deref() == Some(effort.as_str());
        picks.push((
            ferrite_ui::menu::Item::new(effort.clone()).checked(chosen),
            Pick::Effort(step.id.clone(), effort),
        ));
    }
    picks
}
