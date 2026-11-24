//! T3 Code's delegation tools (ADR 0047), offered to every Thread's agent
//! while Settings › Labs › Linked machines is on: `orchestrator_capabilities`,
//! `delegate_task`, `task_status`, `task_cancel`, `machine_list` (T3's
//! `t3_environment_links`) and `project_list` (T3's `t3_project_list`).
//! Their names, descriptions, parameters, answers and refusals are T3's at
//! 0fd48228 (`apps/server/src/mcp/toolkits/orchestrator/tools.ts`,
//! `packages/contracts/src/orchestratorMcp.ts`, `peer/RemoteDelegation.ts`),
//! with a machine where T3 says an environment and Ferrite where it says
//! T3 Code. A Delegate always runs on another machine, as T3's remote task
//! does, so a call answers as T3 answers for one; what Ferrite cannot do
//! there is refused in T3's manner, never taken and dropped. ADR 0047
//! lists each difference and why.
//!
//! Each tool is a few API calls from the Session's own Thread, whose
//! answers are reshaped into T3's and whose refusals are said in these
//! tools' names with T3's codes ([`said`]).

use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::super::schema::{
    DelegateCancelParams, DelegateParams, DelegateStartParams, MachineListParams, MachineParams,
    ThreadParams,
};
use super::super::Method;
use super::{Answer, Definition, Refusal};
use ferrite_core::delegate::{DelegateId, NEXT};
use ferrite_core::provider::Provider;
use ferrite_core::runtime_mode::RuntimeMode;
use ferrite_core::ThreadId;

/// The tools, by T3's names.
pub(super) const NAMES: [&str; 6] = [
    "orchestrator_capabilities",
    "delegate_task",
    "task_status",
    "task_cancel",
    "machine_list",
    "project_list",
];

/// What the agent is told of these tools: T3's `## T3 Code orchestration`
/// (`provider-core/src/server/orchestrationInstructions.ts`), with only
/// what is about delegating and linked machines, which Ferrite has.
pub(super) const INSTRUCTIONS: &str = "## Ferrite orchestration

Ordinary coding tasks need no Ferrite tools.

