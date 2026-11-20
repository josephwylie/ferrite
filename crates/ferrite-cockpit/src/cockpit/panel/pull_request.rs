//! The pull request Surface (ADR 0039; T3's `PullRequestDetailPanel`):
//! the PR of the Thread's branch as a tab of its own, opened from the PR
//! row in Overview's Changes and the `+` menu's Pull request (P).
//!
//! - **Header:** the repo and `#N` on the left, `#N` in its state's ink
//!   and opening the PR on the forge; on the right the primary action
//!   (Merge, armed on the first press: "Confirm merge?") or, once the PR is
//!   merged or closed, its state pill, then `⋯`.
//! - **`⋯`:** hand-offs (Ask a question prepares the current composer; Explain, Fix
//!   findings and Write guide ask the Thread's agent), the PR's state
//!   (Ready for review or Convert to draft, Merge now, Enable auto-merge,
//!   Allow edits from maintainers), the destructive rows (Close, Revert,
//!   Delete branch), armed as every destructive row is, and copies. Ferrite
//!   runs no forge writes of its own: the PR's state changes are asked of
//!   the agent, which has `gh`, as `Create PR` is.
//! - **Views:** Summary, Guide, Timeline and Code as a segmented group,
//!   the checks and comment counts after it; the one chosen is kept on the
//!   Surface (`ThreadSurfaces::pr_tabs`). Code reads the requested PR's published
//!   files when its tab is hovered or shown.
//!
//! The PR's facts (its number, state, checks) are the nav's reading
//! (`facts.pr`); its whole snapshot (body, checks, conversation) is read
//! when the Surface opens (`Cockpit::review_pull_request`).

use super::pr_action::{ActionState, Stage};
use super::pr_read::{Answer, PrFailure, PrRead, PrSource, PrTarget, stamp_age};
use super::*;
use crate::cockpit::menu_button::{Door, Edge};
use crate::cockpit::surfaces::PrTab;
use ferrite_ui::components::IconSize;
use ferrite_core::protocol::{
    WireCheckStatus, WirePr, WirePrCommentKind, WirePrState, WirePrUnread,
};
use ferrite_core::workspace::PrState;

/// A pull request's state, as its pill and its tab's icon show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrTone {
    Open,
    Draft,
    Merged,
    Closed,
}

impl PrTone {
    fn of_reading(state: PrState, draft: bool) -> Self {
        match (state, draft) {
            (PrState::Merged, _) => Self::Merged,
            (PrState::Closed, _) => Self::Closed,
            (PrState::Open, true) => Self::Draft,
            (PrState::Open, false) => Self::Open,
        }
    }

    pub(in crate::cockpit) fn of_snapshot(pr: &WirePr) -> Self {
        Self::of_wire(pr.state, pr.draft)
    }

    pub(in crate::cockpit) fn of_wire(state: WirePrState, draft: bool) -> Self {
        match (state, draft) {
            (WirePrState::Merged, _) => Self::Merged,
            (WirePrState::Closed, _) => Self::Closed,
            (_, true) => Self::Draft,
            _ => Self::Open,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Draft => "Draft",
            Self::Merged => "Merged",
            Self::Closed => "Closed",
        }
    }

    pub(crate) fn glyph(self) -> &'static str {
        match self {
            Self::Merged => icons::GIT_MERGE,
            Self::Draft => icons::GIT_PULL_REQUEST_DRAFT,
            Self::Open | Self::Closed => icons::GIT_PULL_REQUEST,
        }
    }

    /// T3's state tones on Ferrite's hues (`pullRequestIcons`).
    pub(crate) fn ink(self) -> u32 {
        use ferrite_ui::theme::*;
        match self {
            Self::Open => GREEN,
            Self::Draft => INK_SUBTLE,
            Self::Merged => MAGENTA,
            Self::Closed => RED,
        }
    }

    pub(in crate::cockpit) fn settled(self) -> bool {
        matches!(self, Self::Merged | Self::Closed)
    }
}

/// One thing the PR Surface's `⋯` or its primary action does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrAction {
    Ask,
    Explain,
    FixFindings,
    WriteGuide,
    ReadyForReview,
    ConvertToDraft,
    MergeNow,
    AutoMerge,
    AllowEdits,
    Close,
    Revert,
    DeleteBranch,
    CopyLink,
    CopyBranch,
}

impl PrAction {
    fn label(self, guide: bool) -> &'static str {
        match self {
            Self::Ask => "Ask a question",
            Self::Explain => "Explain this PR",
            Self::FixFindings => "Fix findings",
            Self::WriteGuide if guide => "Rewrite guide",
            Self::WriteGuide => "Write guide",
            Self::ReadyForReview => "Ready for review",
            Self::ConvertToDraft => "Convert to draft",
            Self::MergeNow => "Merge now",
            Self::AutoMerge => "Enable auto-merge",
            Self::AllowEdits => "Allow edits from maintainers",
            Self::Close => "Close PR",
            Self::Revert => "Revert changes",
            Self::DeleteBranch => "Delete branch",
            Self::CopyLink => "Copy link",
            Self::CopyBranch => "Copy branch name",
        }
    }

    /// Whether the row asks the Thread's agent (and so is one action in
    /// flight at a time), rather than copying or opening a side chat.
    pub(crate) fn hands_off(self) -> bool {
        !matches!(self, Self::Ask | Self::CopyLink | Self::CopyBranch)
    }

    /// Whether the row arms before it runs.
    fn destructive(self) -> bool {
        matches!(
            self,
            Self::MergeNow | Self::AutoMerge | Self::Close | Self::Revert | Self::DeleteBranch
        )
    }

    /// Whether the row applies to a PR in `tone`.
    fn applies(self, tone: PrTone) -> bool {
        match self {
            Self::ReadyForReview => tone == PrTone::Draft,
            Self::ConvertToDraft | Self::MergeNow => tone == PrTone::Open,
            Self::AutoMerge | Self::AllowEdits | Self::Close | Self::FixFindings => !tone.settled(),
            Self::Revert => tone == PrTone::Merged,
            Self::DeleteBranch => tone.settled(),
            Self::Ask | Self::Explain | Self::WriteGuide | Self::CopyLink | Self::CopyBranch => {
                true
            }
        }
    }

    /// What the Thread's agent is asked, for a row that hands off.
    pub(crate) fn prompt(self, pr: &WirePr) -> Option<String> {
        let n = pr.number;
        let head = &pr.head;
        Some(match self {
            Self::Ask | Self::CopyLink | Self::CopyBranch => return None,
            Self::Explain => format!(
                "Explain pull request #{n} ({url}): what it changes and why, in the order a \
                 reviewer should read it, and anything that deserves a close look. Read it \
                 with `gh pr view {n}` and `gh pr diff {n}`.",
                url = pr.url
            ),
            Self::FixFindings => format!(
                "Address the open review findings on pull request #{n}. Read the unresolved \
                 review comments with `gh pr view {n} --comments`, fix each one on this \
                 branch, commit and push, and say what changed for each."
            ),
            Self::WriteGuide => format!(
                "Write a review guide for pull request #{n}: walk a reviewer through the \
                 change in the order to read it, file by file, with what to check in each. \
                 Post it with `gh pr comment {n}`, its first line `{GUIDE_HEAD}`."
            ),
            Self::ReadyForReview => {
                format!("Mark pull request #{n} ready for review with `gh pr ready {n}`.")
            }
            Self::ConvertToDraft => {
                format!("Convert pull request #{n} to a draft with `gh pr ready {n} --undo`.")
            }
            Self::MergeNow => format!(
                "Merge pull request #{n} now with `gh pr merge {n}`, using the repository's \
                 default merge method, and report the result."
            ),
            Self::AutoMerge => format!(
                "Enable auto-merge on pull request #{n} with `gh pr merge {n} --auto`, using \
                 the repository's default merge method."
            ),
            Self::AllowEdits => format!(
                "Turn on \"Allow edits from maintainers\" for pull request #{n} \
                 (`gh api -X PATCH repos/{{owner}}/{{repo}}/pulls/{n} -F maintainer_can_modify=true`)."
            ),
            Self::Close => format!("Close pull request #{n} with `gh pr close {n}`."),
            Self::Revert => format!(
                "Revert pull request #{n}: open a pull request that reverts its changes \
                 (`gh pr revert {n}`), and report its URL."
            ),
            Self::DeleteBranch => format!(
                "Delete the branch `{head}` of pull request #{n} on the remote with \
                 `git push origin --delete {head}`."
            ),
        })
    }
}

