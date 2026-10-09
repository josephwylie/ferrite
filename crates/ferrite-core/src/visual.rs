//! `show_visual`: the one tool Ferrite itself offers the agents it runs.
//!
//! An agent hands over a self-contained HTML page; the Pane draws it inline
//! in the transcript (the app's off-screen browser) and Ferrite answers the
//! call with a line of text and a PNG screenshot of the render, so the agent
//! can check its own work. Everything a provider needs to offer and answer
//! the tool lives here — its name, schema and description (one place), the
//! operator's setting, the render seam and the answer it becomes — so each
//! provider only adapts its wire (Claude's in-band SDK MCP server, Codex's
//! dynamic tools) and the transcript only folds [`Visual`]s.
//!
//! Persistence needs nothing of its own: a visual is a tool call, and the
//! call's start (its full input, HTML included) and its result are already
//! durable history. Replay rebuilds the same [`Visual`]; the screenshot is
//! never stored.

use std::sync::mpsc::{sync_channel, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The in-band server's name, as both providers namespace the tool with it.
pub const SERVER: &str = "ferrite";
/// The tool's own name inside that namespace.
pub const TOOL: &str = "show_visual";
/// What Claude calls it: SDK MCP tools are `mcp__<server>__<tool>`.
pub const CLAUDE_NAME: &str = "mcp__ferrite__show_visual";
/// What Ferrite calls a Codex dynamic call of it: `<namespace>.<tool>`.
pub const CODEX_NAME: &str = "ferrite.show_visual";

/// How long a render may take before the call is answered with an error.
/// The agent waits on the answer; a stuck renderer must not stall its turn.
pub const RENDER_TIMEOUT: Duration = Duration::from_secs(45);

/// Whether a tool name (as a provider reports the call) is this tool.
pub fn is_tool(name: &str) -> bool {
    name == CLAUDE_NAME || name == CODEX_NAME
}

/// The operator's setting: whether agents are offered the tool, and how
/// readily the description tells them to reach for it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    /// Not offered at all.
    Off,
    /// Offered for when the operator asks to see, mock up or visualise
    /// something.
    #[default]
    WhenAsked,
    /// Offered for that, and for answers that are clearly visual.
    Automatic,
}

impl Level {
    pub const ALL: [Level; 3] = [Level::Off, Level::WhenAsked, Level::Automatic];

    pub fn label(self) -> &'static str {
        match self {
            Level::Off => "Off",
            Level::WhenAsked => "When asked",
            Level::Automatic => "Automatic",
        }
    }
}

/// One `--ferrite-*` CSS custom property a visual may use. The app defines
/// each one on the page's `:root` from Ferrite's live theme; the tool
/// description lists exactly these, so the two cannot drift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    /// The name after `--ferrite-`.
    pub name: &'static str,
    /// What it is for, as the agent reads it.
    pub meaning: &'static str,
}