- Subagents: use your native tool for a child on this machine (Claude `Agent`, `run_in_background:false`; Codex `spawn_agent`, then wait). Use `delegate_task` for a child on another machine, or `delegate_task` by name. Workflow only when asked.
- Call `orchestrator_capabilities` once per thread, before the first `delegate_task`. Again only after an error or a user-named provider change. Settings go in `target.options`. Provider or model unavailable: say so. Never switch unasked or fall back to native tools, Workflow or CLIs. Never make a child's checkout in the shell or in `task`.
- Every new delegate review or re-check is a new `delegate_task` (brief, findings, objections). Fix small review findings yourself; start no delegate for them. When told to proceed without a delegate, or to run fewer, `task_cancel` the extras. Start no replacement.
- Follow a running delegate's `next`. Don't poll it (`task_status`, `sleep`, watchers, Monitor, ScheduleWakeup). Stop a delegate run you replace or no longer need with `task_cancel`.
- As a subagent or child, do the work yourself. Start no subagents unless your task says to.
- Another machine: `machine_list` lists linked ones. Pass its `machineId` to Ferrite tools.
- Unlinked (\"delegate this to my vps\"): say the user links it in Settings → Connections, never ask for a pairing code in chat.
- Ferrite tools may load late. Try the name once before you call it absent.";

/// What `machine_list` says to do for a machine it does not list (T3's
/// `NOT_LISTED`, which names its `t3_environment_link`).
const NOT_LISTED: &str = "Not listed: not linked yet. The user links it in Settings → Connections.";

/// A start that names no machine, or this one: Ferrite delegates only to
/// another.
const NO_MACHINE: &str = "Pass target.machineId: a delegated task runs on a linked machine, not this one. machine_list lists them under links.";

/// T3's refusal of a worktree there, without its `t3_thread_launch`.
const NO_WORKTREE: &str = "A task in a linked machine runs in that project's checkout; isolation \"worktree\" is not supported there. Omit isolation.";

/// A start carries only a runtime mode there (`thread.start`'s `mode`).
const NO_PLAN: &str = "A task in a linked machine runs in default interaction mode; interactionMode \"plan\" is not supported there. Omit interactionMode.";

/// How long `mode: "wait"` waits unasked, and at most (T3's).
const WAIT: Duration = Duration::from_secs(30);
const MOST_WAIT: Duration = Duration::from_secs(45);

/// How often a wait reads its Delegate.
const WAIT_READ: Duration = Duration::from_millis(500);

/// How many models a `search` lists of each provider (T3's
/// `LISTED_MODEL_LIMIT`). Without one every model is listed: T3 lists
/// Claude's and Codex's whole catalogs.
const LISTED_MODELS: usize = 8;

/// T3's `TaskRole`s, each but `general` said to the child first.
const ROLES: [&str; 6] = [
    "implementation",
    "research",
    "review",
    "design",
    "test",
    "general",
];

/// What a call asks the Server.
type Ask<'a> = dyn Fn(Method) -> Result<Value, Failure> + 'a;

/// A refusal as T3's `OrchestratorMcpFailure`: its code and its words.
#[derive(Debug, PartialEq, Eq)]
struct Failure {
    code: &'static str,
    message: String,
}

impl Failure {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

fn invalid(message: impl Into<String>) -> Failure {
    Failure::new("invalid_request", message)
}

/// The API's names a Server refusal says (`delegates::start`), as these
/// tools say them.
const API_WORDS: [(&str, &str); 9] = [
    ("`machine.list`", "machine_list"),
    ("`provider.list`", "orchestrator_capabilities"),
    ("`project.list`", "project_list"),
    ("`machine`", "machineId"),
    ("`project`", "target.projectId"),
    ("`model`", "target.model"),
    ("approval_required", "approval-required"),
    ("auto_accept_edits", "auto-accept-edits"),
    ("full_access", "full-access"),
];

/// The Server's refusal in these tools' words: T3's code for it, and every
/// API name in it these tools' own.
fn said(refusal: Refusal) -> Failure {
    let code = match refusal.code.as_str() {
        "unknown_delegate" => "task_not_found",
        "widens_cap" => "runtime_mode_escalation_denied",
        "linked_machines_off" => "capability_denied",
        "target_required" => "target_required",
        "invalid_request" | "project_not_found" => "invalid_request",
        "unknown_thread" => "thread_not_found",
        _ => "orchestration_error",
    };
    let message = API_WORDS
        .iter()
        .fold(refusal.message, |message, (api, tool)| message.replace(api, tool));
    Failure::new(code, message)
}

/// The tools as the agent is told of them, each schema closed as T3's
/// are: [`super::checked`] holds a call to it.
pub(super) fn definitions() -> Vec<Definition> {
    let here_or_there = json!({
        "type": "string",
        "description": "Linked machine to act in. Omit for this one. Ids are then that machine's.",
    });
    let client_request_id = json!({
        "type": "string",
        "minLength": 1,
        "maxLength": 256,
        "description": "Reuse it to retry the same call. A new call gets a new key.",
    });
    let mut definitions = vec![
        Definition {
            name: "orchestrator_capabilities".into(),
            description: "List providers, models, options, and runtime modes for `delegate_task`. Short list. `search` finds others. Any catalog model works.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "machineId": here_or_there,
                    "search": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 200,
                        "description": "Match model ids and labels. Replaces the short list.",
                    },
                },
            }),
        },
        Definition {
            name: "delegate_task".into(),
            description: "Start one child agent with only `task`. No parent history. Separate checkout or PR per child: `isolation:'worktree'`. Keep `taskId` for `task_status` and `task_cancel`. No secrets in `task`.".into(),
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
                            "driverKind": {
                                "type": "string",
                                "description": "Prefer `providerInstanceId`.",
                            },
                            "model": { "type": "string", "minLength": 1 },
                            "options": {
                                "anyOf": [
                                    {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "id": { "type": "string" },
                                                "value": { "type": ["string", "boolean"] },
                                            },
                                            "required": ["id", "value"],
                                        },
                                    },
                                    {
                                        "type": "object",
                                        "properties": { "effort": { "type": "string" } },
                                    },
                                ],
                                "description": "Options from `orchestrator_capabilities`, such as {effort:'max'}. Given values override inherited. [] or {} clears.",
                            },
                            "machineId": {
                                "type": "string",
                                "description": "Linked machine to run in. Provider and model default to yours; others come from `orchestrator_capabilities` with this `machineId`.",
                            },
                            "projectId": {
                                "type": "string",
                                "description": "With `machineId`: its project to run in. Default: the one with this project's repository.",
                            },
                        },
                        "required": ["machineId"],
                    },
                    "title": { "type": "string", "minLength": 1, "maxLength": 512 },
                    "role": { "type": "string", "enum": ROLES },
                    "isolation": {
                        "type": "string",
                        "enum": ["shared", "worktree"],
                        "description": "'shared' (default): parent checkout. 'worktree': new prepared checkout from the parent commit, for a separate checkout or PR per child. Uncommitted edits stay out.",
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["async", "wait"],
                        "description": "Default 'async'. 'wait' only if this turn needs the result. A question or approval ends the wait.",
                    },
                    "timeoutMs": {
                        "type": "number",
                        "description": "Mode 'wait' only. At most 45 s. A timeout does not cancel the child.",
                    },
                    "clientRequestId": client_request_id,
                    "runtimeMode": {
                        "type": "string",
                        "enum": ["inherit", "approval-required", "auto-accept-edits", "auto", "full-access"],
                        "description": "Omit unless the user asks for a supervised child. Narrow modes pause for human approval, not read-only. Above yours is rejected.",
                    },
                    "interactionMode": {
                        "type": "string",
                        "enum": ["inherit", "default", "plan"],
                    },
                },
                "required": ["task", "target"],
            }),
        },
        Definition {
            name: "task_status".into(),
            description: "Read one delegated task you started. Do not call it on a running task before its completion wakes you. `workState`: working, waiting_for_children, result_available. A finished turn with live child work is not done. `summary`: final result. Reading it marks it delivered.".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "taskId": { "type": "string" } },
                "required": ["taskId"],
            }),
        },
        Definition {
            name: "task_cancel".into(),
            description: "Stop one delegated task by `taskId`. Stops its running and queued turns and its delegates. A finished task keeps its result.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "taskId": { "type": "string" },
                    "reason": { "type": "string", "maxLength": 2_000 },
                    "clientRequestId": client_request_id,
                },
                "required": ["taskId"],
            }),
        },
        Definition {
            name: "machine_list".into(),
            description: "List linked machines (the user's other machines) and whether each answers now. Pass one's `machineId` to a tool to act there; `delegate_task` takes `target.machineId`. Expired: the user links it again in Settings → Connections. `sign_in`: the user signs in to it from a Ferrite window.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        Definition {
            name: "project_list".into(),
            description: "List projects. Pages can shift.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "machineId": here_or_there,
                    "cursor": { "type": "integer", "minimum": 0 },
                    "limit": { "type": "integer", "minimum": 1 },
                },
            }),
        },
    ];
    for definition in &mut definitions {
        super::close(&mut definition.input_schema);
    }
    definitions
}

