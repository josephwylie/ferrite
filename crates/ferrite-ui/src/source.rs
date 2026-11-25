//! What a feature view draws of the window, read where the window keeps it
//! (ADR 0041's amendment of 2026-11-25).
//!
//! A feature view holds a `Source` in place of a handle to the window's
//! root. The root makes one over itself; a test makes one over an entity of
//! its own (`testing`). The read takes `&App`, so a draw through it cannot
//! write, and it reads its owner as an entity, so the owner's notify redraws
//! the view as a read of the root did (vendored gpui, `tracking_reads`).

use std::rc::Rc;

use gpui::{App, Entity};

/// What a feature draws of the window, read where the window keeps it.
/// `Q` is what the feature says to be answered (a search line, a Thread).
pub struct Source<T, Q = ()> {
    read: Rc<dyn Fn(&Q, &App) -> Option<T>>,
}

impl<T: 'static, Q: 'static> Source<T, Q> {
    /// Inputs kept by `owner`, held weakly. A read is `None` once the owner
    /// is gone, and also when `read` has nothing to answer the query with: a
    /// view draws nothing either way.
    pub fn of<E: 'static>(
        owner: &Entity<E>,
        read: impl Fn(&E, &Q, &App) -> Option<T> + 'static,
    ) -> Self {
        let owner = owner.downgrade();
        Self {
            read: Rc::new(move |query, cx| read(owner.upgrade()?.read(cx), query, cx)),
        }
    }

    /// What the owner answers to `query` now. Never call it while the owner
    /// is being updated: gpui has leased it, and the read panics.
    pub fn read(&self, query: &Q, cx: &App) -> Option<T> {
        (self.read)(query, cx)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use gpui::{div, prelude::*, px, Context, Entity, StyleRefinement, TestAppContext, Window};

    use super::Source;

    /// The window's state, as a test keeps it.
    struct Kept {
        words: Vec<&'static str>,
    }

    #[gpui::test]
    fn it_answers_the_query_from_its_owner_until_the_owner_goes(cx: &mut TestAppContext) {
        let kept = cx.new(|_| Kept {
            words: vec!["archive", "board", "arrow"],
        });
        let source: Source<Vec<&'static str>, char> = Source::of(&kept, |kept, first, _| {
            Some(kept.words.iter().copied().filter(|word| word.starts_with(*first)).collect())
        });
        cx.update(|cx| {
            assert_eq!(source.read(&'a', cx), Some(vec!["archive", "arrow"]));
            assert_eq!(source.read(&'z', cx), Some(vec![]));
        });
        kept.update(cx, |kept, _| kept.words.push("ask"));
        cx.update(|cx| assert_eq!(source.read(&'a', cx), Some(vec!["archive", "arrow", "ask"])));
        drop(kept);
        cx.run_until_parked();
        cx.update(|cx| assert_eq!(source.read(&'a', cx), None, "its owner is gone"));
    }

    /// A view that draws what its `Source` answers, and counts its draws.
    struct Reads {
        source: Source<usize>,
        renders: Rc<Cell<usize>>,
    }

    impl Render for Reads {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            div().child(format!("{}", self.source.read(&(), cx).unwrap_or_default()))
        }
    }

    /// A view that reads nothing, beside it.
    struct Sibling {
        renders: Rc<Cell<usize>>,
    }

    impl Render for Sibling {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            div()
        }
    }

    /// The window's parts: each cached, and redrawn when what it read is
    /// notified, as the Cockpit lays its parts out (ADR 0012).
    struct Host {
        reads: Entity<Reads>,
        sibling: Entity<Sibling>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let part = || StyleRefinement::default().size(px(50.));
            div()
                .child(self.reads.clone().cached(part()).tracking_reads())
                .child(self.sibling.clone().cached(part()).tracking_reads())
        }
    }

    #[gpui::test]
    fn its_owners_notify_redraws_the_view_that_read_it_alone(cx: &mut TestAppContext) {
        let kept = cx.new(|_| 1_usize);
        let (reads, sibling) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
        let source = Source::of(&kept, |count, _, _| Some(*count));
        let window = cx.add_window(|_, cx| Host {
            reads: cx.new(|_| Reads {
                source,
                renders: reads.clone(),
            }),
            sibling: cx.new(|_| Sibling {
                renders: sibling.clone(),
            }),
        });
        cx.run_until_parked();
        let drawn = (reads.get(), sibling.get());
        assert_eq!(drawn, (1, 1), "each drew once");

        kept.update(cx, |count, cx| {
            *count = 2;
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(reads.get(), 2, "the view that read it drew again, once");
        assert_eq!(sibling.get(), 1, "its sibling kept its cache");

        window.update(cx, |_, _, cx| cx.notify()).unwrap();
        cx.run_until_parked();
        assert_eq!((reads.get(), sibling.get()), (2, 1), "nothing it read moved");
    }
}
