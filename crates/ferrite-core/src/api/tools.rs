//! The API as a Thread's own tools: every Thread's PR watch
//! ([`PR_TOOLS`], T3 Code's `watch_pull_request` and
//! `unwatch_pull_request`), every Thread's Delegate tools while Settings ›
//! Labs › Linked machines is on (`delegation`, T3's `delegate_task` and the
//! rest), an owned Thread's `report_*` (a Worker's) and
//! `proposal_raise` (a Coordinator's), one per operation whose callers are
//! that Thread's kind (`schema::Callers`), and a Coordinator's Factory
//! reads ([`COORDINATOR_READS`]). As Ferrite's own tools they are
//! pre-allowed beside `update_plan`, never a permission Decision, and a
//! call reaches the Server as an API request whose caller is the Thread
//! its Session was spawned for: the agent never names it.
//!
//! Every tool's schema names all it takes (`additionalProperties: false`
//! on each object) and a call naming anything else is refused in T3's
//! words before the Server hears it ([`checked`]). The agent is told of
//! the tools as T3 tells its agents ([`ThreadTools::instructions`]).
//!
//! The CLI (`ferrite report …`) stays for an agent without them: a Codex
//! Thread resumed from before they were offered keeps its old tools.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use serde_json::{json, Map, Value};

use super::schema::{Callers, Operation};
use super::{CallerId, Heard, Method, Request};
use crate::link;
use crate::{ThreadId, ThreadOwner};

mod delegation;

/// How long a call waits on the Server's answer.
const ANSWER: Duration = Duration::from_secs(30);

/// Tool calls are API callers of their own, numbered apart from the
/// socket's connections.
static NEXT_CALLER: AtomicU64 = AtomicU64::new(1 << 48);

/// What a Coordinator reads of its Factory, as tools: open to anyone,
/// but asked through a shell each could wait on a permission.
const COORDINATOR_READS: [&str; 5] = [
    "task.list",
    "task.get",
    "workflow.list",
    "workflow.get",
    "proposal.list",
];

/// Every Thread's PR watch tools, by T3 Code's names and words, and the
/// operation each is.
const PR_TOOLS: [PrTool; 2] = [
    PrTool {
        name: "watch_pull_request",
        operation: "pr.watch",
        description: "Watch an open PR for this thread, linking it if needed. Ferrite wakes you on failed checks, passed required checks, new comments or reviews, conflicts, and once on merge or close unless you did it via CLI. Handle old comments first; only new ones wake you. After this call, end your turn. The watch stays on: do not call this again on a wake. A wake is not a merge decision. Check readiness yourself.",
    },
    PrTool {
        name: "unwatch_pull_request",
        operation: "pr.unwatch",
        description: "Stop watching a PR for this thread. It stays linked.",
    },
];

/// What every agent is told of its PR tools: T3 Code's
/// `PULL_REQUEST_LINKING_INSTRUCTIONS` (`provider-core/src/server/
/// runtimeInstructions.ts`), with only what is about watching, which
/// Ferrite has.
const PR_INSTRUCTIONS: &str = "<pull_request_linking>
Call watch_pull_request when asked to monitor or babysit, or when work you were asked to do, such as a merge, still waits on checks or reviews, even after an earlier unwatch. Finish any needed branch update and push first, then watch and end your turn. Otherwise act now or hand back. Delegated children never watch, even if the brief says babysit. After PR work, a child's last line is: PR <full URL>, head <SHA>, checks and reviews <state>. Links and watches persist across pushes. For PR waits, no sleep, loops, gh --watch, Monitor or ScheduleWakeup. Before handback, call unwatch_pull_request on any PR you watch.
</pull_request_linking>";

/// What T3 Code refuses a subagent's watch with.
pub const SUBAGENT_WATCH: &str = "This thread is a subagent, so it cannot watch pull requests. Its parent thread owns the pull request: finish your task and report back instead.";

struct PrTool {
    name: &'static str,
    operation: &'static str,
    description: &'static str,
}