/// Run `tool` for `thread`'s agent with `arguments` its schema took,
/// asking the Server through `ask`: T3's answer, or T3's failure.
pub(super) fn call(
    tool: &str,
    arguments: &Value,
    thread: ThreadId,
    ask: &dyn Fn(Method) -> Result<Value, Refusal>,
) -> Answer {
    let ask = |method| ask(method).map_err(said);
    let answered = match tool {
        "orchestrator_capabilities" => capabilities(arguments, thread, &ask),
        "delegate_task" => delegate(arguments, &ask),
        "task_status" => task_id(arguments, thread).and_then(|delegate| {
            let read = ask(Method::DelegateGet(DelegateParams { thread: None, delegate }))?;
            Ok(as_task(&read["delegate"], false))
        }),
        "task_cancel" => cancel(arguments, thread, &ask),
        "machine_list" => ask(Method::MachineList(MachineListParams { reach: true })).map(machines),
        "project_list" => projects(arguments, &ask),
        _ => Err(invalid(format!("Ferrite offers no tool {tool}"))),
    };
    match answered {
        Ok(value) => Answer {
            text: value.to_string(),
            is_error: false,
        },
        Err(failure) => Answer {
            text: json!({ "code": failure.code, "message": failure.message }).to_string(),
            is_error: true,
        },
    }
}

