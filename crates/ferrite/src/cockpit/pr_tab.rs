//! The panel's PR tab (SPEC §5.6.2): a read-only view of the branch's pull
//! request, answering at a glance "can this merge, and what's in the way",
//! and saying what it is. Top to bottom: the head (state pill, `#N`,
//! refresh and open buttons, title, branches), the readiness group, the
//! description (its body, clamped), the checks, the comments, and `Review
//! PR`. With no pull request, the empty state; with no `gh` or no login,
//! its line says how to sign in.
//!
//! It reads the Review's own snapshot (`Cockpit::review_pull_request`, the
//! forge's CLI where the checkout is), off the UI thread: when the tab is
//! opened, then once per `pr::AUTO_EVERY` while it shows and the window has
//! the operator's focus — never while it is hidden (`tend_pr_tab`). The last
//! snapshot stays on screen while the next is read, the `refresh` icon
//! turning; before the first lands, the tab says what the nav label's
//! reading knows (`PrWatch`): the head, or — the label knowing nothing
//! either — that it is checking. Each snapshot also moves that label: its
//! checks and state are the newer word.
//!
//! The snapshot has no per-check page or duration, no review decision and
//! no PR author (SPEC §8), and only says a fork head is a fork, not whose:
//! none of them is drawn.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use super::panel::PanelTab;
use super::*;
use crate::icons::{self, icon};
use ferrite_core::protocol::{
    ReviewPullRequest, WireCheckStatus, WireMerge, WirePr, WirePrComment, WirePrCommentKind,
    WirePrState, WirePrUnread, WireSync,
};
use ferrite_core::workspace::{PrState, PullRequest};
use gpui::base::text::{InlineCodeWash, TextView, TextViewState, TextViewStyle};
use gpui::StyledText;

/// The PR tab's state on the cockpit.
#[derive(Default)]
pub(crate) struct PrTab {
    /// Each Thread's reads.
    reads: HashMap<ThreadId, PrRead>,
    /// The Thread whose tab showed at the last pump: another one, or the
    /// tab coming back, is an opening.
    shown: Option<ThreadId>,
    /// What the operator unfolded, per Thread.
    unfolded: HashSet<(ThreadId, Unfold)>,
}

/// One Thread's PR reads.
#[derive(Default)]
struct PrRead {
    /// The last pull request read: on screen while the next is fetched.
    pr: Option<WirePr>,
    /// Why the last answer had none; `None` before the first answer and
    /// beside a snapshot.
    absent: Option<Absent>,
    /// A read is on its way.
    fetching: bool,
    /// When the last read was asked for.
    asked: Option<Instant>,
}

/// Why the tab has no pull request to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Absent {
    /// The branch has none.
    NoPr,
    /// `gh` is missing or signed out.
    NoCli,
}

/// What the operator unfolded.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Unfold {
    /// A comment card, by its author and stamp.
    Comment(String),
    /// The `N resolved` fold.
    Resolved,
    /// The checks past the first `PR_CHECKS_SHOWN`.
    Checks,
    /// The description past its clamp.
    Description,
}

impl PrTab {
    /// The pull request's page, when a snapshot named it.
    pub(super) fn url(&self, thread: ThreadId) -> Option<&str> {
        let pr = self.reads.get(&thread)?.pr.as_ref()?;
        (!pr.url.is_empty()).then_some(pr.url.as_str())
    }

    /// Whether `thread`'s cadence is due at `now`: none on its way, and
    /// the last asked at least `AUTO_EVERY` ago.
    fn due(&self, thread: ThreadId, now: Instant) -> bool {
        self.reads.get(&thread).is_none_or(|read| {
            !read.fetching
                && read.asked.is_none_or(|asked| {
                    now.saturating_duration_since(asked) >= crate::pr::AUTO_EVERY
                })
        })
    }

    /// Whether a read of `thread`'s pull request is on its way.
    #[cfg(test)]
    pub(crate) fn fetching(&self, thread: ThreadId) -> bool {
        self.reads.get(&thread).is_some_and(|read| read.fetching)
    }

    /// Hold a first read of `thread`'s pull request out, as though `gh`
    /// were still answering.
    #[cfg(test)]
    pub(crate) fn hold_first_read(&mut self, thread: ThreadId) {
        self.reads.insert(
            thread,
            PrRead {
                fetching: true,
                ..PrRead::default()
            },
        );
    }

    fn unfolded(&self, thread: ThreadId, unfold: &Unfold) -> bool {
        self.unfolded.contains(&(thread, unfold.clone()))
    }

    fn toggle(&mut self, thread: ThreadId, unfold: Unfold) {
        let key = (thread, unfold);
        if !self.unfolded.remove(&key) {
            self.unfolded.insert(key);
        }
    }

    /// Adopt an answer. The snapshot it carries, if any, for the label.
    fn landed(
        &mut self,
        thread: ThreadId,
        answer: std::io::Result<ReviewPullRequest>,
    ) -> Option<&WirePr> {
        let read = self.reads.entry(thread).or_default();
        read.fetching = false;
        match answer {
            Ok(ReviewPullRequest { pr: Ok(pr), .. }) => {
                read.pr = Some(pr);
                read.absent = None;
            }
            // A failed read keeps the last snapshot standing.
            Err(_)
            | Ok(ReviewPullRequest {
                unread: Some(WirePrUnread::Failed),
                ..
            }) => {}
            Ok(ReviewPullRequest {
                unread: Some(WirePrUnread::NoCli),
                ..
            }) => {
                read.pr = None;
                read.absent = Some(Absent::NoCli);
            }
            Ok(ReviewPullRequest { unread: None, .. }) => {
                read.pr = None;
                read.absent = Some(Absent::NoPr);
            }
        }
        read.pr.as_ref()
    }
}

impl CockpitView {
    /// The Thread whose PR tab is on screen, if one is.
    fn pr_tab_thread(&self) -> Option<ThreadId> {
        let index = self.panel_pane()?;
        let thread = self.panes[index].thread()?;
        (self.shown_tab(index) == PanelTab::Pr).then_some(thread)
    }

    /// Each pump: read the snapshot of the PR tab on screen when it has
    /// just opened, and then on the cadence while the window is focused.
    /// A hidden tab reads nothing.
    pub(super) fn tend_pr_tab(&mut self, cx: &mut Context<Self>) {
        let shown = self.pr_tab_thread();
        let opened = shown != self.pr_tab.shown;
        self.pr_tab.shown = shown;
        let Some(thread) = shown else {
            return;
        };
        let now = cx.background_executor().now();
        if opened || (self.cockpit.window_active() && self.pr_tab.due(thread, now)) {
            self.read_pr_tab(thread, cx);
        }
    }