/// The Server's end, where every Session's tools are bound.
#[derive(Debug, Clone)]
pub struct Tools(Sender<link::Heard>);

impl Tools {
    /// `heard` is the pump's: a call is heard as an API connection's is.
    pub fn new(heard: Sender<link::Heard>) -> Self {
        Self(heard)
    }

    /// The tools of `thread`, which `owner` owns, if anyone does; with
    /// `delegation`, the Delegate tools too (Linked machines is on).
    pub fn for_thread(
        &self,
        thread: ThreadId,
        owner: Option<&ThreadOwner>,
        delegation: bool,
    ) -> ThreadTools {
        ThreadTools {
            heard: self.0.clone(),
            thread,
            callers: owner.map(|owner| match owner {
                ThreadOwner::Task(_) => Callers::Worker,
                ThreadOwner::Factory(_) => Callers::Coordinator,
            }),
            delegation,
        }
    }
}

/// One Session's tools, bound to its Thread.
#[derive(Debug, Clone)]
pub struct ThreadTools {
    heard: Sender<link::Heard>,
    thread: ThreadId,
    /// An owned Thread's kind; `None` for the operator's own.
    callers: Option<Callers>,
    /// The Delegate tools are offered: Linked machines was on when its
    /// Session started.
    delegation: bool,
}

/// One tool, as a provider is told of it.
pub struct Definition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// What a call answers: the Server's words, or its refusal.
pub struct Answer {
    pub text: String,
    pub is_error: bool,
}

/// The Server's refusal of a request: its code and its words.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Refusal {
    code: String,
    message: String,
}

impl Refusal {
    fn failed(message: impl Into<String>) -> Self {
        Self {
            code: "failed".into(),
            message: message.into(),
        }
    }
}

impl ThreadTools {
    /// An owned Thread's operations, by their own names.
    fn operations(&self) -> impl Iterator<Item = &'static Operation> + '_ {
        Method::OPERATIONS.iter().filter(move |operation| {
            self.callers.is_some_and(|callers| {
                operation.callers == callers
                    || (callers == Callers::Coordinator
                        && COORDINATOR_READS.contains(&operation.name))
            })
        })
    }

    /// What the agent is told of these tools beside them: the PR block,
    /// and while the Delegate tools are offered, T3's orchestration block.
    pub fn instructions(&self) -> String {
        match self.delegation {
            true => format!("{PR_INSTRUCTIONS}\n\n{}", delegation::INSTRUCTIONS),
            false => PR_INSTRUCTIONS.to_string(),
        }
    }

    pub fn definitions(&self) -> Vec<Definition> {
        let pr = PR_TOOLS.iter().map(|tool| Definition {
            name: tool.name.to_string(),
            description: tool.description.to_string(),
            input_schema: pr_schema(tool.operation == "pr.watch"),
        });
        self.operations()
            .map(|operation| Definition {
                name: tool_name(operation),
                description: format!(
                    "{} The same as `ferrite {} {}`{}, with no permission prompt: use this \
                     rather than the CLI.",
                    operation.summary,
                    operation.words().0,
                    operation.words().1,
                    match Some(operation.callers) == self.callers {
                        true => ", about your own Thread",
                        false => "",
                    },
                ),
                input_schema: input_schema(operation),
            })
            .chain(pr)
            .chain(self.delegation.then(delegation::definitions).into_iter().flatten())
            .map(|mut definition| {
                close(&mut definition.input_schema);
                definition
            })
            .collect()
    }

    /// Whether `tool` (its name inside Ferrite's server) is one of these.
    pub fn offers(&self, tool: &str) -> bool {
        self.delegates(tool) || self.operation(tool).is_some()
    }

    fn delegates(&self, tool: &str) -> bool {
        self.delegation && delegation::NAMES.contains(&tool)
    }

