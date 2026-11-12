//! The API as an owned Thread's own tools: a Worker's `report_*`, a
//! Coordinator's `proposal_raise`, one per operation whose callers are
//! that Thread's kind (`schema::Callers`). As Ferrite's own tools they are
//! pre-allowed beside `update_plan`, never a permission Decision, and a
//! call reaches the Server as an API request whose caller is the Thread
//! its Session was spawned for: the agent never names it.
//!
//! The CLI (`ferrite report …`) stays for an agent without them: a Codex
//! Thread resumed from before they were offered keeps its old tools.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use serde_json::{json, Value};

use super::schema::{Callers, Operation};
use super::{CallerId, Heard, Method, Request};
use crate::link;
use crate::{ThreadId, ThreadOwner};

/// How long a call waits on the Server's answer.
const ANSWER: Duration = Duration::from_secs(30);

/// Tool calls are API callers of their own, numbered apart from the
/// socket's connections.
static NEXT_CALLER: AtomicU64 = AtomicU64::new(1 << 48);

/// The Server's end, where every owned Session's tools are bound.
#[derive(Debug, Clone)]
pub struct Tools(Sender<link::Heard>);

impl Tools {
    /// `heard` is the pump's: a call is heard as an API connection's is.
    pub fn new(heard: Sender<link::Heard>) -> Self {
        Self(heard)
    }

    /// The tools of a Thread `owner` owns, bound to `thread`.
    pub fn for_thread(&self, thread: ThreadId, owner: &ThreadOwner) -> ThreadTools {
        ThreadTools {
            heard: self.0.clone(),
            thread,
            callers: match owner {
                ThreadOwner::Task(_) => Callers::Worker,
                ThreadOwner::Factory(_) => Callers::Coordinator,
            },
        }
    }
}

/// One Session's tools, bound to its Thread.
#[derive(Debug, Clone)]
pub struct ThreadTools {
    heard: Sender<link::Heard>,
    thread: ThreadId,
    callers: Callers,
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

impl ThreadTools {
    fn operations(&self) -> impl Iterator<Item = &'static Operation> + '_ {
        Method::OPERATIONS
            .iter()
            .filter(move |operation| operation.callers == self.callers)
    }

    pub fn definitions(&self) -> Vec<Definition> {
        self.operations()
            .map(|operation| Definition {
                name: tool_name(operation),
                description: format!(
                    "{} The same as `ferrite {} {}`, about your own Thread, with no \
                     permission prompt: use this rather than the CLI.",
                    operation.summary,
                    operation.words().0,
                    operation.words().1,
                ),
                input_schema: input_schema(operation),
            })
            .collect()
    }

    /// Whether `tool` (its name inside Ferrite's server) is one of these.
    pub fn offers(&self, tool: &str) -> bool {
        self.operation(tool).is_some()
    }

    fn operation(&self, tool: &str) -> Option<&'static Operation> {
        self.operations()
            .find(|operation| tool_name(operation) == tool)
    }

    /// Ask the Server, off this thread, and `respond` with its answer.
    pub fn call(
        &self,
        tool: &str,
        arguments: &Value,
        respond: impl FnOnce(Answer) + Send + 'static,
    ) {
        let Some(operation) = self.operation(tool) else {
            return respond(refused(format!("Ferrite offers no tool {tool}")));
        };
        let request: Request = match serde_json::from_value(json!({
            "id": "1",
            "method": operation.name,
            "params": arguments,
            "caller": self.thread,
        })) {
            Ok(request) => request,
            Err(error) => return respond(refused(error.to_string())),
        };
        let caller: CallerId = NEXT_CALLER.fetch_add(1, Ordering::Relaxed);
        let (answers, answer) = mpsc::channel();
        let asked = [
            Heard::Joined(caller, answers),
            Heard::Request(caller, request),
            Heard::Hungup(caller),
        ]
        .into_iter()
        .all(|heard| self.heard.send(link::Heard::Api(heard)).is_ok());
        if !asked {
            return respond(refused("the Ferrite Server is not running".into()));
        }
        std::thread::spawn(move || {
            respond(match answer.recv_timeout(ANSWER) {
                Ok(line) => answered(&line),
                Err(_) => refused("the Ferrite Server did not answer".into()),
            })
        });
    }
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
    let text = match (result["said"].as_str(), result["task"].as_object()) {
        (Some(said), _) => said.to_string(),
        (None, Some(task)) => format!(
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
        (Tools::new(heard).for_thread(ThreadId::new(7), &owner), hear)
    }

    #[test]
    fn a_worker_is_offered_its_reports_and_a_coordinator_its_proposal() {
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
                "report_evidence"
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
        assert_eq!(names(&coordinator), ["proposal_raise"]);
        assert!(!coordinator.offers("report_outcome"));
    }

    /// A call is an API request from the bound Thread: whatever the agent
    /// writes, the caller is the Session's own.
    #[test]
    fn a_call_is_a_request_from_the_sessions_own_thread() {
        let (worker, hear) = tools(ThreadOwner::Task("t-1".into()));
        let (told, said) = mpsc::channel();
        worker.call(
            "report_outcome",
            &json!({ "outcome": "finished", "summary": "Done", "worker": 8 }),
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

    #[test]
    fn a_malformed_call_is_refused_without_asking() {
        let (worker, hear) = tools(ThreadOwner::Task("t-1".into()));
        let (told, said) = mpsc::channel();
        worker.call(
            "report_outcome",
            &json!({ "outcome": "maybe" }),
            move |answer| told.send(answer.is_error).unwrap(),
        );
        assert!(said.recv().unwrap());
        assert!(hear.try_recv().is_err(), "nothing reached the Server");
    }
}