/// The `⋯`'s rows, in T3's groups.
const MENU: [&[PrAction]; 4] = [
    &[
        PrAction::Ask,
        PrAction::Explain,
        PrAction::FixFindings,
        PrAction::WriteGuide,
    ],
    &[
        PrAction::ReadyForReview,
        PrAction::ConvertToDraft,
        PrAction::MergeNow,
        PrAction::AutoMerge,
        PrAction::AllowEdits,
    ],
    &[PrAction::Close, PrAction::Revert, PrAction::DeleteBranch],
    &[PrAction::CopyLink, PrAction::CopyBranch],
];

/// How a review guide's comment begins (`PrAction::WriteGuide`): the Guide
/// view shows the latest comment that does.
const GUIDE_HEAD: &str = "## Review guide";

/// The latest review guide among the PR's comments, which the forge's read
/// lists newest first.
fn guide(pr: &WirePr) -> Option<&str> {
    pr.comments
        .iter()
        .map(|comment| comment.body.as_str())
        .find(|body| body.trim_start().starts_with(GUIDE_HEAD))
}

/// `owner/repo`, out of the PR's address on the forge.
fn repo_of(url: &str) -> Option<String> {
    let path = url
        .split("://")
        .nth(1)?
        .split('/')
        .skip(1)
        .collect::<Vec<_>>();
    match path.as_slice() {
        [owner, repo, ..] if !owner.is_empty() && !repo.is_empty() => {
            Some(format!("{owner}/{repo}"))
        }
        _ => None,
    }
}

/// The checks as the views' trailing slot says them: failures first.
fn checks_line(pr: &WirePr) -> Option<(String, u32)> {
    use ferrite_ui::theme::*;
    if pr.checks.is_empty() {
        return None;
    }
    let count = |status: &[WireCheckStatus]| {
        pr.checks
            .iter()
            .filter(|check| status.contains(&check.status))
            .count()
    };
    let failing = count(&[WireCheckStatus::Failure]);
    let running = count(&[WireCheckStatus::Running, WireCheckStatus::Pending]);
    Some(if failing > 0 {
        (format!("{failing} failing"), RED)
    } else if running > 0 {
        (format!("{running} running"), INK_SUBTLE)
    } else {
        ("Checks passing".into(), INK_SUBTLE)
    })
}

/// A hairline at 60% (T3's `border-border/60`).
pub(in crate::cockpit) fn soft_line() -> gpui::Rgba {
    rgba((ferrite_ui::theme::LINE << 8) | 0x99)
}

impl CockpitView {
    /// The PR row and the `+` menu's Pull request: the branch's PR as a
    /// Surface of `thread`'s Panel. Nothing with no PR on the branch.
    pub(crate) fn open_pr_surface(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        if let Some(number) = self.facts.pr(thread).map(|pr| pr.number) {
            self.upsert_surface(thread, SurfaceId::PullRequest(number), cx);
        }
    }

    /// The Thread's PR tab: the PR it shows, else the one its branch has.
    fn pr_tab_target(&self, thread: ThreadId) -> Option<PrTarget> {
        let tab = self.panel.surfaces.get(thread).and_then(|state| {
            state
                .active
                .iter()
                .chain(state.tabs.iter())
                .find_map(|id| match id {
                    SurfaceId::PullRequest(number) => Some(*number),
                    _ => None,
                })
        });
        let number = tab.or_else(|| self.facts.pr(thread).map(|pr| pr.number))?;
        Some(PrTarget::thread(thread, number))
    }

    /// The checkout `source` names the repository of.
    pub(in crate::cockpit) fn pr_source_path(
        &self,
        source: PrSource,
    ) -> Option<std::path::PathBuf> {
        match source {
            PrSource::Thread(thread) => self.thread_path(thread),
            PrSource::Project(project) => self
                .cockpit
                .registry()
                .project(project)
                .map(|project| project.root.clone()),
        }
    }