    /// Read `thread`'s pull request now, unless a read is on its way.
    pub(super) fn read_pr_tab(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(path) = self.thread_path(thread) else {
            return;
        };
        let now = cx.background_executor().now();
        let read = self.pr_tab.reads.entry(thread).or_default();
        if read.fetching {
            return;
        }
        read.asked = Some(now);
        #[cfg(test)]
        self.pr_tab_reads.push(thread);
        // Tests and captures never run `gh`: only a fixture world's forge
        // answers them.
        if cfg!(any(test, feature = "visual-reference")) && !self.cockpit.has_forge_fixture(&path) {
            return;
        }
        if let Some(read) = self.pr_tab.reads.get_mut(&thread) {
            read.fetching = true;
        }
        cx.notify();
        let answer = self.cockpit.review_pull_request(&path);
        cx.spawn(async move |this, cx| {
            let answer = beside::answered(answer, cx).await;
            this.update(cx, |view, cx| view.pr_tab_landed(thread, answer, cx))
                .ok();
        })
        .detach();
    }

    /// Adopt a snapshot read; its checks and state move the nav's label.
    fn pr_tab_landed(
        &mut self,
        thread: ThreadId,
        answer: std::io::Result<ReviewPullRequest>,
        cx: &mut Context<Self>,
    ) {
        if let Some(pr) = self.pr_tab.landed(thread, answer) {
            let reading = PullRequest::of_wire(pr);
            if self.facts.pr_adopt(thread, reading) {
                self.notify_part(parts::Part::Nav, cx);
            }
        }
        cx.notify();
    }

    /// The head's `refresh`: the label's read (`refresh PR status`) and a
    /// new snapshot, both now.
    fn refresh_pr_tab(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        self.check_pr(thread, true, cx);
        self.read_pr_tab(thread, cx);
    }

    /// `Review PR`: the Review beside the Pane, on its pull request
    /// (ADR 0021), where findings read against the diff.
    fn open_review_pr(&mut self, thread: ThreadId, window: &mut Window, cx: &mut Context<Self>) {
        self.open_beside(
            beside::Beside::Review(ferrite_reviewr::model::Scope::Branch),
            window,
            cx,
        );
        if let Some(review) = self.review_of(thread) {
            review.update(cx, |review, cx| review.open_pr(cx));
        }
    }

    /// Unfold `thread`'s description, as a scene showing it whole does.
    #[cfg(feature = "visual-reference")]
    pub(crate) fn fixture_unfold_description(&mut self, thread: ThreadId) {
        self.pr_tab.unfolded.insert((thread, Unfold::Description));
    }

    /// Open `url`, or the pull request's own page without one.
    fn open_pr_link(&mut self, thread: ThreadId, url: Option<String>, cx: &mut Context<Self>) {
        match url {
            Some(url) => cx.open_url(&url),
            None => self.open_pr(thread, cx),
        }
    }

    // ------------------------------------------------------------ drawing

    /// The PR tab's body (see the module doc).
    pub(super) fn panel_pr(
        &self,
        thread: ThreadId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let read = self.pr_tab.reads.get(&thread);
        let fetching = read.is_some_and(|read| read.fetching);
        if let Some(pr) = read.and_then(|read| read.pr.as_ref()) {
            return self.pr_view(thread, pr, fetching, window, cx);
        }
        match read.and_then(|read| read.absent) {
            Some(absent) => self.no_pr(thread, absent),
            // The first read is out: what the label's reading knows.
            None => match self.facts.pr(thread) {
                Some(reading) => {
                    let pill = Pill::of(
                        match reading.state {
                            PrState::Open => WirePrState::Open,
                            PrState::Merged => WirePrState::Merged,
                            PrState::Closed => WirePrState::Closed,
                        },
                        reading.draft,
                    );
                    div()
                        .flex()
                        .flex_col()
                        .child(self.pr_head(thread, pill, reading.number, None, fetching, cx))
                        .into_any_element()
                }
                // Nor has the label's: say it is asking rather than claim
                // there is none.
                None if fetching => checking(),
                // Nobody is asking: no pull request, as far as is known.
                None => self.no_pr(thread, Absent::NoPr),
            },
        }
    }

