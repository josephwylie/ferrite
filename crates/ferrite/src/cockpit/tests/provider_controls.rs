use super::*;

#[gpui::test]
fn contract_context_card_refreshes_and_shows_native_details(cx: &mut TestAppContext) {
    let(core,fake)=cockpit("native-context-ui",1);
    *fake.native_controls.borrow_mut()=true;
    let(_view,cx)=add_cockpit_window(cx,|_,cx|CockpitView::new(core,cx));
    cx.simulate_resize(gpui::size(px(1100.),px(800.)));
    for event in [
        SessionEvent::TokenUsage{total_tokens:12000,input_tokens:10000,cached_input_tokens:1000,output_tokens:2000,reasoning_output_tokens:500,context_window:Some(180000)},
        SessionEvent::ContextDetails{details:ferrite_core::ContextDetails{usable_window:Some(150000),auto_compact_threshold:Some(140000),is_auto_compact_enabled:Some(true),categories:vec![ferrite_core::ContextCategory{name:"Messages".into(),tokens:12000}]}},
    ] { fake.streams.borrow()[0].send(event).unwrap(); }
    tick(cx);
    let meter=cx.debug_bounds("usage-meter-1").unwrap();cx.simulate_mouse_down(meter.center(),MouseButton::Left,gpui::Modifiers::none());cx.run_until_parked();
    assert_eq!(fake.controls.borrow().as_slice(),[ferrite_core::SessionControl::RefreshContext]);
    assert!(cx.debug_bounds("context-usable-150000").is_some());
    assert!(cx.debug_bounds("context-compaction-140000").is_some());
    assert!(cx.debug_bounds("context-category-0-12000").is_some());
}

#[gpui::test]
fn contract_session_controls_use_shared_native_handles(cx:&mut TestAppContext) {
    let(core,fake)=cockpit("native-session-ui",1);*fake.native_controls.borrow_mut()=true;
    let(_view,cx)=add_cockpit_window(cx,|_,cx|CockpitView::new(core,cx));cx.simulate_resize(gpui::size(px(1100.),px(800.)));
    fake.streams.borrow()[0].send(SessionEvent::McpServers{servers:vec![ferrite_core::McpServer{name:"search".into(),status:ferrite_core::McpStatus::NeedsAuth,error:Some("Sign in to search".into())}]}).unwrap();
    fake.streams.borrow()[0].send(SessionEvent::Progress{event:ferrite_core::progress::ProgressEvent::BackgroundSnapshot{tasks:vec![ferrite_core::progress::BackgroundTask{id:"task:1".into(),label:"Background check".into(),detail:"shell".into(),status:ferrite_core::progress::TaskStatus::Working}]}}).unwrap();
    tick(cx);
    let controls=cx.debug_bounds("session-controls-1").expect("capable Sessions expose a shared controls menu");cx.simulate_click(controls.center(),gpui::Modifiers::none());cx.run_until_parked();
    assert!(fake.controls.borrow().contains(&ferrite_core::SessionControl::RefreshMcp));
    let reconnect=cx.debug_bounds("mcp-reconnect-0").unwrap();cx.simulate_click(reconnect.center(),gpui::Modifiers::none());cx.run_until_parked();
    assert!(fake.controls.borrow().contains(&ferrite_core::SessionControl::ReconnectMcp{server:"search".into()}));
    let stop=cx.debug_bounds("background-stop-0").unwrap();cx.simulate_click(stop.center(),gpui::Modifiers::none());cx.run_until_parked();
    assert!(fake.controls.borrow().contains(&ferrite_core::SessionControl::StopTask{id:"task:1".into()}));
    assert!(fake.sent.borrow().is_empty(),"controls cannot become chat prompts");
}
