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
    Attach(Attach),
    Resize(Resize),
    Drag(Drag),
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
    /// Held with the platform's command key (⌘ on macOS).
    pub command: bool,
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
    /// Hold the command key (⌘ on macOS) through the click.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    command: bool,
}

impl TryFrom<ClickParams> for Click {
    type Error = String;

    fn try_from(params: ClickParams) -> Result<Self, String> {
        let at = target(params.selector, params.x, params.y)
            .ok_or("a click is at a `selector`, or at `x` and `y`")?;
        let press = match (params.double, params.right) {
            (false, false) => Press::Single,
            (true, false) => Press::Double,
            (false, true) => Press::Right,
            (true, true) => return Err("a click is `double` or `right`, not both".into()),
        };
        Ok(Self {
            at,
            press,
            command: params.command,
        })
    }
}

impl From<Click> for ClickParams {
    fn from(Click { at, press, command }: Click) -> Self {
        let (selector, x, y) = at.into_params();
        Self {
            selector,
            x,
            y,
            double: press == Press::Double,
            right: press == Press::Right,
            command,
        }
    }
}

/// A selector alone, or `x` and `y` together; nothing else is a target.
fn target(selector: Option<String>, x: Option<u32>, y: Option<u32>) -> Option<Target> {
    match (selector, x, y) {
        (Some(selector), None, None) => Some(Target::Selector(selector)),
        (None, Some(x), Some(y)) => Some(Target::Point { x, y }),
        _ => None,
    }
}

impl Target {
    fn into_params(self) -> (Option<String>, Option<u32>, Option<u32>) {
        match self {
            Target::Selector(selector) => (Some(selector), None, None),
            Target::Point { x, y } => (None, Some(x), Some(y)),
        }
    }
}

/// The window's new size in logical pixels, as dragging its corner makes
/// it: never under the least size the app's window keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Resize {
    pub width: u32,
    pub height: u32,
}

/// The left button pressed at `from`, moved to `to` in steps while held,
/// and let go there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "DragParams", into = "DragParams")]
pub struct Drag {
    pub from: Target,
    pub to: Target,
}

/// A drag as the API and the CLI write it: each end at a selector, or at
/// a point.
#[derive(Serialize, Deserialize, JsonSchema)]
#[schemars(extend("oneOf" = [
    { "type": "object", "required": ["from_selector", "to_selector"],
      "properties": { "from_selector": { "type": "string" }, "to_selector": { "type": "string" } } },
    { "type": "object", "required": ["from_selector", "to_x", "to_y"],
      "properties": { "from_selector": { "type": "string" }, "to_x": { "type": "integer" }, "to_y": { "type": "integer" } } },
    { "type": "object", "required": ["from_x", "from_y", "to_selector"],
      "properties": { "from_x": { "type": "integer" }, "from_y": { "type": "integer" }, "to_selector": { "type": "string" } } },
    { "type": "object", "required": ["from_x", "from_y", "to_x", "to_y"],
      "properties": { "from_x": { "type": "integer" }, "from_y": { "type": "integer" }, "to_x": { "type": "integer" }, "to_y": { "type": "integer" } } },
]))]
struct DragParams {
    /// Press at the centre of what the last frame drew for this
    /// `debug_selector`. Or give `from_x` and `from_y`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    from_selector: Option<String>,
    /// With `from_y`: the point to press, in the window's logical pixels
    /// from its top left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    from_x: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    from_y: Option<u32>,
    /// Let go at the centre of what the last frame drew for this
    /// `debug_selector`. Or give `to_x` and `to_y`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    to_selector: Option<String>,
    /// With `to_y`: the point to let go at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    to_x: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    to_y: Option<u32>,
}

impl TryFrom<DragParams> for Drag {
    type Error = String;

    fn try_from(params: DragParams) -> Result<Self, String> {
        let from = target(params.from_selector, params.from_x, params.from_y)
            .ok_or("a drag is from a `from_selector`, or from `from_x` and `from_y`")?;
        let to = target(params.to_selector, params.to_x, params.to_y)
            .ok_or("a drag is to a `to_selector`, or to `to_x` and `to_y`")?;
        Ok(Self { from, to })
    }
}

impl From<Drag> for DragParams {
    fn from(Drag { from, to }: Drag) -> Self {
        let (from_selector, from_x, from_y) = from.into_params();
        let (to_selector, to_x, to_y) = to.into_params();
        Self {
            from_selector,
            from_x,
            from_y,
            to_selector,
            to_x,
            to_y,
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

/// Local files staged through the focused Composer's normal attachment path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Attach {
    /// Absolute paths on the computer running the headless window.
    pub paths: Vec<PathBuf>,
}

/// What the window did. Points and sizes are in its logical pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum Answer {
    /// The keys pressed; `unhandled` are those nothing took.
    Pressed { unhandled: Vec<String> },
    Typed,
    Attached,
    Clicked { x: f32, y: f32 },
    Found { x: f32, y: f32, width: f32, height: f32 },
    /// `width` and `height` in the PNG's pixels, `scale` of them to a
    /// logical pixel.
    Shot { path: PathBuf, width: u32, height: u32, scale: f32 },
    /// The size the window has now.
    Resized { width: f32, height: f32 },
    /// Where the button went down and where it came up.
    Dragged { from_x: f32, from_y: f32, to_x: f32, to_y: f32 },
}