/// Every token, in the order the description lists them.
pub const TOKENS: &[Token] = &[
    Token {
        name: "bg",
        meaning: "page background (Ferrite's reading plane)",
    },
    Token {
        name: "band",
        meaning: "a raised band / panel / table header",
    },
    Token {
        name: "band2",
        meaning: "a chip, a selected or current item",
    },
    Token {
        name: "fg",
        meaning: "body text",
    },
    Token {
        name: "strong",
        meaning: "titles, emphasised text",
    },
    Token {
        name: "muted",
        meaning: "secondary text: labels, metadata, axes (still readable)",
    },
    Token {
        name: "faint",
        meaning: "structure only (rules, ticks); never text",
    },
    Token {
        name: "border",
        meaning: "1px rules and separators",
    },
    Token {
        name: "border2",
        meaning: "a stronger edge: table head rule, a popover edge",
    },
    Token {
        name: "accent",
        meaning: "the one interactive colour: links, selection, primary action",
    },
    Token {
        name: "on-accent",
        meaning: "text on an accent fill",
    },
    Token {
        name: "float",
        meaning: "a floating surface: tooltip, popover, menu",
    },
    Token {
        name: "shadow",
        meaning: "a floating surface's shadow colour",
    },
    Token {
        name: "ok",
        meaning: "good / passing / added (text-safe)",
    },
    Token {
        name: "warn",
        meaning: "needs attention (text-safe)",
    },
    Token {
        name: "danger",
        meaning: "failed / removed / error (text-safe)",
    },
    Token {
        name: "info",
        meaning: "informational (text-safe)",
    },
    Token {
        name: "red",
        meaning: "terminal red",
    },
    Token {
        name: "green",
        meaning: "terminal green",
    },
    Token {
        name: "yellow",
        meaning: "terminal yellow",
    },
    Token {
        name: "blue",
        meaning: "terminal blue",
    },
    Token {
        name: "magenta",
        meaning: "terminal magenta",
    },
    Token {
        name: "cyan",
        meaning: "terminal cyan",
    },
    Token {
        name: "orange",
        meaning: "terminal orange",
    },
    Token {
        name: "grid",
        meaning: "chart gridlines (quieter than border)",
    },
    Token {
        name: "chart-1",
        meaning: "categorical series 1 (marks only; in order)",
    },
    Token {
        name: "chart-2",
        meaning: "categorical series 2",
    },
    Token {
        name: "chart-3",
        meaning: "categorical series 3",
    },
    Token {
        name: "chart-4",
        meaning: "categorical series 4",
    },
    Token {
        name: "chart-5",
        meaning: "categorical series 5",
    },
    Token {
        name: "chart-6",
        meaning: "categorical series 6",
    },
    Token {
        name: "font-mono",
        meaning: "the one font family (Geist Mono, then system mono)",
    },
    Token {
        name: "font-size",
        meaning: "body text size, follows the operator's reading size",
    },
    Token {
        name: "line",
        meaning: "body line height",
    },
    Token {
        name: "radius",
        meaning: "corner radius: 0, Ferrite is square",
    },
    Token {
        name: "scheme",
        meaning: "`dark` or `light`",
    },
];

/// What the tool is offered as at `level`: its name, description and input
/// schema. `None` when the operator turned it off.
pub fn definition(level: Level) -> Option<Definition> {
    (level != Level::Off).then(|| Definition {
        name: TOOL,
        description: description(level),
        input_schema: input_schema(),
    })
}

/// The tool as one provider-neutral definition.
#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    pub name: &'static str,
    pub description: String,
    pub input_schema: Value,
}

fn input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": {
                "type": "string",
                "description": "A short title shown above the visual (a few words)."
            },
            "caption": {
                "type": "string",
                "description": "Optional one-line caption shown under the title."
            },
            "html": {
                "type": "string",
                "description": "A complete, self-contained HTML document: inline <style> and <script> only, \
                                colours and fonts from the --ferrite-* CSS variables."
            },
            "replace": {
                "type": "boolean",
                "description": "true to replace your previous visual in this conversation (to fix or update it) \
                                instead of adding another."
            }
        },
        "required": ["title", "html"],
        "additionalProperties": false
    })
}

