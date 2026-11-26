//! A send keeps the whole draft until its admission is known. The Composer
//! hands over a `SentDraft`; the Engine says `Delivered`, `Held` for a
//! Session still starting, `Refused`, or (over a Server link that went
//! quiet) `Unconfirmed`. Only `Delivered` lets the draft go: a `Refused` or
//! `Unconfirmed` one goes back to the Composer at once, and a `Held` one
//! waits under its Thread for the prompt to come back if the Session never
//! starts. It belongs to the Thread, not to the Pane that sent it, so
//! switching or closing the Pane does not lose it.
//!
//! The window never waits for the admission (ADR 0041). A sent draft is
//! on its way in `Sends` until the Server answers; a Thread's drafts
//! go one at a time, in the order sent, so each is sent or queued by what
//! the Thread is doing once the one before it is settled. A prompt sent
//! again (`send_again`) goes the same way, with no draft to settle: what
//! follows it hears where it went.

use super::*;
use ferrite_core::prompt_queue::Admission;
use ferrite_core::draft_content::DraftContent;
use ferrite_core::ThreadKey;
use ferrite_pane::composer::SentDraft;
use ferrite_core::replies::Reply;
use super::model_picker::Tuning;
use super::returns::queued_to_take_back;
use std::collections::{HashMap, VecDeque};

/// How much of a prompt names it in a notice.
const NAMED_CHARS: usize = 48;

/// Every Thread's prompts from ↵ until the Thread takes them: those on
/// their way, oldest first (the first is asked of the Server, the rest
/// wait for its answer), and the drafts its Engine holds for a Session
/// still starting, oldest first, kept whole until the Session takes the
/// prompt or hands it back.
#[derive(Default)]
pub(crate) struct Sends {
    on_the_way: HashMap<ThreadKey, VecDeque<Outgoing>>,
    held: HashMap<ThreadKey, Vec<Held>>,
}

struct Held {
    draft: SentDraft,
    /// The Thread has been seen holding the prompt, so its absence from the
    /// queue means the Session took it.
    listed: bool,
    /// The Thread closed with the prompt still held: the Engine let it go
    /// with the Session, so the draft is all that is left of it.
    lost: bool,
}

struct Outgoing {
    /// The prompt as sent: a draft's, trimmed.
    text: String,
    from: Origin,
}

/// Where a prompt on its way came from, and so what its answer settles.
enum Origin {
    /// The Composer's ↵: the draft is kept until the Thread takes it.
    Composer { draft: SentDraft, cited: Vec<Reply> },
    /// A prompt sent again: what follows hears where it went.
    Replay(Replayed),
}

/// What follows a prompt sent again: where it went, or why it did not.
type Replayed = Box<dyn FnOnce(&mut CockpitView, Result<Handoff, String>, &mut Context<CockpitView>)>;

impl Sends {
    /// `outgoing` joins `thread`'s prompts on their way. Whether it is the
    /// first, to be asked of the Server now.
    fn send(&mut self, thread: ThreadKey, outgoing: Outgoing) -> bool {
        let waiting = self.on_the_way.entry(thread).or_default();
        waiting.push_back(outgoing);
        waiting.len() == 1
    }

    /// The text of `thread`'s oldest prompt on its way, to ask. With none
    /// left, the Thread has none on its way.
    fn next(&mut self, thread: ThreadKey) -> Option<String> {
        let next = self
            .on_the_way
            .get(&thread)
            .and_then(|waiting| waiting.front())
            .map(|next| next.text.clone());
        if next.is_none() {
            self.on_the_way.remove(&thread);
        }
        next
    }

    /// `thread`'s oldest prompt on its way, answered: where it came from.
    fn answered(&mut self, thread: ThreadKey) -> Option<Origin> {
        self.on_the_way
            .get_mut(&thread)
            .and_then(VecDeque::pop_front)
            .map(|Outgoing { from, .. }| from)
    }

    /// Keep `held` under `thread`, after the drafts it holds already.
    fn hold(&mut self, thread: ThreadKey, held: Held) {
        self.held.entry(thread).or_default().push(held);
    }