    /// Read `target`'s PR whole for its Surface, unless a read is out, or
    /// (without `again`) it has been read. Tests and captures never run the
    /// forge's CLI: only a fixture world's forge answers them.
    pub(in crate::cockpit) fn read_pr_snapshot(
        &mut self,
        target: PrTarget,
        again: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.pr_source_path(target.source) else {
            return;
        };
        if cfg!(any(test, feature = "visual-reference")) && !self.cockpit.has_forge_fixture(&path) {
            return;
        }
        let Some(request) = self.panel.pr_reads.begin(target, again) else {
            return;
        };
        let number = Some(target.number);
        let answer = self.cockpit.review_pull_request(&path, number);
        cx.spawn(async move |this, cx| {
            let read = crate::cockpit::beside::answered(answer, cx).await;
            this.update(cx, |view, cx| {
                view.pr_read_landed(target, request, read, cx)
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// The forge answered `request` for `target`. Only the request the
    /// target still waits on, for a tab still open, changes anything: a
    /// tab closed or a read asked again meanwhile leaves the answer unheard.
    fn pr_read_landed(
        &mut self,
        target: PrTarget,
        request: u64,
        read: std::io::Result<ferrite_core::protocol::ReviewPullRequest>,
        cx: &mut Context<Self>,
    ) {
        let shown =
            match target.source {
                PrSource::Thread(thread) => self.panel.surfaces.get(thread).is_some_and(|state| {
                    state.tabs.contains(&SurfaceId::PullRequest(target.number))
                }),
                PrSource::Project(_) => true,
            };
        if !shown {
            self.panel.pr_reads.forget(target);
            return;
        }
        let answer = match read {
            Ok(read) => match read.pr {
                Ok(pr) if pr.number == target.number => Answer::Read(Box::new(pr)),
                Ok(pr) => Answer::Failed(PrFailure {
                    kind: WirePrUnread::Failed,
                    note: format!(
                        "Requested pull request #{}, but received #{}",
                        target.number, pr.number
                    ),
                }),
                Err(note) => match read.unread {
                    Some(kind) => Answer::Failed(PrFailure { kind, note }),
                    None => Answer::Gone(PrFailure {
                        kind: WirePrUnread::Failed,
                        note,
                    }),
                },
            },
            Err(error) => Answer::Failed(PrFailure {
                kind: WirePrUnread::Failed,
                note: error.to_string(),
            }),
        };
        if self.panel.pr_reads.finish(target, request, answer) {
            self.pr_action_checked(target, cx);
            cx.notify();
        }
    }

    /// A fixture world's snapshot of `thread`'s PR, as if read.
    #[cfg(any(test, feature = "visual-reference"))]
    pub(crate) fn fixture_pr_snapshot(&mut self, thread: ThreadId, pr: WirePr) {
        let target = PrTarget::thread(thread, pr.number);
        self.panel.pr_reads.install(target, pr);
    }

    /// Ask the forge again for `thread`'s open PR tab: its checks and
    /// conversation move without the PR's number or state moving.
    pub(crate) fn refresh_pr_tab(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let open = self.panel.surfaces.get(thread).is_some_and(|state| {
            state
                .tabs
                .iter()
                .any(|id| matches!(id, SurfaceId::PullRequest(_)))
        });
        if let Some(target) = self.pr_tab_target(thread).filter(|_| open) {
            self.read_pr_snapshot(target, true, cx);
        }
    }

    /// The nav's reading of `thread`'s PR changed. A tab for another PR
    /// than the branch's goes, and a snapshot the reading has overtaken (a
    /// merge the agent ran) is let go, until its read lands: the reading
    /// speaks. Any other change asks the forge again, as checks do.
    pub(crate) fn pr_reading_changed(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(reading) = self.facts.pr(thread) else {
            return;
        };
        let (number, tone) = (
            reading.number,
            PrTone::of_reading(reading.state, reading.draft),
        );
        let target = PrTarget::thread(thread, number);
        if self
            .panel
            .pr_reads
            .snapshot(target)
            .is_some_and(|pr| PrTone::of_snapshot(pr) != tone)
        {
            self.panel.pr_reads.forget(target);
            cx.notify();
        }
        self.refresh_pr_tab(thread, cx);
    }

    /// Merge armed on a PR Surface no longer shown (another tab, the Panel
    /// hidden, another Thread or PR) is disarmed: one press, much later, must
    /// not merge.
    pub(super) fn disarm_unseen_merge(&mut self) {
        let Some(armed) = self.panel.pr_armed else {
            return;
        };
        let shown = self
            .panel_pane()
            .and_then(|index| Some((self.panes[index].thread()?, self.shown_surface(index)?)));
        let on_show = matches!(
            shown,
            Some((thread, SurfaceId::PullRequest(number)))
                if armed == PrTarget::thread(thread, number)
        );
        if !on_show {
            self.panel.pr_armed = None;
        }
    }

    /// The snapshot of `thread`'s PR, once read.
    pub(crate) fn pr_snapshot(&self, thread: ThreadId) -> Option<&WirePr> {
        self.panel.pr_reads.snapshot(self.pr_tab_target(thread)?)
    }

    /// The PR's state: its snapshot's, else the nav's reading's.
    pub(crate) fn pr_tone(&self, thread: ThreadId) -> Option<PrTone> {
        self.pr_snapshot(thread)
            .map(PrTone::of_snapshot)
            .or_else(|| {
                self.facts
                    .pr(thread)
                    .map(|pr| PrTone::of_reading(pr.state, pr.draft))
            })
    }

    /// A view of PR `number` chosen: shown, and kept on its Surface.
    pub(in crate::cockpit) fn select_pr_tab(
        &mut self,
        thread: ThreadId,
        number: u64,
        tab: PrTab,
        cx: &mut Context<Self>,
    ) {
        if tab == PrTab::Code {
            self.ensure_pr_changes(PrTarget::thread(thread, number), cx);
        }
        self.panel.pr_armed = None;
        self.panel
            .pr_actions
            .clear_done(PrTarget::thread(thread, number));
        self.apply_surface(thread, SurfaceAction::SelectPrTab { number, tab });
        cx.notify();
    }

    /// A view of `target`'s PR chosen, in whichever surface shows it.
    fn select_pr_view(&mut self, target: PrTarget, tab: PrTab, cx: &mut Context<Self>) {
        let was = match target.source {
            PrSource::Thread(thread) => self
                .panel
                .surfaces
                .get(thread)
                .map_or(PrTab::Summary, |state| state.pr_tab(target.number)),
            PrSource::Project(_) => self.prs.read(cx).tab(),
        };
        if was != tab {
            self.panel.pr_picked = Some(cx.background_executor().now());
        }
        match target.source {
            PrSource::Thread(thread) => self.select_pr_tab(thread, target.number, tab, cx),
            PrSource::Project(_) => {
                if tab == PrTab::Code {
                    self.ensure_pr_changes(target, cx);
                }
                self.prs.update(cx, |prs, cx| prs.set_tab(tab, cx));
                cx.notify();
            }
        }
    }

    /// The `⋯`'s rows for `target`'s PR as it stands: a row the forge's
    /// reading rules out, or one an action in flight holds, says why.
    pub(in crate::cockpit) fn pr_menu_rows(
        &self,
        target: PrTarget,
    ) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let PrSource::Thread(thread) = target.source else {
            return Vec::new();
        };
        let snapshot = self.panel.pr_reads.snapshot(target);
        let Some(tone) = snapshot.map(PrTone::of_snapshot) else {
            return Vec::new();
        };
        let branch_pr = self
            .facts
            .pr(thread)
            .is_some_and(|pr| pr.number == target.number);
        let guide = snapshot.and_then(guide).is_some();
        let busy = self.panel.pr_actions.pending(target);
        let mut rows = Vec::new();
        for group in MENU {
            let group: Vec<_> = group
                .iter()
                .filter(|action| {
                    action.applies(tone)
                        && (branch_pr
                            || matches!(
                                action,
                                PrAction::Ask | PrAction::CopyLink | PrAction::CopyBranch
                            ))
                })
                .collect();
            if group.is_empty() {
                continue;
            }
            if !rows.is_empty() {
                rows.push(None);
            }
            for action in group {
                let mut item = menu::Item::new(action.label(guide));
                if action.destructive() {
                    item = item.destructive();
                }
                if action.hands_off() {
                    if let Some(why) = snapshot.and_then(|pr| action.blocked_by(pr)) {
                        item = item.disabled_because(why);
                    } else if busy {
                        item = item.disabled_because("Another action on this PR is in flight");
                    }
                }
                rows.push(Some((item, MenuVerb::Pr(*action))));
            }
        }
        rows
    }

    /// The PR Surface: header, then its content.
    pub(super) fn pr_surface(
        &self,
        thread: ThreadId,
        number: u64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        let tab = self
            .panel
            .surfaces
            .get(thread)
            .map_or(PrTab::Summary, |state| state.pr_tab(number));
        div()
            .debug_selector(|| "pr".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .text_size(px(FS_BADGE))
            .child(self.pr_header(PrTarget::thread(thread, number), cx))
            .children(self.pr_action_notice(PrTarget::thread(thread, number), cx))
            .child(self.pr_content(PrTarget::thread(thread, number), tab, cx))
            .into_any_element()
    }

    /// A PR's views, the notice of its newest read, and the view shown,
    /// whichever surface asks.
    pub(in crate::cockpit) fn pr_content(
        &self,
        target: PrTarget,
        tab: PrTab,
        cx: &mut Context<Self>,
    ) -> Div {
        let read = self.panel.pr_reads.get(target);
        let snapshot = read.and_then(|read| read.snapshot.as_ref());
        let thread = match target.source {
            PrSource::Thread(thread) => self
                .facts
                .pr(thread)
                .filter(|pr| pr.number == target.number)
                .map(|_| thread),
            PrSource::Project(_) => None,
        };
        let body = match (snapshot, read.and_then(PrRead::failure)) {
            (Some(pr), _) => match tab {
                PrTab::Summary => self.pr_summary(target, pr, cx),
                PrTab::Guide => self.pr_guide(target, thread, pr, cx),
                PrTab::Timeline => self.pr_timeline(target, pr, cx),
                PrTab::Code => self
                    .changes_pane(tools::Diff::PullRequest(target), cx)
                    .into_any_element(),
            },
            (None, Some(failure)) => pr_note(failure.note.clone().into()),
            (None, None) => pr_note("Reading\u{2026}".into()),
        };
        let notice = read.and_then(|read| match (read.reading(), read.failure()) {
            (true, _) if snapshot.is_some() => Some(pr_refreshing()),
            (_, Some(failure)) => Some(self.pr_failed(target, failure, snapshot.is_some(), cx)),
            _ => None,
        });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .bg(rgb(ferrite_ui::theme::TX_GROUND))
            .children(snapshot.map(pr_identity))
            .child(self.pr_views(target, tab, cx))
            .children(notice)
            .child(ferrite_ui::motion::swap_since(
                self.panel.pr_picked,
                div().flex().flex_col().flex_1().min_h_0().child(body),
            ))
    }

    /// A read that failed: what went wrong and its way back, over the last
    /// good content when there is any.
    fn pr_failed(
        &self,
        target: PrTarget,
        failure: &PrFailure,
        stale: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        let said = if stale {
            format!(
                "{}. Showing the last read. {}",
                failure.headline(),
                failure.note
            )
        } else {
            failure.note.clone()
        };
        let retry = pr_chip("pr-retry", "Retry").on_click(cx.listener(
            move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.read_pr_snapshot(target, true, cx);
            },
        ));
        div()
            .debug_selector(|| "pr-failed".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_4))
            .py(px(SPACE_2))
            .border_b(px(HAIRLINE_W))
            .border_color(soft_line())
            .text_color(rgb(RED))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_normal()
                    .child(SharedString::from(said)),
            )
            .child(retry)
            .into_any_element()
    }

    /// T3's header grid: the crumbs, then the actions.
    fn pr_header(&self, target: PrTarget, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let PrTarget {
            source: PrSource::Thread(thread),
            number,
        } = target
        else {
            return div();
        };
        let tone = self
            .panel
            .pr_reads
            .snapshot(target)
            .map(PrTone::of_snapshot)
            .or_else(|| self.pr_tone(thread))
            .unwrap_or(PrTone::Open);
        let repo = self
            .panel
            .pr_reads
            .snapshot(target)
            .map(|pr| pr.url.clone())
            .or_else(|| self.facts.pr(thread).and_then(|pr| pr.url.clone()))
            .and_then(|url| repo_of(&url));
        let number_semantics = Semantics::button(format!("Pull request #{number}, open on GitHub"));
        let number_handle =
            self.panel.focus.register(PanelControl::PrNumber, number_semantics.clone(), cx);
        let url = self
            .panel
            .pr_reads
            .snapshot(target)
            .map(|pr| pr.url.clone());
        let branch_pr = self.facts.pr(thread).is_some_and(|pr| pr.number == number);
        let crumbs = div()
            .debug_selector(|| "pr-crumbs".into())
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(px(SPACE_1))
            .h(px(PR_ROW_H))
            .pl(px(SPACE_4))
            .children(repo.map(|repo| {
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(INK_SUBTLE))
                    .child(SharedString::from(format!("{repo} /")))
            }))
            .child(
                div()
                    .id("pr-number")
                    .debug_selector(|| "pr-number".into())
                    .relative()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(SPACE_1))
                    .font_weight(W_LABEL)
                    .text_color(rgb(tone.ink()))
                    .cursor_pointer()
                    .hover(|link| link.underline())
                    .tooltip(ferrite_ui::menu::tooltip("Open on GitHub"))
                    .map(|link| controls::focusable(link, &number_handle, &number_semantics))
                    .child(self.panel.focus.measured(PanelControl::PrNumber))
                    .child(SharedString::from(format!("#{number}")))
                    .child(icon(icons::EXTERNAL, PR_LINK_GLYPH, INK_SUBTLE))
                    .on_click(move |_: &ClickEvent, _, cx| {
                        if let Some(url) = &url {
                            cx.open_url(url);
                        }
                    }),
            );
        let primary = if tone.settled() || !branch_pr {
            pr_pill(tone).into_any_element()
        } else {
            self.pr_primary(target, tone, cx)
        };
        let more_semantics = Semantics::button("More pull request actions");
        let more_handle = self.panel.focus.register(PanelControl::PrMore, more_semantics.clone(), cx);
        let more = self.doors().button(
            controls::icon_stop(
                "pr-more",
                icons::ELLIPSIS,
                IconSize::Standard,
                &more_handle,
                &more_semantics,
                cx,
            ),
            pr_more_door(target),
            "More",
            cx,
        );
        div()
            .debug_selector(|| "pr-header".into())
            .bg(rgb(ferrite_ui::theme::TX_GROUND))
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_2))
            .border_b(px(HAIRLINE_W))
            .border_color(soft_line())
            .child(crumbs)
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(SPACE_1))
                    .h(px(PR_ROW_H))
                    .mr(px(SPACE_4))
                    .child(primary)
                    .child(more),
            )
    }

    /// An open PR's primary action: Merge, which asks once more before it
    /// hands off; a draft's, Ready for review. While an action is in
    /// flight on the PR, or the forge's reading rules Merge out, it is
    /// there but cannot be pressed (`pr_action_notice` says why).
    fn pr_primary(&self, target: PrTarget, tone: PrTone, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let armed = self.panel.pr_armed == Some(target);
        let (action, label) = match tone {
            PrTone::Draft => (PrAction::ReadyForReview, "Ready for review"),
            _ if armed => (PrAction::MergeNow, "Confirm merge?"),
            _ => (PrAction::MergeNow, "Merge"),
        };
        let held = self.panel.pr_actions.pending(target)
            || self
                .panel
                .pr_reads
                .snapshot(target)
                .is_some_and(|pr| action.blocked_by(pr).is_some());
        let key = ferrite_ui::pointer::hover_key(&gpui::ElementId::from("pr-primary"));
        let semantics = Semantics::button(label);
        let handle = self.panel.focus.register(PanelControl::PrPrimary, semantics.clone(), cx);
        div()
            .id("pr-primary")
            .debug_selector(|| "pr-primary".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(SURFACE_TAB_H))
            .px(px(PR_SEGMENT_PAD_X))
            .rounded(px(R_CONTROL))
            .bg(paint::RAISED)
            .font_weight(W_LABEL)
            .text_color(rgb(if held { INK_SUBTLE } else { INK_STRONG }))
            .when(!held, |button| button.hover_control(key).press_control())
            .map(|primary| controls::focusable(primary, &handle, &semantics))
            .child(label)
            .when(!held, |button| {
                button.on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    if action == PrAction::MergeNow && view.panel.pr_armed != Some(target) {
                        view.panel.pr_armed = Some(target);
                        cx.notify();
                        return;
                    }
                    view.panel.pr_armed = None;
                    view.run_pr_action(target, action, cx);
                }))
            })
            .into_any_element()
    }

    /// What an action on `target`'s PR stands at, or what keeps Merge from
    /// it, in a line under the header: queued, running, failed with its
    /// retry, done. Merge armed says what it will merge with and what the
    /// panel does not know of it.
    pub(in crate::cockpit) fn pr_action_words(&self, target: PrTarget) -> Option<PrNotice> {
        use ferrite_ui::theme::*;
        let number = target.number;
        let notice = match self.panel.pr_actions.get(target) {
            Some(ActionState::Pending { action, stage, .. }) => {
                let what = match stage {
                    Stage::Queued(_) => "queued behind the agent's running turn",
                    Stage::Running => "the agent is on it",
                    Stage::Checking => "checking the PR",
                };
                (
                    format!("{} #{number}: {what}.", action.working()),
                    INK_SUBTLE,
                    vec![("pr-action-dismiss", "Stop waiting", false)],
                )
            }
            Some(ActionState::Failed { action, error, .. }) => (
                format!("{} #{number} failed. {error}", action.working()),
                RED,
                vec![
                    ("pr-action-retry", "Retry", true),
                    ("pr-action-dismiss", "Dismiss", false),
                ],
            ),
            Some(ActionState::Done { action }) => (
                format!("{} #{number}.", action.done()),
                INK_SUBTLE,
                vec![("pr-action-dismiss", "Dismiss", false)],
            ),
            None => {
                let pr = self.panel.pr_reads.snapshot(target)?;
                let (said, ink) = match (PrTone::of_snapshot(pr), pr.merge) {
                    (PrTone::Open, _) if self.panel.pr_armed == Some(target) => (
                        "Merges with the repository's default method. Your permission and the methods it allows are not read here.".to_string(),
                        INK_SUBTLE,
                    ),
                    (PrTone::Open, _) => (PrAction::MergeNow.blocked_by(pr).map(|why| format!("Merge unavailable. {why}"))?, RED),
                    _ => return None,
                };
                (said, ink, Vec::new())
            }
        };
        Some(notice)
    }

    /// `pr_action_words`, drawn.
    fn pr_action_notice(&self, target: PrTarget, cx: &mut Context<Self>) -> Option<AnyElement> {
        use ferrite_ui::theme::*;
        let (said, ink, buttons) = self.pr_action_words(target)?;
        let buttons = buttons.into_iter().map(|(id, label, retry)| {
            let chip =
                pr_chip(id, label).on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    if retry {
                        view.retry_pr_action(target, cx);
                    } else {
                        view.dismiss_pr_action(target, cx);
                    }
                }));
            // The leaving chip holds one width and the right edge, in
            // every state, so a second click on Retry never lands on the
            // Stop waiting that replaces it.
            if retry {
                chip
            } else {
                chip.w(px(PR_LEAVE_CHIP_W)).justify_center()
            }
        });
        Some(
            div()
                .debug_selector(|| "pr-action".into())
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(SPACE_2))
                .px(px(SPACE_4))
                .py(px(SPACE_2))
                .border_b(px(HAIRLINE_W))
                .border_color(soft_line())
                .text_color(rgb(ink))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .whitespace_normal()
                        .child(SharedString::from(said)),
                )
                .children(buttons)
                .into_any_element(),
        )
    }

    /// The views as a segmented group, the checks and comments after it.
    fn pr_views(&self, target: PrTarget, shown: PrTab, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let tabs = PrTab::ALL.into_iter();
        let segments = tabs.map(|tab| {
            let id = SharedString::from(format!("pr-tab-{}", tab.label()));
            let selector = id.to_string();
            let key = ferrite_ui::pointer::hover_key(&gpui::ElementId::from(id.clone()));
            let pressed = tab == shown;
            let semantics = Semantics::tab(tab.label(), pressed);
            let handle = self.panel.focus.register(PanelControl::PrView(tab), semantics.clone(), cx);
            let segment = div()
                .id(id)
                .debug_selector(move || selector.clone())
                .flex()
                .items_center()
                .h(px(SURFACE_TAB_H))
                .px(px(PR_SEGMENT_PAD_X))
                .rounded(px(R_CONTROL))
                .text_color(rgb(if pressed { INK_STRONG } else { INK_SUBTLE }))
                .map(|segment| {
                    if pressed {
                        segment.bg(rgba(PR_SEGMENT_PRESSED))
                    } else {
                        segment.hover_raised(key)
                    }
                })
                .map(|segment| controls::focusable(segment, &handle, &semantics))
                .child(tab.label())
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.select_pr_view(target, tab, cx);
                }));
            if tab != PrTab::Code {
                return segment.into_any_element();
            };
            // Code's diff is read as it comes near, not after the press:
            // the segment's own hover is its wash's, so a wrapper listens.
            div()
                .id("pr-tab-Code-near")
                .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
                    if *hovered {
                        view.ensure_pr_changes(target, cx);
                    }
                }))
                .child(segment)
                .into_any_element()
        });
        let snapshot = self.panel.pr_reads.snapshot(target);
        let checks = snapshot.and_then(checks_line).map(|(line, ink)| {
            div()
                .debug_selector(|| "pr-checks".into())
                .text_color(rgb(ink))
                .child(SharedString::from(line))
        });
        let comments = snapshot.filter(|pr| !pr.comments.is_empty()).map(|pr| {
            let count = pr.comments.len();
            div()
                .text_color(rgb(INK_SUBTLE))
                .child(SharedString::from(format!(
                    "{count} comment{}",
                    if count == 1 { "" } else { "s" }
                )))
        });
        div()
            .debug_selector(|| "pr-views".into())
            .border_t(px(HAIRLINE_W))
            .border_color(soft_line())
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(SPACE_2))
            .px(px(SPACE_4))
            .py(px(SPACE_2))
            .child(
                div()
                    .debug_selector(|| "pr-segments".into())
                    .flex()
                    .flex_shrink_0()
                    .gap(px(SPACE_0_5))
                    .p(px(SPACE_0_5))
                    .rounded(px(PR_GROUP_R))
                    .bg(rgba(PR_GROUP_GROUND))
                    .children(segments),
            )
            .child(div().flex_1())
            .children(checks)
            .children(comments)
    }

    /// `text` as the Markdown it is, in the reading size of a document.
    fn pr_prose(&self, id: String, text: &str) -> AnyElement {
        use ferrite_ui::theme::*;
        div()
            .font_family(FONT_UI)
            .text_size(px(FS_PROSE))
            .line_height(px(LH_PROSE))
            .child(ferrite_pane::rich::Markdown::new(
                id,
                text.to_string(),
                self.panel.pr_rich.clone(),
            ))
            .into_any_element()
    }

    /// Summary: the description and checks below the persistent PR identity.
    fn pr_summary(&self, target: PrTarget, pr: &WirePr, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let checks = pr
            .checks
            .iter()
            .enumerate()
            .map(|(index, check)| {
                let (glyph, ink) = match check.status {
                    WireCheckStatus::Success => (icons::CHECK_CIRCLE, GREEN),
                    WireCheckStatus::Failure => (icons::X_CIRCLE, RED),
                    WireCheckStatus::Running | WireCheckStatus::Pending => {
                        (icons::CIRCLE_DASHED, YELLOW)
                    }
                    WireCheckStatus::Skipped | WireCheckStatus::Unknown => {
                        (icons::CIRCLE_MINUS, INK_SUBTLE)
                    }
                };
                let row = div()
                    .id(("pr-check", index))
                    .debug_selector(move || format!("pr-check-{index}"))
                    .flex()
                    .items_center()
                    .gap(px(SPACE_2))
                    .h(px(PR_ROW_H))
                    .child(icon(glyph, ICON_CHEVRON, ink))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(SharedString::from(check.name.clone())),
                    );
                match check.url.clone() {
                    Some(url) => row
                        .cursor_pointer()
                        .hover(|row| row.underline())
                        .child(icon(icons::EXTERNAL, PR_LINK_GLYPH, INK_SUBTLE))
                        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&url)))
                        .into_any_element(),
                    None => row.into_any_element(),
                }
            })
            .collect::<Vec<_>>();
        pr_scroll("pr-summary")
            .when(!pr.body.is_empty(), |summary| {
                summary.child(
                    div()
                        .debug_selector(|| "pr-body".into())
                        .mt(px(SPACE_3))
                        .child(self.pr_prose(format!("pr-body-{target:?}"), &pr.body)),
                )
            })
            .when(!pr.checks.is_empty(), |summary| {
                summary
                    .child(
                        div()
                            .mt(px(SPACE_3))
                            .font_weight(W_LABEL)
                            .text_color(rgb(INK_SUBTLE))
                            .child("Checks"),
                    )
                    .children(checks)
            })
            .when(pr.more_checks(), |summary| {
                summary.child(pr_more(
                    "pr-more-checks",
                    "More checks than are shown. See all on the forge",
                    &pr.url,
                    cx,
                ))
            })
            .into_any_element()
    }

    /// Guide: the PR's latest review guide, or the way to ask for one (a
    /// Thread's own PR only: the PRs destination reads and does not ask).
    fn pr_guide(
        &self,
        target: PrTarget,
        thread: Option<ThreadId>,
        pr: &WirePr,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use ferrite_ui::theme::*;
        if let Some(text) = guide(pr) {
            return pr_scroll("pr-guide")
                .child(self.pr_prose(format!("pr-guide-{target:?}"), text))
                .into_any_element();
        }
        div()
            .debug_selector(|| "pr-guide-empty".into())
            .flex()
            .flex_col()
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(SPACE_2))
            .text_color(rgb(INK_SUBTLE))
            .child("No review guide yet")
            .when(thread.is_some(), |empty| {
                let working = matches!(
                    self.panel.pr_actions.get(target),
                    Some(ActionState::Pending {
                        action: PrAction::WriteGuide,
                        ..
                    })
                );
                empty.child(
                    div()
                        .id("pr-write-guide")
                        .debug_selector(|| "pr-write-guide".into())
                        .flex()
                        .items_center()
                        .h(px(SURFACE_TAB_H))
                        .px(px(PR_SEGMENT_PAD_X))
                        .rounded(px(R_CONTROL))
                        .bg(paint::RAISED)
                        .text_color(rgb(if working { INK_SUBTLE } else { INK_STRONG }))
                        .when(!working, |button| {
                            button
                                .hover_control("pr-write-guide")
                                .press_control()
                                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                                    view.run_pr_action(target, PrAction::WriteGuide, cx);
                                }))
                        })
                        .child(if working {
                            "Writing guide\u{2026}"
                        } else {
                            "Write guide"
                        }),
                )
            })
            .into_any_element()
    }

    /// Timeline: the conversation as the forge's read lists it, newest
    /// first (T3's default order): each entry's code context, its
    /// resolved or outdated mark, and its replies beneath.
    fn pr_timeline(&self, target: PrTarget, pr: &WirePr, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        if pr.comments.is_empty() {
            return pr_note("No comments yet".into());
        }
        let entries = pr.comments.iter().enumerate().map(|(index, comment)| {
            let kind = match comment.kind {
                WirePrCommentKind::Review => "reviewed",
                WirePrCommentKind::Finding => "commented on",
                WirePrCommentKind::Comment | WirePrCommentKind::Unknown => "commented",
            };
            let what = match comment.kind {
                WirePrCommentKind::Finding => {
                    format!("{} {kind} {}", comment.author, comment.anchor)
                }
                _ => format!("{} {kind}", comment.author),
            };
            let verdict = comment.verdict.and_then(crate::pr::review_verdict);
            let mark = |text: &'static str, ink: u32| {
                div()
                    .debug_selector(move || format!("pr-{}-{index}", text.to_lowercase()))
                    .flex_shrink_0()
                    .text_color(rgb(ink))
                    .child(text)
            };
            div()
                .id(SharedString::from(format!(
                    "pr-activity-{target:?}-{}",
                    comment
                        .id
                        .as_deref()
                        .map(str::to_string)
                        .unwrap_or_else(|| index.to_string())
                )))
                .debug_selector(move || format!("pr-comment-{index}"))
                .flex()
                .flex_col()
                .gap(px(SPACE_1))
                .py(px(SPACE_2))
                .border_b(px(HAIRLINE_W))
                .border_color(soft_line())
                .child(
                    div()
                        .flex()
                        .gap(px(SPACE_2))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_weight(W_LABEL)
                                .text_color(rgb(INK_STRONG))
                                .child(SharedString::from(what)),
                        )
                        .when(comment.author_is_bot, |row| {
                            row.child(
                                div()
                                    .debug_selector(move || format!("pr-bot-{index}"))
                                    .text_size(px(FS_META))
                                    .text_color(rgb(INK_SUBTLE))
                                    .child("bot"),
                            )
                        })
                        .children(verdict.map(|(text, ink)| {
                            div()
                                .debug_selector(move || format!("pr-verdict-{index}"))
                                .flex_shrink_0()
                                .text_color(rgb(ink))
                                .child(text)
                        }))
                        .when(comment.resolved, |head| head.child(mark("Resolved", GREEN)))
                        .when(comment.outdated, |head| {
                            head.child(mark("Outdated", INK_SUBTLE))
                        })
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(rgb(INK_SUBTLE))
                                .child(stamp_age(&comment.created_at)),
                        ),
                )
                .children(comment.snippet.as_ref().map(|snippet| {
                    div()
                        .debug_selector(move || format!("pr-snippet-{index}"))
                        .px(px(SPACE_2))
                        .py(px(SPACE_1))
                        .rounded(px(R_CONTROL))
                        .bg(paint::RAISED)
                        .font_family(FONT_CODE)
                        .text_size(px(FS_META))
                        .text_color(rgb(INK_SUBTLE))
                        .overflow_x_hidden()
                        .child(SharedString::from(snippet.clone()))
                }))
                .when(!comment.body.is_empty(), |entry| {
                    entry.child(
                        self.pr_prose(format!("pr-comment-{target:?}-{index}"), &comment.body),
                    )
                })
                .children(comment.replies.iter().enumerate().map(|(reply, said)| {
                    div()
                        .id(SharedString::from(format!(
                            "pr-reply-{target:?}-{}-{}",
                            comment.id.as_deref().unwrap_or("legacy"),
                            said.id
                                .as_deref()
                                .map(str::to_string)
                                .unwrap_or_else(|| format!("{index}-{reply}"))
                        )))
                        .debug_selector(move || format!("pr-reply-{index}-{reply}"))
                        .flex()
                        .flex_col()
                        .ml(px(SPACE_3))
                        .pl(px(SPACE_3))
                        .border_l(px(HAIRLINE_W))
                        .border_color(soft_line())
                        .child(
                            div()
                                .flex()
                                .gap(px(SPACE_2))
                                .text_color(rgb(INK_SUBTLE))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .font_weight(W_LABEL)
                                        .child(SharedString::from(format!(
                                            "{} replied",
                                            said.author
                                        ))),
                                )
                                .child(div().flex_shrink_0().child(stamp_age(&said.created_at))),
                        )
                        .child(
                            self.pr_prose(
                                format!("pr-reply-{target:?}-{index}-{reply}"),
                                &said.body,
                            ),
                        )
                }))
        });
        pr_scroll("pr-timeline")
            .children(entries)
            .when(pr.more_comments(), |timeline| {
                timeline.child(pr_more(
                    "pr-more-comments",
                    "Older comments are not shown. See all on the forge",
                    &pr.url,
                    cx,
                ))
            })
            .into_any_element()
    }
}