fn description(level: Level) -> String {
    let when = match level {
        Level::Automatic => {
            "Use it when the operator asks to see, mock up, compare or visualise something, AND on your own \
             whenever the answer is clearly visual: UI options or layouts, data with a shape (trends, \
             distributions, breakdowns), structure (architecture, dependencies, flows, trees), timelines, \
             or results worth seeing at a glance."
        }
        _ => {
            "Use it only when the operator asks to see, mock up, compare or visualise something (\"show me\", \
             \"mock up\", \"draw\", \"chart\", \"visualise\")."
        }
    };
    let tokens = TOKENS
        .iter()
        .map(|token| format!("--ferrite-{} ({})", token.name, token.meaning))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Show an interactive HTML visual inline in the operator's transcript in Ferrite (the app they are \
         watching this conversation in): UI mockups, charts and dashboards, diagrams, treemaps, timelines.\n\n\
         When to use: {when}\n\
         When NOT to use: normal coding work, explanations, or anything a short table, list or code block \
         already shows well. Never for decoration.\n\n\
         How:\n\
         - Say one short line about what you are showing before you call it.\n\
         - `html` is one self-contained document: inline <style> and <script> only. No network: no external \
         scripts, stylesheets, fonts, images or fetches (they are blocked).\n\
         - Take every colour and font from these CSS variables, never literal colours, so the visual matches \
         Ferrite's theme (dark or light): {tokens}. Wrap a subtree in data-ferrite-scheme=\"dark\" or \
         \"light\" to pin it to one theme (e.g. to preview a UI in both).\n\
         - Make it interactive where that helps (hover details, tabs, sorting, toggles). It is shown at the \
         transcript's width, so lay it out fluidly; keep it compact (well under a screen tall).\n\
         - The result is a PNG screenshot of the render. Look at it: if something is wrong (overflow, \
         clipping, unreadable contrast, broken layout), call again with replace: true and the fixed html.",
    )
}

/// An agent's visual in the transcript.
#[derive(Clone, Debug, PartialEq)]
pub struct Visual {
    /// The tool call's id — what the call's result quotes.
    pub id: String,
    pub title: String,
    pub caption: Option<String>,
    /// The page. While `Drawing`, whatever of it has streamed so far.
    pub html: String,
    pub status: Status,
    /// The call asked to replace the visual before it.
    pub replace: bool,
    /// A later visual replaced this one; the Pane shows it folded away.
    pub replaced: bool,
}

/// Where a visual is in its life.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// The call's input is still streaming in: `html` is partial.
    Drawing,
    /// The call is complete and Ferrite is rendering it for the agent.
    Checking,
    /// Rendered, and the screenshot went back to the agent.
    Shown,
    /// It could not be shown: a bad input, a render failure, or a turn that
    /// ended first. The message says which.
    Failed(String),
}

impl Visual {
    /// A visual read from the tool's input (complete or partial).
    pub fn from_input(id: impl Into<String>, input: &Value, status: Status) -> Self {
        let text = |key: &str| input.get(key).and_then(Value::as_str).map(str::to_owned);
        Self {
            id: id.into(),
            title: text("title").unwrap_or_default(),
            caption: text("caption").filter(|caption| !caption.trim().is_empty()),
            html: text("html").unwrap_or_default(),
            status,
            replace: input.get("replace").and_then(Value::as_bool) == Some(true),
            replaced: false,
        }
    }

    /// What copy and search see: the title and caption.
    pub fn text(&self) -> String {
        match &self.caption {
            Some(caption) => format!("{}\n{caption}", self.title),
            None => self.title.clone(),
        }
    }
}

/// What the app renders for the agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderRequest {
    /// The tool call's id, so the render can be the same page the Pane is
    /// already showing for this [`Visual`].
    pub id: String,
    pub title: String,
    pub html: String,
}

/// A finished render: a PNG and the size it was taken at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendered {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// The seam the app's browser plugs into. `render` is called on a provider's
/// reader thread and must return at once: do the work elsewhere and answer
/// through `reply`, from any thread, whenever it is done. Ferrite owns the
/// rest — a reply that never comes is answered with an error after
/// [`RENDER_TIMEOUT`], and a dropped `reply` at once — so the agent always
/// hears back exactly once.
pub trait Renderer: Send + Sync {
    fn render(&self, request: RenderRequest, reply: Reply);
}

/// The one answer to one [`RenderRequest`].
pub struct Reply(std::sync::mpsc::SyncSender<Result<Rendered, String>>);

impl Reply {
    /// Answer the request: the render, or why there is none.
    pub fn send(self, result: Result<Rendered, String>) {
        let _ = self.0.send(result);
    }
}

/// What a Session needs to offer the tool: the operator's level and the
/// app's renderer. A Session spawned without one offers nothing.
#[derive(Clone)]
pub struct Visuals {
    pub level: Level,
    pub renderer: Arc<dyn Renderer>,
}

