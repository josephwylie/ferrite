//! One feature view in a test window of its own, with no Cockpit (ADR
//! 0041's amendment of 2026-11-25). The test owns what the view reads: any
//! entity of its own, which it hands the view as a `Source` and notifies
//! when it changes, as the window's root does. What the view emits it hears
//! through `record`.

use std::{cell::RefCell, rc::Rc};

use gpui::{
    AppContext as _, Context, Entity, EventEmitter, Modifiers, Render, TestAppContext,
    VisualTestContext, Window,
};

/// `build`'s view in a window of its own under the toolkit's root, as the
/// Cockpit's window mounts its content, the window active, its first frame
/// drawn.
pub fn mount<V: Render>(
    cx: &mut TestAppContext,
    build: impl FnOnce(&mut Window, &mut Context<V>) -> V,
) -> (Entity<V>, &mut VisualTestContext) {
    cx.update(crate::theme::init_components);
    let (root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| build(window, cx));
        gpui::component::Root::new(view, window, cx)
    });
    let view = root.read_with(cx, |root, _| {
        root.view().clone().downcast::<V>().expect("the mounted view")
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    (view, cx)
}

/// Every event of `view`'s, kept as `keep` maps it, in the order emitted.
pub struct Heard<T>(Rc<RefCell<Vec<T>>>);

impl<T> Heard<T> {
    /// What was heard since the last take.
    pub fn take(&self) -> Vec<T> {
        self.0.take()
    }
}

/// Hear what `view` emits from now on.
pub fn record<V, E, T>(
    cx: &mut VisualTestContext,
    view: &Entity<V>,
    keep: impl Fn(&E) -> T + 'static,
) -> Heard<T>
where
    V: EventEmitter<E>,
    E: 'static,
    T: 'static,
{
    let heard = Rc::new(RefCell::new(Vec::new()));
    let sink = heard.clone();
    cx.update(|_, cx| {
        cx.subscribe(view, move |_, event, _| sink.borrow_mut().push(keep(event)))
            .detach()
    });
    Heard(heard)
}

/// Whether the last frame drew what `selector` names.
pub fn shows(cx: &mut VisualTestContext, selector: &str) -> bool {
    cx.update(|window, _| window.debug_bounds(selector)).is_some()
}

/// Press what `selector` names, at its centre, and let the window settle.
pub fn click(cx: &mut VisualTestContext, selector: &str) {
    let at = cx
        .update(|window, _| window.debug_bounds(selector))
        .unwrap_or_else(|| panic!("nothing drawn as {selector}"));
    cx.simulate_click(at.center(), Modifiers::none());
    cx.run_until_parked();
}