    fn operation(&self, tool: &str) -> Option<&'static Operation> {
        let pr = PR_TOOLS
            .iter()
            .find(|pr| pr.name == tool)
            .and_then(|pr| Method::OPERATIONS.iter().find(|op| op.name == pr.operation));
        pr.or_else(|| {
            self.operations()
                .find(|operation| tool_name(operation) == tool)
        })
    }

    /// Ask the Server, off this thread, and `respond` with its answer.
    /// `main`: Main made the call, not a subagent, whose PR its parent
    /// owns (T3 Code refuses a subagent's watch). A subagent's Delegate
    /// tools act for its Thread, as T3's do.
    pub fn call(
        &self,
        tool: &str,
        arguments: &Value,
        main: bool,
        respond: impl FnOnce(Answer) + Send + 'static,
    ) {
        let Some(definition) = self.definitions().into_iter().find(|d| d.name == tool) else {
            return respond(refused(format!("Ferrite offers no tool {tool}")));
        };
        let arguments = match checked(tool, &definition.input_schema, arguments) {
            Ok(arguments) => arguments,
            Err(refusal) => return respond(refused(refusal)),
        };
        if self.delegates(tool) {
            let (heard, thread, tool) = (self.heard.clone(), self.thread, tool.to_string());
            std::thread::spawn(move || {
                let ask = |method| ask(&heard, thread, method);
                respond(delegation::call(&tool, &arguments, thread, &ask))
            });
            return;
        }
        let Some(operation) = self.operation(tool) else {
            return respond(refused(format!("Ferrite offers no tool {tool}")));
        };
        if operation.name == "pr.watch" && !main {
            return respond(refused(SUBAGENT_WATCH.into()));
        }
        let request: Request = match serde_json::from_value(json!({
            "id": "1",
            "method": operation.name,
            "params": &arguments,
            "caller": self.thread,
        })) {
            Ok(request) => request,
            Err(error) => return respond(refused(error.to_string())),
        };
        let answer = match send(&self.heard, request) {
            Ok(answer) => answer,
            Err(refusal) => return respond(refused(refusal)),
        };
        std::thread::spawn(move || {
            respond(match answer.recv_timeout(ANSWER) {
                Ok(line) => answered(&line),
                Err(_) => refused(UNANSWERED.into()),
            })
        });
    }
}

const UNANSWERED: &str = "the Ferrite Server did not answer";

/// Put `request` to the Server as a caller of its own: where its one
/// answer line comes.
fn send(heard: &Sender<link::Heard>, request: Request) -> Result<Receiver<Vec<u8>>, String> {
    let caller: CallerId = NEXT_CALLER.fetch_add(1, Ordering::Relaxed);
    let (answers, answer) = mpsc::channel();
    let asked = [
        Heard::Joined(caller, answers),
        Heard::Request(caller, request),
        Heard::Hungup(caller),
    ]
    .into_iter()
    .all(|told| heard.send(link::Heard::Api(told)).is_ok());
    match asked {
        true => Ok(answer),
        false => Err("the Ferrite Server is not running".into()),
    }
}

/// Ask the Server `method` from `thread` and wait for its `result`, or
/// its refusal.
fn ask(heard: &Sender<link::Heard>, thread: ThreadId, method: Method) -> Result<Value, Refusal> {
    let request = Request {
        id: "1".into(),
        method,
        caller: Some(thread),
    };
    let line = send(heard, request)
        .map_err(Refusal::failed)?
        .recv_timeout(ANSWER)
        .map_err(|_| Refusal::failed(UNANSWERED))?;
    let mut value: Value = serde_json::from_slice(&line).unwrap_or_default();
    match value.get("error") {
        Some(error) => Err(Refusal {
            code: error["code"].as_str().unwrap_or("failed").into(),
            message: error["message"].as_str().unwrap_or_default().into(),
        }),
        None => Ok(value["result"].take()),
    }
}

/// How many unknown names a refusal says, and how much of each (T3's).
const NAMED_ARGUMENTS: usize = 5;
const ARGUMENT_NAME_CHARS: usize = 64;