    /// The draft `thread` held, when `prompt` is that prompt handed back by
    /// a Session that never started.
    fn take_held(&mut self, thread: ThreadKey, prompt: &str) -> Option<SentDraft> {
        let held = self.held.get_mut(&thread)?;
        let at = held
            .iter()
            .position(|held| held.draft.prompt().trim() == prompt.trim())?;
        let held = held.remove(at).draft;
        if self.held.get(&thread).is_some_and(Vec::is_empty) {
            self.held.remove(&thread);
        }
        Some(held)
    }

    /// Follow each held draft with its prompt: one the Session took is let
    /// go, one still waiting or handed back and not yet collected is kept,
    /// and one whose Thread closed under it is marked lost. A deleted
    /// Thread's go.
    pub(super) fn settle_held(&mut self, cockpit: &Cockpit) {
        self.held.retain(|&thread, held| {
            let Some(view) = cockpit.thread(thread) else {
                if cockpit.peek(thread).is_err() {
                    return false;
                }
                held.iter_mut().for_each(|held| held.lost = true);
                return true;
            };
            let waiting = view.queue();
            let handed_back = view.recovered().is_some();
            held.retain_mut(|held| {
                let here = waiting
                    .iter()
                    .any(|queued| queued.text.trim() == held.draft.prompt().trim());
                held.listed |= here;
                here || handed_back || !held.listed || held.lost
            });
            !held.is_empty()
        });
    }

    /// The drafts `thread` lost when it closed with a prompt held.
    pub(super) fn take_lost(&mut self, thread: ThreadKey) -> Vec<SentDraft> {
        let Some(held) = self.held.get_mut(&thread) else {
            return Vec::new();
        };
        let (lost, kept) = std::mem::take(held).into_iter().partition(|held| held.lost);
        *held = kept;
        if held.is_empty() {
            self.held.remove(&thread);
        }
        lost.into_iter().map(|held: Held| held.draft).collect()
    }

    /// How many of `thread`'s prompts are on their way.
    #[cfg(test)]
    pub(crate) fn on_the_way(&self, thread: ThreadKey) -> usize {
        self.on_the_way.get(&thread).map_or(0, VecDeque::len)
    }

    /// How many drafts `thread` holds.
    #[cfg(test)]
    pub(crate) fn held(&self, thread: ThreadKey) -> usize {
        self.held.get(&thread).map_or(0, Vec::len)
    }
}

/// What the Server said of a draft on its way.
enum Answered {
    Sent(Admission),
    /// Whether the Thread's queue took it.
    Queued(bool),
}

/// A send's answer as an admission: a link that went before it answered
/// leaves the prompt unconfirmed.
pub(super) fn admission(answer: std::io::Result<Admission>) -> Admission {
    answer.unwrap_or_else(|error| Admission::Unconfirmed(error.to_string()))
}

impl CockpitView {
    /// The Composer's draft goes to `thread`: now, or once the drafts sent
    /// before it are settled.
    pub(super) fn send_draft(
        &mut self,
        thread: ThreadKey,
        draft: SentDraft,
        text: String,
        cited: Vec<Reply>,
        cx: &mut Context<Self>,
    ) {
        self.send_in_turn(thread, text, Origin::Composer { draft, cited }, cx);
    }

    /// Send `prompt` to Pane `index`'s Thread again, queued behind a
    /// running turn. Nothing follows it: a prompt the Thread refuses is
    /// not said.
    pub(super) fn send_again(
        &mut self,
        index: usize,
        thread: ThreadKey,
        prompt: String,
        cx: &mut Context<Self>,
    ) {
        self.send_again_then(index, thread, prompt, |_, _, _| {}, cx);
    }

