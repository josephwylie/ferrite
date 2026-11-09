//! A bounded acceptance scenario in the ordinary native application window.
//! No test Engine, headless GPUI context or alternate browser manager is used.
use super::*;
use gpui::{point, App, Modifiers, PlatformInput};
use std::path::PathBuf;
use std::time::{Duration, Instant};

impl CockpitView {
    pub fn start_browser_smoke(view: Entity<Self>, window: &mut Window, cx: &mut App) {
        let handle = window.window_handle();
        let url = std::env::var("FERRITE_BROWSER_WINDOW_URL").expect("fixture URL");
        let output = PathBuf::from(
            std::env::var_os("FERRITE_BROWSER_WINDOW_OUTPUT").expect("artifact directory"),
        );
        // This opt-in scenario must never operate on an ordinary user launch.
        let store = ferrite_core::server::store_dir();
        let temporary = std::env::temp_dir().canonicalize().expect("temporary root");
        let parent = store.parent().and_then(|path| path.canonicalize().ok());
        assert!(
            parent.is_some_and(|path| path.starts_with(&temporary)),
            "isolated temporary Store required"
        );
        assert!(
            url::Url::parse(&url)
                .is_ok_and(|url| url.scheme() == "http" && url.host_str() == Some("127.0.0.1")),
            "loopback fixture URL required"
        );
        cx.spawn(async move |cx| {
            let started = Instant::now();
            let mut opened = false;
            let mut clicked = false;
            let mut provider_started = false;
            let provider = std::env::var_os("FERRITE_BROWSER_WINDOW_PROVIDER").is_some();
            let mut frames = 0u64;
            let mut last_error = String::new();
            let result = loop {
                cx.background_executor().timer(Duration::from_millis(150)).await;
                let step = handle.update(cx, |_, window, cx| -> Result<bool, String> {
                    if provider_started {
                        let path = output.join("provider-result.json");
                        if !path.exists() { return Ok(false); }
                        let answer: serde_json::Value = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                        return if answer["success"] == true { Ok(true) } else { Err(format!("actual provider Observe failed: {answer}")) };
                    }
                    if !opened {
                        opened = view.update(cx, |view, cx| {
                            let Some(thread) = view.cockpit.threads().first().copied() else {
                                return false;
                            };
                            let Some(index) = view.pane_for(thread) else { return false; };
                            view.focus_thread(thread, cx);
                            // The exact operator hyperlink/preview route, including
                            // its real Server ownership and window realization.
                            view.panes[index].preview.open_url(&url, window)
                        });
                        return Ok(false);
                    }
                    // Draw through this native Window's renderer, as the app does.
                    // Unlike HeadlessAppContext, this requires the platform window.
                    window.draw(cx);
                    let image = window.render_to_image().map_err(|error| error.to_string())?;
                    frames += 1;
                    if frames == 1 || frames % 20 == 0 {
                        image.save(output.join("window-current.png")).map_err(|error| error.to_string())?;
                    }
                    let color = if clicked { [23, 173, 91] } else { [213, 37, 59] };
                    let mut count = 0u64;
                    let (mut x_sum, mut y_sum) = (0u64, 0u64);
                    for (x, y, pixel) in image.enumerate_pixels() {
                        if pixel.0[..3] == color {
                            count += 1;
                            x_sum += u64::from(x);
                            y_sum += u64::from(y);
                        }
                    }
                    if count < 2000 { return Ok(false); }
                    let name = if clicked { "window-after.png" } else { "window-before.png" };
                    image.save(output.join(name)).map_err(|error| error.to_string())?;
                    if clicked {
                        if !provider { return Ok(true); }
                        view.update(cx, |view, _| {
                            let thread = view.cockpit.threads()[0];
                            view.cockpit.send(thread, "Observe the displayed fixture".into());
                        });
                        provider_started = true;
                        return Ok(false);
                    }
                    // Locate the known fixture's painted button independently of
                    // CEF geometry/target APIs, then enter the normal GPUI input path.
                    let scale = window.scale_factor();
                    let position = point(px(x_sum as f32 / count as f32 / scale), px(y_sum as f32 / count as f32 / scale));
                    let modifiers = Modifiers::default();
                    window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position, pressed_button: None, modifiers }), cx);
                    window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent { position, button: MouseButton::Left, modifiers, click_count: 1, first_mouse: false }), cx);
                    window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent { position, button: MouseButton::Left, modifiers, click_count: 1 }), cx);
                    clicked = true;
                    Ok(false)
                });
                match step {
                    Ok(Ok(true)) => break Ok(()),
                    Ok(Err(error)) if provider_started => break Err(error),
                    Ok(Err(error)) => last_error = error,
                    Err(error) => break Err(error.to_string()),
                    _ => {}
                }
                if started.elapsed() > Duration::from_secs(60) {
                    break Err(format!("window fixture timed out: opened={opened}, clicked={clicked}, frames={frames}, last error={last_error}"));
                }
            };
            let stopped = view.update(cx, |view, _| view.cockpit.stop_server_and_wait(Duration::from_secs(10)));
            let error = result.err().or_else(|| match stopped {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(format!("isolated Server did not stop: {error}")),
                Err(error) => Some(error.to_string()),
            });
            let result = serde_json::json!({
                "success": error.is_none(), "error": error,
                "normal_native_window": true, "preview_operator_route": opened,
                "window_pointer_input": clicked, "rendered_frames": frames,
                "provider_requested": provider, "provider_started": provider_started,
                "elapsed_ms": started.elapsed().as_millis(),
                "scope": "operator Preview and optional actual provider Observe; annotations are not tested",
            });
            let _ = std::fs::write(output.join("window-result.json"), serde_json::to_vec_pretty(&result).unwrap());
            let _ = cx.update(|cx| cx.quit());
        }).detach();
    }
}
