//! The cockpit's side of a transcript: what `sync_transcript` hands each
//! retained `TranscriptView` besides its blocks (its banner, its tail row,
//! the call a pending Decision gates, the checkout its paths resolve
//! against), and the commands that act on the transcript the operator is
//! reading — copy and resend the prompt in view, scroll to a target, open a
//! path's hover card, open a path in a reader.
//!
//! The prompt band answers nothing under the pointer, so copy and resend
//! are commands here (the palette and the keymap call them), acting on the
//! prompt whose band is pinned at the top of the body — the turn being read
//! — else the latest.

use std::path::PathBuf;

use ferrite_core::transcript::Transcript;
use ferrite_core::ThreadId;
use gpui::{App, ClipboardItem, Context, IntoElement, SharedString, Window};

use super::{home_relative, CockpitView};
use ferrite_pane::transcript::{BannerFacts, ScrollTarget, TranscriptTail};

impl CockpitView {
    /// What the banner heading `thread`'s transcript says: its checkout on
    /// its branch, and when its first prompt went out.
    pub(super) fn banner_facts(
        &self,
        thread: ThreadId,
        transcript: &Transcript,
    ) -> Option<BannerFacts> {
        let open = self.cockpit.thread(thread)?;
        let workspace_path =
            ferrite_core::workspace::effective_cwd(open.session_project_root(), open.workspace())
                .map(std::path::Path::to_path_buf)
                .unwrap_or_default();
        let workspace = if workspace_path.as_os_str().is_empty() {
            SharedString::default()
        } else {
            SharedString::from(home_relative(&workspace_path))
        };
        Some(BannerFacts {
            workspace,
            workspace_path,
            branch: self
                .facts
                .get(thread)
                .and_then(|facts| facts.branch.clone()),
            started_at: transcript
                .started_at()
                .map(|at| SharedString::from(at.to_string())),
        })
    }

    /// The row after a transcript's last: the pending Decision, as its
    /// card is built for the tail (`CockpitView::decision_tail`, the
    /// Decision card's owner).
    pub(super) fn transcript_tail(&self, index: usize, cx: &App) -> Option<TranscriptTail> {
        self.decision_tail(index, cx)
            .or_else(|| self.failure_tail(index))
    }