    /// A pull request, whole.
    fn pr_view(
        &self,
        thread: ThreadId,
        pr: &WirePr,
        fetching: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::theme::*;
        let pill = Pill::of(pr.state, pr.draft);
        let review_id = gpui::ElementId::from("pr-review");
        let review_key = crate::pointer::hover_key(&review_id);
        div()
            .debug_selector(|| "pr-tab".into())
            .flex()
            .flex_col()
            .child(self.pr_head(thread, pill, pr.number, Some(pr), fetching, cx))
            .children(readiness_group(pr))
            .children(self.description_section(thread, pr, cx))
            .children(self.checks_section(thread, pr, cx))
            .children(self.comments_section(thread, pr, window, cx))
            .child(
                div()
                    .id(review_id)
                    .debug_selector(|| "pr-review".into())
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .h(px(PANEL_BUTTON_H))
                    .mt(px(PR_SECTION_GAP))
                    .rounded(px(R_CONTROL))
                    .bg(paint::RAISED)
                    .text_size(px(FS_LABEL))
                    .font_weight(W_LABEL)
                    .text_color(rgb(TEXT_STRONG))
                    .hover_control(review_key)
                    .press_control()
                    .child("Review PR")
                    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                        view.open_review_pr(thread, window, cx);
                    })),
            )
            .into_any_element()
    }

    /// The head (padding 2 2 14): the pill, `#N`, `refresh` and `external`;
    /// then, from a snapshot, the title and the branches.
    fn pr_head(
        &self,
        thread: ThreadId,
        pill: Pill,
        number: u64,
        pr: Option<&WirePr>,
        fetching: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        use crate::theme::*;
        let refresh = pr_icon_button("pr-refresh", "Refresh PR", "cockpit::RefreshPrStatus")
            .child(if fetching {
                spinning_refresh()
            } else {
                icon(icons::REFRESH, ICON_PR_BUTTON_GLYPH, TEXT_DIM).into_any_element()
            })
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.refresh_pr_tab(thread, cx);
            }));
        let external = pr_icon_button("pr-external", "Open on GitHub", "cockpit::OpenPrInBrowser")
            .child(icon(icons::EXTERNAL, ICON_PR_BUTTON_GLYPH, TEXT_DIM))
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                view.open_pr(thread, cx);
            }));
        div()
            .debug_selector(|| "pr-head".into())
            .flex()
            .flex_col()
            .flex_shrink_0()
            .pt(px(SPACE_0_5))
            .px(px(SPACE_0_5))
            .pb(px(PR_SECTION_GAP))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .child(pill.element())
                    .child(
                        div()
                            .font_family(FONT_CODE)
                            .text_size(px(FS_META))
                            .text_color(rgb(TEXT_DIM))
                            .child(SharedString::from(format!("#{number}"))),
                    )
                    .child(div().flex_1())
                    .child(refresh)
                    .child(external),
            )
            .children(pr.map(|pr| {
                div()
                    .debug_selector(|| "pr-title".into())
                    .mt(px(PR_TITLE_GAP))
                    // A line's trailing space hangs past the edge, as CSS
                    // lets it: GPUI counts it when wrapping.
                    .mr(px(-SPACE_1))
                    .whitespace_normal()
                    .line_clamp(3)
                    .text_size(px(FS_PR_TITLE))
                    .line_height(px(LH_UI))
                    .font_weight(W_STRONG)
                    .text_color(rgb(TEXT_STRONG))
                    .child(SharedString::from(pr.title.clone()))
            }))
            .children(pr.map(|pr| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(SPACE_1_5))
                    .mt(px(SPACE_2))
                    .min_w_0()
                    .text_size(px(FS_META))
                    .text_color(rgb(TEXT_DIM))
                    .child(icon(icons::BRANCH, 13., TEXT_DIM))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(TEXT_BAR))
                            .child(SharedString::from(pr.head.clone())),
                    )
                    .child(icon(icons::ARROW_RIGHT, ICON_CHEVRON, TEXT_FAINT))
                    .child(
                        div()
                            .flex_shrink_0()
                            .child(SharedString::from(pr.base.clone())),
                    )
            }))
    }

    /// The description (a heading, then the pull request's body as its
    /// author wrote it, Markdown set small), clamped to
    /// `PR_DESCRIPTION_LINES` until the operator unfolds it; none for an
    /// empty body.
    fn description_section(
        &self,
        thread: ThreadId,
        pr: &WirePr,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let body = pr.body.trim();
        if body.is_empty() {
            return None;
        }
        let unfolded = self.pr_tab.unfolded(thread, &Unfold::Description);
        Some(
            div()
                .debug_selector(|| "pr-description".into())
                .flex()
                .flex_col()
                .child(section_head("pr-description-head", "Description", None))
                .child(Description {
                    id: SharedString::from(format!("pr-description-{}", thread.get())),
                    body: SharedString::from(body.to_string()),
                    unfolded,
                    toggle: Box::new(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.pr_tab.toggle(thread, Unfold::Description);
                        cx.notify();
                    })),
                }),
        )
    }

    /// The checks (a heading with their count, a 30px row each, worst
    /// first), none without checks.
    fn checks_section(&self, thread: ThreadId, pr: &WirePr, cx: &mut Context<Self>) -> Option<Div> {
        use crate::theme::*;
        if pr.checks.is_empty() {
            return None;
        }
        let mut checks: Vec<_> = pr.checks.iter().collect();
        checks.sort_by_key(|check| check_rank(check.status));
        let unfolded = self.pr_tab.unfolded(thread, &Unfold::Checks);
        let hidden = if unfolded {
            0
        } else {
            checks.len().saturating_sub(PR_CHECKS_SHOWN)
        };
        // The snapshot names no check's page; the label's `gh` read may.
        let runs = self.facts.pr(thread).map(|reading| reading.runs.clone());
        let mut section = div().flex().flex_col().child(section_head(
            "pr-checks-head",
            "Checks",
            Some(checks.len().to_string()),
        ));
        for (index, check) in checks.iter().take(checks.len() - hidden).enumerate() {
            let url = runs.as_ref().and_then(|runs| {
                runs.iter()
                    .find(|run| run.name == check.name)
                    .and_then(|run| run.url.clone())
            });
            let (mark, word, word_ink) = check_face(check.status);
            section = section.child(
                flush_row(gpui::ElementId::from(("pr-check", index)), SPACE_0_5)
                    .debug_selector(move || format!("pr-check-{index}"))
                    .gap(px(PR_ROW_GAP))
                    .child(mark)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(SharedString::from(check.name.clone())),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(FS_META))
                            .text_color(rgb(word_ink))
                            .child(word),
                    )
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.open_pr_link(thread, url.clone(), cx);
                    })),
            );
        }
        let more = pr.more_checks();
        if hidden > 0 {
            let words = if more {
                format!("Show {hidden} more \u{b7} more on GitHub")
            } else {
                format!("Show {hidden} more")
            };
            section = section.child(quiet_row("pr-checks-more", words).on_click(cx.listener(
                move |view, _: &ClickEvent, _, cx| {
                    view.pr_tab.toggle(thread, Unfold::Checks);
                    cx.notify();
                },
            )));
        } else if more {
            section = section.child(self.on_github_row("pr-checks-more", thread, cx));
        }
        Some(section)
    }

    /// The comments: a heading with their tally; the unresolved ones as
    /// cards, newest first; the resolved folded into one row; `More on
    /// GitHub` when the forge holds more. None without comments.
    fn comments_section(
        &self,
        thread: ThreadId,
        pr: &WirePr,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        use crate::theme::*;
        if pr.comments.is_empty() && !pr.more_comments() {
            return None;
        }
        let (open, resolved) = sorted_comments(&pr.comments);
        let mut cards = div().flex().flex_col().gap(px(SPACE_1_5));
        for (index, comment) in open.iter().enumerate() {
            cards = cards.child(self.comment_card(thread, index, comment, window, cx));
        }
        if !resolved.is_empty() {
            let unfolded = self.pr_tab.unfolded(thread, &Unfold::Resolved);
            let words = format!("{} resolved", resolved.len());
            cards = cards.child(
                fold_row(
                    "pr-resolved",
                    format!("pr-resolved-turn-{}", thread.get()).into(),
                    unfolded,
                    words,
                )
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.pr_tab.toggle(thread, Unfold::Resolved);
                    cx.notify();
                })),
            );
            if unfolded {
                for (index, comment) in resolved.iter().enumerate() {
                    let index = open.len() + index;
                    cards = cards.child(
                        self.comment_card(thread, index, comment, window, cx)
                            .opacity(PR_RESOLVED_OPACITY),
                    );
                }
            }
        }
        if pr.more_comments() {
            cards = cards.child(self.on_github_row("pr-comments-more", thread, cx));
        }
        Some(
            div()
                .flex()
                .flex_col()
                .child(section_head(
                    "pr-comments-head",
                    "Comments",
                    Some(comments_tally(&pr.comments)),
                ))
                .child(cards),
        )
    }

    /// One comment's card: who, what kind, how old; a finding's anchor; its
    /// body clamped to three lines. A click unfolds the whole body, the
    /// finding's hunk and the replies, in place.
    fn comment_card(
        &self,
        thread: ThreadId,
        index: usize,
        comment: &WirePrComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        use crate::theme::*;
        let key = Unfold::Comment(format!("{}\u{0}{}", comment.author, comment.created_at));
        let unfolded = self.pr_tab.unfolded(thread, &key);
        let now = ferrite_core::clock::system_time();
        let age = |stamp: &str| {
            ferrite_core::clock::parse_stamp(stamp)
                .map(|at| crate::facts::age_label(at, now))
                .unwrap_or_default()
        };
        let finding = comment.kind == WirePrCommentKind::Finding;
        let line = div()
            .flex()
            .items_center()
            .gap(px(SPACE_2))
            .min_w_0()
            .text_size(px(FS_META))
            .text_color(rgb(TEXT_DIM))
            .child(avatar(&comment.author))
            .child(author(&comment.author))
            .child(kind_word(comment.kind))
            .when(comment.outdated, |line| line.child("outdated"))
            .when(finding && !comment.resolved, |line| {
                line.child(
                    div()
                        .text_size(px(FS_CHROME_CODE))
                        .text_color(rgb(ATTENTION))
                        .child("unresolved"),
                )
            })
            .child(div().flex_1())
            .child(div().flex_shrink_0().child(age(&comment.created_at)));
        let anchor = finding.then(|| Anchor::parse(&comment.anchor)).flatten();
        let cwd = self.thread_path(thread);
        let id = SharedString::from(format!("pr-comment-{index}"));
        let mut card = div()
            .id(id.clone())
            .debug_selector(move || format!("pr-comment-{index}"))
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(SPACE_1_5))
            .py(px(PR_COMMENT_PAD_Y))
            .px(px(SPACE_3))
            .rounded(px(R_BLOCK))
            .bg(paint::RAISED)
            .cursor_pointer()
            .child(line)
            .children(anchor.map(|anchor| {
                let path = anchor.path.clone();
                let start = anchor.start;
                div()
                    .id(("pr-comment-anchor", index))
                    .debug_selector(move || format!("pr-comment-{index}-anchor"))
                    .truncate()
                    .font_family(FONT_CODE)
                    .text_size(px(FS_CHROME_CODE))
                    .text_color(rgb(TEXT_MUTED))
                    .child(SharedString::from(anchor.label()))
                    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        let raw = std::path::PathBuf::from(&path);
                        let path = match &cwd {
                            Some(cwd) if !raw.is_absolute() => cwd.join(raw),
                            _ => raw,
                        };
                        view.open_preview(path, Some(start), window, cx);
                    }))
            }))
            .child(comment_body(
                format!("{id}-body"),
                &comment.body,
                !unfolded,
                window,
            ));
        if unfolded {
            if let Some(snippet) = comment.snippet.as_deref().filter(|s| !s.is_empty()) {
                card = card.child(hunk(snippet));
            }
            if !comment.replies.is_empty() {
                card = card.child(
                    div()
                        .debug_selector(move || format!("pr-comment-{index}-replies"))
                        .flex()
                        .flex_col()
                        .gap(px(SPACE_1_5))
                        .pl(px(PR_REPLY_INDENT))
                        .children(comment.replies.iter().enumerate().map(|(at, reply)| {
                            div()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(SPACE_2))
                                        .text_size(px(FS_META))
                                        .text_color(rgb(TEXT_DIM))
                                        .child(author(&reply.author))
                                        .child(age(&reply.created_at)),
                                )
                                .child(comment_body(
                                    format!("{id}-reply-{at}"),
                                    &reply.body,
                                    false,
                                    window,
                                ))
                        })),
                );
            }
        }
        card.on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
            view.pr_tab.toggle(thread, key.clone());
            cx.notify();
        }))
    }

    /// A quiet row opening the pull request on GitHub.
    fn on_github_row(
        &self,
        id: &'static str,
        thread: ThreadId,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        quiet_row(id, "More on GitHub".into()).on_click(cx.listener(
            move |view, _: &ClickEvent, _, cx| {
                view.open_pr(thread, cx);
            },
        ))
    }

    /// The empty state: no pull request for the branch, or — with no `gh`
    /// or no login — how to sign in.
    fn no_pr(&self, thread: ThreadId, absent: Absent) -> AnyElement {
        use crate::theme::*;
        let (text, highlight) = match absent {
            Absent::NoPr => {
                let branch = self
                    .facts
                    .get(thread)
                    .and_then(|facts| facts.branch.clone());
                let lead = "When a pull request is open for ";
                let tail = ", its checks, merge state and comments show here.";
                match branch {
                    Some(branch) => {
                        let at = lead.len()..lead.len() + branch.len();
                        (format!("{lead}{branch}{tail}"), Some(at))
                    }
                    None => (format!("{lead}this branch{tail}"), None),
                }
            }
            Absent::NoCli => {
                let lead = "Ferrite reads pull requests with the GitHub CLI (gh). Sign in with ";
                let login = "gh auth login";
                let at = lead.len()..lead.len() + login.len();
                (format!("{lead}{login} to see them here."), Some(at))
            }
        };
        let highlights = highlight
            .map(|range| {
                (
                    range,
                    gpui::HighlightStyle {
                        color: Some(rgb(TEXT_BAR).into()),
                        ..Default::default()
                    },
                )
            })
            .into_iter()
            .collect::<Vec<_>>();
        let selector = match absent {
            Absent::NoPr => "panel-no-pr",
            Absent::NoCli => "panel-no-gh",
        };
        div()
            .debug_selector(move || selector.into())
            .flex()
            .flex_col()
            .items_start()
            .gap(px(PR_EMPTY_GAP))
            .pt(px(PR_EMPTY_PAD_T))
            .px(px(SPACE_1))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(PR_EMPTY_BADGE))
                    .rounded(px(R_PANE))
                    .bg(paint::RAISED)
                    .child(icon(icons::GIT_PULL_REQUEST, PR_EMPTY_ICON, TEXT_MUTED)),
            )
            .child(
                div()
                    .text_size(px(FS_TITLE))
                    .font_weight(W_STRONG)
                    .text_color(rgb(TEXT_STRONG))
                    .child("No pull request"),
            )
            .child(
                div()
                    .w_full()
                    .whitespace_normal()
                    .text_size(px(FS_LABEL))
                    .line_height(px(PR_BODY_LH))
                    .text_color(rgb(TEXT_DIM))
                    .child(StyledText::new(text).with_highlights(highlights)),
            )
            .into_any_element()
    }
}