/// The way past a read's cap: `url`, the PR where the forge holds the rest.
fn pr_more(
    id: &'static str,
    said: &'static str,
    url: &str,
    cx: &mut Context<CockpitView>,
) -> AnyElement {
    use ferrite_ui::theme::*;
    let url = url.to_string();
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex()
        .items_center()
        .gap(px(SPACE_1))
        .mt(px(SPACE_2))
        .cursor_pointer()
        .text_color(rgb(INK_SUBTLE))
        .hover(|link| link.underline())
        .child(said)
        .child(icon(icons::EXTERNAL, PR_LINK_GLYPH, INK_SUBTLE))
        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&url)))
        .into_any_element()
}

/// A merged or closed PR's pill (T3's outline Badge): its state's icon and
/// word in its ink.
pub(in crate::cockpit) fn pr_pill(tone: PrTone) -> Div {
    use ferrite_ui::theme::*;
    div()
        .debug_selector(|| "pr-pill".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(SPACE_1))
        .h(px(SURFACE_TAB_H))
        .min_w(px(SURFACE_TAB_H))
        .px(px(PR_PILL_PAD_X))
        .rounded(px(PR_GROUP_R))
        .border_1()
        .border_color(rgb(LINE))
        .bg(rgba(PR_PILL_GROUND))
        .font_weight(W_LABEL)
        .text_color(rgb(tone.ink()))
        .child(icon(tone.glyph(), SURFACE_PLUS_GLYPH, tone.ink()))
        .child(tone.label())
}