    /// The failure strip (SPEC §5.9 item 4) under Main's transcript when the
    /// Thread's last turn failed: `Stopped <age> ago`, the age the nav row
    /// reads (the Thread's last act), and `Retry`, which re-sends the
    /// failed turn's prompt (`retry_failed_turn`) — none for a closed
    /// Session, which has nothing to send to.
    fn failure_tail(&self, index: usize) -> Option<TranscriptTail> {
        use ferrite_core::transcript::Status;
        let pane = self.panes.get(index)?;
        if !pane.is_main() {
            return None;
        }
        let thread = pane.thread()?;
        let turn = self.cockpit.thread(thread)?.turn();
        if turn.status == Status::Streaming
            || !matches!(turn.outcome, Some(ferrite_core::TurnOutcome::Error(_)))
        {
            return None;
        }
        let closed = turn.status == Status::Closed;
        let age = self
            .facts
            .last_used(thread)
            .map(|at| crate::facts::since_label(at, ferrite_core::clock::system_time()))
            .unwrap_or_default();
        let words = SharedString::from(if age.is_empty() {
            "Stopped just now".to_string()
        } else {
            format!("Stopped {age} ago")
        });
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            ("failure", thread, words.as_ref(), closed).hash(&mut hasher);
            hasher.finish()
        };
        let owner = self.decisions.owner();
        Some(TranscriptTail {
            key,
            render: std::rc::Rc::new(move |_: &mut Window, cx: &mut App| {
                let owner = owner.clone();
                let retry = (!closed).then(|| {
                    ferrite_pane::pane::retry_button(thread.get(), cx)
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |view, cx| view.retry_failed_turn(thread, cx));
                        })
                        .into_any_element()
                });
                ferrite_pane::pane::failure_strip(thread.get(), words.clone(), retry)
                    .into_any_element()
            }),
        })
    }

    /// `Retry` on the failure strip: send the failed turn's prompt again,
    /// attachments and all, through the one resend path. Whether it went.
    pub(crate) fn retry_failed_turn(&mut self, thread: ThreadId, cx: &mut Context<Self>) -> bool {
        let Some(index) = self.pane_for(thread) else {
            return false;
        };
        let prompt = self.cockpit.thread(thread).and_then(|open| {
            let main = open
                .activity()
                .subject(&ferrite_core::activity::Subject::Main)?;
            main.transcript()
                .blocks()
                .iter()
                .rev()
                .find_map(|block| match &block.body {
                    ferrite_core::transcript::Body::Prompt(line) => Some(line.clone()),
                    _ => None,
                })
        });
        match prompt {
            Some(prompt) => self.send_again(index, thread, prompt, cx),
            None => false,
        }
    }

    /// ⌘-click on a path target: the file in the Thread's preview, in the
    /// Panel (`open_preview`), at the link's line. The transcript's event
    /// carries no window, so the preview opens on the next turn of the
    /// loop, in the window that holds the cockpit.
    pub(super) fn open_preview_from(
        &mut self,
        index: usize,
        path: PathBuf,
        line: Option<u32>,
        cx: &mut Context<Self>,
    ) {
        let Some(window) = cx.active_window() else {
            return;
        };
        let cockpit = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = cockpit.update(cx, |view, cx| {
                    view.focus_pane(index);
                    view.open_preview(path, line, window, cx);
                });
            });
        });
    }

    /// The prompt the operator is reading in Pane `index`: the pinned
    /// band's turn, else the latest prompt in view.
    fn reading_prompt(&self, index: usize, cx: &App) -> Option<String> {
        self.panes
            .get(index)?
            .transcript()?
            .read(cx)
            .reading_prompt()
    }

    /// Copy the prompt being read in Pane `index` (its words, without its
    /// attachments). Whether there was one.
    pub(crate) fn copy_reading_prompt(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        let Some(prompt) = self.reading_prompt(index, cx) else {
            return false;
        };
        let (words, _) = ferrite_core::prompt_files::split(prompt);
        cx.write_to_clipboard(ClipboardItem::new_string(words));
        true
    }

    /// Send the prompt being read in Pane `index` again, attachments and
    /// all — queued behind a running turn. Only a Thread's Main Subject
    /// takes prompts. Whether it went.
    pub(crate) fn resend_reading_prompt(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        if !self.panes.get(index).is_some_and(|pane| pane.is_main()) {
            return false;
        }
        let Some(thread) = self.panes[index].thread() else {
            return false;
        };
        let Some(prompt) = self.reading_prompt(index, cx) else {
            return false;
        };
        self.send_again(index, thread, prompt, cx)
    }

    /// Send `prompt` to Pane `index`'s Thread again, queued behind a
    /// running turn.
    fn send_again(
        &mut self,
        index: usize,
        thread: ThreadId,
        prompt: String,
        cx: &mut Context<Self>,
    ) -> bool {
        self.focus_pane(index);
        if self
            .cockpit
            .thread(thread)
            .is_some_and(|open| open.needs_queue())
        {
            self.cockpit.queue(thread, prompt);
        } else {
            self.cockpit.send(thread, prompt);
            self.scroll_transcript_to_bottom(index, cx);
        }
        self.facts.acted(&self.cockpit, thread);
        cx.notify();
        true
    }

    /// Scroll Pane `index`'s transcript to `target`: the banner, the live
    /// tail, or a turn's band with its output under it (a scene's views).
    #[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
    pub(crate) fn scroll_transcript(
        &mut self,
        index: usize,
        target: ScrollTarget,
        cx: &mut Context<Self>,
    ) {
        if let Some(transcript) = self.panes.get(index).and_then(|pane| pane.transcript()) {
            transcript.update(cx, |transcript, cx| transcript.scroll_to(target, cx));
        }
    }
}