// ---------------------------------------------------------------- pieces

/// The state pill's four kinds (SPEC §5.6.2). Merged takes no purple:
/// colour stays for state that needs the operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pill {
    Open,
    Draft,
    Merged,
    Closed,
}

impl Pill {
    fn of(state: WirePrState, draft: bool) -> Self {
        match (state, draft) {
            (WirePrState::Merged, _) => Pill::Merged,
            (WirePrState::Closed, _) => Pill::Closed,
            (WirePrState::Open | WirePrState::Unknown, true) => Pill::Draft,
            (WirePrState::Open | WirePrState::Unknown, false) => Pill::Open,
        }
    }

    fn element(self) -> Div {
        use crate::theme::*;
        let (glyph, word, ink) = match self {
            Pill::Open => (icons::GIT_PULL_REQUEST, "Open", RUNNING),
            Pill::Draft => (icons::GIT_PULL_REQUEST, "Draft", TEXT_MUTED),
            Pill::Merged => (icons::GIT_MERGE, "Merged", TEXT),
            Pill::Closed => (icons::GIT_PULL_REQUEST, "Closed", TEXT_DIM),
        };
        div()
            .debug_selector(move || format!("pr-pill-{self:?}"))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(PR_PILL_GAP))
            .h(px(PR_PILL_H))
            .pl(px(PR_PILL_PAD_L))
            .pr(px(PR_PILL_PAD_R))
            .rounded_full()
            .map(|pill| match self {
                Pill::Open => pill.bg(paint::PR_OPEN_PILL),
                Pill::Draft | Pill::Merged | Pill::Closed => pill.bg(paint::SELECTED),
            })
            .text_size(px(FS_META))
            .font_weight(W_LABEL)
            .text_color(rgb(ink))
            .child(icon(glyph, PR_PILL_ICON, ink))
            .child(word)
    }
}