    /// `send_again`, and `then` hears where the prompt went, or why it
    /// did not, once the Server answers.
    pub(super) fn send_again_then(
        &mut self,
        index: usize,
        thread: ThreadKey,
        prompt: String,
        then: impl FnOnce(&mut Self, Result<Handoff, String>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.focus_pane(index, cx);
        self.send_in_turn(thread, prompt, Origin::Replay(Box::new(then)), cx);
    }

    /// `text` goes to `thread`: now, or once the prompts sent before it
    /// are settled.
    fn send_in_turn(&mut self, thread: ThreadKey, text: String, from: Origin, cx: &mut Context<Self>) {
        if self.sends.send(thread, Outgoing { text, from }) {
            self.ask_next_send(thread, cx);
        }
    }

    /// Ask the Server to take `thread`'s oldest draft on its way: queued
    /// behind a running turn, else sent.
    fn ask_next_send(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        let Some(text) = self.sends.next(thread) else {
            return;
        };
        if self
            .cockpit
            .read(cx)
            .thread(thread)
            .is_some_and(|open| open.needs_queue())
        {
            let queued = self.cockpit.update(cx, |cockpit, _| cockpit.queue(thread, text));
            self.when_answered(
                queued,
                move |view, queued, cx| {
                    view.sent(thread, Answered::Queued(queued.unwrap_or(false)), cx)
                },
                cx,
            );
        } else {
            let sent = self.cockpit.update(cx, |cockpit, _| cockpit.send(thread, text));
            self.when_answered(
                sent,
                move |view, sent, cx| view.sent(thread, Answered::Sent(admission(sent)), cx),
                cx,
            );
        }
    }

    /// The Server answered for `thread`'s oldest prompt on its way: settle
    /// it, and ask for the next.
    fn sent(&mut self, thread: ThreadKey, answered: Answered, cx: &mut Context<Self>) {
        let Some(from) = self.sends.answered(thread) else {
            return;
        };
        match from {
            Origin::Composer { draft, cited } => self.sent_draft(thread, draft, cited, answered, cx),
            Origin::Replay(then) => {
                let handoff = self.replayed(thread, answered, cx);
                then(self, handoff, cx);
            }
        }
        self.ask_next_send(thread, cx);
    }

    /// Settle the Composer's `draft` against its answer, and follow an
    /// admitted prompt.
    fn sent_draft(
        &mut self,
        thread: ThreadKey,
        draft: SentDraft,
        cited: Vec<Reply>,
        answered: Answered,
        cx: &mut Context<Self>,
    ) {
        let admitted = match answered {
            Answered::Queued(true) => {
                self.hold_queued(thread, draft);
                true
            }
            Answered::Queued(false) => {
                self.return_send(thread, draft, cx);
                false
            }
            Answered::Sent(admission) => {
                let admitted = self.settle_send(thread, draft, admission, cx);
                if let Some(index) = self.pane_sending(thread, cx).filter(|_| admitted) {
                    self.scroll_transcript_to_new_prompt(index, cx);
                }
                admitted
            }
        };
        if admitted {
            self.keep_cited_sources(thread, &cited, cx);
        }
        self.facts.acted(self.cockpit.read(cx), thread);
        // A first prompt names an untitled Thread.
        self.facts.renamed(self.cockpit.read(cx), thread);
        self.refresh_names(cx);
        cx.notify();
    }

    /// Where a prompt sent again went, by its answer, or why it did not.
    fn replayed(
        &mut self,
        thread: ThreadKey,
        answered: Answered,
        cx: &mut Context<Self>,
    ) -> Result<Handoff, String> {
        let handoff = match answered {
            Answered::Queued(true) | Answered::Sent(Admission::Held) => Ok(Handoff::Queued),
            Answered::Queued(false) => Err("The agent's queue did not take it.".into()),
            Answered::Sent(Admission::Delivered) => {
                if let Some(index) = self.pane_sending(thread, cx) {
                    self.scroll_transcript_to_new_prompt(index, cx);
                }
                Ok(Handoff::Sent)
            }
            Answered::Sent(Admission::Refused(why)) => Err(why),
            Answered::Sent(Admission::Unconfirmed(why)) => Err(format!(
                "It may or may not have been sent ({why}). Check the transcript before retrying."
            )),
        };
        self.facts.acted(self.cockpit.read(cx), thread);
        cx.notify();
        handoff
    }

    /// The Pane whose Composer sends to `thread`, while one shows it.
    fn pane_sending(&self, thread: ThreadKey, cx: &gpui::App) -> Option<usize> {
        self.panes
            .iter()
            .position(|pane| pane.read(cx).is_main() && pane.thread() == Some(thread))
    }

    /// A draft `thread` did not take goes back to its Composer, ahead of
    /// newer typing: now while a Pane shows the Thread, else when one does.
    fn return_send(&mut self, thread: ThreadKey, draft: SentDraft, cx: &mut Context<Self>) {
        match self.pane_sending(thread, cx) {
            Some(index) => self.put_back_send(index, draft, true, cx),
            None => self.sends.hold(thread, Held {
                draft,
                listed: false,
                lost: true,
            }),
        }
    }

    /// Settle `draft` against what became of its send. Whether the prompt
    /// is on its way (the caller then follows it: scroll, title).
    pub(super) fn settle_send(
        &mut self,
        thread: ThreadKey,
        draft: SentDraft,
        admission: Admission,
        cx: &mut Context<Self>,
    ) -> bool {
        match admission {
            Admission::Delivered => true,
            Admission::Held => {
                let listed = self.cockpit.read(cx).thread(thread).is_some_and(|view| {
                    view.queue()
                        .iter()
                        .any(|queued| queued.text.trim() == draft.prompt().trim())
                });
                self.sends.hold(thread, Held {
                    draft,
                    listed,
                    lost: false,
                });
                true
            }
            Admission::Refused(_) => {
                self.return_send(thread, draft, cx);
                false
            }
            Admission::Unconfirmed(reason) => {
                let named = named(draft.line());
                self.cockpit.update(cx, |cockpit, _| cockpit.say(
                    thread,
                    format!(
                        "\"{named}\" may or may not have been sent ({reason}). It is back in the composer: check the transcript before sending it again."
                    ),
                ));
                self.return_send(thread, draft, cx);
                false
            }
        }
    }

    /// `draft` back into Pane `index`'s Composer, ahead of newer typing when
    /// `prepend`.
    pub(super) fn put_back_send(
        &mut self,
        index: usize,
        draft: SentDraft,
        prepend: bool,
        cx: &mut Context<Self>,
    ) {
        self.panes[index]
            .composer(cx)
            .update(cx, |composer, cx| composer.restore(draft, prepend, cx));
        cx.notify();
    }

    /// Keep `draft` under `thread` while its prompt waits in the Thread's
    /// queue, so taking the prompt back to edit gives the draft back whole.
    pub(super) fn hold_queued(&mut self, thread: ThreadKey, draft: SentDraft) {
        self.sends.hold(thread, Held {
            draft,
            listed: true,
            lost: false,
        });
    }

    /// `prompt`, handed back to edit, as typed parts: the draft it was sent
    /// from while this window still holds it, else its text split into
    /// parts.
    pub(super) fn recovered_content(&mut self, thread: ThreadKey, prompt: String) -> DraftContent {
        match self.sends.take_held(thread, &prompt) {
            Some(draft) => draft.content().clone(),
            None => DraftContent::from_prompt(prompt),
        }
    }


}

fn named(prompt: &str) -> String {
    let prompt = prompt.trim();
    match prompt.char_indices().nth(NAMED_CHARS) {
        Some((end, _)) => format!("{}…", &prompt[..end]),
        None => prompt.to_string(),
    }
}

impl CockpitView {
    pub(super) fn submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes.get(self.focused(cx)).map(|pane| pane.read(cx).composer.clone()).is_some_and(|line| line.update(cx, |line, cx| line.holds_send(window, cx))) {
            return;
        }
        // The card's ↵ is its confirm — but in a machine's folder browser,
        // once it is moved, it adds the folder it shows.
        if let Some(card) = self.project_card(cx) {
            let typed = card.update(cx, |card, cx| card.take_remote_path(window, cx));
            if let Some(path) = typed {
                self.adopt_editor_directory(path, cx);
                return;
            }
            if card.read(cx).ready() {
                self.confirm_project_editor(cx);
            }
            return;
        }
        if let Some(sheet) = self.placement_sheet(cx) {
            sheet.update(cx, |sheet, cx| sheet.confirm(cx));
            return;
        }
        if self.nav_view.read(cx).snooze_line().is_some() {
            self.nav_view.update(cx, |nav, cx| nav.finish_snooze(true, cx));
            return;
        }
        if self.rename.is_open() {
            self.finish_rename(true, cx);
            return;
        }
        // A draft's ↵ (#29): on a band chip it opens that chip's popover;
        // on the prompt line it is the first send — the bootstrap — or a
        // typed-out `/import`.
        if let Some(band_focus) = self
            .panes
            .get(self.focused(cx))
            .and_then(|pane| pane.read(cx).draft())
            .map(|draft| draft.band_focus)
        {
            match band_focus {
                Some(chip) => self.open_band_menu(chip, false, cx),
                None if self.typed_import(window, cx) => {}
                None => self.bootstrap_draft(cx),
            }
            return;
        }
        let Some(thread) = self.focused_thread(cx) else {
            return;
        };
        if !self.panes[self.focused(cx)].read(cx).is_main() {
            return;
        }
        // ↵ on an empty line sends a pending question whose every question
        // is answered (digits toggled a multi-select; ↵ sends it).
        if self.panes[self.focused(cx)].read(cx).composer.read(cx).is_empty() && self.send_ready_question(cx) {
            return;
        }
        if self.typed_import(window, cx) {
            return;
        }
        let composer = self.panes[self.focused(cx)].read(cx).composer.clone();
        if !self.admit_files(self.focused(cx), cx) {
            return;
        }
        let draft = composer.update(cx, |composer, cx| composer.take_draft(cx));
        let text = draft.prompt().trim().to_string();
        let cited = draft.content().replies.clone();
        // `/effort max` and `/model sonnet` typed out are Ferrite's own
        // acts, the same as the pickers: the chip stays truthful, Codex
        // (which has no such command) gets them too, and a level or model
        // Ferrite does not know goes to the provider as text.
        if let Some(handled) = self.typed_tuning(thread, &text, cx) {
            match handled {
                Tuning::Effort(effort) => self.pick_effort(thread, effort, cx),
                Tuning::Model(model) => self.pick_provider(
                    thread,
                    ProviderChoice {
                        provider: self
                            .cockpit
                            .read(cx)
                            .thread(thread)
                            .map(|open| open.provider())
                            .unwrap_or(self.prefs.settings.default_provider),
                        model,
                    },
                    cx,
                ),
            }
            return;
        }
        if text.is_empty() {
            // Enter on an empty line takes a held prompt back to edit it.
            self.take_back_held(thread, cx);
            return;
        }
        // Typing does not wait for the agent; sending does.
        self.send_draft(thread, draft, text, cited, cx);
        cx.notify();
    }