/// T3's capabilities: what a Delegate there can run, and what this Thread
/// hands on. Asked of another machine, T3's answer is that machine's,
/// which knows no parent: nothing is inherited there.
fn capabilities(arguments: &Value, thread: ThreadId, ask: &Ask<'_>) -> Result<Value, Failure> {
    let machine = remote_target(&arguments["machineId"], ask)?;
    let search = match arguments["search"].as_str().map(str::trim) {
        None => None,
        Some(search) if !search.is_empty() && search.chars().count() <= 200 => {
            Some(search.to_lowercase())
        }
        Some(_) => return Err(invalid("Pass search: 1 to 200 characters.")),
    };
    let terms: Vec<&str> = search.iter().flat_map(|search| search.split_whitespace()).collect();
    let parent = ask(Method::DelegateDefaults(ThreadParams { thread }))?;
    let here = machine.is_none();
    let listed = ask(Method::ProviderList(MachineParams { machine }))?;
    let providers: Vec<Value> = listed["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|provider| provider_capability(provider, &terms))
        .collect();
    let inherited = |field: &str| match here {
        true => parent[field].clone(),
        false => Value::Null,
    };
    let options = match inherited("effort") {
        Value::String(effort) => json!([{ "id": "effort", "value": effort }]),
        _ => json!([]),
    };
    let mode = serde_json::from_value::<RuntimeMode>(parent["mode"].clone()).ok();
    Ok(json!({
        "parentThreadId": inherited("thread"),
        "inheritedProviderInstanceId": inherited("provider"),
        "inheritedModel": inherited("model"),
        "inheritedOptions": options,
        "runtimeMode": mode.map(t3_mode),
        "interactionMode": "default",
        "providers": providers,
        "features": {
            "appOwnedSubagents": true,
            "asyncPolling": true,
            "cancellation": true,
            "batchThreadCreation": false,
            "threadManagement": false,
            "incrementalThreadRead": false,
            "scheduledTasks": false,
            "maxBatchThreads": 0,
        },
    }))
}

/// One `provider.list` provider as T3's `ProviderCapability`, its models
/// those `terms` match as T3's `listCapabilityModels` matches them: a
/// whole word of the provider's names names it, not a model.
fn provider_capability(provider: &Value, terms: &[&str]) -> Value {
    let id = provider["provider"].as_str().unwrap_or_default();
    let display = serde_json::from_value::<Provider>(provider["provider"].clone())
        .map(|provider| provider.to_string())
        .ok();
    let models: Vec<&Value> = provider["models"].as_array().into_iter().flatten().collect();
    let text = |model: &Value| {
        let field = |name: &str| model[name].as_str().unwrap_or_default().to_string();
        format!("{} {}", field("model"), field("label")).to_lowercase()
    };
    let named = format!("{id} {id} {}", display.as_deref().unwrap_or_default()).to_lowercase();
    let words: HashSet<&str> = named
        .split_whitespace()
        .chain(named.split(|c: char| !c.is_alphanumeric()))
        .collect();
    let model_terms: Vec<&str> = terms
        .iter()
        .copied()
        .filter(|term| {
            !words.contains(term)
                && (!named.contains(term) || models.iter().any(|model| text(*model).contains(term)))
        })
        .collect();
    let (listed, omitted) = match model_terms.is_empty() {
        true => (models, 0),
        false => {
            let matches: Vec<&Value> = models
                .into_iter()
                .filter(|model| model_terms.iter().all(|term| text(*model).contains(term)))
                .collect();
            let omitted = matches.len().saturating_sub(LISTED_MODELS);
            (matches.into_iter().take(LISTED_MODELS).collect(), omitted)
        }
    };
    let models: Vec<Value> = listed.into_iter().map(model_capability).collect();
    let modes: Vec<&str> = provider["modes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|mode| serde_json::from_value::<RuntimeMode>(mode.clone()).ok())
        .map(t3_mode)
        .collect();
    let mut capability = json!({
        "providerInstanceId": id,
        "driverKind": id,
        "displayName": display,
        "models": models,
        "runtimeModes": modes,
        "canRunChildTask": true,
        "canRunCrossProviderChildTask": true,
        "constraints": [],
    });
    if omitted > 0 {
        capability["omittedModels"] = omitted.into();
    }
    capability
}

/// One `provider.list` model as T3's: its label only where it says more
/// than the id, and its efforts as the `effort` option.
fn model_capability(model: &Value) -> Value {
    let mut entry = json!({ "id": model["model"] });
    let (id, label) = (model["model"].as_str(), model["label"].as_str());
    if let Some(label) = label.filter(|label| Some(label.to_lowercase()) != id.map(str::to_lowercase)) {
        entry["label"] = label.into();
    }
    if let Some(efforts) = model["efforts"].as_array().filter(|efforts| !efforts.is_empty()) {
        let mut effort = json!({ "id": "effort", "values": efforts });
        if model["default_effort"].is_string() {
            effort["default"] = model["default_effort"].clone();
        }
        entry["options"] = json!([effort]);
    }
    entry
}

/// T3's `delegate_task` with a target there: one `delegate.start`, and
/// with `mode: "wait"` its reads until it ends or the wait is up.
fn delegate(arguments: &Value, ask: &Ask<'_>) -> Result<Value, Failure> {
    let task = arguments["task"]
        .as_str()
        .map(str::trim)
        .filter(|task| !task.is_empty())
        .ok_or_else(|| invalid("Pass task: what the child agent is to do."))?;
    let target = &arguments["target"];
    let machine = remote_target(&target["machineId"], ask)?
        .ok_or_else(|| Failure::new("target_required", NO_MACHINE))?;
    if arguments["isolation"] == "worktree" {
        return Err(invalid(NO_WORKTREE));
    }
    if arguments["interactionMode"] == "plan" {
        return Err(invalid(NO_PLAN));
    }
    let wait = (arguments["mode"] == "wait").then(|| {
        let asked = arguments["timeoutMs"].as_f64();
        let millis = asked.map_or(WAIT.as_millis() as f64, |millis| millis.max(1.0));
        Duration::from_millis(millis as u64).min(MOST_WAIT)
    });
    let named = target["providerInstanceId"].as_str();
    let provider = match target["driverKind"].as_str() {
        Some(kind) => Some(driven(kind, named, &machine, ask)?),
        None => named.map(str::to_string),
    };
    let provider = provider
        .map(|id| {
            serde_json::from_value::<Provider>(json!(id)).map_err(|_| {
                let unknown = format!("Unknown providerInstanceId {id}: orchestrator_capabilities lists them.");
                Failure::new("provider_unavailable", unknown)
            })
        })
        .transpose()?;
    let mode = match arguments["runtimeMode"].as_str() {
        None | Some("inherit") => None,
        Some(mode) => Some(of_t3_mode(mode).ok_or_else(|| invalid(format!("Unknown runtimeMode {mode}.")))?),
    };
    let role = arguments["role"].as_str().filter(|role| *role != "general");
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
    let info = &started["delegate"];
    match wait {
        None => Ok(as_task(info, false)),
        Some(wait) => {
            let delegate = info["delegate"]
                .as_str()
                .and_then(|id| id.parse().ok())
                .ok_or_else(|| Failure::new("orchestration_error", "The start answered no taskId."))?;
            waited(delegate, wait, ask)
        }
    }
}

/// The linked machine a call names, or none when it names this one, whose
/// id `machine_list` answers first (T3's `remoteTarget`).
fn remote_target(named: &Value, ask: &Ask<'_>) -> Result<Option<String>, Failure> {
    let Some(named) = named.as_str() else { return Ok(None) };
    let here = ask(Method::MachineList(MachineListParams::default()))?;
    Ok((here["server_id"] != named).then(|| named.to_string()))
}

/// The provider there of driver `kind` (T3's `resolveRemoteInstance`): a
/// Ferrite provider is its own one instance, its driver its id.
fn driven(kind: &str, named: Option<&str>, machine: &str, ask: &Ask<'_>) -> Result<String, Failure> {
    let listed = ask(Method::ProviderList(MachineParams {
        machine: Some(machine.to_string()),
    }))?;
    let there: Vec<&str> = listed["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|provider| provider["provider"].as_str())
        .collect();
    match named {
        Some(named) if there.contains(&named) && named != kind => Err(invalid(format!(
            "Provider instance {named} there uses driver {named}, not {kind}."
        ))),
        Some(named) => Ok(named.to_string()),
        None => there
            .into_iter()
            .find(|provider| *provider == kind)
            .map(str::to_string)
            .ok_or_else(|| {
                Failure::new(
                    "provider_unavailable",
                    format!(
                        "No instance of driver {kind} there can run a child task. \
                         orchestrator_capabilities with this machineId lists its providers."
                    ),
                )
            }),
    }
}

/// Read the Delegate until it ends or `wait` is up (T3's `awaitTask`). A
/// read of an ended one by its parent takes its result, so no wake
/// carries it again; one the wait outlived still wakes the parent.
fn waited(delegate: DelegateId, wait: Duration, ask: &Ask<'_>) -> Result<Value, Failure> {
    let until = Instant::now() + wait;
    loop {
        let read = ask(Method::DelegateGet(DelegateParams { thread: None, delegate }))?;
        let info = &read["delegate"];
        if ended(info) {
            return Ok(as_task(info, false));
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Ok(as_task(info, true));
        }
        std::thread::sleep(left.min(WAIT_READ));
    }
}

/// The effort a target's `options` name: T3's record `{effort}` or its
/// list `[{id, value}]`. `None`: nothing given, so it inherits; `Some(None)`:
/// `{}` or `[]`, which clears it.
fn effort(options: &Value) -> Result<Option<Option<String>>, Failure> {
    let pairs: Vec<(String, Value)> = match options {
        Value::Null => return Ok(None),
        Value::Object(record) => record.clone().into_iter().collect(),
        Value::Array(list) => list
            .iter()
            .map(|option| {
                let id = option["id"].as_str().ok_or_else(|| invalid("Each option names its id."))?;
                Ok((id.to_string(), option["value"].clone()))
            })
            .collect::<Result<_, Failure>>()?,
        _ => return Err(invalid("options: {effort:'high'}, or [{id:'effort', value:'high'}].")),
    };
    let mut effort = None;
    for (id, value) in pairs {
        match (id.as_str(), value) {
            ("effort", Value::String(value)) => effort = Some(value),
            ("effort", _) => return Err(invalid("The effort option is a string.")),
            (id, _) => {
                return Err(invalid(format!(
                    "Unknown option {id}: orchestrator_capabilities lists each model's options."
                )))
            }
        }
    }
    Ok(Some(effort))
}

/// `task_cancel`: its retry key changes nothing, as for T3's remote task,
/// whose cancel a retry repeats.
fn cancel(arguments: &Value, thread: ThreadId, ask: &Ask<'_>) -> Result<Value, Failure> {
    let delegate = task_id(arguments, thread)?;
    let reason = arguments["reason"].as_str().map(str::to_string);
    if reason.as_ref().is_some_and(|reason| reason.chars().count() > 2_000) {
        return Err(invalid("Pass reason: at most 2000 characters."));
    }
    let cancelled = ask(Method::DelegateCancel(DelegateCancelParams {
        delegate: DelegateParams { thread: None, delegate },
        reason,
    }))?;
    Ok(json!({ "taskId": cancelled["delegate"], "status": cancelled["status"] }))
}

/// The Delegate `taskId` names; any id this Thread never gave is T3's
/// `task_not_found`.
fn task_id(arguments: &Value, thread: ThreadId) -> Result<DelegateId, Failure> {
    let id = arguments["taskId"]
        .as_str()
        .ok_or_else(|| invalid("Pass taskId, from delegate_task."))?;
    id.parse().map_err(|_| {
        let message = format!("Delegated task {id} does not belong to thread {thread}.");
        Failure::new("task_not_found", message)
    })
}

fn ended(info: &Value) -> bool {
    matches!(
        info["status"].as_str(),
        Some("completed" | "failed" | "cancelled" | "interrupted")
    )
}

/// A Delegate's API info as T3's result for a remote task
/// (`readRemoteTask`), with what to do next while it runs. Ids of what
/// Ferrite has none of (runs, nodes, context transfers, thread links) are
/// left out; a held-up follow says why in `problem`.
fn as_task(info: &Value, wait_timed_out: bool) -> Value {
    let ended = ended(info);
    let result = &info["result"];
    let mut answer = Map::new();
    answer.insert("taskId".into(), info["delegate"].clone());
    answer.insert("childThreadId".into(), Value::Null);
    answer.insert(
        "remoteChild".into(),
        json!({ "machineId": info["machine"], "threadId": info["thread"], "label": info["machine"] }),
    );
    answer.insert("status".into(), info["status"].clone());
    answer.insert(
        "workState".into(),
        if ended { "result_available" } else { "working" }.into(),
    );
    answer.insert("hasPendingChildRuns".into(), false.into());
    answer.insert(
        "latestTerminalStatus".into(),
        if ended { info["status"].clone() } else { Value::Null },
    );
    answer.insert(
        "latestTerminalSummary".into(),
        if ended { result.clone() } else { Value::Null },
    );
    answer.insert("providerInstanceId".into(), info["provider"].clone());
    answer.insert("model".into(), info["model"].clone());
    answer.insert("executedModel".into(), Value::Null);
    answer.insert("summary".into(), result.clone());
    // Its questions and approvals are answered on the machine it runs on.
    answer.insert("hasPendingQuestion".into(), false.into());
    answer.insert("pendingApproval".into(), Value::Null);
    answer.insert("waitTimedOut".into(), wait_timed_out.into());
    if info["problem"].is_string() {
        answer.insert("problem".into(), info["problem"].clone());
    }
    if !ended {
        answer.insert("next".into(), NEXT.into());
    }
    Value::Object(answer)
}

/// `machine.list` with reach as T3's `EnvironmentLinksResult`. A link
/// lets this machine's agents do anything there, so its access is full.
fn machines(listed: Value) -> Value {
    let links: Vec<Value> = listed["machines"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|machine| {
            json!({
                "machineId": machine["id"],
                "label": machine["label"],
                "status": machine["status"],
                "access": "full-access",
                "expiresAt": machine["expires_at"],
                "lastError": machine.get("problem").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    json!({ "machineId": listed["server_id"], "links": links, "notListed": NOT_LISTED })
}

/// `project.list`, here or there, a page at a time as T3's: from `cursor`,
/// 20 unless `limit` (at most 100) says otherwise.
fn projects(arguments: &Value, ask: &Ask<'_>) -> Result<Value, Failure> {
    let cursor = match &arguments["cursor"] {
        Value::Null => 0,
        cursor => cursor
            .as_u64()
            .ok_or_else(|| invalid("Pass cursor: a whole number from 0."))? as usize,
    };
    let limit = match &arguments["limit"] {
        Value::Null => 20,
        limit => match limit.as_u64() {
            Some(limit) if limit >= 1 => limit.min(100) as usize,
            _ => return Err(invalid("Pass limit: a whole number from 1.")),
        },
    };
    let machine = remote_target(&arguments["machineId"], ask)?;
    let listed = ask(Method::ProjectList(MachineParams { machine }))?;
    let all = listed["projects"].as_array().cloned().unwrap_or_default();
    let page: Vec<Value> = all.iter().skip(cursor).take(limit).map(project).collect();
    let next = cursor + page.len();
    let next_cursor = (next < all.len()).then_some(next);
    Ok(json!({ "projects": page, "nextCursor": next_cursor }))
}

/// One `project.list` Project as T3's: its id, title, directory and the
/// repository a Delegate's start matches it by.
fn project(info: &Value) -> Value {
    let repository = info["worktree"]["repository"].as_str();
    json!({
        "id": info["id"],
        "title": info["label"],
        "workspaceRoot": info["directory"],
        "repositoryIdentity": repository.map(|key| json!({ "canonicalKey": key })),
    })
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