/// A 26px icon button of the head (r6), its tooltip naming its act.
fn pr_icon_button(id: &'static str, label: &'static str, action: &'static str) -> Stateful<Div> {
    use crate::theme::*;
    let element = gpui::ElementId::from(id);
    let key = crate::pointer::hover_key(&element);
    div()
        .id(element)
        .debug_selector(move || id.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(PR_ICON_BUTTON))
        .rounded(px(R_TAB))
        .hover_control(key)
        .press_control()
        .tooltip(crate::menu::action_tooltip(label, action))
}

/// The `refresh` icon while a read is out: turning, or — under reduced
/// motion — dimmed.
fn spinning_refresh() -> AnyElement {
    crate::loops_overlay::hosted(|| RefreshSpin.into_any_element()).into_any_element()
}

#[derive(IntoElement, Clone, Copy)]
struct RefreshSpin;

impl RenderOnce for RefreshSpin {
    fn render(self, window: &mut Window, cx: &mut gpui::App) -> impl IntoElement {
        use crate::theme::*;
        let glyph = icon(icons::REFRESH, ICON_PR_BUTTON_GLYPH, TEXT_DIM);
        let glyph = if crate::motion::reduced_motion(cx) {
            glyph.opacity(0.5)
        } else {
            let turn = std::time::Duration::from_millis(PR_SPIN_MS);
            let steps = crate::motion::Motion::Frames(PR_SPIN_STEPS);
            let phase = crate::motion::script_phase(turn, steps, window.current_view(), cx);
            let step = crate::components::frame_at(phase, PR_SPIN_STEPS);
            let angle = std::f32::consts::TAU * step as f32 / PR_SPIN_STEPS as f32;
            glyph.with_transformation(gpui::Transformation::rotate(gpui::radians(angle)))
        };
        div()
            .debug_selector(|| "pr-refresh-spinning".into())
            .child(glyph)
    }
}

/// A section's heading (30): its name in 12/500, its count, if it has
/// one, at the right.
fn section_head(selector: &'static str, name: &'static str, count: Option<String>) -> Div {
    use crate::theme::*;
    div()
        .debug_selector(move || selector.into())
        .flex()
        .items_center()
        .h(px(PANEL_SECTION_H))
        .mt(px(PR_SECTION_GAP))
        .px(px(SPACE_0_5))
        .text_size(px(FS_META))
        .text_color(rgb(TEXT_DIM))
        .child(div().flex_1().font_weight(W_LABEL).child(name))
        .children(count.map(SharedString::from))
}

/// A 30px row a click opens or unfolds, its content `inset` into the
/// column as the section heads' is: under the pointer its hover ground
/// reaches `PR_ROW_BLEED` past the column either side, so the mark or text
/// keeps its place and the ground still clears it.
fn flush_row(id: gpui::ElementId, inset: f32) -> Stateful<Div> {
    use crate::theme::*;
    let key = crate::pointer::hover_key(&id);
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(PANEL_SECTION_H))
        .mx(px(-PR_ROW_BLEED))
        .px(px(inset + PR_ROW_BLEED))
        .rounded(px(R_CONTROL))
        .hover_row(key)
        .press_row()
}

/// A quiet row (`Show N more`, `More on GitHub`).
fn quiet_row(id: &'static str, words: String) -> Stateful<Div> {
    use crate::theme::*;
    flush_row(id.into(), SPACE_1)
        .debug_selector(move || id.into())
        .text_size(px(FS_LABEL))
        .text_color(rgb(TEXT_DIM))
        .child(SharedString::from(words))
}

/// A fold's quiet row (`N resolved`, `Show more`): its chevron turning a
/// quarter as it unfolds, then its words. The chevron's turn is kept under
/// `turn`, its Thread's own, so another Thread's fold never turns it.
fn fold_row(
    id: &'static str,
    turn: SharedString,
    unfolded: bool,
    words: impl Into<SharedString>,
) -> Stateful<Div> {
    use crate::theme::*;
    flush_row(id.into(), SPACE_1)
        .debug_selector(move || id.into())
        .gap(px(SPACE_2))
        .text_size(px(FS_LABEL))
        .text_color(rgb(TEXT_DIM))
        .child(super::panel::fold_chevron(
            turn,
            unfolded,
            ICON_CHEVRON,
            TEXT_DIM,
        ))
        .child(words.into())
}

/// The first read is out and nothing is known yet: a quiet line where the
/// tab will draw, as the Context tab's before its first reading.
fn checking() -> AnyElement {
    use crate::theme::*;
    div()
        .debug_selector(|| "panel-pr-checking".into())
        .pt(px(SPACE_2))
        .px(px(PANEL_ROW_ICON_X))
        .text_size(px(FS_META))
        .text_color(rgb(TEXT_DIM))
        .child("Checking for a pull request\u{2026}")
        .into_any_element()
}

/// The description's body and its fold (`description_section`). The body
/// keeps its own document state per Thread, so the clamp can say whether
/// it cut anything (`TextViewState::is_clamped`): `Show more` shows only
/// when it did; unfolded, `Show less` always does.
#[derive(IntoElement)]
struct Description {
    id: SharedString,
    body: SharedString,
    unfolded: bool,
    toggle: OnClick,
}

/// A click's listener, carried into an element built later.
type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut gpui::App)>;

impl RenderOnce for Description {
    fn render(self, window: &mut Window, cx: &mut gpui::App) -> impl IntoElement {
        use crate::theme::*;
        let body = self.body;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| {
            TextViewState::markdown(&body, cx)
        });
        state.update(cx, |state, cx| state.set_text(&body, cx));
        let view = small_markdown(TextView::new(&state), window).px(px(SPACE_0_5));
        let view = if self.unfolded {
            view
        } else {
            clamp(view, PR_DESCRIPTION_LINES)
        };
        let clamped = state.read(cx).is_clamped();
        if !self.unfolded {
            // The clamp measures as the frame is laid out, after this
            // render: when it finds otherwise than this frame drew, the
            // cockpit draws again with its answer — once, for a body that
            // has just come or changed.
            let cockpit = window.current_view();
            let state = state.clone();
            window.defer(cx, move |_, cx| {
                if state.read(cx).is_clamped() != clamped {
                    // The frame draws the panel: it alone draws again.
                    cx.notify(crate::motion::frames_through(cockpit));
                }
            });
        }
        let more = self.unfolded || clamped;
        let words = if self.unfolded {
            "Show less"
        } else {
            "Show more"
        };
        let toggle = self.toggle;
        div().flex().flex_col().child(view).when(more, |section| {
            section.child(
                fold_row(
                    "pr-description-more",
                    format!("{}-turn", self.id).into(),
                    self.unfolded,
                    words,
                )
                .on_click(move |event, window, cx| toggle(event, window, cx)),
            )
        })
    }
}

/// A row's mark: a 15px icon, or the braille spinner centred in its box.
fn mark(glyph: Option<&'static str>, ink: u32) -> AnyElement {
    use crate::theme::*;
    match glyph {
        Some(glyph) => icon(glyph, PR_MARK, ink).into_any_element(),
        None => div()
            .flex()
            .flex_shrink_0()
            .justify_center()
            .w(px(PR_MARK))
            .font_family(FONT_CODE)
            .text_size(px(FS_META))
            .child(crate::components::braille_spinner(ink))
            .into_any_element(),
    }
}

