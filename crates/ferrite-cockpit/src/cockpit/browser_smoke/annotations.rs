//! Real pointer/keyboard actions over the production annotation editor and Composer.
use super::*;
use ferrite_core::browser_annotations::Annotation;
use std::path::Path;

#[derive(Default)]
pub(super) struct Scenario {
    stage: u8,
    round: usize,
    tool: usize,
    original: Vec<Annotation>,
}
impl Scenario {
    pub fn started(&self) -> bool { self.stage != 0 || self.round > 0 }
    pub fn step(&mut self, view: &Entity<CockpitView>, thread: ferrite_core::ThreadId, window: &mut Window, cx: &mut App, output: &Path) -> Result<bool, String> {
        const TOOLS: [&str; 7] = ["Pen","Line","Arrow","Rectangle","Ellipse","Steps","Text"];
        match self.stage {
            0 => {
                let ready=view.update(cx,|view,_| view.cockpit.browser_attach(thread)).map_err(|e|e.to_string())?;
                if !ready.tabs.iter().any(|tab|Some(tab.id)==ready.active && tab.live && tab.visible && !tab.loading && tab.error.is_none()) { return Ok(false); }
                if click(window,cx,"browser-annotate") { self.stage = 1; }
            }
            1 => {
                if self.round == 1 && !click(window,cx,"browser-annotation-element") { return Ok(false); }
                let Some(bounds) = window.debug_bounds("browser-annotation-overlay") else { return Ok(false); };
                let from = bounds.origin + point(px(50.),px(50.));
                let to = if self.round == 1 { from } else { from + point(px(120.),px(100.)) };
                drag(window,cx,from,to);
                self.stage = 2;
            }
            2 => {
                if window.debug_bounds("annotation-editor").is_none() { return Ok(false); }
                if self.round == 0 {
                    if !click(window,cx,"annotation-ink-Blue") || !click(window,cx,"annotation-width-6") { return Ok(false); }
                    self.stage = 3;
                } else { self.stage = 6; }
            }
            3 => {
                if self.tool == TOOLS.len() { self.stage = 6; return Ok(false); }
                if click(window,cx,&format!("annotation-tool-{}",TOOLS[self.tool])) { self.stage = 4; }
            }
            4 => {
                if TOOLS[self.tool] == "Text" {
                    if !click(window,cx,"annotation-label") { return Ok(false); }
                    type_text(window,cx,"Fixture label");
                }
                self.stage = 5;
            }
            5 => {
                let bounds = view.read(cx).browser_smoke_annotation_picture(thread,cx);
                let Some(bounds) = bounds else { return Ok(false); };
                let from = bounds.origin + point(bounds.size.width * 0.25,bounds.size.height * 0.25);
                let to = bounds.origin + point(bounds.size.width * 0.7,bounds.size.height * 0.6);
                drag(window,cx,from,to);
                self.tool += 1; self.stage = 3;
            }
            6 => {
                // The captured receipt must stay editable after navigation and
                // after closing its final live tab. All changes use the service.
                if self.round == 1 {
                    view.update(cx, |view,_| -> Result<(),String> {
                        let state = view.cockpit.browser_attach(thread).map_err(|e|e.to_string())?;
                        let tab = state.tabs.iter().find(|tab|Some(tab.id)==state.active).ok_or("missing selected tab")?;
                        let document = tab.document();
                        let url = format!("{}?annotation-navigation=1",tab.url.split('?').next().unwrap_or(&tab.url));
                        view.cockpit.browser(thread,ferrite_core::browser::Action::Navigate { document,url }).map_err(|e|e.to_string())?;
                        view.cockpit.browser(thread,ferrite_core::browser::Action::Close { tab: document.tab }).map_err(|e|e.to_string())?;
                        Ok(())
                    })?;
                }
                scroll_editor(window,cx,-3000.);
                self.stage = 7;
            }
            7 => {
                if !click(window,cx,"annotation-comment") { return Ok(false); }
                type_text(window,cx,match self.round { 0=>"Packaged annotation comment",1=>"Immutable element after navigation and close",_=>"REMOVE ME" });
                self.stage = 8;
            }
            8 => { if click(window,cx,"annotation-add") { self.stage = 9; } }
            9 => {
                let annotations = collected(view,thread,cx)?;
                if annotations.len() != self.round+1 { return Ok(false); }
                if self.round == 0 {
                    let kinds: std::collections::BTreeSet<_> = annotations[0].drawings().iter().map(|drawing|serde_json::to_value(drawing).unwrap()["kind"].as_str().unwrap().to_owned()).collect();
                    if kinds.len()!=7 { return Err(format!("Actual drawing controls yielded {kinds:?}")); }
                    self.original = annotations.clone();
                    window.render_to_image().map_err(|e|e.to_string())?.save(output.join("annotation-collected.png")).map_err(|e|e.to_string())?;
                }
                if self.round < 2 {
                    self.round += 1;
                    if self.round == 2 {
                        let url = std::env::var("FERRITE_BROWSER_WINDOW_URL").map_err(|e|e.to_string())?;
                        view.update(cx, |view,_| {
                            let index=view.pane_for(thread).unwrap();
                            view.panes[index].preview.open_url(&url,window);
                        });
                    }
                    self.stage = 0;
                } else { self.stage = 10; }
            }
            10 => { if click(window,cx,"remove-browser-annotation-2") { self.stage=11; } }
            11 => {
                let annotations=collected(view,thread,cx)?;
                if annotations.len()!=2 { return Ok(false); }
                if annotations[0]!=self.original[0] { return Err("Collected evidence changed while adding/removing feedback".into()); }
                std::fs::write(output.join("annotation-before-send.json"),serde_json::to_vec_pretty(&annotations).unwrap()).map_err(|e|e.to_string())?;
                if click(window,cx,"review-browser-annotation-0") { self.stage=12; }
            }
            12 => {
                if window.debug_bounds("browser-annotation-review").is_none() { return Ok(false); }
                window.render_to_image().map_err(|e|e.to_string())?.save(output.join("annotation-review.png")).map_err(|e|e.to_string())?;
                if !click(window,cx,"preview-close") { return Ok(false); }
                let focus=view.read(cx).panes[view.read(cx).pane_for(thread).ok_or("missing Composer")?].composer.read(cx).focus_handle(cx);
                window.focus(&focus,cx);
                self.stage=13;
            }
            13 => {
                type_text(window,cx,"Packaged annotation feedback");
                window.dispatch_keystroke(gpui::Keystroke::parse("enter").unwrap(),cx);
                self.stage=14;
                return Ok(true);
            }
            _ => return Ok(true),
        }
        Ok(false)
    }
}
fn collected(view:&Entity<CockpitView>,thread:ferrite_core::ThreadId,cx:&App)->Result<Vec<Annotation>,String> {
    let view=view.read(cx);let index=view.pane_for(thread).ok_or("missing owning Composer")?;
    Ok(view.panes[index].composer.read(cx).browser_annotations().to_vec())
}
pub(super) fn click(window:&mut Window,cx:&mut App,selector:&str)->bool {
    let Some(bounds)=window.debug_bounds(selector) else{return false;};
    drag(window,cx,bounds.center(),bounds.center());true
}
fn drag(window:&mut Window,cx:&mut App,from:gpui::Point<gpui::Pixels>,to:gpui::Point<gpui::Pixels>) {
    let modifiers=Modifiers::default();
    window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position:from,pressed_button:None,modifiers}),cx);
    window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{position:from,button:MouseButton::Left,modifiers,click_count:1,first_mouse:false}),cx);
    window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position:to,pressed_button:Some(MouseButton::Left),modifiers}),cx);
    window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{position:to,button:MouseButton::Left,modifiers,click_count:1}),cx);
}
fn type_text(window:&mut Window,cx:&mut App,text:&str) {
    for character in text.chars() {
        let mut key=gpui::Keystroke::parse(if character==' ' {"space"} else {"a"}).unwrap();
        key.key=character.to_string();key.key_char=Some(character.to_string());
        window.dispatch_keystroke(key,cx);
    }
}
fn scroll_editor(window:&mut Window,cx:&mut App,y:f32) {
    if let Some(bounds)=window.debug_bounds("annotation-editor") {
        window.dispatch_event(PlatformInput::ScrollWheel(gpui::ScrollWheelEvent{position:bounds.center(),delta:gpui::ScrollDelta::Pixels(point(px(0.),px(y))),modifiers:Modifiers::default(),touch_phase:gpui::TouchPhase::default()}),cx);
    }
}

