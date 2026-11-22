//! T3 Code's delegation tools (ADR 0047), offered to every Thread's agent
//! while Settings › Labs › Linked machines is on: `orchestrator_capabilities`,
//! `delegate_task`, `task_status`, `task_cancel`, and `machine_list` (T3's
//! `t3_environment_links`). Their names, descriptions, parameters and
//! answers are T3's at 0fd48228 (`apps/server/src/mcp/toolkits/orchestrator/
//! tools.ts`, `packages/contracts/src/orchestratorMcp.ts`), with a machine
//! where T3 says an environment and these differences:
//!
//! - A Delegate always runs on a linked machine, in its Project's checkout
//!   (T3's remote task), so `target.machineId` is required and there is no
//!   `isolation`.
//! - Not offered: `mode: "wait"` with `timeoutMs`, `interactionMode`,
//!   `target.driverKind` (a Ferrite provider is its own instance), and
//!   `orchestrator_capabilities`' `search` (Ferrite lists every model).
//! - An answer carries the fields Ferrite fills. T3's constant ones for a
//!   remote task (`childRunId`, `pendingApproval`, …) are left out, and a
//!   held-up follow says why in `problem`.
//!
//! Each tool is one or two API calls from the Session's own Thread
//! (`delegate.*`, `thread.get`, `provider.list`, `machine.list`), whose
//! answers are reshaped into T3's.

use serde_json::{json, Map, Value};

use super::super::schema::{
    DelegateCancelParams, DelegateParams, DelegateStartParams, MachineListParams,
    ProviderListParams, ThreadParams,
};
use super::super::Method;
use super::{Answer, Definition};
use crate::delegate::{DelegateId, NEXT};
use crate::runtime_mode::RuntimeMode;
use crate::store::Provider;
use crate::ThreadId;

/// The tools, by T3's names.
pub(super) const NAMES: [&str; 5] = [
    "orchestrator_capabilities",
    "delegate_task",
    "task_status",
    "task_cancel",
    "machine_list",
];

/// What `machine_list` says to do for a machine it does not list (T3's
/// `NOT_LISTED`, which names its `t3_environment_link`).
pub const NOT_LISTED: &str =
    "Not listed: not linked yet. The user links it in Settings → Connections.";

/// A start that names no machine: Ferrite delegates only to another one.
pub const NO_MACHINE: &str =
    "Pass target.machineId: a delegated task runs on a linked machine. machine_list lists them.";

/// T3's `TaskRole`s, each but `general` said to the child first.
const ROLES: [&str; 6] = [
    "implementation",
    "research",
    "review",
    "design",
    "test",
    "general",
];

pub(super) fn definitions() -> Vec<Definition> {
    let machine = |description: &str| json!({ "type": "string", "description": description });
    let task_id = json!({ "type": "string", "pattern": "^d[0-9]+$" });
    let client_request_id = json!({
        "type": "string",
        "maxLength": 256,
        "description": "Reuse it to retry the same call. A new call gets a new key.",
    });
    vec![
        Definition {
            name: "orchestrator_capabilities".into(),
            description: "List providers, models, options, and runtime modes for `delegate_task`."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "machineId": machine("Linked machine to act in. Omit for this one."),
                },
            }),
        },
        Definition {
            name: "delegate_task".into(),
            description: "Start one child agent on a linked machine with only `task`. No parent history. It works in that machine's checkout of this repository. Keep `taskId` for `task_status` and `task_cancel`. No secrets in `task`.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task": { "type": "string", "minLength": 1, "maxLength": 120_000 },
                    "target": {
                        "type": "object",
                        "properties": {
                            "providerInstanceId": {
                                "type": "string",
                                "enum": ["claude", "codex"],
                                "description": "Provider instance id from `orchestrator_capabilities`.",
                            },
                            "model": { "type": "string" },
                            "options": {
                                "type": "object",
                                "properties": { "effort": { "type": "string" } },
                                "additionalProperties": false,
                                "description": "Options from `orchestrator_capabilities`, such as {effort:'max'}. Given values override inherited.",
                            },
                            "machineId": machine("Linked machine to run in. Provider and model default to yours; others come from `orchestrator_capabilities` with this `machineId`."),
                            "projectId": {
                                "type": "string",
                                "description": "With `machineId`: its project to run in. Default: the one with this project's repository.",
                            },
                        },
                        "required": ["machineId"],
                    },
                    "title": { "type": "string", "minLength": 1, "maxLength": 512 },
                    "role": { "type": "string", "enum": ROLES },
                    "clientRequestId": client_request_id,
                    "runtimeMode": {
                        "type": "string",
                        "enum": ["inherit", "approval-required", "auto-accept-edits", "auto", "full-access"],
                        "description": "Omit unless the user asks for a supervised child. Narrow modes pause for human approval, not read-only. Above yours is rejected.",
                    },
                },
                "required": ["task", "target"],
            }),
        },
        Definition {
            name: "task_status".into(),
            description: "Read one delegated task you started. Do not call it on a running task before its completion wakes you. `workState`: working, result_available. A finished turn with live child work is not done. `summary`: final result. Reading it marks it delivered.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "taskId": task_id },
                "required": ["taskId"],
            }),
        },
        Definition {
            name: "task_cancel".into(),
            description: "Stop one delegated task by `taskId`. Stops its running turn, its subagents and background commands, its queued prompts, and its delegates. A finished task keeps its result.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "taskId": task_id,
                    "reason": { "type": "string", "maxLength": 2_000 },
                },
                "required": ["taskId"],
            }),
        },
        Definition {
            name: "machine_list".into(),
            description: "List linked machines (the user's other machines) and whether each answers now. Pass one's `machineId` to a tool to act there; `delegate_task` takes `target.machineId`. `sign_in`: the user signs in to it from a Ferrite window.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
    ]
}