impl std::fmt::Debug for Visuals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Visuals")
            .field("level", &self.level)
            .finish_non_exhaustive()
    }
}

impl Visuals {
    /// The definition to offer, or `None` when the level is Off.
    pub fn definition(&self) -> Option<Definition> {
        definition(self.level)
    }
}

/// How a call is answered, before any provider shapes it for its wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Answer {
    pub text: String,
    pub png: Option<Vec<u8>>,
    pub is_error: bool,
}

impl Answer {
    fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            png: None,
            is_error: true,
        }
    }

    /// A call that arrived with no renderer to answer it (a resumed Codex
    /// thread whose Session now offers no visuals).
    pub(crate) fn unavailable() -> Self {
        Self::error("Ferrite cannot show visuals in this session.")
    }

    /// The PNG as base64, for both providers' image content.
    pub fn png_base64(&self) -> Option<String> {
        use base64::Engine;
        self.png
            .as_ref()
            .map(|png| base64::engine::general_purpose::STANDARD.encode(png))
    }
}

/// Answer one call of the tool: hand its input to the renderer and call
/// `respond` exactly once with the answer, from another thread. Never blocks
/// the caller (a provider's reader thread).
pub(crate) fn answer(
    visuals: &Visuals,
    id: &str,
    input: &Value,
    respond: impl FnOnce(Answer) + Send + 'static,
) {
    answer_within(visuals, id, input, RENDER_TIMEOUT, respond)
}

fn answer_within(
    visuals: &Visuals,
    id: &str,
    input: &Value,
    timeout: Duration,
    respond: impl FnOnce(Answer) + Send + 'static,
) {
    let visual = Visual::from_input(id, input, Status::Checking);
    if visual.html.trim().is_empty() {
        respond(Answer::error(
            "show_visual needs `html`: a complete, self-contained HTML document.",
        ));
        return;
    }
    let (sender, receiver) = sync_channel::<Result<Rendered, String>>(1);
    let waiter = std::thread::Builder::new()
        .name("ferrite-visual".into())
        .spawn(move || {
            let answer = match receiver.recv_timeout(timeout) {
                Ok(Ok(rendered)) => Answer {
                    text: format!(
                        "Shown to the operator inline. Attached: a screenshot of the render at \
                         {}x{} px. Check it; if anything is off, call show_visual again with \
                         replace: true.",
                        rendered.width, rendered.height
                    ),
                    png: Some(rendered.png),
                    is_error: false,
                },
                Ok(Err(error)) => {
                    Answer::error(format!("The visual could not be rendered: {error}"))
                }
                Err(RecvTimeoutError::Timeout) => Answer::error(format!(
                    "The visual did not finish rendering within {}s.",
                    timeout.as_secs()
                )),
                Err(RecvTimeoutError::Disconnected) => {
                    Answer::error("Ferrite cannot render visuals right now.")
                }
            };
            respond(answer);
        });
    if waiter.is_err() {
        // The OS refused a thread: `respond` went with the closure, so the
        // call goes unanswered until the turn is interrupted. Not worth a
        // render nobody can deliver.
        return;
    }
    visuals.renderer.render(
        RenderRequest {
            id: visual.id,
            title: visual.title,
            html: visual.html,
        },
        Reply(sender),
    );
}

/// A streaming tool input's JSON so far, read as a value: an unterminated
/// string is closed where it stops, a key still arriving is dropped, and a
/// key with no value yet reads as null. `None` when even that cannot be
/// read (the prefix is not the start of an object).
pub fn partial_input(prefix: &str) -> Option<Value> {
    serde_json::from_str(&complete_json(prefix)?).ok()
}

