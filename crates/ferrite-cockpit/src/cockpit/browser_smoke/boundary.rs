//! A closed UI handshake for the real provider boundary scenario. All page
//! actions remain provider browser calls; this helper only supplies operator
//! pointer gestures and the actual Reload toolbar action.
use super::*;
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
                return Ok(false);
            }
            let action = request["action"]
                .as_str()
                .ok_or("Boundary UI request has no action")?
                .to_string();
            let state = view
                .update(cx, |view, _| view.cockpit.browser_attach(thread))
                .map_err(|e| e.to_string())?;
            let Some(tab) = state.tabs.iter().find(|tab| {
                Some(tab.id) == state.active
                    && tab.live
                    && tab.visible
                    && !tab.loading
                    && tab.error.is_none()
            }) else {
                return Ok(false);
            };
            let document = tab.document();
            match action.as_str() {
                "hold" => {
                    let Some(bounds) = window.debug_bounds("browser-preview") else {
                        return Ok(false);
                    };
                    // The fixture's top-left white margin contains no control.
                    let Some(viewport) = view.read(cx).browser_smoke_viewport(thread) else {
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
        let state = view
            .update(cx, |view, _| view.cockpit.browser_attach(thread))
            .map_err(|e| e.to_string())?;
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
        if !ready {
            return Ok(false);
        }
        window.draw(cx);
        window
            .render_to_image()
            .map_err(|e| e.to_string())?
            .save(output.join(format!("boundary-{id}-{action}.png")))
            .map_err(|e| e.to_string())?;
        let answer = serde_json::json!({"id":id,"action":action,"success":true,"state":state});
        let temporary = output.join("boundary-ui-answer.tmp");
        std::fs::write(&temporary, serde_json::to_vec_pretty(&answer).unwrap())
            .map_err(|e| e.to_string())?;
        std::fs::rename(temporary, output.join("boundary-ui-answer.json"))
            .map_err(|e| e.to_string())?;
        self.completed = *id;
        self.pending = None;
        Ok(false)
    }
}