/// A refresh over the last good content: it stays, a mark turns above it.
fn pr_refreshing() -> AnyElement {
    use ferrite_ui::theme::*;
    div()
        .debug_selector(|| "pr-refreshing".into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(SPACE_2))
        .px(px(SPACE_4))
        .py(px(SPACE_2))
        .border_b(px(HAIRLINE_W))
        .border_color(soft_line())
        .text_color(rgb(INK_SUBTLE))
        .child(ferrite_ui::components::working_mark(
            PR_LINK_GLYPH,
            INK_SUBTLE,
        ))
        .child("Refreshing\u{2026}")
        .into_any_element()
}

/// What a PR's notice strip says: its words, their ink, and its chips as
/// (selector, label, whether it retries).
type PrNotice = (String, u32, Vec<(&'static str, &'static str, bool)>);

/// The width of the chip that dismisses a notice, whatever it is called.
const PR_LEAVE_CHIP_W: f32 = 112.;

fn pr_identity(pr: &WirePr) -> Div {
    use ferrite_ui::theme::*;
    let author = pr.author.as_ref().map(|author| {
        div()
            .debug_selector(|| "pr-author".into())
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .size(px(16.))
                    .rounded_full()
                    .bg(paint::RAISED)
                    .children(pr.author_avatar_url.as_ref().map(|url| {
                        gpui::img(SharedString::from(url.clone()))
                            .size(px(16.))
                            .rounded_full()
                    })),
            )
            .child(SharedString::from(author.clone()))
    });
    let updated = pr.updated_at.as_ref().map(|stamp| {
        let age = stamp_age(stamp);
        let label = if !age.is_empty() {
            format!("{age} ago")
        } else if ferrite_core::clock::parse_stamp(stamp).is_some() {
            "just now".into()
        } else {
            stamp.clone()
        };
        div()
            .debug_selector(|| "pr-updated".into())
            .child(SharedString::from(format!("updated {label}")))
    });
    let files = pr.changed_files.map(|count| {
        div()
            .debug_selector(|| "pr-files".into())
            .flex()
            .items_center()
            .gap(px(6.))
            .child(icon(icons::DIFF, 14., INK_SUBTLE))
            .child(SharedString::from(format!(
                "{count} {}",
                if count == 1 { "file" } else { "files" }
            )))
    });
    div()
        .debug_selector(|| "pr-identity".into())
        .flex_shrink_0()
        .min_w_0()
        .px(px(16.))
        .pt(px(4.))
        .pb(px(16.))
        .text_size(px(12.))
        .text_color(rgb(INK_SUBTLE))
        .child(
            div()
                .debug_selector(|| "pr-title".into())
                .truncate()
                .text_size(px(16.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(INK_STRONG))
                .child(SharedString::from(pr.title.clone())),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .min_h(px(20.))
                .mt(px(8.))
                .children(author)
                .when(pr.author.is_some() && pr.updated_at.is_some(), |row| {
                    row.child("·")
                })
                .children(updated)
                .child(div().flex_1())
                .children(pr.url.contains("/pull/").then(|| {
                    pr_copyable(
                        "pr-copy-checkout",
                        format!("gh pr checkout {}", pr.number),
                        "Copy checkout command",
                    )
                })),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .min_h(px(20.))
                .mt(px(16.))
                .child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .gap(px(6.))
                        .font_family(FONT_CODE)
                        .child(div().truncate().child(SharedString::from(pr.base.clone())))
                        .child("←")
                        .child(pr_copyable(
                            "pr-copy-head",
                            pr.head.clone(),
                            "Copy branch name",
                        )),
                )
                .children(files)
                .children(pr.additions.map(|count| {
                    div()
                        .debug_selector(|| "pr-additions".into())
                        .font_family(FONT_CODE)
                        .text_color(rgb(GREEN))
                        .child(SharedString::from(format!("+{count}")))
                }))
                .children(pr.deletions.map(|count| {
                    div()
                        .debug_selector(|| "pr-deletions".into())
                        .font_family(FONT_CODE)
                        .text_color(rgb(RED))
                        .child(SharedString::from(format!("−{count}")))
                })),
        )
}