/// Close `prefix` into a JSON document (see [`partial_input`]).
fn complete_json(prefix: &str) -> Option<String> {
    #[derive(Clone, Copy, PartialEq)]
    enum Expect {
        Key,
        Colon,
        Value,
        Next,
    }
    struct Frame {
        object: bool,
        expect: Expect,
    }
    let mut stack: Vec<Frame> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    // Where the open string (or a bare literal) began, and whether it is a key.
    let mut token_start = 0;
    let mut string_is_key = false;
    let mut literal_start: Option<usize> = None;
    // Where the last comma at the current depth was, to drop with a dangling key.
    let mut last_comma: Option<usize> = None;
    // Whether the document is an object: the first bracket says.
    let mut root_object: Option<bool> = None;
    let bytes = prefix.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
                if let Some(frame) = stack.last_mut() {
                    frame.expect = if string_is_key {
                        Expect::Colon
                    } else {
                        Expect::Next
                    };
                }
            }
            i += 1;
            continue;
        }
        if literal_start.is_some()
            && !(c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'+'))
        {
            literal_start = None;
            if let Some(frame) = stack.last_mut() {
                frame.expect = Expect::Next;
            }
        }
        match c {
            b'"' => {
                in_string = true;
                token_start = i;
                string_is_key = stack
                    .last()
                    .is_some_and(|f| f.object && f.expect == Expect::Key);
            }
            b'{' | b'[' => {
                root_object.get_or_insert(c == b'{');
                if let Some(frame) = stack.last_mut() {
                    frame.expect = Expect::Next;
                }
                stack.push(Frame {
                    object: c == b'{',
                    expect: if c == b'{' {
                        Expect::Key
                    } else {
                        Expect::Value
                    },
                });
                last_comma = None;
            }
            b'}' | b']' => {
                stack.pop();
                if let Some(frame) = stack.last_mut() {
                    frame.expect = Expect::Next;
                }
            }
            b':' => {
                if let Some(frame) = stack.last_mut() {
                    frame.expect = Expect::Value;
                }
            }
            b',' => {
                last_comma = Some(i);
                if let Some(frame) = stack.last_mut() {
                    frame.expect = if frame.object {
                        Expect::Key
                    } else {
                        Expect::Value
                    };
                }
            }
            c if c.is_ascii_whitespace() => {}
            _ => {
                if literal_start.is_none() {
                    literal_start = Some(i);
                }
            }
        }
        i += 1;
    }
    if root_object != Some(true) {
        return None;
    }
    let mut out = prefix.to_owned();
    if in_string {
        if string_is_key {
            // A key still arriving says nothing yet: drop it, and its comma.
            out.truncate(
                last_comma
                    .filter(|at| *at < token_start)
                    .unwrap_or(token_start),
            );
            if let Some(frame) = stack.last_mut() {
                frame.expect = Expect::Next;
            }
        } else {
            // Drop a half-written escape, then close the string.
            if escaped {
                out.pop();
            } else if let Some(at) = out.rfind("\\u") {
                let tail = &out[at + 2..];
                if at > token_start && tail.len() < 4 && !preceded_by_backslash(&out, at) {
                    out.truncate(at);
                }
            }
            out.push('"');
            if let Some(frame) = stack.last_mut() {
                frame.expect = Expect::Next;
            }
        }
    } else if let Some(start) = literal_start {
        // `tru`, `nul`, `1.` — not a value yet.
        let literal = &prefix[start..];
        if serde_json::from_str::<Value>(literal).is_err() {
            out.truncate(start);
            if let Some(frame) = stack.last_mut() {
                frame.expect = Expect::Value;
            }
        } else if let Some(frame) = stack.last_mut() {
            frame.expect = Expect::Next;
        }
    }
    let trimmed = out.trim_end().len();
    out.truncate(trimmed);
    while let Some(frame) = stack.pop() {
        match frame.expect {
            Expect::Colon => out.push_str(":null"),
            Expect::Value if frame.object => out.push_str("null"),
            _ => {
                if out.ends_with(',') {
                    out.pop();
                }
            }
        }
        out.push(if frame.object { '}' } else { ']' });
    }
    Some(out)
}

