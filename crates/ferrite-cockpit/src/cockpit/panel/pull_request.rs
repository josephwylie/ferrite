//! The pull request Surface (ADR 0039; T3's `PullRequestDetailPanel`):
//! the PR of the Thread's branch as a tab of its own, opened from the PR
//! row in Overview's Changes and the `+` menu's Pull request (P).
//!
//! - **Header:** the repo and `#N` on the left, `#N` in its state's ink
//!   and opening the PR on the forge; on the right the primary action
//!   (Merge, armed on the first press: "Confirm merge?") or, once the PR is
//!   merged or closed, its state pill, then `⋯`.
//! - **`⋯`:** hand-offs (Ask a question opens a side chat; Explain, Fix
//!   findings and Write guide ask the Thread's agent), the PR's state
//!   (Ready for review or Convert to draft, Merge now, Enable auto-merge,
//!   Allow edits from maintainers), the destructive rows (Close, Revert,
//!   Delete branch), armed as every destructive row is, and copies. Ferrite
//!   runs no forge writes of its own: the PR's state changes are asked of
//!   the agent, which has `gh`, as `Create PR` is.
//! - **Views:** Summary, Guide, Timeline and Code as a segmented group,
//!   the checks and comment counts after it; the one chosen is kept on the
//!   Surface (`ThreadSurfaces::pr_tabs`). Code is the branch's diff, as
//!   Changes reads it, made only once Code is near: hovered or shown.
//!
//! The PR's facts (its number, state, checks) are the nav's reading
//! (`facts.pr`); its whole snapshot (body, checks, conversation) is read
//! when the Surface opens (`Cockpit::review_pull_request`).

use super::*;
use crate::cockpit::surfaces::PrTab;
use ferrite_core::protocol::{
    WireCheckStatus, WirePr, WirePrCommentKind, WirePrState,
};
use ferrite_core::workspace::PrState;