/// A pending or skipped check's hollow ring, in the 15px box.
fn hollow_ring() -> AnyElement {
    use crate::theme::*;
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(PR_MARK))
        .child(
            div()
                .size(px(PR_MARK_RING))
                .rounded_full()
                .border_1()
                .border_color(rgb(TEXT_FAINT)),
        )
        .into_any_element()
}

/// The order checks list in: failed, running, pending, passed, skipped.
fn check_rank(status: WireCheckStatus) -> u8 {
    match status {
        WireCheckStatus::Failure => 0,
        WireCheckStatus::Running => 1,
        WireCheckStatus::Pending | WireCheckStatus::Unknown => 2,
        WireCheckStatus::Success => 3,
        WireCheckStatus::Skipped => 4,
    }
}

/// A check row's mark, status word and the word's ink.
fn check_face(status: WireCheckStatus) -> (AnyElement, &'static str, u32) {
    use crate::theme::*;
    match status {
        WireCheckStatus::Failure => (mark(Some(icons::X_CIRCLE), BLOCKED), "failed", BLOCKED),
        WireCheckStatus::Running => (mark(None, RUNNING), "running", TEXT_DIM),
        WireCheckStatus::Success => (mark(Some(icons::CHECK_CIRCLE), RUNNING), "passed", TEXT_DIM),
        WireCheckStatus::Pending | WireCheckStatus::Unknown => (hollow_ring(), "pending", TEXT_DIM),
        WireCheckStatus::Skipped => (hollow_ring(), "skipped", TEXT_DIM),
    }
}

/// What one readiness row says.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Ready {
    /// Its name for tests and selectors.
    key: &'static str,
    /// Its icon; `None` is the braille spinner.
    glyph: Option<&'static str>,
    ink: u32,
    text: String,
    meta: Option<String>,
}

/// `N thing` or `N things`.
fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("{n} {one}")
    } else {
        format!("{n} {many}")
    }
}

/// The readiness rows, in the SPEC's order, each only when it has
/// something to say: the checks, then the merge state and the sync — or,
/// merged, the one row that replaces both.
fn readiness(pr: &WirePr) -> Vec<Ready> {
    use crate::theme::*;
    let mut rows = Vec::new();
    let total = pr.checks.len();
    let tally = |wanted: &[WireCheckStatus]| {
        pr.checks
            .iter()
            .filter(|check| wanted.contains(&check.status))
            .count()
    };
    let failed = tally(&[WireCheckStatus::Failure]);
    let running = tally(&[
        WireCheckStatus::Running,
        WireCheckStatus::Pending,
        WireCheckStatus::Unknown,
    ]);
    let passed = tally(&[WireCheckStatus::Success]);
    if failed > 0 {
        rows.push(Ready {
            key: "checks-failed",
            glyph: Some(icons::X_CIRCLE),
            ink: BLOCKED,
            text: format!("{} failed", count(failed, "check", "checks")),
            meta: Some(format!("{passed} of {total} passed")),
        });
    } else if running > 0 {
        rows.push(Ready {
            key: "checks-running",
            glyph: None,
            ink: RUNNING,
            text: "Checks running".into(),
            meta: Some(format!("{} of {total} done", total - running)),
        });
    } else if passed > 0 {
        rows.push(Ready {
            key: "checks-passed",
            glyph: Some(icons::CHECK_CIRCLE),
            ink: RUNNING,
            text: "All checks passed".into(),
            meta: Some(format!("{passed} of {total}")),
        });
    }
    let base = &pr.base;
    if pr.state == WirePrState::Merged {
        rows.push(Ready {
            key: "merged",
            glyph: Some(icons::GIT_MERGE),
            ink: TEXT,
            text: format!("Merged into {base}"),
            meta: None,
        });
        return rows;
    }
    if pr.state != WirePrState::Closed {
        let merge = match pr.merge {
            WireMerge::Conflicting => Some((
                "conflicts",
                icons::WARNING,
                BLOCKED,
                format!("Conflicts with {base}"),
            )),
            WireMerge::Blocked => Some((
                "blocked",
                icons::WARNING,
                ATTENTION,
                "Merging is blocked".into(),
            )),
            WireMerge::Clean => Some((
                "clean",
                icons::CHECK_CIRCLE,
                RUNNING,
                format!("No conflicts with {base}"),
            )),
            WireMerge::Unknown => None,
        };
        rows.extend(merge.map(|(key, glyph, ink, text)| Ready {
            key,
            glyph: Some(glyph),
            ink,
            text,
            meta: None,
        }));
    }
    let sync = match pr.sync {
        WireSync::Unpushed { commits } => Some((
            "unpushed",
            icons::ARROW_UP,
            ATTENTION,
            format!(
                "{} not pushed",
                count(commits as usize, "commit", "commits")
            ),
        )),
        WireSync::Behind { commits } => Some((
            "behind",
            icons::ARROW_DOWN,
            ATTENTION,
            format!("{} to pull", count(commits as usize, "commit", "commits")),
        )),
        WireSync::InSync => Some((
            "in-sync",
            icons::CHECK_CIRCLE,
            TEXT_DIM,
            "In sync with your branch".into(),
        )),
        WireSync::Unknown => None,
    };
    rows.extend(sync.map(|(key, glyph, ink, text)| Ready {
        key,
        glyph: Some(glyph),
        ink,
        text,
        meta: None,
    }));
    rows
}

/// The readiness group (r8, `RAISED`, 40px rows, hairlines between), none
/// when no row has anything to say.
fn readiness_group(pr: &WirePr) -> Option<Div> {
    use crate::theme::*;
    let rows = readiness(pr);
    if rows.is_empty() {
        return None;
    }
    Some(
        div()
            .debug_selector(|| "pr-readiness".into())
            .flex()
            .flex_col()
            .flex_shrink_0()
            .rounded(px(R_BLOCK))
            .bg(paint::RAISED)
            .children(rows.into_iter().enumerate().flat_map(|(index, row)| {
                let key = row.key;
                // The hairline sits between two rows, outside their 40px.
                let rule =
                    (index > 0).then(|| div().h(px(1.)).bg(paint::HAIRLINE).into_any_element());
                let row = div()
                    .debug_selector(move || format!("pr-ready-{key}"))
                    .flex()
                    .items_center()
                    .gap(px(PR_ROW_GAP))
                    .h(px(PR_READY_ROW_H))
                    .px(px(SPACE_3))
                    .child(mark(row.glyph, row.ink))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(SharedString::from(row.text)),
                    )
                    .children(row.meta.map(|meta| {
                        div()
                            .flex_shrink_0()
                            .text_size(px(FS_META))
                            .text_color(rgb(TEXT_DIM))
                            .child(SharedString::from(meta))
                    }))
                    .into_any_element();
                rule.into_iter().chain(std::iter::once(row))
            })),
    )
}