fn pr_copyable(id: &'static str, text: String, tooltip: &'static str) -> gpui::Stateful<Div> {
    let value = text.clone();
    div()
        .id(id)
        .debug_selector(move || id.into())
        .min_w_0()
        .truncate()
        .font_family(ferrite_ui::theme::FONT_CODE)
        .cursor_pointer()
        .tooltip(ferrite_ui::menu::tooltip(tooltip))
        .hover(|copy| copy.text_color(rgb(ferrite_ui::theme::INK_STRONG)))
        .child(SharedString::from(text))
        .on_click(move |_: &ClickEvent, _, cx| {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(value.clone()))
        })
}

fn pr_chip(id: &'static str, label: &'static str) -> gpui::Stateful<Div> {
    use ferrite_ui::theme::*;
    div()
        .id(id)
        .debug_selector(|| id.into())
        .flex()
        .flex_shrink_0()
        .items_center()
        .h(px(SURFACE_TAB_H))
        .px(px(PR_SEGMENT_PAD_X))
        .rounded(px(R_CONTROL))
        .bg(paint::RAISED)
        .font_weight(W_LABEL)
        .text_color(rgb(INK_STRONG))
        .hover_control(ferrite_ui::pointer::hover_key(&gpui::ElementId::from(id)))
        .press_control()
        .child(label)
}

