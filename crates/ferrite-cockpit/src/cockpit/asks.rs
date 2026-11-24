//! What the window does once its Server answers (ADR 0041). An act whose
//! reply the window reads is an `Ask`: the press returns at once and what
//! follows the answer waits here. An Engine in this process answers as it
//! is asked, so what follows runs in the press, as it always did; a
//! Server's answer rings the bell and the pump runs it.

use super::CockpitView;
use ferrite_core::cockpit::Ask;
use gpui::{Context, Window};
use std::io;

/// One answer to come: true once it came and what follows it ran.
type Then = Box<dyn FnMut(&mut CockpitView, &mut Context<CockpitView>) -> bool>;

/// The answers the window waits for, oldest ask first.
#[derive(Default)]
pub(crate) struct Awaited(Vec<Then>);

impl CockpitView {
    /// Run `then` with `ask`'s answer: now when it is answered already,
    /// else from the pump that reads it.
    pub(super) fn when_answered<T: 'static>(
        &mut self,
        mut ask: Ask<T>,
        then: impl FnOnce(&mut Self, io::Result<T>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        if let Some(answer) = ask.answer() {
            return then(self, answer, cx);
        }
        let mut then = Some(then);
        self.awaited.0.push(Box::new(move |view, cx| {
            let Some(answer) = ask.answer() else {
                return false;
            };
            if let Some(then) = then.take() {
                then(view, answer, cx);
            }
            true
        }));
    }

    /// `when_answered` for what needs the window: now when the ask is
    /// answered already, else once the answer is back in the window's
    /// hands, a turn of the loop after the pump reads it.
    pub(super) fn when_answered_in<T: 'static>(
        &mut self,
        mut ask: Ask<T>,
        window: &mut Window,
        then: impl FnOnce(&mut Self, io::Result<T>, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        if let Some(answer) = ask.answer() {
            return then(self, answer, window, cx);
        }
        let window = window.window_handle();
        self.when_answered(
            ask,
            move |_, answer, cx| {
                let view = cx.entity().downgrade();
                cx.defer(move |cx| {
                    let _ = window.update(cx, |_, window, cx| {
                        let _ = view.update(cx, |view, cx| then(view, answer, window, cx));
                    });
                });
            },
            cx,
        );
    }

    /// Run `then` with the answers of all of `asks`, in their order, once
    /// the last has come: a press on several Threads at once.
    pub(super) fn when_all_answered<T: 'static>(
        &mut self,
        mut asks: Vec<Ask<T>>,
        then: impl FnOnce(&mut Self, Vec<io::Result<T>>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let mut answers: Vec<Option<io::Result<T>>> = asks.iter().map(|_| None).collect();
        let mut all_came = move || {
            for (ask, answer) in asks.iter_mut().zip(answers.iter_mut()) {
                if answer.is_none() {
                    *answer = ask.answer();
                }
            }
            answers
                .iter()
                .all(Option::is_some)
                .then(|| answers.drain(..).flatten().collect::<Vec<_>>())
        };
        if let Some(answers) = all_came() {
            return then(self, answers, cx);
        }
        let mut then = Some(then);
        self.awaited.0.push(Box::new(move |view, cx| {
            let Some(answers) = all_came() else {
                return false;
            };
            if let Some(then) = then.take() {
                then(view, answers, cx);
            }
            true
        }));
    }

    /// Run what follows each answer that came, in the order asked. An ask
    /// made by one of them waits behind those already waiting.
    pub(super) fn settle_asks(&mut self, cx: &mut Context<Self>) {
        let mut waiting = std::mem::take(&mut self.awaited.0);
        waiting.retain_mut(|then| !then(self, cx));
        waiting.append(&mut self.awaited.0);
        self.awaited.0 = waiting;
    }
}