/// `arguments` as the tool's `schema` takes them, else T3 Code's words for
/// a call it does not take (`McpHttpServer.ts`' `advertisedArgumentsOnly`,
/// then the schema's own checks): an object names only the properties its
/// schema lists, a value is of its type and among its `enum`, and an
/// object sent as JSON text, as some providers send one, is read as the
/// object. A `null` is a value left out.
fn checked(tool: &str, schema: &Value, arguments: &Value) -> Result<Value, String> {
    let mut arguments = match arguments {
        Value::Null => json!({}),
        arguments => arguments.clone(),
    };
    let refused = |problem: String| format!("Invalid parameters for tool '{tool}': {problem}");
    if !arguments.is_object() {
        return Err(refused(format!("Expected an object, got {arguments}.")));
    }
    match problem(schema, &mut arguments, "") {
        Some(problem) => Err(refused(problem)),
        None => Ok(arguments),
    }
}

/// What is wrong with `value`, named `path` in its call, by `schema`.
fn problem(schema: &Value, value: &mut Value, path: &str) -> Option<String> {
    if value.is_null() {
        return None;
    }
    if let (Some("object"), Value::String(text)) = (schema["type"].as_str(), &*value) {
        if let Ok(object @ Value::Object(_)) = serde_json::from_str::<Value>(text) {
            *value = object;
        }
    }
    let at = path.trim_end_matches('.');
    if let Some(expected) = mistyped(schema, value) {
        return Some(format!("Expected {expected}, got {value} at {at}."));
    }
    if let Some(allowed) = schema["enum"].as_array().filter(|allowed| !allowed.contains(value)) {
        let allowed: Vec<String> = allowed.iter().map(Value::to_string).collect();
        return Some(format!("Expected {}, got {value} at {at}.", allowed.join(" | ")));
    }
    let (Some(properties), Value::Object(given)) = (schema["properties"].as_object(), value) else {
        return None;
    };
    if schema["additionalProperties"] == false {
        let unknown: Vec<&String> = given.keys().filter(|key| !properties.contains_key(*key)).collect();
        if !unknown.is_empty() {
            return Some(unknown_arguments(&unknown, properties, path));
        }
    }
    given
        .iter_mut()
        .find_map(|(key, each)| problem(&properties[key.as_str()], each, &format!("{path}{key}.")))
}

/// T3's refusal of names its schema does not list.
fn unknown_arguments(unknown: &[&String], properties: &Map<String, Value>, path: &str) -> String {
    let named: Vec<String> = unknown
        .iter()
        .take(NAMED_ARGUMENTS)
        .map(|key| {
            let key: String = format!("{path}{key}").chars().take(ARGUMENT_NAME_CHARS).collect();
            Value::String(key).to_string()
        })
        .collect();
    let more = match unknown.len() - named.len() {
        0 => String::new(),
        more => format!(" and {more} more"),
    };
    let accepted: Vec<&str> = properties.keys().map(String::as_str).collect();
    format!(
        "Unknown argument{} {}{more}. Accepted arguments: {}.",
        if unknown.len() == 1 { "" } else { "s" },
        named.join(", "),
        if accepted.is_empty() { "none".to_string() } else { accepted.join(", ") },
    )
}

/// The JSON type `schema` names that `value` is not, if any.
fn mistyped(schema: &Value, value: &Value) -> Option<String> {
    let types: Vec<&str> = match &schema["type"] {
        Value::String(one) => vec![one.as_str()],
        Value::Array(many) => many.iter().filter_map(Value::as_str).collect(),
        _ => return None,
    };
    let is = |kind: &str| match kind {
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "null" => value.is_null(),
        _ => true,
    };
    (!types.iter().any(|kind| is(kind))).then(|| types.join(" | "))
}

/// Every object a tool's schema describes names all it takes:
/// `additionalProperties: false`, which [`checked`] holds a call to.
fn close(schema: &mut Value) {
    match schema {
        Value::Object(map) => {
            if map.contains_key("properties") {
                map.entry("additionalProperties").or_insert(false.into());
            }
            map.values_mut().for_each(close);
        }
        Value::Array(items) => items.iter_mut().for_each(close),
        _ => {}
    }
}