    /// Backspace on an EMPTY Composer line clears the held prompt — the
    /// `⌫ unqueue` the queued row advertises. With text on the line the
    /// Composer consumes the key first and this never runs.
    pub(super) fn unqueue_from_backspace(
        &mut self,
        _: &ferrite_pane::composer::Backspace,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // ⌫ in the palette's line edits the query, never the queue.
        if self.floats.palette.is_some()
            || self.panes.get(self.focused(cx)).is_none_or(|pane| !pane.read(cx).is_main())
        {
            return;
        }
        let Some(thread) = self.focused_thread(cx) else {
            return;
        };
        if !self.panes[self.focused(cx)].read(cx).composer.read(cx).is_empty() {
            return;
        }
        let Some(id) = self
            .cockpit
            .read(cx)
            .thread(thread)
            .and_then(|open| queued_to_take_back(&open))
        else {
            return;
        };
        self.cockpit.update(cx, |cockpit, _| cockpit.remove_queued_item(thread, &id, false));
        cx.notify();
    }

    pub(super) fn interrupt(&mut self, _: &Interrupt, window: &mut Window, cx: &mut Context<Self>) {
        if self.dismiss_popup(cx) {
            return;
        }
        if let Some(transcript) = self.panes.get(self.focused(cx)).and_then(|pane| pane.read(cx).transcript()) {
            if transcript.update(cx, |transcript, cx| transcript.dismiss_offer(cx)) {
                return;
            }
        }
        if self.leave_control(window, cx) {
            return;
        }
        if let Some(connections) = self
            .connections(cx)
            .filter(|connections| connections.read(cx).editing())
        {
            connections.update(cx, |connections, cx| connections.cancel(cx));
            return;
        }
        // The setup sheet's esc closes it; a copy it shows carries on.
        if let Some(sheet) = self.placement_sheet(cx) {
            sheet.update(cx, |sheet, cx| sheet.dismiss(cx));
            return;
        }
        if self.hide_sheet(cx) {
            return;
        }
        if self.nav_view.read(cx).snooze_line().is_some() {
            self.nav_view.update(cx, |nav, cx| nav.finish_snooze(false, cx));
            return;
        }
        if self.rename.is_open() {
            self.finish_rename(false, cx);
            return;
        }
        if !self.cancel_focused_draft_start(cx) {
            cx.notify();
            return;
        }
        // On a draft, escape returns to the prompt (#29): the band's tab
        // focus clears. (An open band menu holds the ComposerMenu keys, so
        // escape there dismisses through `menu_dismiss` instead.)
        if self.update_focused_draft(cx, |draft| draft.band_focus = None).is_some() {
            cx.notify();
            return;
        }
        // Escape stops what the Pane shows: on Main its turn while one runs
        // (T3's Stop, which ends the Thread's PR watches with it), never the
        // Subagents and background work it started, and nothing at rest;
        // on a Subagent that Subagent alone — and once it has nothing to
        // stop, it goes back to its parent.
        if let Some(thread) = self
            .focused_thread(cx)
            .filter(|thread| !self.machine_lost(thread.machine(), cx))
        {
            let selected = self.panes[self.focused(cx)].read(cx).selected.clone();
            match &selected {
                ferrite_core::activity::Subject::Main => {
                    if self.cockpit.read(cx).thread(thread).is_some_and(|open| turn_running(&open)) {
                        self.cockpit.update(cx, |cockpit, _| cockpit.interrupt(thread));
                    }
                }
                ferrite_core::activity::Subject::Subagent(key) => {
                    if !self.stop_subagent(thread, key, cx) {
                        let parent = self
                            .agent_lineage(thread, &selected, cx)
                            .pop()
                            .unwrap_or(ferrite_core::activity::Subject::Main);
                        self.select_subject(thread, parent, window, cx);
                    }
                }
            }
            self.facts.acted(self.cockpit.read(cx), thread);
        }
        cx.notify();
    }

