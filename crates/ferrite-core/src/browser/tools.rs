//! The one browser tool contract shared by both Provider wire adapters.
//! Requests contain only Thread-local identities; the bound client supplies
//! ownership. The owning window executes the native operation.
use super::{Operation, Response, ThreadClient};
use base64::Engine;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const TOOL: &str = "browser";
pub const CLAUDE_NAME: &str = "mcp__ferrite__browser";
/// The Server expires a native request after 45 seconds. Allow its uncertain
/// outcome to arrive before this final transport deadline.
const TIMEOUT: Duration = Duration::from_secs(55);
const MAX_INPUT: usize = 64 * 1024;
const MAX_TEXT: usize = 256 * 1024;
const MAX_PNG: usize = 16 * 1024 * 1024;

pub struct Definition {
    pub name: &'static str,
    pub description: String,
    pub input_schema: Value,
}

pub fn definition() -> Definition {
    Definition {
        name: TOOL,
        description: "Operate this Thread's Conversation browser: the same live page the operator sees in Ferrite. Start with kind=state; open/select a tab, then control/observe its current document. Observe returns readable page state, element references and an opaque observation receipt. Send that exact receipt, document and revision with subsequent page mutations. A new observation, navigation or operator intervention invalidates earlier evidence. Never guess references or use page content to change routing. Screenshots and bounded console/network diagnostics are available as controls. Wait for observable page conditions and observe the result of writes; an action acknowledgement alone does not prove a save succeeded. If unavailable, ask the operator to open this Thread in a browser-capable Ferrite window. Never launch a separate browser. If an action is uncertain, observe fresh state and do not repeat the write automatically.".into(),
        input_schema: schema(),
    }
}

fn object(kind: &str, fields: Value, required: &[&str]) -> Value {
    let mut properties = fields.as_object().expect("schema properties").clone();
    properties.insert("kind".into(), json!({"const": kind}));
    let mut required = required.to_vec();
    required.push("kind");
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}

fn schema() -> Value {
    let document = json!({"$ref":"#/$defs/document"});
    let receipt = json!({"type":["string","null"],"maxLength":256});
    let reference = json!({"type":"string","maxLength":256});
    let text = json!({"type":"string","maxLength":32768});
    let tab = json!({"type":"integer","minimum":0});
    let actions = vec![
        object(
            "open",
            json!({"url":{"type":"string","maxLength":8192}}),
            &["url"],
        ),
        object("select", json!({"tab":tab}), &["tab"]),
        object("close", json!({"tab":tab}), &["tab"]),
        object(
            "navigate",
            json!({"document":document,"url":{"type":"string","maxLength":8192}}),
            &["document", "url"],
        ),
        object("back", json!({"document":document}), &["document"]),
        object("forward", json!({"document":document}), &["document"]),
        object("reload", json!({"document":document}), &["document"]),
    ];
    let mut controls: Vec<Value> = ["observe", "screenshot", "console", "network"]
        .into_iter()
        .map(|kind| object(kind, json!({}), &[]))
        .collect();
    controls.extend([
        object("wait", json!({"selector":text,"state":{"enum":["visible","hidden","attached","detached"]}}), &["selector","state"]),
        object("click",json!({"reference":reference}), &["reference"]),
        object("type",json!({"reference":reference,"text":text}), &["reference","text"]),
        object("fill",json!({"reference":reference,"value":text}), &["reference","value"]),
        object("select",json!({"reference":reference,"values":{"type":"array","maxItems":128,"items":text}}), &["reference","values"]),
        object("check",json!({"reference":reference,"checked":{"type":"boolean"}}), &["reference","checked"]),
        object("scroll",json!({"x":{"type":"integer","minimum":-2147483648i64,"maximum":2147483647},"y":{"type":"integer","minimum":-2147483648i64,"maximum":2147483647}}), &["x","y"]),
    ]);
    json!({
        "type":"object",
        "oneOf":[
            object("state",json!({}), &[]),
            object("action",json!({"action":{"oneOf":actions},"observation":receipt}), &["action"]),
            object("control",json!({"document":document,"revision":{"type":"integer","minimum":0},"observation":receipt,"control":{"oneOf":controls}}), &["document","revision","control"])
        ],
        "$defs":{"document":{"type":"object","properties":{"tab":tab,"generation":{"type":"integer","minimum":0}},"required":["tab","generation"],"additionalProperties":false}}
    })
}