pub(super) fn history(view:&Entity<CockpitView>,window:&mut Window,cx:&mut App,output:&Path,stage:&mut u8)->Result<bool,String> {
    let expected:Vec<Annotation>=serde_json::from_slice(&std::fs::read(std::env::var_os("FERRITE_BROWSER_WINDOW_EXPECTED_ANNOTATIONS").ok_or("missing original annotation evidence")?).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    let restored=view.read(cx).cockpit.threads().iter().find_map(|thread| {
        let mirror=view.read(cx).cockpit.thread(*thread)?;
        let annotations:Vec<_>=mirror.transcript().blocks().iter().filter_map(|block| {
            match &block.body { ferrite_core::transcript::Body::Prompt(words)=>Some(ferrite_core::browser_annotations::split(words.clone()).2), _=>None }
        }).flatten().collect();
        (!annotations.is_empty()).then_some((*thread,annotations))
    });
    let Some((thread,annotations))=restored else{return Ok(false);};
    if annotations!=expected { return Err("Restarted history changed the original Browser annotation context".into()); }
    for annotation in &annotations {
        let directory=annotation.original().parent().and_then(Path::parent).ok_or("missing retained image directory")?;
        ferrite_core::browser_annotations::read_retained(directory,annotation.original_hash()).map_err(|e|e.to_string())?;
    }
    view.update(cx,|view,cx|view.focus_thread(thread,cx));
    window.draw(cx);
    if *stage==0 {
        let state=view.update(cx,|view,_|view.cockpit.browser_attach(thread)).map_err(|e|e.to_string())?;
        if state.tabs.iter().any(|tab|tab.live) { return Err("History-only restoration eagerly realized a browser page".into()); }
        std::fs::write(output.join("annotation-history.json"),serde_json::to_vec_pretty(&serde_json::json!({"annotations":annotations,"restored_tabs":state.tabs,"zero_eager_pages":true})).unwrap()).map_err(|e|e.to_string())?;
        if click(window,cx,"review-browser-annotation-0") { *stage=1; }
        return Ok(false);
    }
    if window.debug_bounds("browser-annotation-review").is_none() { return Ok(false); }
    let image=window.render_to_image().map_err(|e|e.to_string())?;
    if image.pixels().filter(|pixel|pixel.0[..3]==[23,173,91]).count()<200 { return Ok(false); }
    image.save(output.join("annotation-history-review.png")).map_err(|e|e.to_string())?;
    Ok(true)
}