/// Run `tool` for `thread`'s agent, asking the Server through `ask`: an
/// API answer's `result`, or the words of its refusal.
pub(super) fn call(
    tool: &str,
    arguments: &Value,
    thread: ThreadId,
    ask: &dyn Fn(Method) -> Result<Value, String>,
) -> Answer {
    let answered = match tool {
        "orchestrator_capabilities" => capabilities(arguments, thread, ask),
        "delegate_task" => delegate(arguments, ask),
        "task_status" => task_id(arguments).and_then(|delegate| {
            let read = ask(Method::DelegateGet(DelegateParams { thread: None, delegate }))?;
            Ok(as_task(&read["delegate"]))
        }),
        "task_cancel" => task_id(arguments).and_then(|delegate| {
            let reason = arguments["reason"].as_str().map(str::to_string);
            let cancelled = ask(Method::DelegateCancel(DelegateCancelParams {
                delegate: DelegateParams { thread: None, delegate },
                reason,
            }))?;
            Ok(json!({ "taskId": cancelled["delegate"], "status": cancelled["status"] }))
        }),
        "machine_list" => ask(Method::MachineList(MachineListParams { reach: true })).map(machines),
        _ => Err(format!("Ferrite offers no tool {tool}")),
    };
    match answered {
        Ok(value) => Answer {
            text: value.to_string(),
            is_error: false,
        },
        Err(text) => Answer {
            text,
            is_error: true,
        },
    }
}