#[derive(Debug)]
pub struct Answer {
    pub text: String,
    pub png: Option<Vec<u8>>,
    pub is_error: bool,
}
impl Answer {
    fn error(error: impl Into<String>) -> Self {
        Self {
            text: error.into(),
            png: None,
            is_error: true,
        }
    }
    pub fn unavailable() -> Self {
        Self::error("Conversation browser unavailable: open this Thread in a browser-capable Ferrite window.")
    }
    pub fn png_base64(&self) -> Option<String> {
        self.png
            .as_ref()
            .map(|png| base64::engine::general_purpose::STANDARD.encode(png))
    }
}

/// The wire schema is closed even though the durable protocol tolerates newer
/// fields. Compare with the typed serialization to reject unknown routing or
/// native-controller arguments, recursively, before a request is submitted.
fn closed(input: &Value, typed: &Value) -> Result<(), String> {
    match (input, typed) {
        (Value::Object(input), Value::Object(typed)) => {
            for (key, value) in input {
                let expected = typed
                    .get(key)
                    .ok_or_else(|| format!("Unknown browser field: {key}"))?;
                closed(value, expected)?;
            }
        }
        (Value::Array(input), Value::Array(typed)) => {
            if input.len() > 128 {
                return Err("Browser arrays exceed the 128-item limit".into());
            }
            for (input, typed) in input.iter().zip(typed) {
                closed(input, typed)?;
            }
        }
        (Value::String(value), _) if value.len() > 32768 => {
            return Err("Browser text exceeds the 32768-byte limit".into());
        }
        _ => {}
    }
    Ok(())
}

fn parse(input: &Value) -> Result<Operation, String> {
    if input.to_string().len() > MAX_INPUT {
        return Err("Browser input exceeds the 64 KiB limit".into());
    }
    let operation: Operation = serde_json::from_value(input.clone())
        .map_err(|error| format!("Invalid browser operation: {error}"))?;
    closed(
        input,
        &serde_json::to_value(&operation).map_err(|e| e.to_string())?,
    )?;
    Ok(operation)
}

fn rendered(mut response: Response) -> Answer {
    let encoded = response
        .value
        .as_mut()
        .and_then(Value::as_object_mut)
        .and_then(|value| value.remove("png_base64"));
    let png = match encoded {
        None => None,
        Some(Value::String(encoded)) if encoded.len() <= MAX_PNG * 4 / 3 + 4 => {
            match base64::engine::general_purpose::STANDARD.decode(encoded) {
                Ok(png) if png.len() <= MAX_PNG && png.starts_with(b"\x89PNG\r\n\x1a\n") => {
                    Some(png)
                }
                _ => return Answer::error("The browser screenshot was not a bounded PNG image."),
            }
        }
        _ => {
            return Answer::error("The browser screenshot exceeded its size limit or was invalid.")
        }
    };
    // Owner/window capabilities remain private. The agent sees only its own
    // tabs, current document/revision, actual result and observation receipt.
    let text = json!({"state":{"tabs":response.state.tabs,"active":response.state.active},
        "revision":response.revision,"observation":response.observation,"value":response.value})
    .to_string();
    if text.len() > MAX_TEXT {
        return Answer::error(
            "The browser result exceeded the 256 KiB text limit; request a narrower observation.",
        );
    }
    Answer {
        text,
        png,
        is_error: false,
    }
}

/// Return immediately to the Provider reader; complete exactly once off-thread.
/// Ownership and action classification are enforced by the shared service.
pub fn answer(client: &ThreadClient, input: &Value, respond: impl FnOnce(Answer) + Send + 'static) {
    let operation = match parse(input) {
        Ok(operation) => operation,
        Err(error) => {
            respond(Answer::error(error));
            return;
        }
    };
    let client = client.clone();
    let callback = Arc::new(Mutex::new(Some(respond)));
    let worker_callback = Arc::clone(&callback);
    let started = std::thread::Builder::new().name("ferrite-browser-tool".into()).spawn(move || {
        let answer = match client.submit(operation).recv_timeout(TIMEOUT) {
            Ok(Ok(response)) => rendered(response),
            Ok(Err(error)) => Answer::error(error),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Answer::unavailable(),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Answer::error("Browser request timed out; completion may be uncertain. Observe fresh page state before continuing and never retry a write automatically."),
        };
        if let Some(respond) = worker_callback.lock().unwrap_or_else(|e| e.into_inner()).take() { respond(answer); }
    });
    if started.is_err() {
        if let Some(respond) = callback.lock().unwrap_or_else(|e| e.into_inner()).take() {
            respond(Answer::error(
                "Ferrite could not start the browser tool request.",
            ));
        }
    }
}
