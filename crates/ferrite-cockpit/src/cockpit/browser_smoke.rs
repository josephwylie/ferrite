//! A bounded acceptance scenario in the ordinary native application window.
//! No test Engine, headless GPUI context or alternate browser manager is used.
use super::*;
mod annotations;
mod boundary;
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
        std::fs::write(
            output.join("window-start.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "pid": std::process::id(), "scale": window.scale_factor(),
                "stage": "production CockpitView constructed in native Window",
            }))
            .unwrap(),
        )
        .expect("retain native window startup evidence");
        ferrite_core::log!("browser window smoke: production CockpitView constructed");
        cx.spawn(async move |cx| {
            let started = Instant::now();
            let mut opened = false;
            let mut clicked = false;
            let mut provider_started = false;
            let mut ready_document = None;
            let mut scenario_thread = None;
            let codex = std::env::var("FERRITE_BROWSER_WINDOW_PROVIDER").ok().as_deref() == Some("codex");
            let boundaries = std::env::var("FERRITE_BROWSER_WINDOW_SCENARIO").ok().as_deref() == Some("boundaries");
            let mut boundary = boundary::Scenario::default();
            let history = std::env::var("FERRITE_BROWSER_WINDOW_SCENARIO").ok().as_deref() == Some("annotation-history");
            let mut history_stage = 0;
            let annotate = std::env::var("FERRITE_BROWSER_WINDOW_SCENARIO").ok().as_deref() == Some("annotations");
            let mut annotation = annotations::Scenario::default();
            let held_screenshot = std::env::var("FERRITE_BROWSER_WINDOW_SCENARIO").ok().as_deref() == Some("held-screenshot");
            let work_orders = std::env::var("FERRITE_BROWSER_WINDOW_SCENARIO").ok().as_deref() == Some("work-orders");
            let provider = std::env::var_os("FERRITE_BROWSER_WINDOW_PROVIDER").is_some();
            let mut preview_expanded = false;
            let mut controlled_viewport = false;
            let mut baseline_started = None;
            let mut quiet_started = None;
            let mut restoring_page: Option<(ferrite_core::browser::Document,String)> = None;
            let mut reclamation_phase = false;
            let mut frames = 0u64;
            let mut last_error = String::new();
            let result = loop {
                cx.background_executor().timer(Duration::from_millis(150)).await;
                let step = handle.update(cx, |_, window, cx| -> Result<bool, String> {
                    if boundaries {
                        if let Some(thread)=scenario_thread { boundary.step(&view,thread,window,cx,&output)?; }
                    }
                    if history {
                        frames += 1;
                        return annotations::history(&view,window,cx,&output,&mut history_stage);
                    }
                    if provider_started {
                        if let Some(quiet) = quiet_started {
                            if Instant::now().duration_since(quiet)<Duration::from_secs(3) { return Ok(false); }
                            if !boundaries { return Ok(true); }
                            if !reclamation_phase { phase(&output,"reclamation-and-restoration");reclamation_phase=true; }
                            let thread=scenario_thread.expect("scenario Thread");
                            let state=view.update(cx,|view,_|view.cockpit.browser_attach(thread)).map_err(|e|e.to_string())?;
                            if let Some((document,ref url))=restoring_page {
                                if let Some(tab)=state.tabs.iter().find(|tab|tab.id==document.tab && tab.live && !tab.loading && tab.error.is_none() && tab.url==*url) {
                                    if tab.document()==document { return Err("Reclaimed page restoration reused stale document identity".into()); }
                                    std::fs::write(output.join("lifecycle-restoration.json"),serde_json::to_vec_pretty(&serde_json::json!({"restored":tab,"previous_document":document,"urls_only_not_unsaved_form_restoration":true})).unwrap()).map_err(|e|e.to_string())?;
                                    window.draw(cx);window.render_to_image().map_err(|e|e.to_string())?.save(output.join("window-restored.png")).map_err(|e|e.to_string())?;
                                    return Ok(true);
                                }
                            } else {
                                std::fs::write(output.join("lifecycle-before-restoration.json"),serde_json::to_vec_pretty(&state.tabs).unwrap()).map_err(|e|e.to_string())?;
                                if let Some(tab)=state.tabs.iter().find(|tab|Some(tab.id)!=state.active && !tab.live) {
                                    restoring_page=Some((tab.document(),tab.url.clone()));
                                    view.update(cx,|view,_|view.cockpit.browser(thread,ferrite_core::browser::Action::Select{tab:tab.id})).map_err(|e|e.to_string())?;
                                    phase(&output,"restored-page");quiet_started=Some(Instant::now());return Ok(false);
                                }
                            }
                            if Instant::now().duration_since(quiet)>Duration::from_secs(15) { return Err("A clean inactive page was not reclaimed and restored in the bounded lifecycle scenario".into()); }
                            return Ok(false);
                        }
                        let path = output.join("provider-result.json");
                        if !path.exists() { return Ok(false); }
                        let answer: serde_json::Value = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                        if answer["success"] != true {
                            window.draw(cx);
                            if let Ok(image) = window.render_to_image() {
                                let _ = image.save(output.join("window-provider-error.png"));
                            }
                            return Err(format!("actual provider browser action failed: {answer}"));
                        }
                        // Provider acknowledgements alone do not prove the owning
                        // Preview changed. Decode the actual native Window again.
                        window.draw(cx);
                        let image = window.render_to_image().map_err(|error| error.to_string())?;
                        if boundaries {
                            image.save(output.join("window-boundaries.png")).map_err(|e|e.to_string())?;
                            phase(&output,"quiet-after-boundaries");quiet_started=Some(Instant::now());return Ok(false);
                        }
                        if annotate {
                            image.save(output.join("window-annotations-sent.png")).map_err(|e|e.to_string())?;
                            phase(&output,"quiet-after-annotations"); quiet_started=Some(Instant::now());
                            return Ok(false);
                        }
                        if work_orders {
                            image.save(output.join("window-work-order.png")).map_err(|error| error.to_string())?;
                            phase(&output,"quiet-after-work-orders"); quiet_started=Some(Instant::now());
                            return Ok(false);
                        }
                        let expected = if held_screenshot { [23, 173, 91] } else { [29, 97, 219] };
                        let colored = image.pixels().filter(|pixel| pixel.0[..3] == expected).count();
                        if colored < 2000 { return Ok(false); }
                        image.save(output.join(if held_screenshot { "window-held-screenshot.png" } else { "window-provider.png" })).map_err(|error| error.to_string())?;
                        if held_screenshot { return Ok(true); }
                        phase(&output,"quiet-after-provider-action"); quiet_started=Some(Instant::now());
                        return Ok(false);
                    }
                    if !opened {
                        opened = view.update(cx, |view, cx| -> Result<bool, String> {
                            let thread = if let Some(thread) = scenario_thread { thread }
                            else if codex {
                                // A fresh production Thread advertises dynamic
                                // tools at thread/start. Imported Codex sessions
                                // retain their original capabilities on resume.
                                let thread = view.cockpit.open(ferrite_core::store::Provider::Codex,
                                    ferrite_core::workspace::WorkspaceChoice::Main {
                                        checkout: std::env::current_dir().map_err(|error| error.to_string())?,
                                    }).map_err(|error| error.to_string())?;
                                scenario_thread = Some(thread);
                                return Ok(false);
                            } else {
                                let Some(thread) = view.cockpit.threads().first().copied() else { return Ok(false); };
                                thread
                            };
                            scenario_thread = Some(thread);
                            let Some(index) = view.pane_for(thread) else { return Ok(false); };
                            view.focus_thread(thread, cx);
                            if baseline_started.is_none() {
                                baseline_started=Some(Instant::now()); phase(&output,"sealed-window-baseline");
                            }
                            if baseline_started.unwrap().elapsed()<Duration::from_secs(3) { return Ok(false); }
                            phase(&output,"first-browser-use");
                            Ok(view.panes[index].preview.open_url(&url, window))
                        })?;
                        window.draw(cx);
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
                    if (work_orders || annotate || boundaries) && !controlled_viewport {
                        if !preview_expanded {
                            if annotations::click(window,cx,"preview-expand") { preview_expanded=true; }
                            return Ok(false);
                        }
                        let viewport=view.read(cx).browser_smoke_viewport(scenario_thread.expect("scenario Thread"));
                        let Some(viewport)=viewport else{return Ok(false);};
                        if viewport.width==0 || viewport.height==0 { return Ok(false); }
                        std::fs::write(output.join("browser-viewport.json"),serde_json::to_vec_pretty(&serde_json::json!({"width_css":viewport.width,"height_css":viewport.height,"device_scale":viewport.scale})).unwrap()).map_err(|e|e.to_string())?;
                        controlled_viewport=true;
                    }
                    if annotate && clicked {
                        let ready=annotation.started() || image.pixels().filter(|pixel|pixel.0[..3]==[23,173,91]).count()>=2000;
                        if ready && annotation.step(&view,scenario_thread.expect("scenario Thread"),window,cx,&output)? {
                            provider_started=true;
                        }
                        return Ok(false);
                    }
                    if work_orders {
                        let document = view.update(cx, |view, _| {
                            let thread = scenario_thread.expect("scenario Thread");
                            let state = view.cockpit.browser_attach(thread).map_err(|error| error.to_string())?;
                            Ok::<_, String>(state.tabs.iter().find(|tab| Some(tab.id) == state.active)
                                .filter(|tab| tab.live && tab.visible && !tab.loading && tab.error.is_none() && tab.url == url)
                                .map(|tab| tab.document()))
                        })?;
                        let stable = document.is_some() && ready_document == document;
                        ready_document = document;
                        if !stable { return Ok(false); }
                        image.save(output.join("window-work-order-before.png")).map_err(|error| error.to_string())?;
                        view.update(cx, |view, _| {
                            let thread = scenario_thread.expect("scenario Thread");
                            view.cockpit.send(thread, "Complete the prescribed work order through browser tools".into());
                        });
                        provider_started = true;
                        return Ok(false);
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
                    if !clicked {
                        // Painted pixels can precede the native navigation ACK.
                        // Wait for the public service's loaded document to remain
                        // current across a frame before injecting one gesture.
                        // Otherwise production correctly discards that gesture
                        // when its older document is superseded in the queue.
                        let document = view.update(cx, |view, _| {
                            let thread = scenario_thread.expect("scenario Thread");
                            let state = view.cockpit.browser_attach(thread).map_err(|error| error.to_string())?;
                            Ok::<_, String>(state.tabs.iter()
                                .find(|tab| Some(tab.id) == state.active)
                                .filter(|tab| tab.live && tab.visible && !tab.loading && tab.error.is_none() && tab.url == url)
                                .map(|tab| tab.document()))
                        })?;
                        let stable = document.is_some() && ready_document == document;
                        ready_document = document;
                        if !stable { return Ok(false); }
                        std::fs::write(output.join("input-ready.json"), serde_json::to_vec_pretty(&serde_json::json!({
                            "document":document,"public_service_loaded":true,"stable_across_frames":true,
                        })).unwrap()).map_err(|error| error.to_string())?;
                    }
                    let name = if clicked { "window-after.png" } else { "window-before.png" };
                    image.save(output.join(name)).map_err(|error| error.to_string())?;
                    if clicked {
                        if !provider { return Ok(true); }
                        if held_screenshot {
                            // Keep a real operator gesture active through first
                            // controller use. No synthetic unpause or mouse-up.
                            let scale = window.scale_factor();
                            let position = point(px(x_sum as f32 / count as f32 / scale), px(y_sum as f32 / count as f32 / scale));
                            window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent { position, button: MouseButton::Left, modifiers: Modifiers::default(), click_count: 1, first_mouse: false }), cx);
                        }
                        view.update(cx, |view, _| {
                            let thread = scenario_thread.expect("scenario Thread");
                            view.cockpit.send(thread, "Observe the displayed fixture, click its button, and observe the result".into());
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
                if started.elapsed() > Duration::from_secs(if work_orders { std::env::var("FERRITE_BROWSER_WINDOW_TASKS").ok().and_then(|value|value.parse::<u64>().ok()).unwrap_or(3).min(10)*180 } else if boundaries { 180 } else if annotate || history { 240 } else { 60 }) {
                    break Err(format!("window fixture timed out: opened={opened}, clicked={clicked}, frames={frames}, last error={last_error}"));
                }
            };
            phase(&output,"app-shutdown");
            // The external owner stops the Server after this window exits;
            // stopping here races the ordinary window's automatic reconnect.
            let error = result.err();
            let result = serde_json::json!({
                "success": error.is_none(), "error": error,
                "normal_native_window": true, "preview_operator_route": opened,
                "window_pointer_input": clicked, "rendered_frames": frames,
                "provider_requested": provider, "annotations": annotate, "boundaries": boundaries, "annotation_history": history, "held_screenshot": held_screenshot, "scenario_thread": scenario_thread, "work_orders": work_orders, "provider_started": provider_started,
                "elapsed_ms": started.elapsed().as_millis(),
                "scope": if boundaries { "actual provider/manual/stale/uncertain/recovery boundaries with independent page oracle" } else if history { "restart and native history review of original annotation evidence with zero eager pages" } else if annotate { "actual annotation overlay, all drawing controls, review/remove/batch Composer send and native provider PNG delivery" } else if work_orders { "one prescribed work order through actual provider and owning Preview; annotations are not tested" } else if held_screenshot { "first provider Screenshot during a held operator gesture; no mutation receipt permitted" } else { "operator Preview and optional actual provider Observe/Click/Observe; annotations are not tested" },
            });
            let _ = std::fs::write(output.join("window-result.json"), serde_json::to_vec_pretty(&result).unwrap());
            let _ = cx.update(|cx| cx.quit());
        }).detach();
    }
}

fn phase(output:&std::path::Path,name:&str) {
    let value=serde_json::json!({"phase":name,"emitted_unix_ms":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()});
    if std::fs::write(output.join("phase.tmp"),value.to_string()).is_ok() { let _=std::fs::rename(output.join("phase.tmp"),output.join("phase.json")); }
}