fn capabilities(
    arguments: &Value,
    thread: ThreadId,
    ask: &dyn Fn(Method) -> Result<Value, String>,
) -> Result<Value, String> {
    let machine = arguments["machineId"].as_str().map(str::to_string);
    let parent = ask(Method::ThreadGet(ThreadParams { thread }))?;
    let parent = &parent["thread"];
    let listed = ask(Method::ProviderList(ProviderListParams { machine }))?;
    let providers: Vec<Value> = listed["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(provider_capability)
        .collect();
    let mut answer = json!({
        "parentThreadId": thread,
        "inheritedProviderInstanceId": parent["provider"],
        "inheritedModel": parent.get("model").cloned().unwrap_or(Value::Null),
        "providers": providers,
    });
    if let Some(effort) = parent["effort"].as_str() {
        answer["inheritedOptions"] = json!([{ "id": "effort", "value": effort }]);
    }
    Ok(answer)
}

/// One `provider.list` provider as T3's `ProviderCapability`.
fn provider_capability(provider: &Value) -> Value {
    let models: Vec<Value> = provider["models"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|model| {
            let mut entry = json!({ "id": model["model"] });
            // T3 leaves out a label that only restates the id.
            if model["label"] != model["model"] && model["label"].is_string() {
                entry["label"] = model["label"].clone();
            }
            if let Some(efforts) = model["efforts"].as_array().filter(|efforts| !efforts.is_empty()) {
                let mut effort = json!({ "id": "effort", "values": efforts });
                if model["default_effort"].is_string() {
                    effort["default"] = model["default_effort"].clone();
                }
                entry["options"] = json!([effort]);
            }
            entry
        })
        .collect();
    let modes: Vec<&str> = provider["modes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|mode| serde_json::from_value::<RuntimeMode>(mode.clone()).ok())
        .map(t3_mode)
        .collect();
    let display = serde_json::from_value::<Provider>(provider["provider"].clone())
        .map(|provider| provider.to_string())
        .ok();
    json!({
        "providerInstanceId": provider["provider"],
        "displayName": display,
        "models": models,
        "runtimeModes": modes,
        "canRunChildTask": true,
        "canRunCrossProviderChildTask": true,
        "constraints": [],
    })
}

fn delegate(arguments: &Value, ask: &dyn Fn(Method) -> Result<Value, String>) -> Result<Value, String> {
    let task = arguments["task"]
        .as_str()
        .map(str::trim)
        .filter(|task| !task.is_empty())
        .ok_or("Pass task: what the child agent is to do.")?;
    // Some providers send the whole target as JSON text (T3 decodes both).
    let target = match &arguments["target"] {
        Value::String(text) => {
            serde_json::from_str::<Value>(text).map_err(|error| format!("target: {error}"))?
        }
        target => target.clone(),
    };
    let machine = target["machineId"].as_str().ok_or(NO_MACHINE)?.to_string();
    let provider = match &target["providerInstanceId"] {
        Value::Null => None,
        named => Some(serde_json::from_value::<Provider>(named.clone()).map_err(|_| {
            format!("Unknown providerInstanceId {named}: orchestrator_capabilities lists them.")
        })?),
    };
    let mode = match arguments["runtimeMode"].as_str() {
        None | Some("inherit") => None,
        Some(mode) => Some(of_t3_mode(mode).ok_or_else(|| format!("Unknown runtimeMode {mode}."))?),
    };
    let role = arguments["role"].as_str().filter(|role| *role != "general");
    if let Some(role) = role.filter(|role| !ROLES.contains(role)) {
        return Err(format!("Unknown role {role}."));
    }
    let started = ask(Method::DelegateStart(DelegateStartParams {
        thread: None,
        machine,
        // T3's `taskPrompt`: the role the caller asked for, said first.
        task: match role {
            Some(role) => format!("Act as the {role} sub-agent for this task.\n\n{task}"),
            None => task.to_string(),
        },
        checkout: None,
        project: target["projectId"].as_str().map(str::to_string),
        title: arguments["title"].as_str().map(str::to_string),
        provider,
        request: arguments["clientRequestId"].as_str().map(str::to_string),
        model: target["model"].as_str().map(str::to_string),
        effort: effort(&target["options"])?,
        mode,
    }))?;
    Ok(as_task(&started["delegate"]))
}

/// The effort a target's `options` name: T3's record `{effort}` or its
/// list `[{id, value}]`; nothing given inherits.
fn effort(options: &Value) -> Result<Option<String>, String> {
    let pairs: Vec<(String, Value)> = match options {
        Value::Null => return Ok(None),
        Value::Object(record) => record.clone().into_iter().collect(),
        Value::Array(list) => list
            .iter()
            .map(|option| {
                let id = option["id"].as_str().ok_or("Each option names its id.")?;
                Ok((id.to_string(), option["value"].clone()))
            })
            .collect::<Result<_, String>>()?,
        _ => return Err("options: {effort:'high'}, or [{id:'effort', value:'high'}].".into()),
    };
    let mut effort = None;
    for (id, value) in pairs {
        match (id.as_str(), value) {
            ("effort", Value::String(value)) => effort = Some(value),
            ("effort", _) => return Err("The effort option is a string.".into()),
            (id, _) => {
                return Err(format!(
                    "Unknown option {id}: orchestrator_capabilities lists each model's options."
                ))
            }
        }
    }
    Ok(effort)
}

fn task_id(arguments: &Value) -> Result<DelegateId, String> {
    arguments["taskId"]
        .as_str()
        .ok_or_else(|| "Pass taskId, from delegate_task.".to_string())?
        .parse()
}

/// A Delegate's API info as T3's task result for a remote task, with what
/// to do next while it runs.
fn as_task(info: &Value) -> Value {
    let status = info["status"].as_str().unwrap_or_default();
    let ended = matches!(status, "completed" | "failed" | "cancelled" | "interrupted");
    let mut answer = Map::new();
    answer.insert("taskId".into(), info["delegate"].clone());
    answer.insert(
        "remoteChild".into(),
        json!({ "machineId": info["machine"], "threadId": info["thread"], "label": info["machine"] }),
    );
    answer.insert("status".into(), status.into());
    answer.insert(
        "workState".into(),
        if ended { "result_available" } else { "working" }.into(),
    );
    answer.insert("providerInstanceId".into(), info["provider"].clone());
    answer.insert("model".into(), info["model"].clone());
    answer.insert("summary".into(), info["result"].clone());
    if info["problem"].is_string() {
        answer.insert("problem".into(), info["problem"].clone());
    }
    if !ended {
        answer.insert("next".into(), NEXT.into());
    }
    Value::Object(answer)
}

/// `machine.list` with reach as T3's `EnvironmentLinksResult`.
fn machines(listed: Value) -> Value {
    let machines: Vec<Value> = listed["machines"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|machine| {
            json!({
                "machineId": machine["id"],
                "label": machine["label"],
                "status": machine["status"],
                "lastError": machine.get("problem").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    json!({ "machines": machines, "notListed": NOT_LISTED })
}

/// T3's spelling of a runtime mode.
fn t3_mode(mode: RuntimeMode) -> &'static str {
    match mode {
        RuntimeMode::ApprovalRequired => "approval-required",
        RuntimeMode::AutoAcceptEdits => "auto-accept-edits",
        RuntimeMode::Auto => "auto",
        RuntimeMode::FullAccess => "full-access",
    }
}

fn of_t3_mode(mode: &str) -> Option<RuntimeMode> {
    RuntimeMode::ALL.into_iter().find(|known| t3_mode(*known) == mode)
}

#[cfg(test)]
mod tests;