    /// Tab walks a draft's band (#29), or a Thread Pane's rendered tool
    /// disclosures before returning to the Composer.
    /// Tab on an empty Composer that is showing a predicted follow-up puts
    /// that text in the line — editable, unsent. Nothing reaches the provider
    /// without a second, deliberate ↵: ghost text is a guess, and a guess is
    /// not consent to start a turn.
    ///
    /// The condition is `followup::suggest` itself, the same call the idle
    /// line renders from, so the key and the ghost text can never disagree
    /// about whether there is something to accept.
    pub(super) fn accept_suggestion(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if self.sheet_up(cx)
            || self.rename.is_open()
            || self.nav_view.read(cx).snooze_line().is_some()
            || self.composer_menu.read(cx).is_open()
        {
            return false;
        }
        let Some(thread) = self.focused_thread(cx) else {
            return false;
        };
        let Some(open) = self.cockpit.read(cx).thread(thread) else {
            return false;
        };
        let offered = followup::suggest(
            open.pending().is_some(),
            Some(open.transcript()),
            open.suggestion(),
        );
        let Some(text) = offered.acceptable().map(str::to_string) else {
            return false;
        };
        let Some(composer) = self
            .panes
            .get(self.focused(cx))
            .map(|pane| pane.read(cx).composer.clone())
        else {
            return false;
        };
        // Only Tab from the empty input accepts its suggestion. Tab from a
        // transcript action or attachment continues that control's focus walk.
        if !composer.focus_handle(cx).is_focused(window) || !composer.read(cx).is_empty() {
            return false;
        }
        composer.update(cx, |composer, cx| composer.set(text, cx));
        cx.notify();
        true
    }

    /// A typed-out `/import` (the slash menu muted) opens the import
    /// picker, as its menu row does, rather than going to the provider as
    /// text.
    pub(super) fn typed_import(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.panes.get(self.focused(cx)) else {
            return false;
        };
        let (identity, composer) = (pane.identity, pane.read(cx).composer.clone());
        if composer.read(cx).prompt().trim() != "/import" {
            return false;
        }
        let into = match identity {
            PaneIdentity::Draft(_) => Some(identity),
            PaneIdentity::Thread(_) => {
                composer.update(cx, |composer, cx| composer.take(cx));
                self.import_target(cx)
            }
        };
        self.open_import_palette(into, window, cx);
        true
    }
}