fn pr_note(note: SharedString) -> AnyElement {
    use ferrite_ui::theme::*;
    div()
        .debug_selector(|| "pr-note".into())
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .text_size(px(FS_LABEL))
        .text_color(rgb(INK_SUBTLE))
        .child(note)
        .into_any_element()
}

/// A view's own scroll, wrapping its prose.
fn pr_scroll(selector: &'static str) -> gpui::Stateful<Div> {
    use ferrite_ui::theme::*;
    div()
        .id(selector)
        .debug_selector(move || selector.into())
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px(px(SPACE_4))
        .py(px(SPACE_3))
        .whitespace_normal()
        .text_color(rgb(INK_STRONG))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_guide_wins_among_a_newest_first_read() {
        use ferrite_core::protocol::WirePrComment;
        let said = |body: &str| WirePrComment {
            id: None,
            author_is_bot: false,
            place: None,
            kind: WirePrCommentKind::Comment,
            author: "bot".into(),
            anchor: "comment".into(),
            verdict: None,
            body: body.into(),
            snippet: None,
            created_at: String::new(),
            resolved: false,
            outdated: false,
            replies: Vec::new(),
        };
        use ferrite_core::protocol::{WireMerge, WireSync};
        let pr = WirePr {
            number: 1,
            title: String::new(),
            url: String::new(),
            body: String::new(),
            author: None,
            author_avatar_url: None,
            updated_at: None,
            changed_files: None,
            additions: None,
            deletions: None,
            state: WirePrState::Open,
            draft: false,
            head: String::new(),
            base: String::new(),
            merge: WireMerge::Unknown,
            sync: WireSync::InSync,
            checks: Vec::new(),
            comments: vec![
                said("## Review guide\nnewest"),
                said("thanks"),
                said("## Review guide\noldest"),
            ],
            truncated: false,
            checks_truncated: false,
            comments_truncated: false,
        };
        assert_eq!(guide(&pr), Some("## Review guide\nnewest"));
    }

    #[test]
    fn the_repo_comes_from_the_prs_address() {
        assert_eq!(
            repo_of("https://github.com/o/r/pull/48").as_deref(),
            Some("o/r")
        );
        assert_eq!(repo_of("not a url"), None);
    }

    #[test]
    fn each_state_offers_only_what_applies() {
        let offered = |tone| -> Vec<PrAction> {
            MENU.iter()
                .flat_map(|group| group.iter().copied())
                .filter(|action| action.applies(tone))
                .collect()
        };
        assert!(offered(PrTone::Open).contains(&PrAction::MergeNow));
        assert!(!offered(PrTone::Open).contains(&PrAction::Revert));
        assert!(offered(PrTone::Draft).contains(&PrAction::ReadyForReview));
        assert!(!offered(PrTone::Draft).contains(&PrAction::MergeNow));
        let merged = offered(PrTone::Merged);
        assert!(merged.contains(&PrAction::Revert) && merged.contains(&PrAction::DeleteBranch));
        assert!(!merged.contains(&PrAction::Close));
    }
}

/// The PR header's `⋯` menu, hung from its right edge.
pub(super) fn pr_more_door(target: PrTarget) -> Door {
    Door::new(MenuTarget::PrMore(target), Edge::Right)
}
