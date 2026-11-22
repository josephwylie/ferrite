//! A closed UI handshake for the real provider boundary scenario. All page
//! actions remain provider browser calls; this helper only supplies operator
//! pointer gestures and the actual Reload toolbar action. While a hold is
//! asked, each tick adds a line to the hold trace (`hold_trace`), so a run
//! says what the window saw when the hold went.
use super::*;
use ferrite_core::browser::State;
use ferrite_pane::visual::hold_trace;
use std::path::Path;

#[derive(Default)]
pub(super) struct Scenario {
    pending: Option<(u64, String, ferrite_core::browser::Document)>,
    completed: u64,
    held: Option<gpui::Point<gpui::Pixels>>,
}
impl Scenario {
    pub fn step(
        &mut self,
        view: &Entity<CockpitView>,
        thread: ferrite_core::ThreadId,
        window: &mut Window,
        cx: &mut App,
        output: &Path,
    ) -> Result<bool, String> {
        if self.pending.is_none() {
            let path = output.join("boundary-ui-request.json");
            if !path.exists() {
                return Ok(false);
            }
            let request: serde_json::Value =
                serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            let id = request["id"]
                .as_u64()
                .ok_or("Boundary UI request has no id")?;
            if id <= self.completed {
                if self.held.is_some() {
                    if let Some(state) = super::served(view, thread, cx)? {
                        trace(view, thread, window, cx, Tick::Holding, id, &state);
                    }
                }
                return Ok(false);
            }
            let action = request["action"]
                .as_str()
                .ok_or("Boundary UI request has no action")?
                .to_string();
            let Some(state) = super::served(view, thread, cx)? else {
                return Ok(false);
            };
            let Some(tab) = state.tabs.iter().find(|tab| {
                Some(tab.id) == state.active
                    && tab.live
                    && tab.visible
                    && !tab.loading
                    && tab.error.is_none()
            }) else {
                if action == "hold" {
                    trace(view, thread, window, cx, Tick::PageNotReady, id, &state);
                }
                return Ok(false);
            };
            let document = tab.document();
            match action.as_str() {
                "hold" => {
                    let Some(bounds) = window.debug_bounds("browser-preview") else {
                        trace(view, thread, window, cx, Tick::NotLaidOut, id, &state);
                        return Ok(false);
                    };
                    // The fixture's top-left white margin contains no control.
                    let Some(viewport) = view.read(cx).browser_smoke_viewport(thread) else {
                        trace(view, thread, window, cx, Tick::NotLaidOut, id, &state);
                        return Ok(false);
                    };
                    // browser-preview also contains tabs/address controls. Its
                    // native content is bottom aligned, so use the real viewport.
                    let position = bounds.origin
                        + point(
                            px(12.),
                            bounds.size.height - px(viewport.height as f32) + px(12.),
                        );
                    window.dispatch_event(
                        PlatformInput::MouseMove(MouseMoveEvent {
                            position,
                            pressed_button: None,
                            modifiers: Modifiers::default(),
                        }),
                        cx,
                    );
                    window.dispatch_event(
                        PlatformInput::MouseDown(MouseDownEvent {
                            position,
                            button: MouseButton::Left,
                            modifiers: Modifiers::default(),
                            click_count: 1,
                            first_mouse: false,
                        }),
                        cx,
                    );
                    self.held = Some(position);
                    trace(view, thread, window, cx, Tick::Pressed, id, &state);
                }
                "release" => {
                    let position = self
                        .held
                        .take()
                        .ok_or("No held operator gesture to release")?;
                    window.dispatch_event(
                        PlatformInput::MouseUp(MouseUpEvent {
                            position,
                            button: MouseButton::Left,
                            modifiers: Modifiers::default(),
                            click_count: 1,
                        }),
                        cx,
                    );
                }
                "reload" => {
                    if !super::annotations::click(window, cx, "browser-reload") {
                        return Ok(false);
                    }
                }
                _ => return Err("Unsupported boundary UI action".into()),
            }
            self.pending = Some((id, action, document));
            return Ok(false);
        }
        let (id, action, before) = self.pending.as_ref().unwrap();
        let Some(state) = super::served(view, thread, cx)? else {
            return Ok(false);
        };
        let ready = state
            .tabs
            .iter()
            .find(|tab| Some(tab.id) == state.active)
            .is_some_and(|tab| match action.as_str() {
                "hold" => tab.protected,
                "release" => !tab.protected,
                "reload" => {
                    tab.document() != *before && tab.live && !tab.loading && tab.error.is_none()
                }
                _ => false,
            });
        let hold = action == "hold";
        if !ready {
            if hold {
                trace(view, thread, window, cx, Tick::Unprotected, *id, &state);
            }
            return Ok(false);
        }
        window.draw(cx).clear(cx);
        window
            .render_to_image()
            .map_err(|e| e.to_string())?
            .save(output.join(format!("boundary-{id}-{action}.png")))
            .map_err(|e| e.to_string())?;
        let answer = serde_json::json!({"id":id,"action":action,"success":true,"state":{"tabs":state.tabs,"active":state.active}});
        let temporary = output.join("boundary-ui-answer.tmp");
        std::fs::write(&temporary, serde_json::to_vec_pretty(&answer).unwrap())
            .map_err(|e| e.to_string())?;
        std::fs::rename(temporary, output.join("boundary-ui-answer.json"))
            .map_err(|e| e.to_string())?;
        if hold {
            trace(view, thread, window, cx, Tick::Answered, *id, &state);
        }
        self.completed = *id;
        self.pending = None;
        Ok(false)
    }
}

/// What a tick did with a hold the provider asked for.
#[derive(Debug)]
enum Tick {
    /// The active tab was not yet live, visible and loaded.
    PageNotReady,
    /// The preview or its page was not laid out yet.
    NotLaidOut,
    /// The press went to the page.
    Pressed,
    /// Pressed, and the Server's state does not say protected yet.
    Unprotected,
    /// The state said protected: the screenshot was saved and the answer
    /// written.
    Answered,
    /// Answered, the button still down, before the provider's next ask.
    Holding,
}

/// One line of the hold trace for this tick: what it did, the active
/// page's gesture and view as the window holds them, whether the window has
/// the operator (as the pump reads it, and as the platform says), and the
/// active tab as the Server's state has it.
fn trace(
    view: &Entity<CockpitView>,
    thread: ferrite_core::ThreadId,
    window: &Window,
    cx: &App,
    tick: Tick,
    ui: u64,
    state: &State,
) {
    let tab = state.tabs.iter().find(|tab| Some(tab.id) == state.active);
    let view = view.read(cx);
    let hold = view.browser_smoke_hold(thread, cx);
    hold_trace::record(serde_json::json!({
        "kind": "tick",
        "ui": ui,
        "tick": format!("{tick:?}"),
        "manual_active": hold.map(|(active, _)| active),
        "page_visible": hold.map(|(_, visible)| visible),
        "window_active": view.cockpit.read(cx).window_active(),
        "platform_active": window.is_window_active(),
        "tab": tab.map(|tab| tab.id),
        "generation": tab.map(|tab| tab.generation),
        "tab_visible": tab.map(|tab| tab.visible),
        "protected": tab.map(|tab| tab.protected),
    }));
}