/// The comments in the order they list: the findings still unresolved,
/// then the reviews and plain comments, each newest first; apart, the
/// resolved, newest first.
fn sorted_comments(comments: &[WirePrComment]) -> (Vec<&WirePrComment>, Vec<&WirePrComment>) {
    let (mut open, mut resolved): (Vec<_>, Vec<_>) =
        comments.iter().partition(|comment| !comment.resolved);
    // RFC 3339 in UTC sorts as text.
    let newest_first = |a: &&WirePrComment, b: &&WirePrComment| b.created_at.cmp(&a.created_at);
    open.sort_by(|a, b| {
        let unresolved = |c: &&WirePrComment| c.kind != WirePrCommentKind::Finding;
        unresolved(a)
            .cmp(&unresolved(b))
            .then_with(|| newest_first(a, b))
    });
    resolved.sort_by(newest_first);
    (open, resolved)
}

/// The comments heading's tally: `N · M unresolved` (the findings still
/// open), or `N · all resolved`.
fn comments_tally(comments: &[WirePrComment]) -> String {
    let open = comments
        .iter()
        .filter(|comment| comment.kind == WirePrCommentKind::Finding && !comment.resolved)
        .count();
    let total = comments.len();
    if open == 0 {
        format!("{total} \u{b7} all resolved")
    } else {
        format!("{total} \u{b7} {open} unresolved")
    }
}

/// The author's initial on an 18px circle.
fn avatar(author: &str) -> Div {
    use crate::theme::*;
    let initial: String = author
        .chars()
        .find(|c| c.is_alphanumeric())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .size(px(PR_AVATAR))
        .rounded_full()
        .bg(paint::SEND)
        .text_size(px(FS_PR_AVATAR))
        .font_weight(W_STRONG)
        .text_color(rgb(TEXT_STRONG))
        .child(SharedString::from(initial))
}

/// An author's name, 12.5/500 `TEXT_STRONG`.
fn author(name: &str) -> Div {
    use crate::theme::*;
    div()
        .min_w_0()
        .truncate()
        .text_size(px(FS_LABEL))
        .font_weight(W_LABEL)
        .text_color(rgb(TEXT_STRONG))
        .child(SharedString::from(name.to_string()))
}

/// The kind word beside the author.
fn kind_word(kind: WirePrCommentKind) -> &'static str {
    match kind {
        WirePrCommentKind::Review => "review",
        WirePrCommentKind::Finding => "finding",
        WirePrCommentKind::Comment | WirePrCommentKind::Unknown => "comment",
    }
}

/// A finding's place: its path and inclusive lines.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Anchor {
    path: String,
    start: u32,
    end: u32,
}

impl Anchor {
    /// `path:start-end` or `path:line`; `None` for a finding on no line.
    fn parse(anchor: &str) -> Option<Self> {
        let (path, lines) = anchor.rsplit_once(':')?;
        let (start, end) = match lines.split_once('-') {
            Some((start, end)) => (start.parse().ok()?, end.parse().ok()?),
            None => {
                let line = lines.parse().ok()?;
                (line, line)
            }
        };
        Some(Self {
            path: path.to_string(),
            start,
            end,
        })
    }

    /// The anchor as the card reads it: the file's name and its lines
    /// (`scheduled_work.rs:89–91`).
    fn label(&self) -> String {
        let name = self.path.rsplit('/').next().unwrap_or(&self.path);
        if self.start == self.end {
            format!("{name}:{}", self.start)
        } else {
            format!("{name}:{}\u{2013}{}", self.start, self.end)
        }
    }
}

/// A comment's body, Markdown set small (`small_markdown`), clamped to
/// `PR_BODY_LINES` while `folded`.
fn comment_body(id: String, body: &str, folded: bool, window: &Window) -> TextView {
    let view = small_markdown(
        TextView::markdown(SharedString::from(id), body.to_string()),
        window,
    );
    if folded {
        clamp(view, crate::theme::PR_BODY_LINES)
    } else {
        view
    }
}

/// `view` cut to its first `lines`. The clamp clips the laid-out document
/// to whole lines; the ellipsis comes from each paragraph's own clamp, so a
/// body whose first paragraph runs past the clamp ends on `…`
/// (pr-states.png). One that runs past it in a later block is cut without
/// one, never with a stray `…` above the clip.
fn clamp(view: TextView, lines: usize) -> TextView {
    view.max_lines(lines).line_clamp(lines).text_ellipsis()
}

/// Markdown set small, as a comment card or the description reads it:
/// 12.5/18 `TEXT`, inline code as chips, and every heading at the body's
/// size — set apart by weight, not size, in a column this narrow.
fn small_markdown(view: TextView, window: &Window) -> TextView {
    use crate::theme::*;
    let rem = f32::from(window.rem_size());
    let style = TextViewStyle::default()
        .with_dark(true)
        .with_foreground(rgb(TEXT).into())
        .with_muted_foreground(rgb(TEXT_DIM).into())
        .with_link(rgb(TEXT_STRONG).into())
        .with_link_underline_on_hover(true)
        .with_paragraph_gap(gpui::rems(PR_BODY_LH / 2. / rem))
        .with_strong(gpui::HighlightStyle {
            color: Some(rgb(TEXT_STRONG).into()),
            font_weight: Some(W_STRONG),
            ..Default::default()
        })
        .with_inline_code(gpui::HighlightStyle {
            color: Some(rgb(TEXT_STRONG).into()),
            font_weight: Some(W_BODY),
            ..Default::default()
        })
        .with_inline_code_font(Some(FONT_CODE.into()))
        // The transcript's chip, a step over the card's `RAISED`. The
        // mockup sets its code a step smaller (11.5) inside 4px of padding;
        // a run here keeps the body's size, which is already the mockup's
        // chip wide, so no room is laid out and the ground reaches 4px over
        // the neighbouring spaces: the lines break where the mockup's do.
        .with_inline_code_wash(Some(InlineCodeWash {
            color: rgb(PR_CODE_CHIP).into(),
            overhang: px(SPACE_1),
            pad: px(0.),
            ..crate::rich::inline_code_wash()
        }))
        .with_code_background(rgb(PR_CODE_CHIP).into())
        // A heading is a line of the body in its strong weight, with a
        // paragraph's gap more above it than below, so it heads what
        // follows; a list's markers hang dim in their own box, as the
        // transcript's, its text on one edge.
        .with_heading_base_font_size(px(FS_LABEL))
        .with_heading_font_size(|_, base| base)
        .with_heading_spacing(
            gpui::rems(PR_BODY_LH / 2. / rem),
            Some(gpui::rems(SPACE_1 / rem)),
        )
        .with_list_hang(Some(gpui::base::text::ListHang {
            width: px(PR_LIST_HANG),
            gap: px(SPACE_1_5),
        }))
        .with_list_markers(
            gpui::StyleRefinement::default().text_color(rgb(TEXT_DIM)),
            gpui::StyleRefinement::default().text_color(rgb(TEXT_DIM)),
        );
    let style = (1..=6).fold(style, |style, level| {
        style.with_heading(
            level,
            gpui::StyleRefinement::default()
                .font_weight(W_STRONG)
                .text_color(rgb(TEXT_STRONG))
                .line_height(px(PR_BODY_LH)),
        )
    });
    view.style(style)
        .selectable(false)
        .font_family(FONT_UI)
        .text_size(px(FS_LABEL))
        .line_height(px(PR_BODY_LH))
        .min_w_0()
        // A line's trailing space hangs past the edge (as the title's),
        // by the narrower face's space.
        .mr(px(-SPACE_0_5))
        .whitespace_normal()
}

