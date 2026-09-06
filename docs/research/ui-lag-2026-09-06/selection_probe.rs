use gpui::{prelude::*, canvas, div, Context, HitboxBehavior, IntoElement, Render, TestAppContext, Window};
use gpui::base::{TextSelectionHandle, TextSelectionLayer, TextSelectionRegistration};
use std::time::Instant;

struct Probe { handles: Vec<TextSelectionHandle>, register: bool }
impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let handles = self.handles.clone();
        let register = self.register;
        div().size_full().child(TextSelectionLayer).child(canvas(
            move |bounds, window, cx| {
                if register {
                    for (i, handle) in handles.iter().enumerate() {
                        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
                        handle.register(TextSelectionRegistration::new(hitbox, bounds)
                            .with_document_order(i as u64), window, cx);
                    }
                }
            },
            |_, _, _, _| (),
        ).size_full())
    }
}
#[gpui::test]
fn selection_registration_scaling(cx: &mut TestAppContext) {
    for n in [0, 64, 128, 256, 512] {
        let view = cx.add_window(move |_, cx| Probe {
            handles: (0..n).map(|_| TextSelectionHandle::new("probe", cx)).collect(),
            register: true,
        });
        for _ in 0..3 { cx.update_window(*view, |_, window, cx| {window.refresh();let _ = window.draw(cx);}).unwrap(); }
        let mut times=Vec::new();
        for _ in 0..9 {
            let t=Instant::now();
            cx.update_window(*view, |_, window, cx| {window.refresh(); let _ = window.draw(cx);}).unwrap();
            times.push(t.elapsed().as_secs_f64()*1000.);
        }
        times.sort_by(f64::total_cmp);
        eprintln!("selection_probe participants={n} median_ms={:.3} min_ms={:.3} max_ms={:.3}", times[4], times[0], times[8]);
    }
}
