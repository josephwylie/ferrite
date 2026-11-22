//! The API as a Thread's own tools: every Thread's PR watch
//! ([`PR_TOOLS`], T3 Code's `watch_pull_request` and
//! `unwatch_pull_request`), an owned Thread's `report_*` (a Worker's) and
//! `proposal_raise` (a Coordinator's), one per operation whose callers are
//! that Thread's kind (`schema::Callers`), and a Coordinator's Factory
//! reads ([`COORDINATOR_READS`]). As Ferrite's own tools they are
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

    /// The tools of `thread`, which `owner` owns, if anyone does.
    pub fn for_thread(&self, thread: ThreadId, owner: Option<&ThreadOwner>) -> ThreadTools {
        ThreadTools {
            heard: self.0.clone(),
            thread,
            callers: owner.map(|owner| match owner {
                ThreadOwner::Task(_) => Callers::Worker,
                ThreadOwner::Factory(_) => Callers::Coordinator,
            }),
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
            .collect()
    }

    /// Whether `tool` (its name inside Ferrite's server) is one of these.
    pub fn offers(&self, tool: &str) -> bool {
        self.operation(tool).is_some()
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
    /// owns (T3 Code refuses a subagent's watch).
    pub fn call(
        &self,
        tool: &str,
        arguments: &Value,
        main: bool,
        respond: impl FnOnce(Answer) + Send + 'static,
    ) {
        let Some(operation) = self.operation(tool) else {
            return respond(refused(format!("Ferrite offers no tool {tool}")));
        };
        if operation.name == "pr.watch" && !main {
            return respond(refused(SUBAGENT_WATCH.into()));
        }
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
        (Tools::new(heard).for_thread(ThreadId::new(7), Some(&owner)), hear)
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

    /// A call is an API request from the bound Thread: whatever the agent
    /// writes, the caller is the Session's own.
    #[test]
    fn a_call_is_a_request_from_the_sessions_own_thread() {
        let (worker, hear) = tools(ThreadOwner::Task("t-1".into()));
        let (told, said) = mpsc::channel();
        worker.call(
            "report_outcome",
            &json!({ "outcome": "finished", "summary": "Done", "worker": 8 }),
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
        let tools = Tools::new(heard).for_thread(ThreadId::new(7), None);
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
}
