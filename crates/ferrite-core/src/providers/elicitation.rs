//! Normalization for the MCP standard's flat elicitation schema.
//!
//! Provider adapters retain their envelopes and reply encoding; this module
//! owns only the schema subset that both adapters expose to the shared form.

use serde_json::Value;

use crate::{FormChoice, FormField, FormFieldKind};

pub(super) fn fields(schema: &Value) -> Result<Vec<FormField>, String> {
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        return Err("form schema must be an object".into());
    }
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| "form schema has no properties".to_string())?;
    let required = schema.get("required").and_then(Value::as_array);
    properties
        .iter()
        .map(|(id, field)| {
            let kind = match field.get("type").and_then(Value::as_str) {
                Some("string") => match choices(field)? {
                    Some(options) => FormFieldKind::Enum {
                        options,
                        multi_select: false,
                        min_items: None,
                        max_items: None,
                        default: field.get("default").cloned(),
                    },
                    None => FormFieldKind::String {
                        min_length: field
                            .get("minLength")
                            .and_then(Value::as_u64)
                            .map(|n| n as usize),
                        max_length: field
                            .get("maxLength")
                            .and_then(Value::as_u64)
                            .map(|n| n as usize),
                        default: field
                            .get("default")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    },
                },
                Some("number") => FormFieldKind::Number {
                    minimum: field.get("minimum").and_then(Value::as_f64),
                    maximum: field.get("maximum").and_then(Value::as_f64),
                    default: field.get("default").and_then(Value::as_f64),
                },
                Some("integer") => FormFieldKind::Integer {
                    minimum: field.get("minimum").and_then(Value::as_i64),
                    maximum: field.get("maximum").and_then(Value::as_i64),
                    default: field.get("default").and_then(Value::as_i64),
                },
                Some("boolean") => FormFieldKind::Boolean {
                    default: field.get("default").and_then(Value::as_bool),
                },
                Some("array") => FormFieldKind::Enum {
                    options: choices(
                        field
                            .get("items")
                            .ok_or_else(|| "array has no items".to_string())?,
                    )?
                    .ok_or_else(|| "array items are not choices".to_string())?,
                    multi_select: true,
                    min_items: field
                        .get("minItems")
                        .and_then(Value::as_u64)
                        .map(|n| n as usize),
                    max_items: field
                        .get("maxItems")
                        .and_then(Value::as_u64)
                        .map(|n| n as usize),
                    default: field.get("default").cloned(),
                },
                _ => return Err(format!("{} has an unsupported type", id)),
            };
            Ok(FormField {
                id: id.clone(),
                label: field
                    .get("title")
                    .and_then(Value::as_str)
                    .filter(|label| !label.is_empty())
                    .unwrap_or(id)
                    .into(),
                description: field
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                required: required
                    .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(id))),
                kind,
            })
        })
        .collect()
}

fn choices(schema: &Value) -> Result<Option<Vec<FormChoice>>, String> {
    if let Some(values) = schema.get("enum") {
        let values = values
            .as_array()
            .ok_or_else(|| "enum is not an array".to_string())?;
        let labels = match schema.get("enumNames") {
            Some(names) => {
                let names = names
                    .as_array()
                    .ok_or_else(|| "enumNames is not an array".to_string())?;
                if names.len() != values.len() {
                    return Err("enumNames does not match enum choices".into());
                }
                Some(names)
            }
            None => None,
        };
        let choices = values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let value = value
                    .as_str()
                    .ok_or_else(|| "enum choice is not text".to_string())?;
                let label = labels
                    .and_then(|labels| labels[index].as_str())
                    .filter(|label| !label.is_empty())
                    .unwrap_or(value);
                Ok(FormChoice {
                    value: value.into(),
                    label: label.into(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        ensure_unique(&choices)?;
        return Ok(Some(choices));
    }
    let alternatives = schema.get("oneOf").or_else(|| schema.get("anyOf"));
    let Some(alternatives) = alternatives else {
        return Ok(None);
    };
    let choices = alternatives
        .as_array()
        .ok_or_else(|| "choices are not an array".to_string())?
        .iter()
        .map(|choice| {
            let value = choice
                .get("const")
                .and_then(Value::as_str)
                .ok_or_else(|| "choice has no text value".to_string())?;
            Ok(FormChoice {
                value: value.into(),
                label: choice
                    .get("title")
                    .and_then(Value::as_str)
                    .filter(|title| !title.is_empty())
                    .unwrap_or(value)
                    .into(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    ensure_unique(&choices)?;
    Ok(Some(choices))
}

fn ensure_unique(choices: &[FormChoice]) -> Result<(), String> {
    if choices.iter().enumerate().any(|(index, choice)| {
        choices[..index]
            .iter()
            .any(|other| other.value == choice.value)
    }) {
        Err("form choices contain duplicate values".into())
    } else {
        Ok(())
    }
}