fn preceded_by_backslash(text: &str, at: usize) -> bool {
    text[..at].bytes().rev().take_while(|b| *b == b'\\').count() % 2 == 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn off_offers_nothing_and_the_other_levels_differ_only_in_when() {
        assert!(definition(Level::Off).is_none());
        let asked = definition(Level::WhenAsked).unwrap();
        let automatic = definition(Level::Automatic).unwrap();
        assert_eq!(asked.name, "show_visual");
        assert_eq!(asked.input_schema, automatic.input_schema);
        assert_ne!(asked.description, automatic.description);
        assert!(automatic.description.contains("on your own"));
        assert!(!asked.description.contains("on your own"));
    }

    #[test]
    fn the_description_lists_every_token_and_the_rules() {
        let text = definition(Level::Automatic).unwrap().description;
        for token in TOKENS {
            assert!(
                text.contains(&format!("--ferrite-{} (", token.name)),
                "{}",
                token.name
            );
        }
        for rule in [
            "No network",
            "replace: true",
            "When NOT to use",
            "one short line",
            "screenshot",
        ] {
            assert!(text.contains(rule), "{rule}");
        }
    }

    #[test]
    fn both_providers_names_are_the_tool_and_nothing_else_is() {
        assert!(is_tool("mcp__ferrite__show_visual"));
        assert!(is_tool("ferrite.show_visual"));
        for other in ["show_visual", "mcp__other__show_visual", "Bash"] {
            assert!(!is_tool(other), "{other}");
        }
    }

    #[test]
    fn levels_round_trip_as_settings_words() {
        for (level, word) in [
            (Level::Off, "\"off\""),
            (Level::WhenAsked, "\"when-asked\""),
            (Level::Automatic, "\"automatic\""),
        ] {
            assert_eq!(serde_json::to_string(&level).unwrap(), word);
            assert_eq!(serde_json::from_str::<Level>(word).unwrap(), level);
        }
    }

    #[test]
    fn a_partial_input_reads_what_has_arrived() {
        let cases: &[(&str, Value)] = &[
            ("{", json!({})),
            ("{\"ti", json!({})),
            ("{\"title\"", json!({"title": null})),
            ("{\"title\":", json!({"title": null})),
            ("{\"title\": \"Hel", json!({"title": "Hel"})),
            ("{\"title\": \"Hello\", ", json!({"title": "Hello"})),
            ("{\"title\": \"Hello\", \"htm", json!({"title": "Hello"})),
            (
                "{\"title\": \"Hello\", \"html\": \"<p class=\\\"a",
                json!({"title": "Hello", "html": "<p class=\"a"}),
            ),
            ("{\"html\": \"a\\", json!({"html": "a"})),
            ("{\"html\": \"a\\u00", json!({"html": "a"})),
            ("{\"html\": \"a\\u00e9b", json!({"html": "aéb"})),
            ("{\"replace\": tr", json!({"replace": null})),
            ("{\"replace\": true", json!({"replace": true})),
            ("{\"n\": [1, 2", json!({"n": [1, 2]})),
            ("{\"title\": \"a\"}", json!({"title": "a"})),
        ];
        for (prefix, expected) in cases {
            assert_eq!(partial_input(prefix).as_ref(), Some(expected), "{prefix}");
        }
        assert_eq!(partial_input(""), None);
        assert_eq!(partial_input("[1"), None);
    }

    #[test]
    fn every_prefix_of_a_real_input_reads() {
        let full = json!({
            "title": "Q3 \"revenue\"",
            "caption": "by region — é",
            "html": "<style>p{color:var(--ferrite-fg)}</style><p>\\n</p>",
            "replace": true
        })
        .to_string();
        for end in (0..=full.len())
            .filter(|end| full.is_char_boundary(*end))
            .skip(1)
        {
            let value = partial_input(&full[..end]);
            assert!(value.is_some(), "prefix {end}: {:?}", &full[..end]);
        }
        assert_eq!(
            partial_input(&full),
            Some(serde_json::from_str(&full).unwrap())
        );
    }

    struct Scripted(Mutex<Vec<RenderRequest>>, Option<Result<Rendered, String>>);
    impl Renderer for Scripted {
        fn render(&self, request: RenderRequest, reply: Reply) {
            self.0.lock().unwrap().push(request);
            if let Some(result) = self.1.clone() {
                std::thread::spawn(move || reply.send(result));
            }
            // `None`: hold nothing, so the reply drops unanswered.
        }
    }

    fn run(renderer: Scripted, input: Value, timeout: Duration) -> (Answer, Vec<RenderRequest>) {
        let renderer = Arc::new(renderer);
        let visuals = Visuals {
            level: Level::WhenAsked,
            renderer: renderer.clone(),
        };
        let (tx, rx) = std::sync::mpsc::channel();
        answer_within(&visuals, "call-1", &input, timeout, move |answer| {
            tx.send(answer).unwrap()
        });
        let answer = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "answered twice"
        );
        let requests = renderer.0.lock().unwrap().clone();
        (answer, requests)
    }

    #[test]
    fn a_render_answers_with_text_and_the_png() {
        let png = vec![0x89, b'P', b'N', b'G'];
        let (answer, requests) = run(
            Scripted(
                Mutex::default(),
                Some(Ok(Rendered {
                    png: png.clone(),
                    width: 640,
                    height: 300,
                })),
            ),
            json!({"title": "Hi", "html": "<p>hi</p>"}),
            Duration::from_secs(5),
        );
        assert_eq!(
            requests,
            vec![RenderRequest {
                id: "call-1".into(),
                title: "Hi".into(),
                html: "<p>hi</p>".into()
            }]
        );
        assert!(!answer.is_error);
        assert_eq!(answer.png, Some(png));
        assert!(answer.text.contains("640x300"));
        assert_eq!(answer.png_base64().as_deref(), Some("iVBORw=="));
    }

    #[test]
    fn a_failed_render_answers_with_an_error_and_no_image() {
        let (answer, _) = run(
            Scripted(Mutex::default(), Some(Err("engine crashed".into()))),
            json!({"title": "Hi", "html": "<p>hi</p>"}),
            Duration::from_secs(5),
        );
        assert!(answer.is_error);
        assert!(answer.png.is_none());
        assert!(answer.text.contains("engine crashed"));
    }

    #[test]
    fn a_dropped_reply_is_answered_at_once() {
        let (answer, _) = run(
            Scripted(Mutex::default(), None),
            json!({"title": "Hi", "html": "<p>hi</p>"}),
            Duration::from_secs(30),
        );
        assert!(answer.is_error);
    }

    #[test]
    fn a_reply_that_never_comes_times_out() {
        struct Silent(Mutex<Vec<Reply>>);
        impl Renderer for Silent {
            fn render(&self, _: RenderRequest, reply: Reply) {
                self.0.lock().unwrap().push(reply);
            }
        }
        let visuals = Visuals {
            level: Level::Automatic,
            renderer: Arc::new(Silent(Mutex::default())),
        };
        let (tx, rx) = std::sync::mpsc::channel();
        answer_within(
            &visuals,
            "c",
            &json!({"title": "t", "html": "<p>"}),
            Duration::from_millis(50),
            move |answer| tx.send(answer).unwrap(),
        );
        let answer = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(answer.is_error);
        assert!(answer.text.contains("did not finish"));
    }

    #[test]
    fn an_input_without_html_is_refused_without_rendering() {
        let (answer, requests) = run(
            Scripted(Mutex::default(), Some(Err("unused".into()))),
            json!({"title": "Hi"}),
            Duration::from_secs(5),
        );
        assert!(answer.is_error);
        assert!(requests.is_empty());
    }

    #[test]
    fn a_visual_reads_its_input() {
        let visual = Visual::from_input(
            "id",
            &json!({"title": "T", "caption": " ", "html": "<p>", "replace": true}),
            Status::Drawing,
        );
        assert_eq!(visual.title, "T");
        assert_eq!(visual.caption, None);
        assert!(visual.replace);
        assert!(!visual.replaced);
        assert_eq!(visual.text(), "T");
    }
}