/// The PR's snapshot, as the Surface last asked for it.
pub(crate) enum PrRead {
    Reading,
    Read(Result<WirePr, String>),
}

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

    fn of_snapshot(pr: &WirePr) -> Self {
        match (pr.state, pr.draft) {
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

    fn settled(self) -> bool {
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
            Self::Ask | Self::Explain | Self::WriteGuide | Self::CopyLink | Self::CopyBranch => true,
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
            Self::ReadyForReview => format!("Mark pull request #{n} ready for review with `gh pr ready {n}`."),
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
    &[PrAction::Ask, PrAction::Explain, PrAction::FixFindings, PrAction::WriteGuide],
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

/// The latest review guide among the PR's comments.
fn guide(pr: &WirePr) -> Option<&str> {
    pr.comments
        .iter()
        .rev()
        .map(|comment| comment.body.as_str())
        .find(|body| body.trim_start().starts_with(GUIDE_HEAD))
}

/// `owner/repo`, out of the PR's address on the forge.
fn repo_of(url: &str) -> Option<String> {
    let path = url.split("://").nth(1)?.split('/').skip(1).collect::<Vec<_>>();
    match path.as_slice() {
        [owner, repo, ..] if !owner.is_empty() && !repo.is_empty() => Some(format!("{owner}/{repo}")),
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
        pr.checks.iter().filter(|check| status.contains(&check.status)).count()
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
fn soft_line() -> gpui::Rgba {
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

    /// Read the branch's PR whole for its Surface, unless a read is out or
    /// in. Tests and captures never run the forge's CLI: only a fixture
    /// world's forge answers them.
    pub(super) fn read_pr_snapshot(&mut self, thread: ThreadId, again: bool, cx: &mut Context<Self>) {
        if !again && self.panel.pr_reads.contains_key(&thread) {
            return;
        }
        let Some(path) = self.thread_path(thread) else {
            return;
        };
        if cfg!(any(test, feature = "visual-reference")) && !self.cockpit.has_forge_fixture(&path) {
            return;
        }
        self.panel.pr_reads.insert(thread, PrRead::Reading);
        let answer = self.cockpit.review_pull_request(&path);
        cx.spawn(async move |this, cx| {
            let read = match crate::cockpit::beside::answered(answer, cx).await {
                Ok(read) => read.pr,
                Err(error) => Err(error.to_string()),
            };
            this.update(cx, |view, cx| {
                if let Ok(pr) = &read {
                    view.retire_other_prs(thread, pr.number);
                }
                view.panel.pr_reads.insert(thread, PrRead::Read(read));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// A fixture world's snapshot of `thread`'s PR, as if read.
    #[cfg(any(test, feature = "visual-reference"))]
    pub(crate) fn fixture_pr_snapshot(&mut self, thread: ThreadId, pr: WirePr) {
        self.retire_other_prs(thread, pr.number);
        self.panel.pr_reads.insert(thread, PrRead::Read(Ok(pr)));
    }

    /// The nav's reading of `thread`'s PR changed. A tab for another PR
    /// than the branch's goes, and a snapshot the reading has overtaken (a
    /// merge the agent ran, a new PR on the branch) is let go, to be read
    /// again as the Surface next shows: until then the reading speaks.
    pub(crate) fn pr_reading_changed(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(reading) = self.facts.pr(thread) else {
            return;
        };
        let (number, tone) = (reading.number, PrTone::of_reading(reading.state, reading.draft));
        self.retire_other_prs(thread, number);
        let overtaken = self
            .pr_snapshot(thread)
            .is_some_and(|pr| pr.number != number || PrTone::of_snapshot(pr) != tone);
        if overtaken {
            self.panel.pr_reads.remove(&thread);
            cx.notify();
        }
    }

    /// The branch's PR is `number`: a tab for another (one restored from
    /// before the branch moved on) goes, so nothing in it acts on a PR it
    /// does not name.
    fn retire_other_prs(&mut self, thread: ThreadId, number: u64) {
        let others: Vec<SurfaceId> = self.panel.surfaces.get(thread).map_or_else(Vec::new, |state| {
            state
                .tabs
                .iter()
                .filter(|id| matches!(id, SurfaceId::PullRequest(other) if *other != number))
                .cloned()
                .collect()
        });
        for id in others {
            self.forget_surface(thread, &id);
        }
    }

    /// Merge armed on a PR Surface no longer shown (another tab, the Panel
    /// hidden, another Thread) is disarmed: one press, much later, must
    /// not merge.
    pub(super) fn disarm_unseen_merge(&mut self) {
        let Some(armed) = self.panel.pr_armed else {
            return;
        };
        let shown = self
            .panel_pane()
            .and_then(|index| Some((self.panes[index].thread()?, self.shown_surface(index)?)));
        if !matches!(shown, Some((thread, SurfaceId::PullRequest(_))) if thread == armed) {
            self.panel.pr_armed = None;
        }
    }

    /// The PR's snapshot, once read.
    fn pr_snapshot(&self, thread: ThreadId) -> Option<&WirePr> {
        match self.panel.pr_reads.get(&thread) {
            Some(PrRead::Read(Ok(pr))) => Some(pr),
            _ => None,
        }
    }

    /// The PR's state: its snapshot's, else the nav's reading's.
    pub(crate) fn pr_tone(&self, thread: ThreadId) -> Option<PrTone> {
        self.pr_snapshot(thread).map(PrTone::of_snapshot).or_else(|| {
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
            self.ensure_branch_changes(thread, cx);
        }
        self.panel.pr_armed = None;
        self.surfaces_mut(thread).pr_tabs.insert(number, tab);
        cx.notify();
    }

    /// The `⋯`'s rows for the PR as it stands; an armed row asks again.
    pub(in crate::cockpit) fn pr_menu_rows(&self, thread: ThreadId) -> Vec<Option<(menu::Item, MenuVerb)>> {
        let Some(tone) = self.pr_tone(thread) else {
            return Vec::new();
        };
        let guide = self.pr_snapshot(thread).and_then(guide).is_some();
        let mut rows = Vec::new();
        for group in MENU {
            let group: Vec<_> = group.iter().filter(|action| action.applies(tone)).collect();
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
                rows.push(Some((item, MenuVerb::Pr(*action))));
            }
        }
        rows
    }

    /// A row of the `⋯` (or the primary action): copy, open a side chat
    /// on the PR, or ask the Thread's agent, queued behind a running turn.
    pub(in crate::cockpit) fn run_pr_action(&mut self, thread: ThreadId, action: PrAction, cx: &mut Context<Self>) {
        let Some(pr) = self.pr_snapshot(thread).cloned() else {
            return;
        };
        match action {
            PrAction::CopyLink => cx.write_to_clipboard(ClipboardItem::new_string(pr.url)),
            PrAction::CopyBranch => cx.write_to_clipboard(ClipboardItem::new_string(pr.head)),
            PrAction::Ask => {
                let passage = format!("Pull request #{} {}", pr.number, pr.title);
                self.open_side_chat(thread, Some(passage), cx);
            }
            _ => {
                let (Some(prompt), Some(index)) = (action.prompt(&pr), self.pane_for(thread)) else {
                    return;
                };
                self.send_again(index, thread, prompt, cx);
            }
        }
        cx.notify();
    }

    /// The PR Surface: header, views, the view shown.
    pub(super) fn pr_surface(&self, thread: ThreadId, number: u64, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let tab = self
            .panel
            .surfaces
            .get(thread)
            .map_or(PrTab::Summary, |state| state.pr_tab(number));
        let snapshot = self.pr_snapshot(thread);
        let body = match (self.panel.pr_reads.get(&thread), snapshot) {
            (_, Some(pr)) => match tab {
                PrTab::Summary => self.pr_summary(pr),
                PrTab::Guide => self.pr_guide(thread, pr, cx),
                PrTab::Timeline => self.pr_timeline(pr),
                PrTab::Code => self.changes_pane(tools::Diff::PullRequest(thread), cx).into_any_element(),
            },
            (Some(PrRead::Read(Err(note))), _) => pr_note(note.clone().into()),
            _ => pr_note("Reading\u{2026}".into()),
        };
        div()
            .debug_selector(|| "pr".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .text_size(px(FS_BADGE))
            .child(self.pr_header(thread, number, cx))
            .child(self.pr_views(thread, number, tab, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(body),
            )
            .into_any_element()
    }

    /// T3's header grid: the crumbs, then the actions.
    fn pr_header(&self, thread: ThreadId, number: u64, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let tone = self.pr_tone(thread).unwrap_or(PrTone::Open);
        let repo = self
            .pr_snapshot(thread)
            .map(|pr| pr.url.clone())
            .or_else(|| self.facts.pr(thread).and_then(|pr| pr.url.clone()))
            .and_then(|url| repo_of(&url));
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
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(SPACE_1))
                    .font_weight(W_LABEL)
                    .text_color(rgb(tone.ink()))
                    .cursor_pointer()
                    .hover(|link| link.underline())
                    .tooltip(ferrite_ui::menu::tooltip("Open on GitHub"))
                    .child(SharedString::from(format!("#{number}")))
                    .child(icon(icons::EXTERNAL, PR_LINK_GLYPH, INK_SUBTLE))
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.open_pr(thread, cx);
                    })),
            );
        let more = ferrite_pane::preview::head_door("pr-more", "pr-more".into(), icons::ELLIPSIS)
            .tooltip(ferrite_ui::menu::tooltip("More"))
            .on_click(cx.listener(move |view, event: &ClickEvent, _, cx| {
                cx.stop_propagation();
                view.open_context_menu(MenuTarget::PrMore(thread), event.position(), cx);
            }));
        let primary = if tone.settled() {
            pr_pill(tone).into_any_element()
        } else {
            self.pr_primary(thread, tone, cx)
        };
        div()
            .debug_selector(|| "pr-header".into())
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
    /// hands off; a draft's, Ready for review.
    fn pr_primary(&self, thread: ThreadId, tone: PrTone, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let (action, label) = match tone {
            PrTone::Draft => (PrAction::ReadyForReview, "Ready for review"),
            _ if self.panel.pr_armed == Some(thread) => (PrAction::MergeNow, "Confirm merge?"),
            _ => (PrAction::MergeNow, "Merge"),
        };
        let key = ferrite_ui::pointer::hover_key(&gpui::ElementId::from("pr-primary"));
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
            .text_color(rgb(INK_STRONG))
            .hover_control(key)
            .press_control()
            .child(label)
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                if action == PrAction::MergeNow && view.panel.pr_armed != Some(thread) {
                    view.panel.pr_armed = Some(thread);
                    cx.notify();
                    return;
                }
                view.panel.pr_armed = None;
                view.run_pr_action(thread, action, cx);
            }))
            .into_any_element()
    }

    /// The views as a segmented group, the checks and comments after it.
    fn pr_views(&self, thread: ThreadId, number: u64, shown: PrTab, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let segments = PrTab::ALL.into_iter().map(|tab| {
            let id = SharedString::from(format!("pr-tab-{}", tab.label()));
            let selector = id.to_string();
            let key = ferrite_ui::pointer::hover_key(&gpui::ElementId::from(id.clone()));
            let pressed = tab == shown;
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
                .child(tab.label())
                .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                    view.select_pr_tab(thread, number, tab, cx);
                }));
            if tab != PrTab::Code {
                return segment.into_any_element();
            }
            // Code's diff is read as it comes near, not after the press:
            // the segment's own hover is its wash's, so a wrapper listens.
            div()
                .id("pr-tab-Code-near")
                .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
                    if *hovered {
                        view.ensure_branch_changes(thread, cx);
                    }
                }))
                .child(segment)
                .into_any_element()
        });
        let snapshot = self.pr_snapshot(thread);
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

    /// Summary: the title, where it merges, the description, the checks.
    fn pr_summary(&self, pr: &WirePr) -> AnyElement {
        use ferrite_ui::theme::*;
        let merge = match pr.merge {
            ferrite_core::protocol::WireMerge::Clean => Some("Ready to merge"),
            ferrite_core::protocol::WireMerge::Conflicting => Some("Has conflicts"),
            ferrite_core::protocol::WireMerge::Blocked => Some("Blocked"),
            ferrite_core::protocol::WireMerge::Unknown => None,
        };
        let facts = std::iter::once(format!("{} \u{2192} {}", pr.head, pr.base))
            .chain(merge.map(str::to_string))
            .collect::<Vec<_>>()
            .join(" \u{b7} ");
        let checks = pr.checks.iter().map(|check| {
            let (glyph, ink) = match check.status {
                WireCheckStatus::Success => (icons::CHECK_CIRCLE, GREEN),
                WireCheckStatus::Failure => (icons::X_CIRCLE, RED),
                WireCheckStatus::Running | WireCheckStatus::Pending => (icons::CIRCLE_DASHED, YELLOW),
                WireCheckStatus::Skipped | WireCheckStatus::Unknown => (icons::CIRCLE_MINUS, INK_SUBTLE),
            };
            div()
                .flex()
                .items_center()
                .gap(px(SPACE_2))
                .h(px(PR_ROW_H))
                .child(icon(glyph, ICON_CHEVRON, ink))
                .child(div().min_w_0().truncate().child(SharedString::from(check.name.clone())))
        });
        pr_scroll("pr-summary")
            .child(
                div()
                    .debug_selector(|| "pr-title".into())
                    .text_size(px(FS_UI))
                    .font_weight(W_LABEL)
                    .text_color(rgb(INK_STRONG))
                    .child(SharedString::from(pr.title.clone())),
            )
            .child(div().text_color(rgb(INK_SUBTLE)).child(SharedString::from(facts)))
            .when(!pr.body.is_empty(), |summary| {
                summary.child(
                    div()
                        .mt(px(SPACE_3))
                        .text_size(px(FS_META))
                        .child(SharedString::from(pr.body.clone())),
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
            .into_any_element()
    }

    /// Guide: the PR's latest review guide, or the way to ask for one.
    fn pr_guide(&self, thread: ThreadId, pr: &WirePr, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        if let Some(text) = guide(pr) {
            return pr_scroll("pr-guide")
                .text_size(px(FS_META))
                .child(SharedString::from(text.to_string()))
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
            .child(
                div()
                    .id("pr-write-guide")
                    .debug_selector(|| "pr-write-guide".into())
                    .flex()
                    .items_center()
                    .h(px(SURFACE_TAB_H))
                    .px(px(PR_SEGMENT_PAD_X))
                    .rounded(px(R_CONTROL))
                    .bg(paint::RAISED)
                    .text_color(rgb(INK_STRONG))
                    .hover_control("pr-write-guide")
                    .press_control()
                    .child("Write guide")
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        view.run_pr_action(thread, PrAction::WriteGuide, cx);
                    })),
            )
            .into_any_element()
    }

    /// Timeline: the conversation, newest first (T3's default order).
    fn pr_timeline(&self, pr: &WirePr) -> AnyElement {
        use ferrite_ui::theme::*;
        if pr.comments.is_empty() {
            return pr_note("No comments yet".into());
        }
        let entries = pr.comments.iter().rev().map(|comment| {
            let kind = match comment.kind {
                WirePrCommentKind::Review => "reviewed",
                WirePrCommentKind::Finding => "commented on",
                WirePrCommentKind::Comment | WirePrCommentKind::Unknown => "commented",
            };
            let what = match comment.kind {
                WirePrCommentKind::Finding => format!("{} {kind} {}", comment.author, comment.anchor),
                _ => format!("{} {kind}", comment.author),
            };
            div()
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
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(rgb(INK_SUBTLE))
                                .child(SharedString::from(comment.created_at.clone())),
                        ),
                )
                .when(!comment.body.is_empty(), |entry| {
                    entry.child(
                        div()
                            .text_size(px(FS_META))
                            .child(SharedString::from(comment.body.clone())),
                    )
                })
        });
        pr_scroll("pr-timeline").children(entries).into_any_element()
    }
}

/// A merged or closed PR's pill (T3's outline Badge): its state's icon and
/// word in its ink.
fn pr_pill(tone: PrTone) -> Div {
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