/// A PR tool's parameters, in T3 Code's words: the PR, and for a watch
/// how long it keeps the Thread working.
fn pr_schema(watch: bool) -> Value {
    let mut properties = json!({
        "url": { "type": "string" },
        "repository": {
            "type": "string",
            "description": "With `number`, when no `url`.",
        },
        "number": {
            "type": "integer",
            "minimum": 1,
            "description": "With `repository`, when no `url`.",
        },
    });
    if watch {
        properties["until"] = json!({
            "type": "string",
            "enum": ["checks", "merged"],
            "description": "How long this thread stays working. Omit to keep the current value; a new watch starts at \"checks\". \"checks\": until checks finish, for a PR you hand to the user. \"merged\": until it merges, closes or is unwatched, when you act on it yourself, such as merging after approval.",
        });
    }
    json!({ "type": "object", "properties": properties })
}

/// `report.outcome` → `report_outcome`.
fn tool_name(operation: &Operation) -> String {
    operation.name.replace('.', "_")
}

/// The operation's parameters, every definition inlined: a tool's schema
/// is read whole.
fn input_schema(operation: &Operation) -> Value {
    let mut schema = serde_json::to_value((operation.params)()).unwrap_or_default();
    let defs = schema
        .as_object_mut()
        .and_then(|schema| schema.remove("$defs"))
        .unwrap_or_default();
    inline(&mut schema, &defs);
    if let Some(schema) = schema.as_object_mut() {
        schema.remove("$schema");
        schema.remove("title");
    }
    schema
}

fn inline(value: &mut Value, defs: &Value) {
    if let Some(name) = value["$ref"]
        .as_str()
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
    {
        *value = defs[name].clone();
        inline(value, defs);
        return;
    }
    match value {
        Value::Object(map) => map.values_mut().for_each(|each| inline(each, defs)),
        Value::Array(items) => items.iter_mut().for_each(|each| inline(each, defs)),
        _ => {}
    }
}

fn refused(text: String) -> Answer {
    Answer {
        text,
        is_error: true,
    }
}

