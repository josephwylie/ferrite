//! Real pointer/keyboard actions over the in-page annotation editor and Composer.
use super::*;
use ferrite_core::browser_annotations::{Annotation, Selection};
use std::path::Path;

const COMMENTS: [&str; 3] = ["Packaged annotation comment","Annotated fixture element","REMOVE ME"];

#[derive(Default)]
pub(super) struct Scenario {
    stage: u8,
    round: usize,
    ticks: u8,
    button: Option<gpui::Bounds<gpui::Pixels>>,
    original: Vec<Annotation>,
}
impl Scenario {
    pub fn started(&self) -> bool { self.stage != 0 || self.round > 0 }
    /// Rounds draw a stroke, pick the fixture button and mark a region, each
    /// in its own guest session, then the region is removed before sending.
    pub fn step(&mut self, view: &Entity<CockpitView>, thread: ferrite_core::ThreadId, window: &mut Window, cx: &mut App, output: &Path) -> Result<bool, String> {
        match self.stage {
            0 => {
                let ready=view.update(cx,|view,_| view.cockpit.browser_attach(thread)).map_err(|e|e.to_string())?;
                if !ready.tabs.iter().any(|tab|Some(tab.id)==ready.active && tab.live && tab.visible && !tab.loading && tab.error.is_none()) { return Ok(false); }
                if view.read(cx).browser_smoke_annotation_ready(thread).is_some() { return Ok(false); }
                // Composer thumbnails repeat the button's colour after the first round.
                if self.button.is_none() { self.button=fixture_button(window)?; }
                if self.button.is_some() && click(window,cx,"browser-annotate") { self.stage = 1; }
            }
            1 => {
                if view.read(cx).browser_smoke_annotation_ready(thread)!=Some(true) { return Ok(false); }
                match self.round { 0=>type_text(window,cx,"d"), 2=>type_text(window,cx,"r"), _=>{} }
                self.stage = 2;
            }
            2 => {
                let button=self.button.ok_or("missing fixture button")?;
                let (left,top,right,bottom)=(button.origin.x,button.origin.y,button.origin.x+button.size.width,button.origin.y+button.size.height);
                match self.round {
                    0 => drag(window,cx,point(left+px(40.),top+px(70.)),point(right-px(40.),bottom-px(30.))),
                    1 => drag(window,cx,button.center(),button.center()),
                    _ => drag(window,cx,point(left+px(20.),bottom+px(40.)),point(left+px(160.),bottom+px(140.))),
                }
                self.ticks = 0; self.stage = 3;
            }
            3 => {
                // The guest focuses its comment field on a timer after the first target.
                self.ticks += 1;
                if self.ticks < 4 { return Ok(false); }
                type_text(window,cx,COMMENTS[self.round]);
                self.ticks = 0; self.stage = 4;
            }
            4 => {
                self.ticks += 1;
                if self.ticks < 2 { return Ok(false); }
                press(window,cx,gpui::Keystroke::parse("enter").unwrap());
                self.stage = 5;
            }
            5 => {
                let annotations = collected(view,thread,cx)?;
                if annotations.len() != self.round+1 { return Ok(false); }
                let annotation = &annotations[self.round];
                let Selection::Live { feedback } = &annotation.capture().selection else { return Err(format!("Round {} did not capture live feedback",self.round)); };
                let targeted = match self.round {
                    0 => !feedback.strokes.is_empty(),
                    1 => feedback.elements.iter().any(|target|target.element.tag_name=="button"),
                    _ => !feedback.regions.is_empty(),
                };
                if !targeted || annotation.comment()!=COMMENTS[self.round] || annotation.original().is_none() {
                    return Err(format!("Round {} delivered the wrong live feedback: {feedback:?}",self.round));
                }
                if self.round == 0 {
                    window.render_to_image().map_err(|e|e.to_string())?.save(output.join("annotation-collected.png")).map_err(|e|e.to_string())?;
                }
                self.original = annotations;
                self.round += 1;
                self.stage = if self.round < COMMENTS.len() { 0 } else { 10 };
            }
            10 => { if click(window,cx,"remove-browser-annotation-2") { self.stage=11; } }
            11 => {
                let annotations=collected(view,thread,cx)?;
                if annotations.len()!=2 { return Ok(false); }
                if annotations[..]!=self.original[..2] { return Err("Collected evidence changed while adding/removing feedback".into()); }
                std::fs::write(output.join("annotation-before-send.json"),serde_json::to_vec_pretty(&annotations).unwrap()).map_err(|e|e.to_string())?;
                if click(window,cx,"review-browser-annotation-0") { self.stage=12; }
            }
            12 => {
                if window.debug_bounds("browser-annotation-review").is_none() { return Ok(false); }
                window.render_to_image().map_err(|e|e.to_string())?.save(output.join("annotation-review.png")).map_err(|e|e.to_string())?;
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
fn fixture_button(window:&mut Window)->Result<Option<gpui::Bounds<gpui::Pixels>>,String> {
    let image=window.render_to_image().map_err(|e|e.to_string())?;
    let scale=window.scale_factor();
    let (mut left,mut top,mut right,mut bottom,mut count)=(u32::MAX,u32::MAX,0,0,0);
    for (x,y,pixel) in image.enumerate_pixels() {
        if pixel.0[..3]==[23,173,91] { left=left.min(x);top=top.min(y);right=right.max(x);bottom=bottom.max(y);count+=1; }
    }
    if count<2000 { return Ok(None); }
    let corner=|x:u32,y:u32|point(px(x as f32/scale),px(y as f32/scale));
    Ok(Some(gpui::Bounds::from_corners(corner(left,top),corner(right,bottom))))
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
    super::trace_input("driver-drag",(from,to));
    window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position:from,pressed_button:None,modifiers}),cx);
    window.dispatch_event(PlatformInput::MouseDown(MouseDownEvent{position:from,button:MouseButton::Left,modifiers,click_count:1,first_mouse:false}),cx);
    for step in 1..=8 {
        let t=step as f32/8.;
        let position=point(from.x+(to.x-from.x)*t,from.y+(to.y-from.y)*t);
        window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent{position,pressed_button:Some(MouseButton::Left),modifiers}),cx);
    }
    window.dispatch_event(PlatformInput::MouseUp(MouseUpEvent{position:to,button:MouseButton::Left,modifiers,click_count:1}),cx);
}
fn type_text(window:&mut Window,cx:&mut App,text:&str) {
    for character in text.chars() {
        let mut key=gpui::Keystroke::parse(if character==' ' {"space"} else {"a"}).unwrap();
        key.key=character.to_string();key.key_char=Some(character.to_string());
        press(window,cx,key);
    }
}
fn press(window:&mut Window,cx:&mut App,key:gpui::Keystroke) {
    window.dispatch_keystroke(key.clone(),cx);
    // The page counts a key held until its release as operator input.
    window.dispatch_event(PlatformInput::KeyUp(gpui::KeyUpEvent{keystroke:key}),cx);
}
pub(super) fn history(view:&Entity<CockpitView>,window:&mut Window,cx:&mut App,output:&Path,stage:&mut u8)->Result<bool,String> {
    let expected:Vec<Annotation>=serde_json::from_slice(&std::fs::read(std::env::var_os("FERRITE_BROWSER_WINDOW_EXPECTED_ANNOTATIONS").ok_or("missing original annotation evidence")?).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    let original:serde_json::Value=serde_json::from_slice(&std::fs::read(output.parent().ok_or("missing original output directory")?.join("window-result.json")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    let owning_thread:ferrite_core::ThreadId=serde_json::from_value(original["scenario_thread"].clone()).map_err(|e|e.to_string())?;
    // History follows the drawn Thread; a fresh Codex Thread is not the
    // imported Thread initially focused by the restarted window.
    view.update(cx,|view,cx|view.focus_thread(owning_thread,cx));
    let restored=std::iter::once(&owning_thread).find_map(|thread| {
        let mirror=view.read(cx).cockpit.thread(*thread)?;
        let annotations:Vec<_>=mirror.transcript().blocks().iter().filter_map(|block| {
            match &block.body { ferrite_core::transcript::Body::Prompt(words)=>Some(ferrite_core::browser_annotations::split(words.clone()).2), _=>None }
        }).flatten().collect();
        (!annotations.is_empty()).then_some((*thread,annotations))
    });
    let Some((thread,annotations))=restored else{return Ok(false);};
    if annotations!=expected { return Err("Restarted history changed the original Browser annotation context".into()); }
    for annotation in &annotations {
        let directory=annotation.original().ok_or("missing retained screenshot")?.parent().and_then(Path::parent).ok_or("missing retained image directory")?;
        ferrite_core::browser_annotations::read_retained(directory,annotation.original_hash().ok_or("missing retained screenshot hash")?).map_err(|e|e.to_string())?;
    }
    view.update(cx,|view,cx|view.focus_thread(thread,cx));
    window.draw(cx).clear(cx);
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