/// A finding's hunk, as the forge quotes it: its last lines in the code
/// face, the signs in the diff hues.
fn hunk(snippet: &str) -> Div {
    use crate::theme::*;
    const SHOWN: usize = 6;
    let lines: Vec<&str> = snippet
        .lines()
        .filter(|line| !line.starts_with("@@"))
        .collect();
    let tail = &lines[lines.len().saturating_sub(SHOWN)..];
    div()
        .flex()
        .flex_col()
        .py(px(SPACE_1_5))
        .px(px(SPACE_2))
        .rounded(px(R_TAB))
        .bg(paint::BAR)
        .overflow_hidden()
        .font_family(FONT_CODE)
        .text_size(px(FS_PANEL_DIFF))
        .line_height(px(LH_PANEL_DIFF))
        .children(tail.iter().map(|line| {
            let ink = match line.chars().next() {
                Some('+') => RUNNING,
                Some('-') => BLOCKED,
                _ => TEXT_MUTED,
            };
            div()
                .truncate()
                .text_color(rgb(ink))
                .child(SharedString::from(line.to_string()))
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::protocol::WireCheck;

    fn pr(
        state: WirePrState,
        merge: WireMerge,
        sync: WireSync,
        checks: &[WireCheckStatus],
    ) -> WirePr {
        WirePr {
            number: 146,
            title: "Requeue scheduled work".into(),
            url: "https://github.com/o/r/pull/146".into(),
            body: String::new(),
            state,
            draft: false,
            head: "scheduled-work-crash".into(),
            base: "main".into(),
            merge,
            sync,
            checks: checks
                .iter()
                .enumerate()
                .map(|(at, status)| WireCheck {
                    name: format!("check {at}"),
                    status: *status,
                })
                .collect(),
            comments: Vec::new(),
            truncated: false,
            checks_truncated: false,
            comments_truncated: false,
        }
    }

    fn texts(pr: &WirePr) -> Vec<(String, Option<String>)> {
        readiness(pr)
            .into_iter()
            .map(|row| (row.text, row.meta))
            .collect()
    }

    use WireCheckStatus::{Failure, Pending, Running, Skipped, Success};

    /// The readiness rows of each column of the mockups, in the SPEC's
    /// order: checks, merge, sync.
    #[test]
    fn readiness_reads_each_state() {
        let open = pr(
            WirePrState::Open,
            WireMerge::Clean,
            WireSync::Unpushed { commits: 1 },
            &[Running, Success, Success],
        );
        assert_eq!(
            texts(&open),
            [
                ("Checks running".into(), Some("2 of 3 done".into())),
                ("No conflicts with main".into(), None),
                ("1 commit not pushed".into(), None),
            ]
        );
        let failing = pr(
            WirePrState::Open,
            WireMerge::Conflicting,
            WireSync::InSync,
            &[Failure, Success, Success],
        );
        assert_eq!(
            texts(&failing),
            [
                ("1 check failed".into(), Some("2 of 3 passed".into())),
                ("Conflicts with main".into(), None),
                ("In sync with your branch".into(), None),
            ]
        );
        let merged = pr(
            WirePrState::Merged,
            WireMerge::Clean,
            WireSync::InSync,
            &[Success, Success, Success],
        );
        assert_eq!(
            texts(&merged),
            [
                ("All checks passed".into(), Some("3 of 3".into())),
                ("Merged into main".into(), None),
            ],
            "merged replaces the merge and sync rows"
        );
        let blocked = pr(
            WirePrState::Open,
            WireMerge::Blocked,
            WireSync::Behind { commits: 2 },
            &[],
        );
        assert_eq!(
            texts(&blocked),
            [
                ("Merging is blocked".into(), None),
                ("2 commits to pull".into(), None),
            ],
            "no checks, no checks row"
        );
    }

    /// Checks list worst first: failed, running, pending, passed, skipped.
    #[test]
    fn checks_list_worst_first() {
        let mut statuses = vec![Skipped, Success, Pending, Running, Failure];
        statuses.sort_by_key(|status| check_rank(*status));
        assert_eq!(statuses, [Failure, Running, Pending, Success, Skipped]);
    }

    fn comment(kind: WirePrCommentKind, at: &str, resolved: bool) -> WirePrComment {
        WirePrComment {
            kind,
            author: "claude".into(),
            anchor: "src/a.rs:89-91".into(),
            body: "body".into(),
            snippet: None,
            created_at: at.into(),
            resolved,
            outdated: false,
            replies: Vec::new(),
        }
    }

    /// Unresolved findings first, then the rest, newest first; the tally
    /// counts the open findings.
    #[test]
    fn comments_list_unresolved_newest_first() {
        use WirePrCommentKind::{Comment, Finding};
        let comments = vec![
            comment(Finding, "2026-10-21T01:19:30Z", true),
            comment(Comment, "2026-10-21T01:27:30Z", false),
            comment(Finding, "2026-10-21T01:25:30Z", false),
        ];
        let (open, resolved) = sorted_comments(&comments);
        let stamps = |list: &[&WirePrComment]| -> Vec<String> {
            list.iter().map(|c| c.created_at.clone()).collect()
        };
        assert_eq!(
            stamps(&open),
            ["2026-10-21T01:25:30Z", "2026-10-21T01:27:30Z"],
            "the open finding first, though the comment is newer"
        );
        assert_eq!(stamps(&resolved), ["2026-10-21T01:19:30Z"]);
        assert_eq!(comments_tally(&comments), "3 \u{b7} 1 unresolved");
        let all = vec![comment(Finding, "2026-10-21T01:19:30Z", true)];
        assert_eq!(comments_tally(&all), "1 \u{b7} all resolved");
    }

    /// A finding's anchor reads its file's name and lines, an en dash
    /// between them, and opens at its first.
    #[test]
    fn an_anchor_reads_its_file_and_lines() {
        let range = Anchor::parse("crates/core/src/scheduled_work.rs:89-91").unwrap();
        assert_eq!(range.label(), "scheduled_work.rs:89\u{2013}91");
        assert_eq!((range.start, range.end), (89, 91));
        let line = Anchor::parse("src/a.rs:120").unwrap();
        assert_eq!(line.label(), "a.rs:120");
        assert_eq!(Anchor::parse("src/a.rs"), None);
    }

    /// The pill's four kinds.
    #[test]
    fn the_pill_names_the_state() {
        assert_eq!(Pill::of(WirePrState::Open, false), Pill::Open);
        assert_eq!(Pill::of(WirePrState::Open, true), Pill::Draft);
        assert_eq!(Pill::of(WirePrState::Merged, true), Pill::Merged);
        assert_eq!(Pill::of(WirePrState::Closed, false), Pill::Closed);
    }
}