/// The Server's answer line as the agent reads it.
fn answered(line: &[u8]) -> Answer {
    let value: Value = serde_json::from_slice(line).unwrap_or_default();
    if let Some(error) = value.get("error") {
        return refused(format!(
            "Refused ({}): {}",
            error["code"].as_str().unwrap_or("failed"),
            error["message"].as_str().unwrap_or_default()
        ));
    }
    let result = &value["result"];
    let report = result.get("changed").is_some();
    let text = match (result["said"].as_str(), result["task"].as_object()) {
        (Some(said), _) => said.to_string(),
        (None, Some(task)) if report => format!(
            "Task {} is {}{}.",
            task["id"].as_str().unwrap_or_default(),
            task["status"].as_str().unwrap_or_default(),
            match result["changed"] == false {
                true => "; your report changed nothing",
                false => "",
            }
        ),
        _ => result.to_string(),
    };
    Answer {
        text,
        is_error: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools(owner: ThreadOwner) -> (ThreadTools, mpsc::Receiver<link::Heard>) {
        let (heard, hear) = mpsc::channel();
        (Tools::new(heard).for_thread(ThreadId::new(7), Some(&owner), false), hear)
    }

    #[test]
    fn a_worker_is_offered_its_reports_and_a_coordinator_its_proposal_and_reads() {
        let names = |tools: &ThreadTools| {
            tools
                .definitions()
                .into_iter()
                .map(|definition| definition.name)
                .collect::<Vec<_>>()
        };
        let (worker, _) = tools(ThreadOwner::Task("t-1".into()));
        assert_eq!(
            names(&worker),
            [
                "report_outcome",
                "report_question",
                "report_revision",
                "report_evidence",
                "watch_pull_request",
                "unwatch_pull_request"
            ]
        );
        let schema = &worker.definitions()[0].input_schema;
        assert!(
            schema.to_string().contains("head_changed") && !schema.to_string().contains("$ref"),
            "the outcome's words are inlined: {schema}"
        );
        assert!(!schema["properties"]
            .as_object()
            .unwrap()
            .contains_key("worker"));
        let (coordinator, _) = tools(ThreadOwner::Factory(
            serde_json::from_value(json!(3)).unwrap(),
        ));
        assert_eq!(
            names(&coordinator),
            [
                "task_list",
                "task_get",
                "workflow_list",
                "workflow_get",
                "proposal_raise",
                "proposal_list",
                "watch_pull_request",
                "unwatch_pull_request"
            ]
        );
        assert!(!coordinator.offers("report_outcome"));
        assert!(!coordinator.offers("workflow_save"), "reads, not acts");
        let briefing = crate::factory::briefing(serde_json::from_value(json!(3)).unwrap());
        let factory_tools = names(&coordinator)
            .into_iter()
            .filter(|tool| !tool.ends_with("_pull_request"));
        for tool in factory_tools {
            assert!(
                briefing.contains(&format!("`{tool}`")),
                "{tool}: {briefing}"
            );
        }
    }

    /// A call is an API request from the bound Thread: the caller is the
    /// Session's own, never one the agent names.
    #[test]
    fn a_call_is_a_request_from_the_sessions_own_thread() {
        let (worker, hear) = tools(ThreadOwner::Task("t-1".into()));
        let (told, said) = mpsc::channel();
        worker.call(
            "report_outcome",
            &json!({ "outcome": "finished", "summary": "Done" }),
            true,
            move |answer| told.send((answer.text, answer.is_error)).unwrap(),
        );
        let Ok(link::Heard::Api(Heard::Joined(caller, answers))) = hear.recv() else {
            panic!("joined first");
        };
        let Ok(link::Heard::Api(Heard::Request(asked, request))) = hear.recv() else {
            panic!("then asked");
        };
        assert_eq!((asked, request.caller), (caller, Some(ThreadId::new(7))));
        assert_eq!(request.method.name(), "report.outcome");
        answers
            .send(
                br#"{"id":"1","result":{"task":{"id":"t-1","status":"completed"},"changed":true}}"#
                    .to_vec(),
            )
            .unwrap();
        assert_eq!(
            said.recv_timeout(Duration::from_secs(5)).unwrap(),
            ("Task t-1 is completed.".to_string(), false)
        );
    }

    /// A read answers the Task whole; only a report's answer is said as
    /// the Task's state.
    #[test]
    fn a_read_answers_the_task_whole() {
        let read = answered(
            br#"{"id":"1","result":{"task":{"id":"t-1","status":"working","reason":null}}}"#,
        );
        assert!(read.text.contains("\"reason\""), "{}", read.text);
    }

    /// Every Thread watches PRs with T3 Code's tools and words; a
    /// subagent's watch is refused before the Server hears it, as T3
    /// refuses one, while its unwatch goes.
    #[test]
    fn a_subagents_watch_is_refused_without_asking() {
        let (heard, hear) = mpsc::channel();
        let tools = Tools::new(heard).for_thread(ThreadId::new(7), None, false);
        let definitions = tools.definitions();
        let names: Vec<&str> = definitions.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["watch_pull_request", "unwatch_pull_request"]);
        assert!(definitions[0].description.starts_with(
            "Watch an open PR for this thread, linking it if needed. Ferrite wakes you"
        ));
        assert_eq!(
            definitions[0].input_schema["properties"]["until"]["enum"],
            json!(["checks", "merged"])
        );
        let (told, said) = mpsc::channel();
        let pr = json!({ "url": "https://github.com/acme/app/pull/7" });
        tools.call("watch_pull_request", &pr, false, move |answer| {
            told.send((answer.text, answer.is_error)).unwrap()
        });
        assert_eq!(said.recv().unwrap(), (SUBAGENT_WATCH.to_string(), true));
        assert!(hear.try_recv().is_err(), "nothing reached the Server");

        tools.call("unwatch_pull_request", &pr, false, |_| {});
        let Ok(link::Heard::Api(Heard::Joined(..))) = hear.recv() else {
            panic!("joined first");
        };
        let Ok(link::Heard::Api(Heard::Request(_, request))) = hear.recv() else {
            panic!("then asked");
        };
        assert_eq!(
            (request.method.name(), request.caller),
            ("pr.unwatch", Some(ThreadId::new(7)))
        );
    }

    #[test]
    fn a_malformed_call_is_refused_without_asking() {
        let (worker, hear) = tools(ThreadOwner::Task("t-1".into()));
        let (told, said) = mpsc::channel();
        worker.call(
            "report_outcome",
            &json!({ "outcome": "maybe" }),
            true,
            move |answer| told.send(answer.is_error).unwrap(),
        );
        assert!(said.recv().unwrap());
        assert!(hear.try_recv().is_err(), "nothing reached the Server");
    }

    /// Every tool a Thread can be offered, of every kind of Thread.
    fn every_tool() -> Vec<Definition> {
        let (heard, _) = mpsc::channel();
        let tools = Tools::new(heard);
        let worker = ThreadOwner::Task("t-1".into());
        let coordinator = ThreadOwner::Factory(serde_json::from_value(json!(3)).unwrap());
        [Some(&worker), Some(&coordinator), None]
            .into_iter()
            .flat_map(|owner| tools.for_thread(ThreadId::new(7), owner, true).definitions())
            .collect()
    }

    /// Each object a schema describes, and where.
    fn objects<'a>(schema: &'a Value, at: String, found: &mut Vec<(String, &'a Value)>) {
        match schema {
            Value::Object(map) => {
                if map.contains_key("properties") {
                    found.push((at.clone(), schema));
                }
                for (key, each) in map {
                    objects(each, format!("{at}/{key}"), found);
                }
            }
            Value::Array(items) => {
                for (index, each) in items.iter().enumerate() {
                    objects(each, format!("{at}/{index}"), found);
                }
            }
            _ => {}
        }
    }

    /// Every tool's schema, and each object in it, names all it takes.
    #[test]
    fn every_tools_schema_names_all_it_takes() {
        let tools = every_tool();
        assert!(tools.iter().any(|tool| tool.name == "delegate_task"));
        assert!(tools.iter().any(|tool| tool.name == "report_outcome"));
        assert!(tools.iter().any(|tool| tool.name == "proposal_raise"));
        for tool in &tools {
            assert_eq!(tool.input_schema["type"], "object", "{}", tool.name);
            assert_eq!(tool.input_schema["additionalProperties"], false, "{}", tool.name);
            let mut found = Vec::new();
            objects(&tool.input_schema, tool.name.clone(), &mut found);
            for (at, object) in found {
                assert_eq!(object["additionalProperties"], false, "{at}");
            }
        }
    }

    /// What `tool` answers `arguments` with, and whether it asked the Server.
    fn answer(tools: &ThreadTools, hear: &mpsc::Receiver<link::Heard>, tool: &str, arguments: Value) -> (String, bool) {
        let (told, said) = mpsc::channel();
        tools.call(tool, &arguments, true, move |answer| {
            told.send(answer.text).unwrap()
        });
        let text = said.recv_timeout(Duration::from_secs(5)).unwrap();
        (text, hear.try_recv().is_ok())
    }

    /// A call naming what its tool does not take, or a value its schema
    /// rules out, is refused in T3's words before the Server hears it:
    /// one check for every tool.
    #[test]
    fn a_call_naming_what_its_tool_does_not_take_is_refused_without_asking() {
        let (heard, hear) = mpsc::channel();
        let worker = ThreadOwner::Task("t-1".into());
        let tools = Tools::new(heard).for_thread(ThreadId::new(7), Some(&worker), true);
        let there = json!({ "machineId": "pc" });
        for (tool, arguments, said) in [
            (
                "report_outcome",
                json!({ "outcome": "finished", "summary": "Done", "worker": 8 }),
                "Invalid parameters for tool 'report_outcome': Unknown argument \"worker\". Accepted arguments: outcome, summary.",
            ),
            (
                "delegate_task",
                json!({ "task": "x", "target": there, "bogus": 1, "later": true }),
                "Invalid parameters for tool 'delegate_task': Unknown arguments \"bogus\", \"later\". Accepted arguments: task, target, title, role, isolation, mode, timeoutMs, clientRequestId, runtimeMode, interactionMode.",
            ),
            (
                "delegate_task",
                json!({ "task": "x", "target": { "machineId": "pc", "environmentId": "pc" } }),
                "Invalid parameters for tool 'delegate_task': Unknown argument \"target.environmentId\". Accepted arguments: providerInstanceId, driverKind, model, options, machineId, projectId.",
            ),
            (
                "delegate_task",
                json!({ "task": "x", "target": there, "mode": "later" }),
                "Invalid parameters for tool 'delegate_task': Expected \"async\" | \"wait\", got \"later\" at mode.",
            ),
            (
                "delegate_task",
                json!({ "task": "x", "target": there, "timeoutMs": "soon" }),
                "Invalid parameters for tool 'delegate_task': Expected number, got \"soon\" at timeoutMs.",
            ),
            (
                "machine_list",
                json!({ "reach": false }),
                "Invalid parameters for tool 'machine_list': Unknown argument \"reach\". Accepted arguments: none.",
            ),
            (
                "watch_pull_request",
                json!({ "number": "7" }),
                "Invalid parameters for tool 'watch_pull_request': Expected integer, got \"7\" at number.",
            ),
        ] {
            assert_eq!(answer(&tools, &hear, tool, arguments), (said.to_string(), false), "{tool}");
        }
    }

    /// A target sent as JSON text, as some providers send one, is the
    /// object it says (T3 decodes both).
    #[test]
    fn a_target_sent_as_json_text_is_read_as_the_object() {
        let (heard, hear) = mpsc::channel();
        let tools = Tools::new(heard).for_thread(ThreadId::new(7), None, true);
        let target = r#"{"machineId":"pc","model":"opus"}"#;
        tools.call("delegate_task", &json!({ "task": "x", "target": target }), true, |_| {});
        let asked = || loop {
            match hear.recv_timeout(Duration::from_secs(5)) {
                Ok(link::Heard::Api(Heard::Joined(_, answers))) => {
                    let Ok(link::Heard::Api(Heard::Request(_, request))) = hear.recv() else {
                        panic!("joined, then asked");
                    };
                    return (request.method, answers);
                }
                Ok(_) => continue,
                Err(error) => panic!("nothing asked: {error}"),
            }
        };
        let (here, answers) = asked();
        assert_eq!(here, Method::MachineList(Default::default()));
        let this = br#"{"id":"1","result":{"server_id":"srv-here","machines":[]}}"#;
        answers.send(this.to_vec()).unwrap();
        let Method::DelegateStart(start) = asked().0 else {
            panic!("then a delegate.start");
        };
        assert_eq!((start.machine.as_str(), start.model.as_deref()), ("pc", Some("opus")));
    }

    /// Every agent is told of its PR tools, and of the Delegate tools and
    /// the linked machines only while it is offered them.
    #[test]
    fn the_agent_is_told_only_of_the_tools_it_has() {
        let (heard, _) = mpsc::channel();
        let tools = Tools::new(heard);
        let told = |delegation| tools.for_thread(ThreadId::new(7), None, delegation).instructions();
        let (plain, linked) = (told(false), told(true));
        assert!(plain.contains("watch_pull_request") && plain.contains("unwatch_pull_request"));
        for absent in ["link_pull_request", "pull_request_create", "ci_logs", "link_issue"] {
            assert!(!linked.contains(absent), "{absent}");
        }
        assert!(!plain.contains("machine_list"));
        assert!(linked.starts_with(&plain) && linked.contains("`machine_list`"));
    }
}
