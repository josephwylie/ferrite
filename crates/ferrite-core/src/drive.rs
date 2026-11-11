//! Driving a headless window (ADR 0038): what an API caller asks the
//! `ferrite --headless` window attached to a Server to do (`window.*`),
//! and what the window answers. Each action's parameters are the API
//! method's own; the Server hands them to the window as they came.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One thing for the window to do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    Key(Keys),
    Type(Text),
    Click(Click),
    Find(Find),
    Shot(Shot),
}

/// Keystrokes pressed in order, as gpui writes them: `cmd-,`, `escape`,
/// `shift-tab`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Keys {
    pub keys: Vec<String>,
}

/// Text typed key by key into whatever holds the keyboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Text {
    pub text: String,
}

/// A click: where, and which.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "ClickParams", into = "ClickParams")]
pub struct Click {
    pub at: Target,
    pub press: Press,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// The centre of what the last frame drew for this `debug_selector`.
    Selector(String),
    /// A point in the window's logical pixels, from its top left.
    Point { x: u32, y: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Single,
    Double,
    Right,
}

/// A click as the API and the CLI write it: at a `selector`, or at `x`
/// and `y`.
#[derive(Serialize, Deserialize, JsonSchema)]
#[schemars(extend("oneOf" = [
    { "type": "object", "required": ["selector"], "properties": { "selector": { "type": "string" } } },
    { "type": "object", "required": ["x", "y"], "properties": { "x": { "type": "integer" }, "y": { "type": "integer" } } },
]))]
struct ClickParams {
    /// Click the centre of what the last frame drew for this
    /// `debug_selector` (the stable handle; `window.find` gives its
    /// bounds). Or give `x` and `y`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selector: Option<String>,
    /// With `y`: the point to click, in the window's logical pixels from
    /// its top left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    x: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    y: Option<u32>,
    /// A double click.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    double: bool,
    /// A right click.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    right: bool,
}

impl TryFrom<ClickParams> for Click {
    type Error = String;

    fn try_from(params: ClickParams) -> Result<Self, String> {
        let at = match (params.selector, params.x, params.y) {
            (Some(selector), None, None) => Target::Selector(selector),
            (None, Some(x), Some(y)) => Target::Point { x, y },
            _ => return Err("a click is at a `selector`, or at `x` and `y`".into()),
        };
        let press = match (params.double, params.right) {
            (false, false) => Press::Single,
            (true, false) => Press::Double,
            (false, true) => Press::Right,
            (true, true) => return Err("a click is `double` or `right`, not both".into()),
        };
        Ok(Self { at, press })
    }
}

impl From<Click> for ClickParams {
    fn from(Click { at, press }: Click) -> Self {
        let (selector, x, y) = match at {
            Target::Selector(selector) => (Some(selector), None, None),
            Target::Point { x, y } => (None, Some(x), Some(y)),
        };
        Self {
            selector,
            x,
            y,
            double: press == Press::Double,
            right: press == Press::Right,
        }
    }
}

/// Where the last frame drew an element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Find {
    /// The element's `debug_selector`.
    pub selector: String,
}

/// The window's frame, saved as a PNG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Shot {
    /// Where the window writes it: absolute, on the window's computer.
    /// The CLI reads a relative one from where it runs.
    pub path: PathBuf,
}

/// What the window did. Points and sizes are in its logical pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum Answer {
    /// The keys pressed; `unhandled` are those nothing took.
    Pressed { unhandled: Vec<String> },
    Typed,
    Clicked { x: f32, y: f32 },
    Found { x: f32, y: f32, width: f32, height: f32 },
    /// `width` and `height` in the PNG's pixels, `scale` of them to a
    /// logical pixel.
    Shot { path: PathBuf, width: u32, height: u32, scale: f32 },
}
