use gpui::{prelude::*, div, px, Context, Entity, IntoElement, Render, TestAppContext, Window};
use gpui::base::{TextSelectionLayer, text::{TextView,TextViewState}};
use std::time::Instant;

struct Probe { docs: Vec<Entity<TextViewState>>, selectable: bool }
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut body=div().id("scroll").w(px(900.)).h(px(600.)).overflow_y_scroll().flex().flex_col();
        for doc in &self.docs {
            body=body.child(div().w_full().min_w_0().flex_shrink_0().child(
                TextView::new(doc).w_full().min_w_0().max_lines(usize::MAX).selectable(self.selectable)
            ));
        }
        div().size_full().child(TextSelectionLayer).child(body)
    }
}
#[gpui::test]
fn transcript_frame_cost(cx: &mut TestAppContext) {
    cx.update(|cx| gpui::component::init(cx));
    for n in [32, 128, 200] {
        for selectable in [false,true] {
            let view=cx.add_window(move |_,cx| Probe{
                docs:(0..n).map(|i|cx.new(|cx|TextViewState::markdown(&format!("Block {i}: plain text with **bold** and `code`.\n\nSecond paragraph with enough words to form transcript content."),cx))).collect(),
                selectable,
            });
            for _ in 0..2 {cx.update_window(*view,|_,w,cx|{w.refresh();let _=w.draw(cx);}).unwrap();}
            let mut times=Vec::new();
            for _ in 0..5 {
                let t=Instant::now();cx.update_window(*view,|_,w,cx|{w.refresh();let _=w.draw(cx);}).unwrap();
                times.push(t.elapsed().as_secs_f64()*1000.);
            }
            times.sort_by(f64::total_cmp);
            eprintln!("transcript_probe docs={n} selectable={selectable} median_ms={:.3} min_ms={:.3} max_ms={:.3}",times[2],times[0],times[4]);
        }
    }
}
